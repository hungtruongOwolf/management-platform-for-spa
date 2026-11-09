//! Daily review (spec staff-management US3a): the sheet's daily Khen / Phàn nàn / Ghi chú
//! columns — the daily part of the evaluation, kept by `evaluation.record` holders. The monthly
//! "Phản hồi khách hàng" criterion is scored with these counts in view.

use crate::month::Month;
use crate::period;
use crate::repo::{self, Cell, ReviewRow};
use crate::scope::RecordScope;
use chrono::NaiveDate;
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use org::profile::Profile;
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use std::collections::{BTreeSet, HashSet};
use utoipa::ToSchema;
use uuid::Uuid;

pub const MAX_ENTRIES: usize = 100;
pub const MAX_COUNT: u32 = 99;
pub const MAX_NOTE_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct DailyReview {
    pub staff_id: Uuid,
    pub date: NaiveDate,
    pub compliments: u32,
    pub complaints: u32,
    pub note: Option<String>,
}

impl From<ReviewRow> for DailyReview {
    fn from(r: ReviewRow) -> Self {
        Self {
            staff_id: r.staff_id,
            date: r.work_date,
            compliments: u32::try_from(r.compliments).unwrap_or(0),
            complaints: u32::try_from(r.complaints).unwrap_or(0),
            note: r.note,
        }
    }
}

/// One day of one person; all zero and no note clears the day.
#[derive(Debug, Clone)]
pub struct ReviewEntry {
    pub staff_id: Uuid,
    pub date: NaiveDate,
    pub compliments: u32,
    pub complaints: u32,
    pub note: Option<String>,
}

impl ReviewEntry {
    const fn is_empty(&self) -> bool {
        self.compliments == 0 && self.complaints == 0 && self.note.is_none()
    }
}

fn validate(entries: &[ReviewEntry], today: NaiveDate, staff: &[Profile]) -> Result<(), AppError> {
    let mut errors = Vec::new();
    if entries.is_empty() || entries.len() > MAX_ENTRIES {
        errors.push(FieldError::new("entries", "count"));
    }
    let mut seen = HashSet::new();
    for (i, e) in entries.iter().enumerate() {
        let f = |name: &str| format!("entries[{i}].{name}");
        if !seen.insert((e.staff_id, e.date)) {
            errors.push(FieldError::new(f("date"), "duplicate"));
        }
        if e.date > today {
            errors.push(FieldError::new(f("date"), "in_future"));
        }
        // Same errors as attendance (AC3a.1 → AC2.3), `not_employed` included.
        match staff.iter().find(|p| p.id == e.staff_id) {
            None => errors.push(FieldError::new(f("staff_id"), "unknown")),
            Some(p) if !p.employment.on(e.date) => {
                errors.push(FieldError::new(f("date"), "not_employed"));
            }
            Some(_) => {}
        }
        if e.compliments > MAX_COUNT {
            errors.push(FieldError::new(f("compliments"), "out_of_range"));
        }
        if e.complaints > MAX_COUNT {
            errors.push(FieldError::new(f("complaints"), "out_of_range"));
        }
        if e.note
            .as_ref()
            .is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS)
        {
            errors.push(FieldError::new(f("note"), "too_long"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

pub async fn reviews_in(
    tx: &mut TenantTx,
    month: Month,
    staff: Option<Uuid>,
) -> Result<Vec<DailyReview>, AppError> {
    Ok(
        repo::reviews_between(tx, month.first_day(), month.last_day(), staff)
            .await?
            .into_iter()
            .map(DailyReview::from)
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ReviewSheet {
    pub month: String,
    pub today: NaiveDate,
    pub closed: bool,
    pub staff: Vec<StaffReviews>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StaffReviews {
    pub staff_id: Uuid,
    pub full_name: String,
    pub position: String,
    pub compliments: u32,
    pub complaints: u32,
    pub days: Vec<DailyReview>,
    /// Decision C: whether the caller may record this person's day.
    pub can_record: bool,
}

/// `GET /daily-reviews?month=` (`evaluation.record`); not the caller's own row unless they are
/// the owner ([`RecordScope::lists`]).
pub async fn sheet(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<ReviewSheet, AppError> {
    actor.require(Permission::EvaluationRecord)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let info = period::info(&mut tx, month).await?;
    let profiles = org::profile::profiles_in(&mut tx).await?;
    let all = reviews_in(&mut tx, month, None).await?;
    let scope = RecordScope::load(&mut tx, actor).await?;
    tx.commit().await?;
    let staff = profiles
        .iter()
        .filter(|p| period::employed_in(p, month) && scope.lists(p.id))
        .map(|p| {
            let days: Vec<DailyReview> =
                all.iter().filter(|r| r.staff_id == p.id).cloned().collect();
            StaffReviews {
                staff_id: p.id,
                full_name: p.full_name.clone(),
                position: p.position.clone(),
                compliments: days.iter().map(|d| d.compliments).sum(),
                complaints: days.iter().map(|d| d.complaints).sum(),
                days,
                can_record: scope.may_record(p.id),
            }
        })
        .collect();
    Ok(ReviewSheet {
        month: month.to_string(),
        today,
        closed: info.closed,
        staff,
    })
}

/// `PUT /daily-reviews` (`evaluation.record`; decision C scope, `403 staff.record_owner_only`).
pub async fn record(
    pool: &PgPool,
    actor: &Actor,
    entries: &[ReviewEntry],
) -> Result<Vec<DailyReview>, AppError> {
    actor.require(Permission::EvaluationRecord)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let today = period::today(&mut tx).await?;
    let profiles = org::profile::profiles_in(&mut tx).await?;
    RecordScope::load(&mut tx, actor)
        .await?
        .require(entries.iter().map(|e| e.staff_id))?;
    validate(entries, today, &profiles)?;
    let months: BTreeSet<Month> = entries.iter().map(|e| Month::of(e.date)).collect();
    for m in months {
        period::require_open(&mut tx, m).await?;
    }
    let by = actor.user_id.as_uuid();
    // A fixed number of statements per batch (was three per cell): what was there, one write,
    // one clear; then an audit row per cell that changed.
    let cells: Vec<Cell> = entries
        .iter()
        .map(|e| Cell {
            staff_id: e.staff_id,
            day: e.date,
        })
        .collect();
    let recorded: Vec<DailyReview> = repo::review_cells(&mut tx, &cells)
        .await?
        .into_iter()
        .map(DailyReview::from)
        .collect();
    let mut out = Vec::new();
    let mut writes: Vec<serde_json::Value> = Vec::new();
    let mut clears: Vec<Cell> = Vec::new();
    let mut changed: Vec<(&ReviewEntry, Option<DailyReview>)> = Vec::new();
    for e in entries {
        let before = recorded
            .iter()
            .find(|r| r.staff_id == e.staff_id && r.date == e.date)
            .cloned();
        if e.is_empty() {
            clears.push(Cell {
                staff_id: e.staff_id,
                day: e.date,
            });
        } else {
            let row = ReviewRow {
                staff_id: e.staff_id,
                work_date: e.date,
                compliments: i32::try_from(e.compliments).unwrap_or(0),
                complaints: i32::try_from(e.complaints).unwrap_or(0),
                note: e.note.clone(),
            };
            writes.push(json!({
                "staff_id": row.staff_id, "work_date": row.work_date,
                "compliments": row.compliments, "complaints": row.complaints, "note": row.note,
            }));
            out.push(DailyReview::from(row));
        }
        let differs = match &before {
            None => !e.is_empty(),
            Some(b) => {
                (b.compliments, b.complaints, &b.note) != (e.compliments, e.complaints, &e.note)
            }
        };
        if differs {
            changed.push((e, before));
        }
    }
    if !writes.is_empty() {
        repo::upsert_reviews_many(&mut tx, &serde_json::Value::Array(writes), by).await?;
    }
    if !clears.is_empty() {
        repo::delete_reviews_many(&mut tx, &clears).await?;
    }
    for (e, before) in changed {
        let summary = |c: u32, k: u32, note: bool| json!({"date": e.date, "compliments": c, "complaints": k, "has_note": note});
        audit::record(
            &mut tx,
            actor,
            AuditEntry {
                action: "evaluation.daily_review",
                entity_type: "staff",
                entity_id: e.staff_id,
                before: before
                    .as_ref()
                    .map(|b| summary(b.compliments, b.complaints, b.note.is_some())),
                after: Some(summary(e.compliments, e.complaints, e.note.is_some())),
                reason: None,
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(out)
}
