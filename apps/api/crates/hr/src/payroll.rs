//! Monthly payroll (spec staff-management US5): live payslips for an open month, frozen
//! snapshots for a closed one, and the close itself. Salary data: `payroll.read` to see,
//! `payroll.close` to close. Technicians see only their own payslip through `me`.

use crate::adjustment::{self, AdjustmentKind};
use crate::attendance::{self, AttendanceDay};
use crate::calc::{self, CommissionLine, Day, MonthInput, PayFlag, Payslip, UpsellLine};
use crate::evaluation;
use crate::month::Month;
use crate::period::{self, PeriodInfo};
use crate::repo;
use crate::rules::{self, PayrollRules};
use chrono::{DateTime, NaiveDate, Utc};
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, Permission, TenantTx};
use org::Employment;
use org::profile::Profile;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashSet;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PayrollTotals {
    pub staff: u32,
    /// Σ of the payslips' `base_pay` (the month's base, prorated for a month not worked in full).
    pub base_salary: i64,
    pub deductions: i64,
    pub commission: i64,
    pub bonuses: i64,
    pub penalties: i64,
    /// Σ of the payslips' `meal_allowance` (Hỗ trợ tiền ăn, part of net pay).
    pub meal_allowance: i64,
    /// Σ of the payslips' `net_pay_shortfall`: deductions beyond a month's pay, not deducted.
    /// `base_salary − deductions + commission + meal_allowance + bonuses − penalties +
    /// net_pay_shortfall = net_pay`, as on each payslip.
    pub net_pay_shortfall: i64,
    pub net_pay: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PayrollMonth {
    pub month: String,
    pub closed: bool,
    pub closed_at: Option<DateTime<Utc>>,
    pub closed_by_name: Option<String>,
    /// The month has ended (it can be closed).
    pub ended: bool,
    pub payslips: Vec<Payslip>,
    pub totals: PayrollTotals,
    /// Who must get pay terms or an evaluation before the month can close (ADR-0019 D1;
    /// `POST /payroll/close` answers `409 payroll.missing_data` with the same list). Empty once
    /// closed.
    pub missing: Vec<MissingPayrollData>,
    /// Refunds dated this month of packages closed by someone who does not work in it, and where
    /// each was settled (ADR-0019 D2b).
    pub leaver_refunds: Vec<LeaverRefund>,
}

fn totals(slips: &[Payslip]) -> PayrollTotals {
    PayrollTotals {
        staff: u32::try_from(slips.len()).unwrap_or(u32::MAX),
        base_salary: slips.iter().map(|s| s.base_pay).sum(),
        deductions: slips
            .iter()
            .map(|s| s.leave_deduction + s.late_deduction)
            .sum(),
        commission: slips
            .iter()
            .map(|s| s.service_commission + s.closing_commission)
            .sum(),
        bonuses: slips.iter().map(|s| s.bonuses).sum(),
        penalties: slips.iter().map(|s| s.penalties).sum(),
        meal_allowance: slips.iter().map(|s| s.meal_allowance).sum(),
        net_pay_shortfall: slips.iter().map(|s| s.net_pay_shortfall).sum(),
        net_pay: slips.iter().map(|s| s.net_pay).sum(),
    }
}

/// The payroll month that settles a refund dated `day` for the closer of the refunded package
/// (ADR-0019 D2b): the refund's own month when they work in it, else the month of their last
/// working day before it (a leaver's final month); `None` when they had not started yet.
pub fn settling_month(employment: &Employment, day: NaiveDate) -> Option<Month> {
    let own = Month::of(day);
    if employment.overlaps(own.first_day(), own.last_day()) {
        return Some(own);
    }
    employment
        .periods()
        .iter()
        .filter_map(|p| p.ended_on)
        .filter(|end| *end < day)
        .max()
        .map(Month::of)
}

/// A refund of a package closed by someone who does not work in the refund's month (ADR-0019
/// D2b): deducted from their last month while it was open, else not clawed back. Listed with the
/// month of the refund so it is never dropped silently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LeaverRefund {
    pub refund_id: Uuid,
    /// The closer of the package.
    pub staff_id: Uuid,
    pub full_name: String,
    /// The package.
    pub description: String,
    /// The amount refunded (positive).
    pub amount: i64,
    pub refund_date: NaiveDate,
    /// `YYYY-MM` of the payslip it is deducted from (the closer's last month, open when the
    /// refund was made); `null` = not clawed back: that month was already closed, the spa bears
    /// it.
    pub deducted_in: Option<String>,
}

/// What is missing for someone on the month's payroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MissingItem {
    /// No pay terms: every amount would be frozen at 0đ.
    PayTerms,
    /// No monthly evaluation: the Tết level would be frozen at 0đ.
    Evaluation,
}

/// Someone whose data must be completed before the month can close (ADR-0019 D1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MissingPayrollData {
    pub staff_id: Uuid,
    pub full_name: String,
    pub missing: Vec<MissingItem>,
}

/// Who blocks the close (ADR-0019 D1): everyone on the payroll without pay terms or without the
/// month's evaluation, except `exempt` — the profiles of people who close payroll themselves
/// (the owner is not paid or evaluated by the books).
pub fn missing_data(slips: &[Payslip], exempt: &HashSet<Uuid>) -> Vec<MissingPayrollData> {
    slips
        .iter()
        .filter(|s| !exempt.contains(&s.staff_id))
        .filter_map(|s| {
            let missing: Vec<MissingItem> = [
                (PayFlag::NoPayTerms, MissingItem::PayTerms),
                (PayFlag::EvaluationMissing, MissingItem::Evaluation),
            ]
            .into_iter()
            .filter(|(flag, _)| s.flags.contains(flag))
            .map(|(_, item)| item)
            .collect();
            (!missing.is_empty()).then(|| MissingPayrollData {
                staff_id: s.staff_id,
                full_name: s.full_name.clone(),
                missing,
            })
        })
        .collect()
}

/// Profiles linked to a login that may close payroll (the owner): not required to have pay terms
/// or an evaluation.
async fn close_exempt(tx: &mut TenantTx) -> Result<HashSet<Uuid>, AppError> {
    Ok(
        repo::supervising_staff(tx, &[Permission::PayrollClose.as_str().to_owned()])
            .await?
            .into_iter()
            .collect(),
    )
}

/// The month's refunds of packages closed by someone not working in it, and where each was
/// settled (ADR-0019 D2b). A deduction counts once the refund was recorded before its settling
/// month closed.
async fn leaver_refunds(tx: &mut TenantTx, month: Month) -> Result<Vec<LeaverRefund>, AppError> {
    let refunds = repo::package_refunds_between(tx, month.first_day(), month.last_day()).await?;
    if refunds.is_empty() {
        return Ok(Vec::new());
    }
    let profiles = org::profile::profiles_in(tx).await?;
    let mut out = Vec::new();
    for r in refunds {
        let Some(p) = profiles.iter().find(|p| p.id == r.staff_id) else {
            continue;
        };
        if period::employed_in(p, month) {
            // On their own payslip of this month (ADR-0010).
            continue;
        }
        let deducted_in = match settling_month(&p.employment, r.business_date) {
            None => None,
            Some(last) => {
                let info = period::info(tx, last).await?;
                let deducted = !info.closed || info.closed_at.is_some_and(|at| r.created_at < at);
                deducted.then(|| last.to_string())
            }
        };
        out.push(LeaverRefund {
            refund_id: r.refund_id,
            staff_id: p.id,
            full_name: p.full_name.clone(),
            description: r.description,
            amount: r.amount,
            refund_date: r.business_date,
            deducted_in,
        });
    }
    Ok(out)
}

/// Computes the live payslips of `month` for `only` (one person) or everyone employed in it,
/// under `rules` (the caller loads them once, so the close audit records exactly the rules the
/// snapshots used), with the ids of the later refunds settled in it (D2b). No authorization:
/// callers check theirs first.
async fn compute_in(
    tx: &mut TenantTx,
    month: Month,
    only: Option<Uuid>,
    today: NaiveDate,
    rules: &PayrollRules,
) -> Result<(Vec<Payslip>, Vec<Uuid>), AppError> {
    let profiles: Vec<Profile> = org::profile::profiles_in(tx)
        .await?
        .into_iter()
        .filter(|p| period::employed_in(p, month) && only.is_none_or(|id| id == p.id))
        .collect();
    let pays = org::profile::pay_terms(tx).await?;
    let days = attendance::days_in(tx, month, only).await?;
    let (from, to) = (month.first_day(), month.last_day());
    let lines = repo::commission_lines(tx, from, to).await?;
    let upsell = repo::upsell_lines(tx, from, to).await?;
    // D2b: a refund after this month of a package closed by someone whose last month this is
    // comes back from this payslip while the month is open.
    let later: Vec<repo::PackageRefundRow> = match to.succ_opt().filter(|next| *next <= today) {
        None => Vec::new(),
        Some(next) => repo::package_refunds_between(tx, next, today)
            .await?
            .into_iter()
            .filter(|r| {
                profiles.iter().any(|p| {
                    p.id == r.staff_id
                        && settling_month(&p.employment, r.business_date) == Some(month)
                })
            })
            .collect(),
    };
    let adjustments = adjustment::adjustments_in(tx, month, only).await?;
    let evaluations = evaluation::evaluations_in(tx, month, only).await?;
    let recorded_until = attendance::recorded_until(month, today);
    let slips = profiles
        .iter()
        .map(|p| {
            let my_days: Vec<Day> = days
                .iter()
                .filter(|d| d.staff_id == p.id)
                .map(AttendanceDay::as_calc)
                .collect();
            let my_lines: Vec<CommissionLine> = lines
                .iter()
                .filter(|l| l.staff_id == p.id)
                .map(|l| CommissionLine {
                    service_name: l.service_name.clone(),
                    source: l.source.clone(),
                    qty: l.qty,
                    value: l.value,
                    full_value: l.full_value,
                })
                .collect();
            let my_upsell: Vec<UpsellLine> = upsell
                .iter()
                .filter(|l| l.staff_id == p.id)
                .map(|l| UpsellLine {
                    kind: l.kind.clone(),
                    description: l.description.clone(),
                    qty: l.qty,
                    value: l.value,
                })
                .chain(
                    later
                        .iter()
                        .filter(|r| r.staff_id == p.id)
                        .map(|r| UpsellLine {
                            kind: "package_refund".to_owned(),
                            description: r.description.clone(),
                            qty: 1,
                            value: -r.amount,
                        }),
                )
                .collect();
            let mine = adjustments.iter().filter(|a| a.staff_id == p.id);
            let sum = |kind: AdjustmentKind, money: bool| -> i64 {
                mine.clone()
                    .filter(|a| a.kind == kind)
                    .map(|a| {
                        if money {
                            a.amount.unwrap_or(0)
                        } else {
                            i64::from(a.points.unwrap_or(0))
                        }
                    })
                    .sum()
            };
            let input = MonthInput {
                month,
                recorded_until,
                employment: p.employment.clone(),
                pay: pays.iter().find(|(id, _)| *id == p.id).map(|(_, t)| *t),
                days: &my_days,
                commission_lines: &my_lines,
                upsell_lines: &my_upsell,
                bonuses: sum(AdjustmentKind::Bonus, true),
                penalties: sum(AdjustmentKind::Penalty, true),
                bonus_points: sum(AdjustmentKind::Bonus, false),
                penalty_points: sum(AdjustmentKind::Penalty, false),
                evaluation: evaluations
                    .iter()
                    .find(|e| e.staff_id == p.id)
                    .map(|e| e.result),
            };
            calc::compute(p.id, &p.full_name, &p.position, &input, rules)
        })
        .collect();
    Ok((slips, later.iter().map(|r| r.refund_id).collect()))
}

fn parse_slips(raw: Vec<Value>) -> Result<Vec<Payslip>, AppError> {
    raw.into_iter()
        .map(|v| Payslip::from_snapshot(v).map_err(|e| AppError::Internal(format!("payslip: {e}"))))
        .collect()
}

/// The month's payslips: frozen snapshots once closed, live otherwise.
pub async fn payslips_in(
    tx: &mut TenantTx,
    month: Month,
    only: Option<Uuid>,
    info: &PeriodInfo,
    today: NaiveDate,
) -> Result<Vec<Payslip>, AppError> {
    if info.closed {
        parse_slips(repo::payslips(tx, month.first_day(), only).await?)
    } else {
        let (rules, _) = rules::load(tx).await?;
        Ok(compute_in(tx, month, only, today, &rules).await?.0)
    }
}

/// `GET /payroll?month=` (`payroll.read`).
pub async fn month(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<PayrollMonth, AppError> {
    actor.require(Permission::PayrollRead)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let info = period::info(&mut tx, month).await?;
    let payslips = payslips_in(&mut tx, month, None, &info, today).await?;
    let missing = if info.closed {
        Vec::new()
    } else {
        missing_data(&payslips, &close_exempt(&mut tx).await?)
    };
    let leaver_refunds = leaver_refunds(&mut tx, month).await?;
    tx.commit().await?;
    Ok(PayrollMonth {
        month: month.to_string(),
        closed: info.closed,
        closed_at: info.closed_at,
        closed_by_name: info.closed_by_name,
        ended: month.last_day() < today,
        totals: totals(&payslips),
        payslips,
        missing,
        leaver_refunds,
    })
}

/// `GET /payroll/payslips/{staff_id}?month=` (`payroll.read`): one payslip, and whether its month
/// is closed.
pub async fn payslip(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
    staff_id: Uuid,
) -> Result<(Payslip, bool), AppError> {
    actor.require(Permission::PayrollRead)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let info = period::info(&mut tx, month).await?;
    let slip = payslips_in(&mut tx, month, Some(staff_id), &info, today)
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok((slip, info.closed))
}

pub struct CloseInput {
    pub month: Month,
    /// Close although some employed days have no attendance.
    pub allow_incomplete: bool,
}

/// `POST /payroll/close` (`payroll.close`, Idempotency-Key): freezes every payslip of an ended
/// month and locks its records. Never reopened; corrections go into a later month.
pub async fn close(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &CloseInput,
    to_body: impl FnOnce(PayrollMonth) -> Result<Value, AppError>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::PayrollClose)?;
    actor.require(Permission::PayrollRead)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let today = period::today(&mut tx).await?;
    let month = input.month;
    if month.last_day() >= today {
        return Err(AppError::Unprocessable {
            code: "payroll.month_not_ended",
        });
    }
    if repo::lock_period(&mut tx, month.first_day(), true).await? == "closed" {
        return Err(period::closed());
    }
    let (rules, _) = rules::load(&mut tx).await?;
    let (slips, settled_refunds) = compute_in(&mut tx, month, None, today, &rules).await?;
    // D1: a month closed without pay terms or an evaluation would freeze 0đ with no way back.
    let missing = missing_data(&slips, &close_exempt(&mut tx).await?);
    if !missing.is_empty() {
        return Err(AppError::ConflictDetails {
            code: "payroll.missing_data",
            details: json!({ "staff": missing }),
        });
    }
    if !input.allow_incomplete
        && slips
            .iter()
            .any(|s| s.flags.contains(&PayFlag::UnrecordedDays))
    {
        return Err(AppError::Unprocessable {
            code: "payroll.unrecorded_days",
        });
    }
    for s in &slips {
        let detail = serde_json::to_value(s).map_err(|e| AppError::Internal(e.to_string()))?;
        repo::insert_payslip(&mut tx, month.first_day(), s.staff_id, s.net_pay, &detail).await?;
    }
    repo::close_period(&mut tx, month.first_day(), actor.user_id.as_uuid()).await?;
    let t = totals(&slips);
    let leaver_refunds = leaver_refunds(&mut tx, month).await?;
    // D2b: both outcomes of a leaver's refund are on record — deducted here, or not clawed back.
    let not_clawed_back: Vec<Value> = leaver_refunds
        .iter()
        .filter(|r| r.deducted_in.is_none())
        .map(|r| json!({"refund_id": r.refund_id, "staff_id": r.staff_id, "amount": r.amount}))
        .collect();
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "payroll.close",
            entity_type: "tenant",
            entity_id: actor.tenant_id.as_uuid(),
            before: None,
            after: Some(json!({
                "month": month.to_string(), "payslips": t.staff, "net_pay": t.net_pay,
                "allow_incomplete": input.allow_incomplete,
                "rules": serde_json::to_value(&rules).ok(),
                "leaver_refunds_deducted": settled_refunds,
                "leaver_refunds_not_clawed_back": not_clawed_back,
            })),
            reason: None,
        },
    )
    .await?;
    let info = period::info(&mut tx, month).await?;
    let body = to_body(PayrollMonth {
        month: month.to_string(),
        closed: info.closed,
        closed_at: info.closed_at,
        closed_by_name: info.closed_by_name,
        ended: true,
        totals: t,
        payslips: slips,
        missing: Vec::new(),
        leaver_refunds,
    })?;
    idempotency::complete(&mut tx, actor, key, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::Level;
    use chrono::Datelike;
    use org::Period;
    use org::profile::PayTerms;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    fn period(started: Option<&str>, ended: Option<&str>) -> Period {
        Period {
            id: Uuid::nil(),
            started_on: started.map(d),
            ended_on: ended.map(d),
            end_reason: None,
        }
    }

    fn settled(e: &Employment, day: &str) -> Option<String> {
        settling_month(e, d(day)).map(|m| m.to_string())
    }

    /// ADR-0019 D2b: a refund after the closer left is settled in their last month; while they
    /// work in the refund's month it stays there (ADR-0010).
    #[test]
    fn a_refund_is_settled_in_the_closers_last_month_once_they_left() {
        let october = Some("2026-10".to_owned());
        let left = Employment::new(vec![period(Some("2026-03-01"), Some("2026-10-20"))]);
        assert_eq!(settled(&left, "2026-11-05"), october);
        assert_eq!(settled(&left, "2027-02-01"), october, "any later refund");
        assert_eq!(
            settled(&left, "2026-10-25"),
            october,
            "left on the 20th, refund on the 25th: still their month"
        );
        assert_eq!(
            settled(&Employment::always(), "2026-11-05"),
            Some("2026-11".to_owned())
        );
        // Left in October, rehired in December: a November refund goes back to October; a
        // December one is on the December payslip.
        let rehired = Employment::new(vec![
            period(Some("2026-03-01"), Some("2026-10-20")),
            period(Some("2026-12-10"), None),
        ]);
        assert_eq!(settled(&rehired, "2026-11-15"), october);
        assert_eq!(settled(&rehired, "2026-12-01"), Some("2026-12".to_owned()));
        let later_start = Employment::new(vec![period(Some("2027-01-01"), None)]);
        assert_eq!(settled(&later_start, "2026-11-15"), None);
    }

    fn payslip(id: u128, pay: bool, evaluated: bool) -> Payslip {
        let input = MonthInput {
            month: Month::of(d("2026-10-01")),
            recorded_until: None,
            employment: Employment::always(),
            pay: pay.then_some(PayTerms {
                base_salary: 5_000_000,
                commission_rate_bp: 500,
                loyalty_bonus_monthly: 0,
                meal_allowance: 0,
                version: 1,
            }),
            days: &[],
            commission_lines: &[],
            upsell_lines: &[],
            bonuses: 0,
            penalties: 0,
            bonus_points: 0,
            penalty_points: 0,
            evaluation: evaluated.then_some(Level::M1),
        };
        calc::compute(
            Uuid::from_u128(id),
            "Lan",
            "KTV",
            &input,
            &PayrollRules::default(),
        )
    }

    /// The payroll table's columns (Lương cứng, Trừ nghỉ / muộn, Hoa hồng, Tiền ăn, Thưởng − phạt)
    /// add up to Thực lĩnh on every row and in the totals; the shortfall of a month whose
    /// deductions exceed its pay (ADR-0019 D3) is the one extra term, shown with its flag.
    #[test]
    fn the_table_columns_add_up_to_net_pay_on_every_row_and_in_the_totals() {
        let month = Month::of(d("2026-10-01"));
        let days = |leave: &[u32], late: u32| -> Vec<Day> {
            month
                .iter_days()
                .map(|date| Day {
                    date,
                    status: if leave.contains(&date.day()) {
                        attendance::AttendanceStatus::AbsentK
                    } else {
                        attendance::AttendanceStatus::Present
                    },
                    late_minutes: if date.day() <= 4 { late } else { 0 },
                })
                .collect()
        };
        let lines = [CommissionLine {
            service_name: "Thư Giãn".to_owned(),
            source: "one_off".to_owned(),
            qty: 9,
            value: 2_250_000,
            full_value: 2_250_000,
        }];
        let full = days(&[], 0);
        let missed = days(&[3, 9, 10, 17], 40);
        let rules = PayrollRules {
            late_penalty_amount: 20_000,
            ..PayrollRules::default()
        };
        let slip =
            |id: u128, days: &[Day], employment: Employment, bonuses: i64, penalties: i64| {
                let input = MonthInput {
                    month,
                    recorded_until: None,
                    employment,
                    pay: Some(PayTerms {
                        base_salary: 4_500_000,
                        commission_rate_bp: 700,
                        loyalty_bonus_monthly: 500_000,
                        meal_allowance: 400_000,
                        version: 1,
                    }),
                    days,
                    commission_lines: &lines,
                    upsell_lines: &[],
                    bonuses,
                    penalties,
                    bonus_points: 0,
                    penalty_points: 0,
                    evaluation: Some(Level::M2),
                };
                calc::compute(Uuid::from_u128(id), "Lan", "KTV", &input, &rules)
            };
        let slips = [
            slip(1, &full, Employment::always(), 0, 0),
            slip(2, &missed, Employment::always(), 200_000, 50_000),
            slip(
                3,
                &full,
                Employment::between(Some(d("2026-10-20")), None),
                0,
                0,
            ),
            slip(4, &full, Employment::always(), 0, 9_000_000),
        ];
        assert!(slips.iter().all(|s| s.meal_allowance > 0), "{slips:?}");
        assert!(slips[1].leave_deduction > 0 && slips[1].late_deduction > 0);
        assert!(slips[2].base_pay < slips[2].base_salary, "a partial month");
        assert!(slips[3].net_pay == 0 && slips[3].net_pay_shortfall > 0);
        assert!(slips[3].flags.contains(&PayFlag::NetPayShortfall));
        for s in &slips {
            let columns = s.base_pay - (s.leave_deduction + s.late_deduction)
                + (s.service_commission + s.closing_commission)
                + s.meal_allowance
                + (s.bonuses - s.penalties);
            assert_eq!(columns + s.net_pay_shortfall, s.net_pay, "{}", s.staff_id);
        }
        let t = totals(&slips);
        assert_eq!(
            t.meal_allowance,
            slips.iter().map(|s| s.meal_allowance).sum::<i64>()
        );
        assert_eq!(
            t.base_salary - t.deductions + t.commission + t.meal_allowance + t.bonuses
                - t.penalties
                + t.net_pay_shortfall,
            t.net_pay
        );
    }

    /// ADR-0019 D1: the close is blocked by anyone on the payroll without pay terms or an
    /// evaluation, except the profile of whoever closes payroll (the owner).
    #[test]
    fn missing_pay_terms_or_evaluation_blocks_the_close_except_for_the_owner() {
        let slips = [
            payslip(1, true, true),
            payslip(2, false, true),
            payslip(3, true, false),
            payslip(4, false, false),
        ];
        let got: Vec<(u128, Vec<MissingItem>)> = missing_data(&slips, &HashSet::new())
            .iter()
            .map(|m| (m.staff_id.as_u128(), m.missing.clone()))
            .collect();
        assert_eq!(
            got,
            vec![
                (2, vec![MissingItem::PayTerms]),
                (3, vec![MissingItem::Evaluation]),
                (4, vec![MissingItem::PayTerms, MissingItem::Evaluation]),
            ]
        );
        let owner: HashSet<Uuid> = [Uuid::from_u128(4)].into_iter().collect();
        assert_eq!(missing_data(&slips, &owner).len(), 2);
        assert!(missing_data(&slips[..1], &HashSet::new()).is_empty());
    }
}
