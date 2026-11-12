//! Append-only audit trail written in the same transaction as the change it describes.
//!
//! Every action a spa's trail can hold is one [`Action`]: the owner's "Nhật ký thao tác"
//! (`reporting::activity`, ADR-0021) groups and words each of them with an exhaustive `match`, so a
//! new action does not compile until it has a place on that screen.
//!
//! **When an entry says it happened (ADR-0023).** Every entry of one transaction carries one
//! instant, so the readers can tell what one action wrote (the rows of one instant fold into one
//! line, ADR-0021 D9; an edit of a bonus is the void + the new one of one instant). The instant is
//! the database clock when the transaction writes its entries, which [`record`] defers to
//! [`TenantTx::commit`] (or [`TenantTx::write_audit`]): after every row lock the use case took. A
//! request that started first but waited on a lock held by another one therefore sorts after it,
//! as its change did; the transaction's start (`now()`) or the handler's clock would sort it
//! before. Inside one instant, entries keep the order they were recorded in (their ids are UUIDv7,
//! monotonic within the process). Only a back-filling actor ([`Actor::at_clock`], `app seed-demo`)
//! sets the instant up front.

use crate::db::TenantTx;
use crate::error::AppError;
use crate::permission::Actor;
use serde_json::Value;
use uuid::Uuid;

macro_rules! actions {
    ($($(#[$meta:meta])* $variant:ident => $name:literal),+ $(,)?) => {
        /// What an audit entry records. The key is stored in `audit_log.action`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Action { $($(#[$meta])* $variant),+ }

        impl Action {
            pub const ALL: &'static [Action] = &[$(Action::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Action::$variant => $name),+ }
            }

            /// The action of a stored key; `None` for a key no code writes (legacy rows).
            pub fn parse(key: &str) -> Option<Action> {
                match key {
                    $($name => Some(Action::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

actions! {
    // Front desk.
    VisitCreate => "visit.create",
    VisitAddition => "visit.addition",
    VisitUpdate => "visit.update",
    VisitCorrect => "visit.correct",
    VisitVoid => "visit.void",
    VisitConvertToPackage => "visit.convert_to_package",
    SaleCreate => "sale.create",
    SaleVoid => "sale.void",
    SaleNoteUpdate => "sale.note_update",
    // Packages.
    CustomerPackageCreate => "customer_package.create",
    CustomerPackageExtend => "customer_package.extend",
    CustomerPackageCancel => "customer_package.cancel",
    CustomerPackageUseExpiredOverride => "customer_package.use_expired_override",
    OpeningBalanceImport => "opening_balance.import",
    SalePaperBook => "sale.paper_book",
    // Appointments.
    AppointmentCreate => "appointment.create",
    AppointmentUpdate => "appointment.update",
    AppointmentArrive => "appointment.arrive",
    AppointmentUndoArrival => "appointment.undo_arrival",
    AppointmentComplete => "appointment.complete",
    AppointmentNoShow => "appointment.no_show",
    AppointmentCancel => "appointment.cancel",
    AppointmentSettings => "appointment.settings",
    // Customers.
    CustomerCreate => "customer.create",
    CustomerUpdate => "customer.update",
    CustomerHealthNoteSet => "customer.health_note_set",
    CustomerFollowUp => "customer.follow_up",
    // Staff books.
    AttendanceRecord => "attendance.record",
    EvaluationDailyReview => "evaluation.daily_review",
    EvaluationRecord => "evaluation.record",
    StaffAdjustment => "staff.adjustment",
    StaffAdjustmentVoid => "staff.adjustment_void",
    StaffCreate => "staff.create",
    StaffUpdate => "staff.update",
    StaffEnd => "staff.end",
    StaffRehire => "staff.rehire",
    StaffPayUpdate => "staff.pay_update",
    StaffAccountLink => "staff.account_link",
    // Payroll.
    PayrollRulesUpdate => "payroll.rules_update",
    PayrollClose => "payroll.close",
    // Catalog.
    CatalogCategoryCreate => "catalog.category_create",
    CatalogCategoryUpdate => "catalog.category_update",
    CatalogServiceCreate => "catalog.service_create",
    CatalogServiceUpdate => "catalog.service_update",
    CatalogPlanCreate => "catalog.plan_create",
    CatalogPlanUpdate => "catalog.plan_update",
    // Logins and accounts.
    StaffLoginCreate => "staff.login_create",
    StaffLoginPasswordReset => "staff.login_password_reset",
    StaffLoginRole => "staff.login_role",
    AccountPasswordChange => "account.password_change",
    AccountPasswordResetRequested => "account.password_reset_requested",
    AccountPasswordResetDismissed => "account.password_reset_dismissed",
    // What the platform team did to the spa (`actor_kind = 'platform_admin'`, ADR-0018 D6).
    AccountLockedByPlatform => "account.locked_by_platform",
    AccountUnlockedByPlatform => "account.unlocked_by_platform",
    AccountMfaResetByPlatform => "account.mfa_reset_by_platform",
    AccountSessionsRevokedByPlatform => "account.sessions_revoked_by_platform",
    AccountPasswordResetByPlatform => "account.password_reset_by_platform",
    /// Also written in SQL, by the role-template function of migration 0040.
    RolePermissionsChanged => "role.permissions_changed",
    RolePermissionsReset => "role.permissions_reset",
    FeatureChanged => "feature.changed",
    TenantCreated => "tenant.created",
    TenantSuspended => "tenant.suspended",
    TenantUnsuspended => "tenant.unsuspended",
    TenantOwnerMfaChanged => "tenant.owner_mfa_changed",
}

/// One audited change. `before`/`after` must already have sensitive fields (health notes) redacted.
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    pub action: Action,
    pub entity_type: &'a str,
    pub entity_id: Uuid,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub reason: Option<&'a str>,
}

/// Records `entry` as done by the signed-in `actor`; written with the transaction's other entries
/// (module docs).
pub async fn record(
    tx: &mut TenantTx,
    actor: &Actor,
    entry: AuditEntry<'_>,
) -> Result<(), AppError> {
    pend(tx, Some(actor.user_id.as_uuid()), "user", actor.ip, entry);
    Ok(())
}

/// An entry in the spa's own trail for something a platform operator did to the spa
/// (`actor_kind = 'platform_admin'`, ADR-0018 D6), so the owner sees it. `operator` is the
/// `platform_operator` id, not a spa account.
pub async fn record_platform(
    tx: &mut TenantTx,
    operator: Uuid,
    ip: Option<std::net::IpAddr>,
    entry: AuditEntry<'_>,
) -> Result<(), AppError> {
    pend(tx, Some(operator), "platform_admin", ip, entry);
    Ok(())
}

/// An audit entry with no signed-in actor (`actor_kind = 'system'`), e.g. a forgotten-password
/// request from the login screen: who asked is not authenticated, so no user is named as actor.
pub async fn record_system(
    tx: &mut TenantTx,
    ip: Option<std::net::IpAddr>,
    entry: AuditEntry<'_>,
) -> Result<(), AppError> {
    pend(tx, None, "system", ip, entry);
    Ok(())
}

/// An entry recorded in a transaction, waiting for [`write_pending`]. Its id is taken when it is
/// recorded, so entries of one instant read in the order they were recorded.
#[derive(Debug)]
pub(crate) struct Pending {
    id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_kind: &'static str,
    action: Action,
    entity_type: String,
    entity_id: Uuid,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
    ip: Option<sqlx::types::ipnetwork::IpNetwork>,
}

fn pend(
    tx: &mut TenantTx,
    actor_user_id: Option<Uuid>,
    actor_kind: &'static str,
    ip: Option<std::net::IpAddr>,
    entry: AuditEntry<'_>,
) {
    tx.audit.push(Pending {
        id: Uuid::now_v7(),
        actor_user_id,
        actor_kind,
        action: entry.action,
        entity_type: entry.entity_type.to_owned(),
        entity_id: entry.entity_id,
        before: entry.before,
        after: entry.after,
        reason: entry.reason.map(str::to_owned),
        ip: ip.map(sqlx::types::ipnetwork::IpNetwork::from),
    });
}

/// Writes the transaction's pending entries in the order they were recorded. The first one takes
/// the database clock of its insert unless the instant is already fixed (a back-filling actor, or
/// an earlier write in this transaction); every other entry of the transaction carries the same.
pub(crate) async fn write_pending(tx: &mut TenantTx) -> Result<(), AppError> {
    for p in std::mem::take(&mut tx.audit) {
        let at = sqlx::query_scalar!(
            r#"INSERT INTO app.audit_log
                 (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, before, after, reason, ip, at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, COALESCE($12, clock_timestamp()))
               RETURNING at"#,
            p.id,
            tx.tenant().as_uuid(),
            p.actor_user_id,
            p.actor_kind,
            p.action.as_str(),
            p.entity_type,
            p.entity_id,
            p.before,
            p.after,
            p.reason,
            p.ip,
            tx.clock,
        )
        .fetch_one(tx.conn())
        .await?;
        tx.clock.get_or_insert(at);
    }
    Ok(())
}

/// Placeholder written instead of sensitive values (health notes, visit notes).
pub const REDACTED: &str = "[redacted]";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_action_key_is_unique_well_formed_and_round_trips() {
        let keys: BTreeSet<&str> = Action::ALL.iter().map(|a| a.as_str()).collect();
        assert_eq!(keys.len(), Action::ALL.len());
        for a in Action::ALL {
            // The CHECK of `audit_log.action` (migration 0001).
            let parts: Vec<&str> = a.as_str().split('.').collect();
            assert!(parts.len() >= 2, "{a:?}");
            assert!(
                parts.iter().all(|p| !p.is_empty()
                    && p.chars().all(|c| c.is_ascii_lowercase() || c == '_')),
                "{a:?}"
            );
            assert_eq!(Action::parse(a.as_str()), Some(*a));
        }
        assert_eq!(Action::parse("visit.something_old"), None);
    }
}
