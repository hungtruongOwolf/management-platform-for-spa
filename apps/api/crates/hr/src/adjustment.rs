//! Bonuses and penalties (spec staff-management US4): reason, amount and/or points, the day it
//! happened and who recorded it. Append-only: a wrong entry is offset by one of the opposite kind.
//! The month of `occurred_on` is the payroll month, which must be open; correcting a closed month
//! means recording the entry in an open month with `corrects_month` set.

use crate::month::Month;
use crate::period;
use crate::repo::{self, AdjustmentInsert, AdjustmentRow};
use chrono::{DateTime, NaiveDate, Utc};
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

pub const MAX_AMOUNT: i64 = 100_000_000;
pub const MAX_POINTS: u32 = 100;
pub const MAX_REASON_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentKind {
    Bonus,
    Penalty,
}

impl AdjustmentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bonus => "bonus",
            Self::Penalty => "penalty",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Bonus, Self::Penalty]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Adjustment {
    pub id: Uuid,
    pub staff_id: Uuid,
    pub occurred_on: NaiveDate,
    pub kind: AdjustmentKind,
    /// Đồng; `None` = points only.
    pub amount: Option<i64>,
    pub points: Option<u32>,
    pub reason: String,
    /// `YYYY-MM` of the closed month this entry corrects.
    pub corrects_month: Option<String>,
    pub by_name: String,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<AdjustmentRow> for Adjustment {
    type Error = AppError;
    fn try_from(r: AdjustmentRow) -> Result<Self, AppError> {
        Ok(Self {
            kind: AdjustmentKind::parse(&r.kind)
                .ok_or_else(|| AppError::Internal(format!("adjustment {} kind {}", r.id, r.kind)))?,
            id: r.id,
            staff_id: r.staff_id,
            occurred_on: r.work_date,
            amount: r.amount,
            points: r.points.and_then(|p| u32::try_from(p).ok()),
            reason: r.reason,
            corrects_month: r.corrects_period.map(|d| Month::of(d).to_string()),
            by_name: r.by_name,
            created_at: r.created_at,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AdjustmentInput {
    pub staff_id: Uuid,
    pub kind: AdjustmentKind,
    pub amount: Option<i64>,
    pub points: Option<u32>,
    pub reason: String,
    /// Default today.
    pub occurred_on: Option<NaiveDate>,
    pub corrects_month: Option<Month>,
}

pub async fn adjustments_in(
    tx: &mut TenantTx,
    month: Month,
    staff: Option<Uuid>,
) -> Result<Vec<Adjustment>, AppError> {
    repo::adjustments_between(tx, month.first_day(), month.last_day(), staff)
        .await?
        .into_iter()
        .map(Adjustment::try_from)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct AdjustmentLog {
    pub month: String,
    pub closed: bool,
    pub items: Vec<Adjustment>,
}

/// `GET /adjustments?month=` (`evaluation.record`), newest first.
pub async fn log(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<AdjustmentLog, AppError> {
    actor.require(Permission::EvaluationRecord)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let info = period::info(&mut tx, month).await?;
    let items = adjustments_in(&mut tx, month, None).await?;
    tx.commit().await?;
    Ok(AdjustmentLog {
        month: month.to_string(),
        closed: info.closed,
        items,
    })
}

struct Clean {
    occurred_on: NaiveDate,
    reason: String,
}

fn validate(input: &AdjustmentInput, today: NaiveDate) -> Result<Clean, AppError> {
    let mut errors = Vec::new();
    if input.amount.is_none() && input.points.is_none() {
        errors.push(FieldError::new("amount", "amount_or_points_required"));
    }
    if input.amount.is_some_and(|a| !(1..=MAX_AMOUNT).contains(&a)) {
        errors.push(FieldError::new("amount", "out_of_range"));
    }
    if input.points.is_some_and(|p| !(1..=MAX_POINTS).contains(&p)) {
        errors.push(FieldError::new("points", "out_of_range"));
    }
    let reason = input.reason.trim().to_owned();
    if reason.is_empty() || reason.chars().count() > MAX_REASON_CHARS {
        errors.push(FieldError::new("reason", "invalid"));
    }
    let occurred_on = input.occurred_on.unwrap_or(today);
    if occurred_on > today {
        errors.push(FieldError::new("occurred_on", "in_future"));
    }
    if input
        .corrects_month
        .is_some_and(|m| m >= Month::of(occurred_on))
    {
        errors.push(FieldError::new("corrects_month", "not_earlier"));
    }
    if errors.is_empty() {
        Ok(Clean {
            occurred_on,
            reason,
        })
    } else {
        Err(AppError::Validation(errors))
    }
}

/// `POST /adjustments` (`evaluation.record`, Idempotency-Key). The recorder is the session's user.
pub async fn record(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &AdjustmentInput,
    to_body: impl FnOnce(Adjustment) -> Result<Value, AppError>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::EvaluationRecord)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let today = period::today(&mut tx).await?;
    let c = validate(input, today)?;
    let profiles = org::profile::profiles_in(&mut tx).await?;
    if !profiles.iter().any(|p| p.id == input.staff_id) {
        return Err(AppError::Validation(vec![FieldError::new(
            "staff_id", "unknown",
        )]));
    }
    if let Some(corrected) = input.corrects_month
        && !period::info(&mut tx, corrected).await?.closed
    {
        return Err(AppError::Validation(vec![FieldError::new(
            "corrects_month",
            "not_closed",
        )]));
    }
    period::require_open(&mut tx, Month::of(c.occurred_on)).await?;
    let id = Uuid::now_v7();
    repo::insert_adjustment(
        &mut tx,
        &AdjustmentInsert {
            id,
            staff_id: input.staff_id,
            work_date: c.occurred_on,
            kind: input.kind.as_str(),
            amount: input.amount,
            points: input.points.and_then(|p| i32::try_from(p).ok()),
            reason: &c.reason,
            corrects_period: input.corrects_month.map(Month::first_day),
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.adjustment",
            entity_type: "staff",
            entity_id: input.staff_id,
            before: None,
            after: Some(json!({
                "adjustment_id": id, "kind": input.kind.as_str(), "amount": input.amount,
                "points": input.points, "occurred_on": c.occurred_on,
                "corrects_month": input.corrects_month.map(|m| m.to_string()),
            })),
            reason: Some(&c.reason),
        },
    )
    .await?;
    let row = repo::adjustment(&mut tx, id)
        .await?
        .ok_or_else(|| AppError::Internal("adjustment vanished".into()))?;
    let body = to_body(Adjustment::try_from(row)?)?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    fn input() -> AdjustmentInput {
        AdjustmentInput {
            staff_id: Uuid::nil(),
            kind: AdjustmentKind::Bonus,
            amount: Some(200_000),
            points: None,
            reason: " Khách khen đích danh ".into(),
            occurred_on: None,
            corrects_month: None,
        }
    }

    #[test]
    fn amount_or_points_and_a_reason() -> Result<(), AppError> {
        let today = d("2026-11-05");
        let c = validate(&input(), today)?;
        assert_eq!((c.occurred_on, c.reason.as_str()), (today, "Khách khen đích danh"));
        let none = AdjustmentInput { amount: None, ..input() };
        assert!(validate(&none, today).is_err());
        let points = AdjustmentInput { amount: None, points: Some(5), ..input() };
        assert!(validate(&points, today).is_ok());
        let blank = AdjustmentInput { reason: "  ".into(), ..input() };
        assert!(validate(&blank, today).is_err());
        let zero = AdjustmentInput { amount: Some(0), ..input() };
        assert!(validate(&zero, today).is_err());
        let future = AdjustmentInput { occurred_on: Some(d("2026-11-06")), ..input() };
        assert!(validate(&future, today).is_err());
        let same_month = AdjustmentInput {
            corrects_month: Some(Month::of(today)),
            ..input()
        };
        assert!(validate(&same_month, today).is_err(), "corrects an earlier month only");
        Ok(())
    }
}
