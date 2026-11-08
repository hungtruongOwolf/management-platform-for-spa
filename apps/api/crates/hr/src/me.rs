//! Self-service (spec staff-management US6): a staff member whose profile is linked to their
//! login sees **their own** month — attendance, daily reviews, evaluation, bonuses/penalties and
//! payslip — and nobody else's. The staff id comes from the session's user, never from the
//! request (non-negotiable 3). A former staff member (ADR-0014) still reads their own months,
//! but only those they worked in.

use crate::adjustment::{self, Adjustment};
use crate::attendance::{self, StaffSheet};
use crate::calc::Payslip;
use crate::evaluation::{self, Evaluation};
use crate::month::Month;
use crate::payroll;
use crate::period;
use crate::review::{self, DailyReview};
use crate::rules;
use chrono::NaiveDate;
use kernel::{Actor, AppError, Permission, TenantTx};
use org::profile::Profile;
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyMonth {
    pub month: String,
    pub closed: bool,
    pub attendance: StaffSheet,
    pub reviews: Vec<DailyReview>,
    pub evaluation: Option<Evaluation>,
    pub adjustments: Vec<Adjustment>,
    /// `None` before the person's pay terms are set.
    pub payslip: Option<Payslip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyStaff {
    /// `false`: this login is not linked to a staff profile (ask the owner).
    pub linked: bool,
    /// Present only for a former staff member (ADR-0014): the last day they worked. Months they
    /// did not work in are `403 staff.not_employed_in_month`; the default month is this day's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_on: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<MyMonth>,
}

/// Who reads their own pay: a working member (`staff.read_own`), any month; a former staff
/// member (`staff.read_own_past`, derived at request time), only the months they worked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnScope {
    Working,
    Former,
}

pub(crate) fn own_scope(actor: &Actor) -> Result<OwnScope, AppError> {
    if actor.can(Permission::StaffReadOwn) {
        Ok(OwnScope::Working)
    } else if actor.can(Permission::StaffReadOwnPast) {
        Ok(OwnScope::Former)
    } else {
        Err(AppError::forbidden())
    }
}

/// The last day a former staff member worked.
pub(crate) fn last_day(profile: &Profile) -> Result<NaiveDate, AppError> {
    profile
        .employment
        .latest()
        .and_then(|p| p.ended_on)
        .ok_or_else(|| AppError::Internal("former staff without an end date".into()))
}

/// A former staff member reads only a month they worked in; `None` = the month they left.
pub(crate) fn former_month(profile: &Profile, asked: Option<Month>) -> Result<Month, AppError> {
    let month = match asked {
        Some(m) => m,
        None => Month::of(last_day(profile)?),
    };
    if period::employed_in(profile, month) {
        Ok(month)
    } else {
        Err(AppError::Forbidden {
            code: "staff.not_employed_in_month",
        })
    }
}

/// The caller's own profile (the session's staff link).
pub(crate) async fn own_profile(tx: &mut TenantTx, staff_id: Uuid) -> Result<Profile, AppError> {
    org::profile::profiles_in(tx)
        .await?
        .into_iter()
        .find(|p| p.id == staff_id)
        .ok_or_else(|| AppError::Internal("linked staff vanished".into()))
}

/// `GET /me/staff?month=` (`staff.read_own`, or `staff.read_own_past` for the months worked).
pub async fn my_month(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<MyStaff, AppError> {
    let scope = own_scope(actor)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let Some(staff_id) = org::profile::own_staff_id(&mut tx, actor).await? else {
        tx.commit().await?;
        return Ok(MyStaff {
            linked: false,
            left_on: None,
            data: None,
        });
    };
    let profile = own_profile(&mut tx, staff_id).await?;
    let today = period::today(&mut tx).await?;
    let (month, left_on) = match scope {
        OwnScope::Working => (month.unwrap_or_else(|| Month::of(today)), None),
        OwnScope::Former => (former_month(&profile, month)?, Some(last_day(&profile)?)),
    };
    let (rules, _) = rules::load(&mut tx).await?;
    let info = period::info(&mut tx, month).await?;
    let days = attendance::days_in(&mut tx, month, Some(staff_id)).await?;
    let reviews = review::reviews_in(&mut tx, month, Some(staff_id)).await?;
    let evaluation = evaluation::evaluations_in(&mut tx, month, Some(staff_id))
        .await?
        .pop();
    let adjustments = adjustment::adjustments_in(&mut tx, month, Some(staff_id)).await?;
    let has_pay = org::profile::pay_terms(&mut tx)
        .await?
        .iter()
        .any(|(id, _)| *id == staff_id);
    let payslip = if has_pay && period::employed_in(&profile, month) {
        payroll::payslips_in(&mut tx, month, Some(staff_id), &info, today)
            .await?
            .pop()
    } else {
        None
    };
    tx.commit().await?;
    Ok(MyStaff {
        linked: true,
        left_on,
        data: Some(MyMonth {
            month: month.to_string(),
            closed: info.closed,
            attendance: attendance::sheet_of(
                &profile,
                month,
                today,
                rules.default_shift_start,
                days,
            ),
            reviews,
            evaluation,
            adjustments,
            payslip,
        }),
    })
}

/// The signed-in staff member's own evaluation history: what `GET /evaluations/history` shows a
/// manager, without the writer-only `can_record` and `staff_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyHistoryData {
    pub full_name: String,
    pub position: String,
    pub from: String,
    pub to: String,
    pub criteria: Vec<evaluation::Criterion>,
    /// Newest month first; only the months the person was employed in.
    pub months: Vec<evaluation::HistoryMonth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MyEvaluationHistory {
    /// `false`: this login is not linked to a staff profile (as `GET /me/staff`).
    pub linked: bool,
    /// Present only for a former staff member (ADR-0014): the last day they worked; the window
    /// then ends at that month by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_on: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<MyHistoryData>,
}

/// `GET /me/evaluations/history?from=&to=` (`staff.read_own`, or `staff.read_own_past` for a
/// former staff member; EM 2026-11-08). The person is the session's linked profile, never a
/// parameter. Same range rules as the managers' history (default 12 months, at most 24). A former
/// staff member is never refused for a range: `to` defaults to — and is clamped at — the month
/// they left, and months they were not employed in are simply left out (as for anyone, ADR-0013).
pub async fn my_evaluation_history(
    pool: &PgPool,
    actor: &Actor,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<MyEvaluationHistory, AppError> {
    let scope = own_scope(actor)?;
    let from = from.map(|m| Month::parse("from", m)).transpose()?;
    let to = to.map(|m| Month::parse("to", m)).transpose()?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let Some(staff_id) = org::profile::own_staff_id(&mut tx, actor).await? else {
        tx.commit().await?;
        return Ok(MyEvaluationHistory {
            linked: false,
            left_on: None,
            data: None,
        });
    };
    let profile = own_profile(&mut tx, staff_id).await?;
    let today = period::today(&mut tx).await?;
    let current = Month::of(today);
    let (last, left_on) = match scope {
        OwnScope::Working => (current, None),
        OwnScope::Former => {
            let left = last_day(&profile)?;
            (Month::of(left).min(current), Some(left))
        }
    };
    let (from, to) = evaluation::history_range(from, to, last)?;
    let months = evaluation::history_months(&mut tx, &profile, from, to, today).await?;
    tx.commit().await?;
    Ok(MyEvaluationHistory {
        linked: true,
        left_on,
        data: Some(MyHistoryData {
            full_name: profile.full_name.clone(),
            position: profile.position.clone(),
            from: from.to_string(),
            to: to.to_string(),
            criteria: evaluation::CRITERIA.to_vec(),
            months,
        }),
    })
}
