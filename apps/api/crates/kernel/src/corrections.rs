//! Rules shared by every correction of recorded money or sessions (spec R1-1 US6): who may void a
//! record of a given business day, and the mandatory reason.

use crate::error::{AppError, FieldError};
use crate::permission::{Actor, Permission};
use chrono::NaiveDate;

pub const MAX_REASON: usize = 500;

/// May `actor` void a record of `record_day` when the spa's business day is `today` (AC6.3)?
///
/// Until spec #2 defines closing the cash book, every day before today counts as closed.
/// - no `record.void_same_day` → `403 auth.forbidden`;
/// - closed day without `record.void_closed_day` → `403 day.locked`;
/// - closed day with it (🔒) → `401 auth.reauth_required` unless the password was re-entered
///   within the re-auth window.
pub fn authorize_void(
    actor: &Actor,
    record_day: NaiveDate,
    today: NaiveDate,
) -> Result<(), AppError> {
    actor.require(Permission::RecordVoidSameDay)?;
    authorize_closed_day(actor, record_day, today)
}

/// The 🔒 closed-day rule alone, for every change of a recorded day (voids and corrections such as
/// `PATCH /visits/{id}`); the caller checks its own base permission first.
/// - `record_day` is today → allowed;
/// - closed day without `record.void_closed_day` → `403 day.locked`;
/// - closed day with it → `401 auth.reauth_required` unless re-authenticated within the window.
pub fn authorize_closed_day(
    actor: &Actor,
    record_day: NaiveDate,
    today: NaiveDate,
) -> Result<(), AppError> {
    if record_day >= today {
        return Ok(());
    }
    if !actor.can(Permission::RecordVoidClosedDay) {
        return Err(AppError::Forbidden { code: "day.locked" });
    }
    actor.require_recent_auth()
}

/// The reason every void, extension and cancellation must carry: trimmed, 1–500 characters.
/// Missing or blank → `422 validation_failed` on `reason` (`required`).
pub fn reason(input: Option<&str>) -> Result<String, AppError> {
    let reason = input.map(str::trim).unwrap_or_default();
    if reason.is_empty() {
        return Err(AppError::Validation(vec![FieldError::new(
            "reason", "required",
        )]));
    }
    if reason.chars().count() > MAX_REASON {
        return Err(AppError::Validation(vec![FieldError::new(
            "reason", "too_long",
        )]));
    }
    Ok(reason.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{TenantId, UserId};

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap_or_default()
    }

    fn actor(perms: &[Permission], recent: bool) -> Actor {
        Actor::new(UserId::new(), TenantId::new(), perms.iter().copied(), None)
            .with_recent_auth(recent)
    }

    const MANAGER: &[Permission] = &[Permission::RecordVoidSameDay];
    const OWNER: &[Permission] = &[
        Permission::RecordVoidSameDay,
        Permission::RecordVoidClosedDay,
    ];

    fn code(r: Result<(), AppError>) -> Option<&'static str> {
        r.err().map(|e| e.code())
    }

    #[test]
    fn same_day_needs_only_the_same_day_permission() {
        assert_eq!(
            code(authorize_void(&actor(MANAGER, false), day(4), day(4))),
            None
        );
        assert_eq!(
            code(authorize_void(&actor(&[], true), day(4), day(4))),
            Some("auth.forbidden")
        );
    }

    #[test]
    fn closed_day_is_locked_for_managers_and_needs_reauth_for_owners() {
        assert_eq!(
            code(authorize_void(&actor(MANAGER, true), day(3), day(4))),
            Some("day.locked")
        );
        assert_eq!(
            code(authorize_void(&actor(OWNER, false), day(3), day(4))),
            Some("auth.reauth_required")
        );
        assert_eq!(
            code(authorize_void(&actor(OWNER, true), day(3), day(4))),
            None
        );
        assert_eq!(
            code(authorize_void(&actor(OWNER, false), day(4), day(4))),
            None,
            "today needs no re-auth"
        );
    }

    #[test]
    fn reason_is_trimmed_and_required() {
        assert_eq!(
            reason(Some("  ghi nhầm  ")).ok().as_deref(),
            Some("ghi nhầm")
        );
        for blank in [None, Some(""), Some("   ")] {
            assert!(matches!(reason(blank), Err(AppError::Validation(_))));
        }
        assert!(reason(Some(&"x".repeat(501))).is_err());
        assert!(reason(Some(&"ễ".repeat(500))).is_ok());
    }
}
