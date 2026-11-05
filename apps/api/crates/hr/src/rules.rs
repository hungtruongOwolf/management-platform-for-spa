//! The spa's payroll rules: defaults for the open contradictions in compensation-rules.md,
//! each configurable by the owner (`PUT /payroll/rules`). Stored as JSON in `app.payroll_rules`;
//! a missing row or key means the default. Every number here was a hard-coded cell in the sheet.

use crate::attendance::LeaveCode;
use crate::repo;
use chrono::NaiveTime;
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

/// How Thưởng gắn bó is accrued in a month the person did not work in full (⚠️ #7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PartialMonthLoyalty {
    /// By calendar days employed in the month (default).
    Prorate,
    /// The full monthly amount as soon as one day was worked.
    Full,
    /// Nothing for a month not worked in full.
    NoneIfPartial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, default)]
pub struct PayrollRules {
    /// Leave days per month without a pay deduction (§4: 2).
    pub free_leave_days: u32,
    /// Which absence codes use up those free days (⚠️ #2). Default: P, ĐX and K, as the sheet
    /// counts them; a code left out is always deducted.
    pub free_leave_codes: Vec<LeaveCode>,
    /// Shift start used when a staff member has none (`HH:MM`, sheet: 09:00).
    #[schema(value_type = String, example = "09:00")]
    #[serde(with = "hhmm")]
    pub default_shift_start: NaiveTime,
    /// Minutes late that do not count as late (⚠️ #4; default 0: every minute is recorded).
    pub late_grace_minutes: u32,
    /// Deduction per late arrival beyond the grace (⚠️ #4; default 0đ: lateness is only recorded).
    pub late_penalty_amount: i64,
    /// Count bonus (gift) package sessions at their per-session value for commission (⚠️ #3;
    /// default false: a gift session earns nothing, as `visit_line.line_value`).
    pub commission_on_bonus_sessions: bool,
    /// Pay the "chốt liệu trình mới" commission to the closer of a package sale (⚠️ #3; R1
    /// always collects in full, so collected = sold value).
    pub closing_commission: bool,
    /// ⚠️ #7.
    pub loyalty_partial_month: PartialMonthLoyalty,
    /// Tết bonus recorded per month for Mức 1, 2, 3 (§6: 500k / 400k / 300k).
    pub tet_level_amounts: [i64; 3],
}

impl Default for PayrollRules {
    fn default() -> Self {
        Self {
            free_leave_days: 2,
            free_leave_codes: vec![LeaveCode::LeaveP, LeaveCode::LeaveDx, LeaveCode::AbsentK],
            default_shift_start: NaiveTime::from_hms_opt(9, 0, 0).unwrap_or(NaiveTime::MIN),
            late_grace_minutes: 0,
            late_penalty_amount: 0,
            commission_on_bonus_sessions: false,
            closing_commission: true,
            loyalty_partial_month: PartialMonthLoyalty::Prorate,
            tet_level_amounts: [500_000, 400_000, 300_000],
        }
    }
}

mod hhmm {
    use chrono::NaiveTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(t: &NaiveTime, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&t.format("%H:%M").to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveTime, D::Error> {
        let raw = String::deserialize(d)?;
        NaiveTime::parse_from_str(&raw, "%H:%M").map_err(serde::de::Error::custom)
    }
}

impl PayrollRules {
    pub fn validate(&self) -> Result<(), AppError> {
        let mut errors = Vec::new();
        if self.free_leave_days > 10 {
            errors.push(FieldError::new("free_leave_days", "out_of_range"));
        }
        let mut codes = self.free_leave_codes.clone();
        codes.sort();
        codes.dedup();
        if codes.len() != self.free_leave_codes.len() {
            errors.push(FieldError::new("free_leave_codes", "duplicate"));
        }
        if self.late_grace_minutes > 240 {
            errors.push(FieldError::new("late_grace_minutes", "out_of_range"));
        }
        if !(0..=10_000_000).contains(&self.late_penalty_amount) {
            errors.push(FieldError::new("late_penalty_amount", "out_of_range"));
        }
        if self
            .tet_level_amounts
            .iter()
            .any(|a| !(0..=100_000_000).contains(a))
        {
            errors.push(FieldError::new("tet_level_amounts", "out_of_range"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(errors))
        }
    }

    pub fn is_free(&self, code: LeaveCode) -> bool {
        self.free_leave_codes.contains(&code)
    }
}

/// Rules of the spa inside the caller's transaction (no authorization: callers have it).
pub async fn load(tx: &mut TenantTx) -> Result<(PayrollRules, i64), AppError> {
    match repo::rules(tx).await? {
        None => Ok((PayrollRules::default(), 0)),
        // A stored value that no longer parses falls back to the defaults rather than failing
        // every payroll screen; the owner sees the defaults and saves again.
        Some((raw, version)) => Ok((serde_json::from_value(raw).unwrap_or_default(), version)),
    }
}

/// `GET /payroll/rules` (`payroll.read`).
pub async fn get(pool: &PgPool, actor: &Actor) -> Result<(PayrollRules, i64), AppError> {
    actor.require(Permission::PayrollRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let rules = load(&mut tx).await?;
    tx.commit().await?;
    Ok(rules)
}

/// `PUT /payroll/rules` (`payroll.close`). Applies to every month not yet closed.
pub async fn put(
    pool: &PgPool,
    actor: &Actor,
    rules: &PayrollRules,
) -> Result<(PayrollRules, i64), AppError> {
    actor.require(Permission::PayrollClose)?;
    rules.validate()?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let (before, _) = load(&mut tx).await?;
    let value = serde_json::to_value(rules).map_err(|e| AppError::Internal(e.to_string()))?;
    let version = repo::put_rules(&mut tx, &value, actor.user_id.as_uuid()).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "payroll.rules_update",
            entity_type: "tenant",
            entity_id: actor.tenant_id.as_uuid(),
            before: serde_json::to_value(&before).ok(),
            after: Some(value),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((rules.clone(), version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_take_the_defaults_and_unknown_keys_are_refused() {
        let partial: PayrollRules =
            serde_json::from_value(serde_json::json!({"late_grace_minutes": 5})).unwrap_or_default();
        assert_eq!(partial.late_grace_minutes, 5);
        assert_eq!(partial.free_leave_days, 2);
        assert_eq!(partial.default_shift_start.format("%H:%M").to_string(), "09:00");
        assert!(serde_json::from_value::<PayrollRules>(serde_json::json!({"x": 1})).is_err());
        let round = serde_json::to_value(PayrollRules::default()).unwrap_or_default();
        assert_eq!(round["default_shift_start"], "09:00");
        assert_eq!(round["free_leave_codes"], serde_json::json!(["leave_p", "leave_dx", "absent_k"]));
    }

    #[test]
    fn validation_bounds_every_number() {
        assert!(PayrollRules::default().validate().is_ok());
        let dup = PayrollRules {
            free_leave_codes: vec![LeaveCode::LeaveP, LeaveCode::LeaveP],
            ..PayrollRules::default()
        };
        assert!(dup.validate().is_err());
        let neg = PayrollRules {
            late_penalty_amount: -1,
            ..PayrollRules::default()
        };
        assert!(neg.validate().is_err());
    }
}
