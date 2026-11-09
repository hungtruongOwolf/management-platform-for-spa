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
use org::profile::Profile;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
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
        net_pay: slips.iter().map(|s| s.net_pay).sum(),
    }
}

/// Computes the live payslips of `month` for `only` (one person) or everyone employed in it,
/// under `rules` (the caller loads them once, so the close audit records exactly the rules the
/// snapshots used). No authorization: callers check theirs first.
async fn compute_in(
    tx: &mut TenantTx,
    month: Month,
    only: Option<Uuid>,
    today: NaiveDate,
    rules: &PayrollRules,
) -> Result<Vec<Payslip>, AppError> {
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
    let adjustments = adjustment::adjustments_in(tx, month, only).await?;
    let evaluations = evaluation::evaluations_in(tx, month, only).await?;
    let recorded_until = attendance::recorded_until(month, today);
    Ok(profiles
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
        .collect())
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
        compute_in(tx, month, only, today, &rules).await
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
    tx.commit().await?;
    Ok(PayrollMonth {
        month: month.to_string(),
        closed: info.closed,
        closed_at: info.closed_at,
        closed_by_name: info.closed_by_name,
        ended: month.last_day() < today,
        totals: totals(&payslips),
        payslips,
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
    let slips = compute_in(&mut tx, month, None, today, &rules).await?;
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
    })?;
    idempotency::complete(&mut tx, actor, key, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}
