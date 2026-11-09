//! Staff use cases. Other contexts call [`require_active`] inside their own `TenantTx`.

use crate::repo::{self, StaffRow};
use identity::SystemRole;
use kernel::{Actor, AppError, Permission, TenantTx};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffStatus {
    Active,
    Left,
}

impl StaffStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Left => "left",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "left" => Some(Self::Left),
            _ => None,
        }
    }
}

pub struct Staff {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    pub status: StaffStatus,
}

impl TryFrom<StaffRow> for Staff {
    type Error = AppError;
    fn try_from(r: StaffRow) -> Result<Self, AppError> {
        let status = StaffStatus::parse(&r.status)
            .ok_or_else(|| AppError::Internal(format!("staff {} has status {}", r.id, r.status)))?;
        Ok(Self {
            id: r.id,
            full_name: r.full_name,
            position: r.position,
            status,
        })
    }
}

/// Picker list (`staff.read_basic`): names and status only, `None` = everyone.
pub async fn list(
    pool: &PgPool,
    actor: &Actor,
    status: Option<StaffStatus>,
) -> Result<Vec<Staff>, AppError> {
    actor.require(Permission::StaffReadBasic)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let rows = repo::list(&mut tx, status.map(StaffStatus::as_str)).await?;
    tx.commit().await?;
    rows.into_iter().map(Staff::try_from).collect()
}

/// An active profile that serves customers: a column of the appointments calendar and a choice
/// in its technician picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServingStaff {
    pub id: Uuid,
    pub full_name: String,
}

/// Who serves customers (ADR-0019 D14), decided by the linked login's role, never by the
/// position text: a technician or a manager does; an owner does not (no profile flag marks an
/// owner as taking customers); a login with no such role does not. A profile without a login is
/// a technician (ADR-0015), whatever its position says. `login_roles` = `None`: no login.
pub fn serves_customers(login_roles: Option<&[String]>) -> bool {
    let Some(keys) = login_roles else {
        return true;
    };
    let has = |role: SystemRole| keys.iter().any(|k| k == role.key());
    !has(SystemRole::Owner) && (has(SystemRole::Technician) || has(SystemRole::Manager))
}

/// Active staff who serve customers ([`serves_customers`]), by name, inside the caller's
/// transaction. The caller authorizes.
pub async fn serving_customers_in(tx: &mut TenantTx) -> Result<Vec<ServingStaff>, AppError> {
    Ok(repo::active_with_login_roles(tx)
        .await?
        .into_iter()
        .filter(|r| serves_customers(r.linked.then_some(r.role_keys.as_slice())))
        .map(|r| ServingStaff {
            id: r.id,
            full_name: r.full_name,
        })
        .collect())
}

/// A staff member verified to be currently working at this spa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveStaff {
    pub id: Uuid,
    pub full_name: String,
}

/// Checks, inside the caller's transaction, that `id` is an active staff member of this tenant
/// and keeps them from being marked as left until the transaction ends (AC3.6, AC4.8).
/// Unknown, foreign-tenant and departed staff are all `422 staff.inactive`: the answer reveals
/// nothing about other spas. The caller authorizes; this function only validates.
pub async fn require_active(tx: &mut TenantTx, id: Uuid) -> Result<ActiveStaff, AppError> {
    match repo::get_for_share(tx, id).await? {
        Some(row) if row.status == StaffStatus::Active.as_str() => Ok(ActiveStaff {
            id: row.id,
            full_name: row.full_name,
        }),
        _ => Err(AppError::Unprocessable {
            code: "staff.inactive",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| (*k).to_owned()).collect()
    }

    #[test]
    fn who_serves_customers_comes_from_the_login_role() {
        // No login: a technician, whatever the position text says (ADR-0015).
        assert!(serves_customers(None));
        assert!(serves_customers(Some(&roles(&["technician"]))));
        // ADR-0019 D14: a linked manager takes customers too.
        assert!(serves_customers(Some(&roles(&["manager"]))));
        // The owner does not (no "takes customers" flag exists), even with another role.
        assert!(!serves_customers(Some(&roles(&["owner"]))));
        assert!(!serves_customers(Some(&roles(&["manager", "owner"]))));
        // A login left with no serving role does not.
        assert!(!serves_customers(Some(&roles(&[]))));
        assert!(!serves_customers(Some(&roles(&["reception"]))));
    }
}
