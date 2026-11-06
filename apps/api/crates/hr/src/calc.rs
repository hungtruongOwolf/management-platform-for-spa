//! Pure payroll calculator (principles: pure calculators). Given one staff member's month — pay
//! terms, attendance, commission inputs, adjustments, evaluation — and the spa's rules, computes
//! the payslip. No I/O: every rule is unit-tested here.
//!
//! Net pay = base − leave deduction − late deduction + service commission + closing commission
//!         + meal allowance + bonuses − penalties.
//! Thưởng gắn bó and the Tết level are *recorded* on the payslip (paid at the milestone / at Tết),
//! never added to the month's net (compensation-rules.md §6–7).

use crate::attendance::{AttendanceStatus, LeaveCode};
use crate::evaluation::Level;
use crate::month::Month;
use crate::rules::{PartialMonthLoyalty, PayrollRules};
use chrono::NaiveDate;
use org::profile::PayTerms;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// `a × b ÷ c`, rounded half up, without overflow. `c` must be positive.
pub fn mul_div_round(a: i64, b: i64, c: i64) -> i64 {
    if c <= 0 {
        return 0;
    }
    let (a, b, c) = (i128::from(a), i128::from(b), i128::from(c));
    let n = a * b;
    let q = if n >= 0 {
        (n + c / 2) / c
    } else {
        (n - c / 2) / c
    };
    i64::try_from(q).unwrap_or(i64::MAX)
}

/// One recorded day as the calculator needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Day {
    pub date: NaiveDate,
    pub status: AttendanceStatus,
    pub late_minutes: u32,
}

/// Commission base of one service performed in the month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CommissionLine {
    pub service_name: String,
    /// `one_off` or `package`.
    pub source: String,
    /// Sessions / quantity performed.
    pub qty: i64,
    /// What the lines are worth (`visit_line.line_value`): gift sessions count 0.
    pub value: i64,
    /// Same lines at full per-session value, gift sessions included.
    pub full_value: i64,
}

/// Upsell credit of one item sold in the month ("người giới thiệu / chốt", compensation-rules
/// §3.2, EM 2026-11-06 Q-U3): a package closed by this person or a one-off service they
/// recommended. Performing the service is a separate credit ([`CommissionLine`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpsellLine {
    /// `package` or `service`.
    pub kind: String,
    pub description: String,
    /// Packages sold / services recommended.
    pub qty: i64,
    /// Price paid (packages, after discount) or `line_value` (services).
    pub value: i64,
}

pub struct MonthInput<'a> {
    pub month: Month,
    /// Days before this one are not counted (unfinished current month); `None` = whole month.
    pub recorded_until: Option<NaiveDate>,
    pub joined_on: Option<NaiveDate>,
    pub left_on: Option<NaiveDate>,
    pub pay: Option<PayTerms>,
    pub days: &'a [Day],
    pub commission_lines: &'a [CommissionLine],
    /// Upsell credits: packages closed (Σ = `sale.total` of open sales) and one-off services
    /// recommended by this person.
    pub upsell_lines: &'a [UpsellLine],
    pub bonuses: i64,
    pub penalties: i64,
    pub bonus_points: i64,
    pub penalty_points: i64,
    pub evaluation: Option<Level>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AttendanceTotals {
    pub present: u32,
    pub late_count: u32,
    pub late_minutes: u32,
    pub leave_p: u32,
    pub leave_dx: u32,
    pub absent_k: u32,
    /// Employed days (up to yesterday in the current month) with nothing recorded.
    pub unrecorded: u32,
}

impl AttendanceTotals {
    pub const fn leave_days(&self) -> u32 {
        self.leave_p + self.leave_dx + self.absent_k
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayFlag {
    /// No pay terms set: every amount is 0.
    NoPayTerms,
    /// Some employed days have no attendance yet.
    UnrecordedDays,
    /// ≥ 5 K days in the month (Nhóm A #8: tự ý bỏ việc) — owner decides.
    AbsentK5Plus,
    /// No evaluation for the month: no Tết level.
    EvaluationMissing,
}

/// The computed payslip. Stored as JSON in `app.payslip.detail` when the month closes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Payslip {
    pub staff_id: Uuid,
    pub full_name: String,
    pub position: String,
    /// `YYYY-MM`.
    pub month: String,
    pub days_in_month: u32,
    pub employed_days: u32,
    pub base_salary: i64,
    pub commission_rate_bp: i32,
    /// base ÷ (days in month − free leave days), rounded.
    pub day_rate: i64,
    pub attendance: AttendanceTotals,
    pub free_leave_days: u32,
    /// Leave days that use up the free allowance (codes in `free_leave_codes`).
    pub leave_days_counted: u32,
    /// Days deducted: counted days beyond the allowance + days of codes never free.
    pub deducted_leave_days: u32,
    pub leave_deduction: i64,
    /// Late arrivals beyond the grace minutes.
    pub late_penalised: u32,
    pub late_deduction: i64,
    pub service_commission_base: i64,
    pub service_commission: i64,
    /// Upsell base ("hoa hồng giới thiệu / chốt", §3.2) = packages + recommended services.
    pub closing_commission_base: i64,
    pub closing_commission: i64,
    /// Part of the upsell base from packages closed.
    #[serde(default)]
    pub upsell_package_base: i64,
    /// Part of the upsell base from one-off services recommended (Q-U3).
    #[serde(default)]
    pub upsell_service_base: i64,
    pub meal_allowance: i64,
    pub bonuses: i64,
    pub penalties: i64,
    pub bonus_points: i64,
    pub penalty_points: i64,
    pub net_pay: i64,
    /// Thưởng gắn bó recorded this month (paid at the milestone; not in `net_pay`).
    pub loyalty_bonus_accrued: i64,
    pub evaluation_level: Option<Level>,
    /// Tết level of the month (lower of the leave level and the evaluation); `None` = not
    /// evaluated yet. `fail` = "không tính".
    pub tet_level: Option<Level>,
    /// Tết bonus recorded for the month (not in `net_pay`).
    pub tet_amount: i64,
    pub commission_lines: Vec<CommissionLine>,
    /// What the upsell commission is computed from, item by item (older snapshots: empty).
    #[serde(default)]
    pub upsell_lines: Vec<UpsellLine>,
    pub flags: Vec<PayFlag>,
}

fn employed(input: &MonthInput<'_>, day: NaiveDate) -> bool {
    employed_on(input.joined_on, input.left_on, day)
}

fn employed_on(joined_on: Option<NaiveDate>, left_on: Option<NaiveDate>, day: NaiveDate) -> bool {
    joined_on.is_none_or(|j| day >= j) && left_on.is_none_or(|l| day <= l)
}

fn totals(input: &MonthInput<'_>) -> AttendanceTotals {
    summarize(
        input.month,
        input.recorded_until,
        input.joined_on,
        input.left_on,
        input.days,
    )
}

/// Month totals of one person's recorded days (the sheet's "Cộng" row). Days before
/// `recorded_until` (or all days) without a record while employed count as unrecorded.
pub fn summarize(
    month: Month,
    recorded_until: Option<NaiveDate>,
    joined_on: Option<NaiveDate>,
    left_on: Option<NaiveDate>,
    days: &[Day],
) -> AttendanceTotals {
    let mut t = AttendanceTotals::default();
    for d in days {
        match d.status {
            AttendanceStatus::Present => {
                t.present += 1;
                if d.late_minutes > 0 {
                    t.late_count += 1;
                    t.late_minutes += d.late_minutes;
                }
            }
            AttendanceStatus::LeaveP => t.leave_p += 1,
            AttendanceStatus::LeaveDx => t.leave_dx += 1,
            AttendanceStatus::AbsentK => t.absent_k += 1,
        }
    }
    let recorded: std::collections::HashSet<NaiveDate> = days.iter().map(|d| d.date).collect();
    t.unrecorded = u32::try_from(
        month
            .iter_days()
            .filter(|d| recorded_until.is_none_or(|until| *d < until))
            .filter(|d| employed_on(joined_on, left_on, *d) && !recorded.contains(d))
            .count(),
    )
    .unwrap_or(u32::MAX);
    t
}

/// Tết level by leave days (§6): < 2 → 1, = 2 → 2, 3–4 → 3, > 4 → không tính.
pub const fn leave_level(leave_days: u32) -> Level {
    match leave_days {
        0 | 1 => Level::M1,
        2 => Level::M2,
        3 | 4 => Level::M3,
        _ => Level::Fail,
    }
}

pub fn compute(
    staff_id: Uuid,
    full_name: &str,
    position: &str,
    input: &MonthInput<'_>,
    rules: &PayrollRules,
) -> Payslip {
    let pay = input.pay.unwrap_or(PayTerms {
        base_salary: 0,
        commission_rate_bp: 0,
        loyalty_bonus_monthly: 0,
        meal_allowance: 0,
        version: 0,
    });
    let dim = input.month.days();
    let employed_days = u32::try_from(
        input
            .month
            .iter_days()
            .filter(|d| employed(input, *d))
            .count(),
    )
    .unwrap_or(0);
    let att = totals(input);

    let count = |code: LeaveCode| match code {
        LeaveCode::LeaveP => att.leave_p,
        LeaveCode::LeaveDx => att.leave_dx,
        LeaveCode::AbsentK => att.absent_k,
    };
    let all_codes = [LeaveCode::LeaveP, LeaveCode::LeaveDx, LeaveCode::AbsentK];
    let counted: u32 = all_codes
        .iter()
        .filter(|c| rules.is_free(**c))
        .map(|c| count(*c))
        .sum();
    let never_free: u32 = all_codes
        .iter()
        .filter(|c| !rules.is_free(**c))
        .map(|c| count(*c))
        .sum();
    let deducted = counted.saturating_sub(rules.free_leave_days) + never_free;
    let divisor = i64::from(dim.saturating_sub(rules.free_leave_days).max(1));
    let day_rate = mul_div_round(pay.base_salary, 1, divisor);
    let leave_deduction = mul_div_round(pay.base_salary, i64::from(deducted), divisor);

    let late_penalised = u32::try_from(
        input
            .days
            .iter()
            .filter(|d| {
                d.status == AttendanceStatus::Present && d.late_minutes > rules.late_grace_minutes
            })
            .count(),
    )
    .unwrap_or(0);
    let late_deduction = rules
        .late_penalty_amount
        .saturating_mul(i64::from(late_penalised));

    let service_base: i64 = input
        .commission_lines
        .iter()
        .map(|l| {
            if rules.commission_on_bonus_sessions {
                l.full_value
            } else {
                l.value
            }
        })
        .sum();
    let rate = i64::from(pay.commission_rate_bp);
    let service_commission = mul_div_round(service_base, rate, 10_000);
    let upsell_of = |kind: &str| -> i64 {
        input
            .upsell_lines
            .iter()
            .filter(|l| l.kind == kind)
            .map(|l| l.value)
            .sum()
    };
    let (package_base, referred_base) = if rules.closing_commission {
        (upsell_of("package"), upsell_of("service"))
    } else {
        (0, 0)
    };
    let closing_base = package_base + referred_base;
    let closing_commission = mul_div_round(closing_base, rate, 10_000);

    let meal = if employed_days > 0 {
        pay.meal_allowance
    } else {
        0
    };
    let net_pay = pay.base_salary - leave_deduction - late_deduction
        + service_commission
        + closing_commission
        + meal
        + input.bonuses
        - input.penalties;

    let loyalty = match rules.loyalty_partial_month {
        PartialMonthLoyalty::Prorate => mul_div_round(
            pay.loyalty_bonus_monthly,
            i64::from(employed_days),
            i64::from(dim),
        ),
        PartialMonthLoyalty::Full if employed_days > 0 => pay.loyalty_bonus_monthly,
        PartialMonthLoyalty::NoneIfPartial if employed_days == dim => pay.loyalty_bonus_monthly,
        PartialMonthLoyalty::Full | PartialMonthLoyalty::NoneIfPartial => 0,
    };

    let tet_level = input
        .evaluation
        .map(|e| e.max(leave_level(att.leave_days())));
    let tet_amount = match tet_level {
        Some(Level::M1) => rules.tet_level_amounts[0],
        Some(Level::M2) => rules.tet_level_amounts[1],
        Some(Level::M3) => rules.tet_level_amounts[2],
        Some(Level::Fail) | None => 0,
    };

    let mut flags = Vec::new();
    if input.pay.is_none() {
        flags.push(PayFlag::NoPayTerms);
    }
    if att.unrecorded > 0 {
        flags.push(PayFlag::UnrecordedDays);
    }
    if att.absent_k >= 5 {
        flags.push(PayFlag::AbsentK5Plus);
    }
    if input.evaluation.is_none() {
        flags.push(PayFlag::EvaluationMissing);
    }

    Payslip {
        staff_id,
        full_name: full_name.to_owned(),
        position: position.to_owned(),
        month: input.month.to_string(),
        days_in_month: dim,
        employed_days,
        base_salary: pay.base_salary,
        commission_rate_bp: pay.commission_rate_bp,
        day_rate,
        attendance: att,
        free_leave_days: rules.free_leave_days,
        leave_days_counted: counted,
        deducted_leave_days: deducted,
        leave_deduction,
        late_penalised,
        late_deduction,
        service_commission_base: service_base,
        service_commission,
        closing_commission_base: closing_base,
        closing_commission,
        upsell_package_base: package_base,
        upsell_service_base: referred_base,
        meal_allowance: meal,
        bonuses: input.bonuses,
        penalties: input.penalties,
        bonus_points: input.bonus_points,
        penalty_points: input.penalty_points,
        net_pay,
        loyalty_bonus_accrued: loyalty,
        evaluation_level: input.evaluation,
        tet_level,
        tet_amount,
        commission_lines: input.commission_lines.to_vec(),
        upsell_lines: input.upsell_lines.to_vec(),
        flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    fn nov() -> Month {
        Month::of(d("2026-11-01"))
    }

    fn terms() -> PayTerms {
        PayTerms {
            base_salary: 5_000_000,
            commission_rate_bp: 500,
            loyalty_bonus_monthly: 500_000,
            meal_allowance: 300_000,
            version: 1,
        }
    }

    /// Every day of November present, except `leaves` (date, code).
    fn days(leaves: &[(u32, AttendanceStatus)]) -> Vec<Day> {
        nov()
            .iter_days()
            .map(|date| {
                let status = leaves
                    .iter()
                    .find(|(day, _)| date == d(&format!("2026-11-{day:02}")))
                    .map_or(AttendanceStatus::Present, |(_, s)| *s);
                Day {
                    date,
                    status,
                    late_minutes: 0,
                }
            })
            .collect()
    }

    fn input<'a>(days: &'a [Day], lines: &'a [CommissionLine]) -> MonthInput<'a> {
        MonthInput {
            month: nov(),
            recorded_until: None,
            joined_on: None,
            left_on: None,
            pay: Some(terms()),
            days,
            commission_lines: lines,
            upsell_lines: &[],
            bonuses: 0,
            penalties: 0,
            bonus_points: 0,
            penalty_points: 0,
            evaluation: Some(Level::M1),
        }
    }

    fn slip(input: &MonthInput<'_>, rules: &PayrollRules) -> Payslip {
        compute(Uuid::nil(), "Mai", "KTV", input, rules)
    }

    #[test]
    fn rounding_is_half_up() {
        assert_eq!(mul_div_round(5_000_000, 1, 28), 178_571);
        assert_eq!(mul_div_round(5, 1, 2), 3);
        assert_eq!(mul_div_round(150_000, 500, 10_000), 7_500);
        assert_eq!(mul_div_round(1, 1, 0), 0);
    }

    #[test]
    fn two_free_days_then_one_day_rate_per_extra_day_as_the_sheet() {
        let rules = PayrollRules::default();
        let two = days(&[
            (3, AttendanceStatus::LeaveP),
            (7, AttendanceStatus::LeaveDx),
        ]);
        let s = slip(&input(&two, &[]), &rules);
        assert_eq!((s.deducted_leave_days, s.leave_deduction), (0, 0));
        assert_eq!(s.day_rate, 178_571, "5tr ÷ (30 − 2)");
        assert_eq!(s.net_pay, 5_000_000 + 300_000);

        let four = days(&[
            (3, AttendanceStatus::LeaveP),
            (7, AttendanceStatus::LeaveDx),
            (8, AttendanceStatus::AbsentK),
            (9, AttendanceStatus::AbsentK),
        ]);
        let s = slip(&input(&four, &[]), &rules);
        assert_eq!(s.deducted_leave_days, 2);
        // Sheet E55: ROUND(MAX(0,H44−2) × E54 ÷ (K8−2), 0).
        assert_eq!(s.leave_deduction, 357_143);
        assert_eq!(s.tet_level, Some(Level::M3), "4 days off caps Tết at Mức 3");
        assert_eq!(s.tet_amount, 300_000);
    }

    #[test]
    fn k_can_be_made_never_free() {
        let rules = PayrollRules {
            free_leave_codes: vec![LeaveCode::LeaveP, LeaveCode::LeaveDx],
            ..PayrollRules::default()
        };
        let one_k = days(&[(8, AttendanceStatus::AbsentK)]);
        let s = slip(&input(&one_k, &[]), &rules);
        assert_eq!((s.leave_days_counted, s.deducted_leave_days), (0, 1));
        assert_eq!(s.leave_deduction, 178_571);
        let default = slip(&input(&one_k, &[]), &PayrollRules::default());
        assert_eq!(
            default.deducted_leave_days, 0,
            "default: K uses a free day (sheet)"
        );
    }

    #[test]
    fn lateness_is_recorded_and_only_fined_when_configured() {
        let mut ds = days(&[]);
        ds[0].late_minutes = 5;
        ds[1].late_minutes = 20;
        let s = slip(&input(&ds, &[]), &PayrollRules::default());
        assert_eq!(
            (s.attendance.late_count, s.attendance.late_minutes),
            (2, 25)
        );
        assert_eq!(s.late_deduction, 0, "default: record only");
        let fined = PayrollRules {
            late_grace_minutes: 10,
            late_penalty_amount: 20_000,
            ..PayrollRules::default()
        };
        let s = slip(&input(&ds, &[]), &fined);
        assert_eq!((s.late_penalised, s.late_deduction), (1, 20_000));
    }

    #[test]
    fn commission_on_line_values_gift_sessions_optional_and_closing() {
        let lines = [
            CommissionLine {
                service_name: "Gội đầu dưỡng sinh".into(),
                source: "one_off".into(),
                qty: 4,
                value: 600_000,
                full_value: 600_000,
            },
            CommissionLine {
                service_name: "Body đá nóng".into(),
                source: "package".into(),
                qty: 3,
                value: 400_000,
                full_value: 600_000,
            },
        ];
        let ds = days(&[]);
        let upsell = [UpsellLine {
            kind: "package".into(),
            description: "Gói Đại Thông Lạc".into(),
            qty: 1,
            value: 2_000_000,
        }];
        let mut i = input(&ds, &lines);
        i.upsell_lines = &upsell;
        let s = slip(&i, &PayrollRules::default());
        assert_eq!(s.service_commission_base, 1_000_000);
        assert_eq!(s.service_commission, 50_000);
        assert_eq!(s.closing_commission, 100_000);
        assert_eq!(s.upsell_package_base, 2_000_000);
        assert_eq!(s.net_pay, 5_000_000 + 50_000 + 100_000 + 300_000);
        let gifts = PayrollRules {
            commission_on_bonus_sessions: true,
            closing_commission: false,
            ..PayrollRules::default()
        };
        let s = slip(&i, &gifts);
        assert_eq!((s.service_commission, s.closing_commission), (60_000, 0));
    }

    #[test]
    fn loyalty_and_tet_are_recorded_not_paid() {
        let ds = days(&[]);
        let mut i = input(&ds, &[]);
        i.bonuses = 200_000;
        i.penalties = 50_000;
        let s = slip(&i, &PayrollRules::default());
        assert_eq!(s.loyalty_bonus_accrued, 500_000);
        assert_eq!((s.tet_level, s.tet_amount), (Some(Level::M1), 500_000));
        assert_eq!(s.net_pay, 5_000_000 + 300_000 + 200_000 - 50_000);
    }

    #[test]
    fn mid_month_joiners_are_prorated_by_rule_and_not_counted_as_absent() {
        let ds: Vec<Day> = days(&[])
            .into_iter()
            .filter(|x| x.date >= d("2026-11-16"))
            .collect();
        let mut i = input(&ds, &[]);
        i.joined_on = Some(d("2026-11-16"));
        let s = slip(&i, &PayrollRules::default());
        assert_eq!(s.employed_days, 15);
        assert_eq!(s.attendance.unrecorded, 0);
        assert_eq!(s.loyalty_bonus_accrued, 250_000);
        let none = PayrollRules {
            loyalty_partial_month: PartialMonthLoyalty::NoneIfPartial,
            ..PayrollRules::default()
        };
        assert_eq!(slip(&i, &none).loyalty_bonus_accrued, 0);
        let full = PayrollRules {
            loyalty_partial_month: PartialMonthLoyalty::Full,
            ..PayrollRules::default()
        };
        assert_eq!(slip(&i, &full).loyalty_bonus_accrued, 500_000);
    }

    #[test]
    fn flags_missing_data_and_five_k_days() {
        let ds = days(&[
            (2, AttendanceStatus::AbsentK),
            (3, AttendanceStatus::AbsentK),
            (4, AttendanceStatus::AbsentK),
            (5, AttendanceStatus::AbsentK),
            (6, AttendanceStatus::AbsentK),
        ]);
        let mut i = input(&ds[..20], &[]);
        i.pay = None;
        i.evaluation = None;
        let s = slip(&i, &PayrollRules::default());
        assert_eq!(
            s.flags,
            vec![
                PayFlag::NoPayTerms,
                PayFlag::UnrecordedDays,
                PayFlag::AbsentK5Plus,
                PayFlag::EvaluationMissing
            ]
        );
        assert_eq!(s.attendance.unrecorded, 10);
        assert_eq!(s.net_pay, 0);
        assert_eq!(s.tet_level, None);
        i.recorded_until = Some(d("2026-11-21"));
        assert_eq!(
            slip(&i, &PayrollRules::default()).attendance.unrecorded,
            0,
            "future days"
        );
    }

    #[test]
    fn upsell_counts_packages_and_recommended_services_apart_from_performing() {
        // Q-U3: the upsell rate applies to packages AND one-off services recommended; performing
        // a service is the "làm" credit, computed separately.
        let ds = days(&[]);
        let performed = [CommissionLine {
            service_name: "Ngâm chân".into(),
            source: "one_off".into(),
            qty: 1,
            value: 100_000,
            full_value: 100_000,
        }];
        let upsell = [
            UpsellLine {
                kind: "package".into(),
                description: "Gói Đại Thông Lạc".into(),
                qty: 1,
                value: 3_500_000,
            },
            UpsellLine {
                kind: "service".into(),
                description: "Ngâm chân".into(),
                qty: 1,
                value: 100_000,
            },
        ];
        let mut i = input(&ds, &performed);
        i.upsell_lines = &upsell;
        let s = slip(&i, &PayrollRules::default());
        assert_eq!(s.service_commission, 5_000);
        assert_eq!(
            (s.upsell_package_base, s.upsell_service_base),
            (3_500_000, 100_000)
        );
        assert_eq!(s.closing_commission_base, 3_600_000);
        assert_eq!(s.closing_commission, 180_000);
        assert_eq!(s.upsell_lines.len(), 2);
        assert_eq!(s.net_pay, 5_000_000 + 5_000 + 180_000 + 300_000);
    }

    #[test]
    fn an_old_snapshot_without_upsell_fields_still_reads() {
        let ds = days(&[]);
        let s = slip(&input(&ds, &[]), &PayrollRules::default());
        let mut json = serde_json::to_value(&s).unwrap_or_default();
        if let Some(o) = json.as_object_mut() {
            o.remove("upsell_lines");
            o.remove("upsell_package_base");
            o.remove("upsell_service_base");
        }
        let back: Result<Payslip, _> = serde_json::from_value(json);
        assert!(back.is_ok_and(|b| b.upsell_lines.is_empty() && b.upsell_package_base == 0));
    }
}
