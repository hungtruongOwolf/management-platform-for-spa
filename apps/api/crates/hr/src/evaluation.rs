//! Monthly evaluation (spec staff-management US3b): the sheet's 100-point scorecard, done on
//! days 1–5 of the next month. Six criteria; the evaluator types each criterion's points, 0 to its
//! maximum (EM decision D, 2026-11-07: no forced Mức 1/2/3 choice, a new form starts empty). The
//! sheet's levels stay as the guide (Mức 1 = 100 %, Mức 2 = 80 %, Mức 3 = 60 %, Chưa đạt ≤ 30 % of
//! the maximum) and each criterion's level is read from its points with the month's thresholds
//! (≥ 85 / 70 / 50 %). The month's result follows the total, capped at Mức 3 after a serious
//! complaint or repeated rule breaking. Mức 3 / Chưa đạt need a coaching plan.

use crate::attendance;
use crate::calc::AttendanceTotals;
use crate::calc::{self, Day};
use crate::month::Month;
use crate::period;
use crate::repo::{self, EvaluationRow, EvaluationWrite};
use crate::review;
use crate::scope::RecordScope;
use chrono::{DateTime, Utc};
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

pub const MAX_TEXT_CHARS: usize = 1000;

/// A level, ordered best (M1) to worst (Fail): `max` of two levels is the lower result.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Mức 1 (Xuất sắc).
    M1,
    /// Mức 2 (Tốt).
    M2,
    /// Mức 3 (Đạt).
    M3,
    /// Chưa đạt.
    Fail,
}

impl Level {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::M1 => "m1",
            Self::M2 => "m2",
            Self::M3 => "m3",
            Self::Fail => "fail",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::M1, Self::M2, Self::M3, Self::Fail]
            .into_iter()
            .find(|l| l.as_str() == s)
    }

    /// Result of a total score (out of 100), or of a criterion's share of its maximum in percent.
    pub const fn of_total(total: u32) -> Self {
        match total {
            85.. => Self::M1,
            70..=84 => Self::M2,
            50..=69 => Self::M3,
            _ => Self::Fail,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct Criterion {
    pub key: &'static str,
    pub label: &'static str,
    pub max: u32,
}

/// The scorecard (sheet rows 61–66).
pub const CRITERIA: [Criterion; 6] = [
    Criterion {
        key: "technique",
        label: "Kỹ thuật liệu trình",
        max: 40,
    },
    Criterion {
        key: "attitude",
        label: "Thái độ và giao tiếp",
        max: 15,
    },
    Criterion {
        key: "customer_feedback",
        label: "Phản hồi khách hàng",
        max: 15,
    },
    Criterion {
        key: "diligence",
        label: "Chuyên cần",
        max: 10,
    },
    Criterion {
        key: "hygiene",
        label: "Vệ sinh và tác phong",
        max: 10,
    },
    Criterion {
        key: "learning",
        label: "Tinh thần học hỏi",
        max: 10,
    },
];

/// One scored criterion, as stored in `staff_evaluation.criteria`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScoredCriterion {
    pub key: String,
    pub max: u32,
    pub level: Level,
    pub points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriterionInput {
    pub key: String,
    /// Typed by the evaluator, 0 to the criterion's maximum; `None` = not filled in yet.
    pub points: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct EvaluationInput {
    pub staff_id: Uuid,
    pub month: Month,
    pub criteria: Vec<CriterionInput>,
    pub serious_issue: bool,
    pub coaching_plan: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub criteria: Vec<ScoredCriterion>,
    pub total: u32,
    /// By total alone.
    pub by_total: Level,
    /// After the serious-issue cap.
    pub result: Level,
}

/// A criterion's level from its points: the share of its maximum, with the month's thresholds.
pub const fn level_of(points: u32, max: u32) -> Level {
    if max == 0 {
        return Level::Fail;
    }
    Level::of_total(points * 100 / max)
}

/// Scores a full scorecard: every criterion filled in, each within its maximum.
pub fn score(criteria: &[CriterionInput], serious_issue: bool) -> Result<Score, AppError> {
    let mut errors = Vec::new();
    let mut scored = Vec::new();
    for c in &CRITERIA {
        let field = format!("criteria.{}.points", c.key);
        match criteria
            .iter()
            .find(|x| x.key == c.key)
            .and_then(|x| x.points)
        {
            None => errors.push(FieldError::new(field, "required")),
            Some(p) if p > c.max => errors.push(FieldError::new(field, "above_max")),
            Some(points) => scored.push(ScoredCriterion {
                key: c.key.to_owned(),
                max: c.max,
                level: level_of(points, c.max),
                points,
            }),
        }
    }
    if criteria.len() != CRITERIA.len() {
        errors.push(FieldError::new("criteria", "count"));
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let total = scored.iter().map(|c| c.points).sum();
    let by_total = Level::of_total(total);
    // The cap only ever lowers a result (Mức 1/2 → Mức 3); Chưa đạt stays Chưa đạt.
    let result = if serious_issue {
        by_total.max(Level::M3)
    } else {
        by_total
    };
    Ok(Score {
        criteria: scored,
        total,
        by_total,
        result,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Evaluation {
    pub staff_id: Uuid,
    pub month: String,
    pub criteria: Vec<ScoredCriterion>,
    pub serious_issue: bool,
    pub total_points: u32,
    pub result: Level,
    pub coaching_plan: Option<String>,
    pub comment: Option<String>,
    pub by_name: String,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

fn from_row(r: EvaluationRow, month: Month) -> Result<Evaluation, AppError> {
    let corrupt = || AppError::Internal(format!("evaluation of {} is corrupt", r.staff_id));
    Ok(Evaluation {
        staff_id: r.staff_id,
        month: month.to_string(),
        criteria: serde_json::from_value(r.criteria).map_err(|_| corrupt())?,
        serious_issue: r.serious_issue,
        total_points: u32::try_from(r.total_points).map_err(|_| corrupt())?,
        result: Level::parse(&r.result_level).ok_or_else(corrupt)?,
        coaching_plan: r.coaching_plan,
        comment: r.comment,
        by_name: r.by_name,
        updated_at: r.updated_at,
        version: r.version,
    })
}

pub async fn evaluations_in(
    tx: &mut TenantTx,
    month: Month,
    staff: Option<Uuid>,
) -> Result<Vec<Evaluation>, AppError> {
    repo::evaluations(tx, month.first_day(), staff)
        .await?
        .into_iter()
        .map(|r| from_row(r, month))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StaffEvaluation {
    pub staff_id: Uuid,
    pub full_name: String,
    pub position: String,
    /// The month's daily reviews, as context for "Phản hồi khách hàng".
    pub compliments: u32,
    pub complaints: u32,
    /// The month's attendance, as context for "Chuyên cần".
    pub attendance: AttendanceTotals,
    pub evaluation: Option<Evaluation>,
    /// Decision C: whether the caller may score this person.
    pub can_record: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct EvaluationSheet {
    pub month: String,
    pub closed: bool,
    pub criteria: Vec<Criterion>,
    pub staff: Vec<StaffEvaluation>,
}

/// `GET /evaluations?month=` (`evaluation.record`).
pub async fn sheet(
    pool: &PgPool,
    actor: &Actor,
    month: Option<Month>,
) -> Result<EvaluationSheet, AppError> {
    actor.require(Permission::EvaluationRecord)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = period::today(&mut tx).await?;
    let month = month.unwrap_or_else(|| Month::of(today));
    let info = period::info(&mut tx, month).await?;
    let profiles = org::profile::profiles_in(&mut tx).await?;
    let evals = evaluations_in(&mut tx, month, None).await?;
    let reviews = review::reviews_in(&mut tx, month, None).await?;
    let days = attendance::days_in(&mut tx, month, None).await?;
    let scope = RecordScope::load(&mut tx, actor).await?;
    tx.commit().await?;
    let staff = profiles
        .iter()
        .filter(|p| period::employed_in(p, month))
        .map(|p| {
            let mine: Vec<Day> = days
                .iter()
                .filter(|d| d.staff_id == p.id)
                .map(attendance::AttendanceDay::as_calc)
                .collect();
            let rs = reviews.iter().filter(|r| r.staff_id == p.id);
            StaffEvaluation {
                staff_id: p.id,
                full_name: p.full_name.clone(),
                position: p.position.clone(),
                compliments: rs.clone().map(|r| r.compliments).sum(),
                complaints: rs.map(|r| r.complaints).sum(),
                attendance: calc::summarize(
                    month,
                    attendance::recorded_until(month, today),
                    p.joined_on,
                    p.left_on,
                    &mine,
                ),
                evaluation: evals.iter().find(|e| e.staff_id == p.id).cloned(),
                can_record: scope.may_record(p.id),
            }
        })
        .collect();
    Ok(EvaluationSheet {
        month: month.to_string(),
        closed: info.closed,
        criteria: CRITERIA.to_vec(),
        staff,
    })
}

fn clean_text(field: &str, raw: Option<&str>, errors: &mut Vec<FieldError>) -> Option<String> {
    let t = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if t.as_ref()
        .is_some_and(|s| s.chars().count() > MAX_TEXT_CHARS)
    {
        errors.push(FieldError::new(field, "too_long"));
    }
    t
}

/// `PUT /evaluations` (`evaluation.record`; decision C scope, `403 staff.record_owner_only`):
/// records or rescores one person's month.
pub async fn record(
    pool: &PgPool,
    actor: &Actor,
    input: &EvaluationInput,
) -> Result<Evaluation, AppError> {
    actor.require(Permission::EvaluationRecord)?;
    let s = score(&input.criteria, input.serious_issue)?;
    let mut errors = Vec::new();
    let coaching = clean_text("coaching_plan", input.coaching_plan.as_deref(), &mut errors);
    let comment = clean_text("comment", input.comment.as_deref(), &mut errors);
    if matches!(s.result, Level::M3 | Level::Fail) && coaching.is_none() {
        errors.push(FieldError::new("coaching_plan", "required"));
    }
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = period::today(&mut tx).await?;
    if input.month > Month::of(today) {
        errors.push(FieldError::new("month", "in_future"));
    }
    RecordScope::load(&mut tx, actor)
        .await?
        .require([input.staff_id])?;
    let profiles = org::profile::profiles_in(&mut tx).await?;
    match profiles.iter().find(|p| p.id == input.staff_id) {
        None => errors.push(FieldError::new("staff_id", "unknown")),
        Some(p) if !period::employed_in(p, input.month) => {
            errors.push(FieldError::new("month", "not_employed"));
        }
        Some(_) => {}
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    period::require_open(&mut tx, input.month).await?;
    let before = evaluations_in(&mut tx, input.month, Some(input.staff_id))
        .await?
        .pop();
    let criteria =
        serde_json::to_value(&s.criteria).map_err(|e| AppError::Internal(e.to_string()))?;
    repo::upsert_evaluation(
        &mut tx,
        &EvaluationWrite {
            staff_id: input.staff_id,
            month: input.month.first_day(),
            criteria: &criteria,
            serious_issue: input.serious_issue,
            total_points: i32::try_from(s.total).unwrap_or(0),
            result_level: s.result.as_str(),
            coaching_plan: coaching.as_deref(),
            comment: comment.as_deref(),
        },
        actor.user_id.as_uuid(),
    )
    .await?;
    let after = evaluations_in(&mut tx, input.month, Some(input.staff_id))
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal("evaluation vanished".into()))?;
    let summary = |e: &Evaluation| {
        json!({"month": e.month, "total_points": e.total_points, "result": e.result.as_str(),
               "serious_issue": e.serious_issue, "criteria": e.criteria})
    };
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "evaluation.record",
            entity_type: "staff",
            entity_id: input.staff_id,
            before: before.as_ref().map(summary),
            after: Some(summary(&after)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(points: [u32; 6]) -> Vec<CriterionInput> {
        CRITERIA
            .iter()
            .zip(points)
            .map(|(c, p)| CriterionInput {
                key: c.key.into(),
                points: Some(p),
            })
            .collect()
    }

    fn errors(r: Result<Score, AppError>) -> Vec<String> {
        match r {
            Err(AppError::Validation(e)) => e
                .iter()
                .map(|f| format!("{} {}", f.field, f.code))
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn typed_points_add_up_to_the_month_level() -> Result<(), AppError> {
        use Level::*;
        let s = score(&card([40, 15, 15, 10, 10, 10]), false)?;
        assert_eq!((s.total, s.result), (100, M1));
        // The sheet's Mức 2 everywhere: 32 + 12 + 12 + 8 + 8 + 8 = 80 → Mức 2.
        let s = score(&card([32, 12, 12, 8, 8, 8]), false)?;
        assert_eq!((s.total, s.result), (80, M2));
        // Any value in between, not only the level anchors.
        let s = score(&card([35, 13, 11, 9, 7, 10]), false)?;
        assert_eq!((s.total, s.result), (85, M1));
        assert_eq!(score(&card([24, 9, 9, 6, 6, 6]), false)?.result, M3);
        let s = score(&card([0; 6]), false)?;
        assert_eq!((s.total, s.result), (0, Fail));
        Ok(())
    }

    #[test]
    fn each_criterion_reads_its_level_from_its_share() {
        // The sheet's anchors land on their own level.
        assert_eq!(level_of(40, 40), Level::M1);
        assert_eq!(level_of(32, 40), Level::M2);
        assert_eq!(level_of(24, 40), Level::M3);
        assert_eq!(level_of(12, 40), Level::Fail);
        assert_eq!(level_of(34, 40), Level::M1, "85 %");
        assert_eq!(level_of(33, 40), Level::M2, "82 %");
        assert_eq!(level_of(7, 10), Level::M2);
        assert_eq!(level_of(4, 10), Level::Fail);
    }

    #[test]
    fn serious_issue_caps_the_result_at_m3() -> Result<(), AppError> {
        let s = score(&card([40, 15, 15, 10, 10, 10]), true)?;
        assert_eq!((s.by_total, s.result), (Level::M1, Level::M3));
        let s = score(&card([10, 4, 4, 3, 3, 3]), true)?;
        assert_eq!(s.result, Level::Fail, "the cap never improves a result");
        Ok(())
    }

    #[test]
    fn every_criterion_is_required_and_within_its_maximum() {
        let mut c = card([40, 15, 15, 10, 10, 10]);
        c[0].points = Some(41);
        assert_eq!(
            errors(score(&c, false)),
            ["criteria.technique.points above_max"]
        );
        c[0].points = None;
        assert_eq!(
            errors(score(&c, false)),
            ["criteria.technique.points required"]
        );
        let five = &card([40, 15, 15, 10, 10, 10])[..5];
        assert!(!errors(score(five, false)).is_empty(), "every criterion");
    }

    #[test]
    fn totals_map_to_levels() {
        assert_eq!(Level::of_total(85), Level::M1);
        assert_eq!(Level::of_total(84), Level::M2);
        assert_eq!(Level::of_total(70), Level::M2);
        assert_eq!(Level::of_total(69), Level::M3);
        assert_eq!(Level::of_total(50), Level::M3);
        assert_eq!(Level::of_total(49), Level::Fail);
        assert_eq!(
            Level::M2.max(Level::M3),
            Level::M3,
            "max is the lower result"
        );
    }
}
