//! "Ngày của tôi" (EM 2026-11-07): one business day of a staff member's own month, in detail —
//! attendance, reviews, bonuses/penalties, the services they performed and what they sold or
//! recommended, each with the commission it earns.
//!
//! Scope is the session's own staff link (`staff.user_id = session user`), never a staff id from
//! the request (non-negotiable 3). The fields are those `GET /me/staff` already shows for the
//! month, plus customer names and line values of the person's own work: no phone, no visit or
//! customer notes, no customer ids, nothing of another staff member.

use crate::adjustment::{self, Adjustment};
use crate::attendance::{self, AttendanceDay};
use crate::calc::mul_div_round;
use crate::month::Month;
use crate::payroll;
use crate::period;
use crate::repo::{self, DayLineRow, DayUpsellRow};
use crate::review::{self, DailyReview};
use crate::rules;
use chrono::{DateTime, NaiveDate, Utc};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

/// One service line the person performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyDayLine {
    pub service_name: String,
    /// `one_off` (dịch vụ lẻ) or `package` (buổi liệu trình).
    pub source: String,
    pub qty: i64,
    /// What the line counts towards the commission base (the spa's rule decides whether gift
    /// sessions count at full price).
    pub value: i64,
    /// `value × commission rate`, rounded; `null` before the person's pay terms are set. The
    /// payslip rounds the month's sum once, so the day's figures may differ from it by a đồng.
    pub commission: Option<i64>,
}

/// A completed visit in which the person performed at least one line; `lines` are only theirs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyDayVisit {
    pub visit_id: Uuid,
    pub check_in_at: DateTime<Utc>,
    pub customer_name: String,
    pub lines: Vec<MyDayLine>,
}

/// An upsell credit of the day ("giới thiệu / chốt").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyDayUpsell {
    /// `package` (closed), `package_refund` (a package they closed, refunded today: negative) or
    /// `service` (a one-off service they recommended).
    pub kind: String,
    pub description: String,
    pub customer_name: String,
    pub qty: i64,
    pub value: i64,
    pub commission: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyDayData {
    pub date: NaiveDate,
    pub today: NaiveDate,
    /// The month's payroll is closed (figures are final).
    pub closed: bool,
    /// `null`: not recorded (yet).
    pub attendance: Option<AttendanceDay>,
    /// `null`: no compliments or complaints recorded.
    pub review: Option<DailyReview>,
    /// Bonuses and penalties dated that day.
    pub adjustments: Vec<Adjustment>,
    /// Earliest first.
    pub visits: Vec<MyDayVisit>,
    pub upsell: Vec<MyDayUpsell>,
    /// The rate the payslip applies (basis points); `null` before pay terms are set.
    pub commission_rate_bp: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyDay {
    /// `false`: this login is not linked to a staff profile (ask the owner).
    pub linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<MyDayData>,
}

/// Parses `YYYY-MM-DD` inside the months the API accepts; bad input is `422 invalid_date`.
pub fn parse_date(raw: &str) -> Result<NaiveDate, AppError> {
    let invalid = || AppError::Validation(vec![FieldError::new("date", "invalid_date")]);
    let raw = raw.trim();
    let date = NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| invalid())?;
    let month = raw.get(..7).ok_or_else(invalid)?;
    Month::parse("date", month).map_err(|_| invalid())?;
    Ok(date)
}

/// `GET /me/staff/day?date=` (`staff.read_own`). Default today.
pub async fn my_day(
    pool: &PgPool,
    actor: &Actor,
    date: Option<NaiveDate>,
) -> Result<MyDay, AppError> {
    actor.require(Permission::StaffReadOwn)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let Some(staff_id) = org::profile::own_staff_id(&mut tx, actor).await? else {
        tx.commit().await?;
        return Ok(MyDay {
            linked: false,
            data: None,
        });
    };
    let today = period::today(&mut tx).await?;
    let date = date.unwrap_or(today);
    let month = Month::of(date);
    let info = period::info(&mut tx, month).await?;
    let (rules, _) = rules::load(&mut tx).await?;
    // A closed month keeps the rate frozen in its payslip; an open one uses today's pay terms.
    let rate = if info.closed {
        payroll::payslips_in(&mut tx, month, Some(staff_id), &info, today)
            .await?
            .pop()
            .map(|p| p.commission_rate_bp)
    } else {
        org::profile::pay_terms(&mut tx)
            .await?
            .into_iter()
            .find(|(id, _)| *id == staff_id)
            .map(|(_, t)| t.commission_rate_bp)
    };
    let attendance = attendance::days_in(&mut tx, month, Some(staff_id))
        .await?
        .into_iter()
        .find(|d| d.date == date);
    let review = review::reviews_in(&mut tx, month, Some(staff_id))
        .await?
        .into_iter()
        .find(|r| r.date == date);
    let adjustments = adjustment::adjustments_in(&mut tx, month, Some(staff_id))
        .await?
        .into_iter()
        .filter(|a| a.occurred_on == date)
        .collect();
    let lines = repo::day_lines(&mut tx, staff_id, date).await?;
    let upsell = repo::day_upsell(&mut tx, staff_id, date).await?;
    tx.commit().await?;
    let commission = |value: i64| rate.map(|r| mul_div_round(value, i64::from(r), 10_000));
    Ok(MyDay {
        linked: true,
        data: Some(MyDayData {
            date,
            today,
            closed: info.closed,
            attendance,
            review,
            adjustments,
            visits: group_visits(lines, rules.commission_on_bonus_sessions, &commission),
            upsell: upsell
                .into_iter()
                .map(|u: DayUpsellRow| MyDayUpsell {
                    commission: commission(u.value),
                    kind: u.kind,
                    description: u.description,
                    customer_name: u.customer_name,
                    qty: u.qty,
                    value: u.value,
                })
                .collect(),
            commission_rate_bp: rate,
        }),
    })
}

/// Rows arrive ordered by visit, then line position: fold consecutive rows of one visit into one
/// entry. `full_price` = gift sessions count at their full value (the spa's rule).
fn group_visits(
    rows: Vec<DayLineRow>,
    full_price: bool,
    commission: &dyn Fn(i64) -> Option<i64>,
) -> Vec<MyDayVisit> {
    let mut out: Vec<MyDayVisit> = Vec::new();
    for r in rows {
        let value = if full_price { r.full_value } else { r.value };
        let line = MyDayLine {
            service_name: r.service_name,
            source: r.source,
            qty: r.qty,
            value,
            commission: commission(value),
        };
        match out.last_mut() {
            Some(v) if v.visit_id == r.visit_id => v.lines.push(line),
            _ => out.push(MyDayVisit {
                visit_id: r.visit_id,
                check_in_at: r.check_in_at,
                customer_name: r.customer_name,
                lines: vec![line],
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(visit: u128, name: &str, value: i64, full_value: i64) -> DayLineRow {
        DayLineRow {
            visit_id: Uuid::from_u128(visit),
            check_in_at: DateTime::<Utc>::UNIX_EPOCH,
            customer_name: "Nguyễn Thị Hoa".into(),
            service_name: name.into(),
            source: "package".into(),
            qty: 1,
            value,
            full_value,
        }
    }

    #[test]
    fn lines_of_one_visit_are_grouped_with_their_commission() {
        let ten_percent = |v: i64| Some(mul_div_round(v, 1_000, 10_000));
        let visits = group_visits(
            vec![
                row(1, "Gội đầu", 0, 80_000),
                row(1, "Massage", 125_005, 125_005),
                row(2, "Xông hơi", 50_000, 50_000),
            ],
            false,
            &ten_percent,
        );
        assert_eq!(visits.len(), 2);
        let first = visits.first().map(|v| {
            v.lines
                .iter()
                .map(|l| (l.service_name.as_str(), l.value, l.commission))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            first,
            Some(vec![
                ("Gội đầu", 0, Some(0)),
                ("Massage", 125_005, Some(12_501))
            ])
        );
    }

    #[test]
    fn gift_sessions_count_at_full_price_when_the_rule_says_so() {
        let visits = group_visits(vec![row(1, "Gội đầu", 0, 80_000)], true, &|_| None);
        assert_eq!(
            visits
                .first()
                .and_then(|v| v.lines.first())
                .map(|l| (l.value, l.commission)),
            Some((80_000, None))
        );
    }

    #[test]
    fn dates_are_validated() {
        assert!(parse_date("2026-11-07").is_ok());
        for bad in ["2026-11", "2026-02-30", "1999-01-01", "hôm nay", ""] {
            assert!(parse_date(bad).is_err(), "{bad}");
        }
    }
}
