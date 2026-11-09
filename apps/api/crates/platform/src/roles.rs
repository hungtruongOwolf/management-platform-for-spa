//! Roles and their permissions ("chức năng", spec US6–US7, ADR-0018 D5): the catalogue, the
//! default templates the operator edits, and a spa's own grants for one system role. Guardrails
//! are pure ([`check_grants`]) and run before anything is written. Changes take effect on the next
//! request: permissions are read on every request (`app.session_actor`).

use crate::audit::{self, PlatformEntry};
use crate::auth::OperatorSession;
use crate::gate_refusal;
use crate::tenants::Reason;
use identity::roles::{NOT_GRANTABLE, SystemRole};
use kernel::audit::{AuditEntry, record_platform};
use kernel::{AppError, FieldError, Permission, TenantId, TenantTx};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::BTreeSet;
use std::str::FromStr;
use utoipa::ToSchema;
use uuid::Uuid;

/// What the owner role always keeps: without these nobody could run the spa's staff and logins
/// or read what operators did to it.
pub const OWNER_FLOOR: &[Permission] = &[
    Permission::StaffManage,
    Permission::StaffRead,
    Permission::StaffReadBasic,
    Permission::AuditRead,
];

/// Only the owner role may hold these: decision C, ADR-0015/0016 (`account.owner_protected`) and
/// ADR-0014 (an owner is never downgraded) all recognise the owner by `staff.manage`.
pub const OWNER_ONLY: &[Permission] = &[Permission::StaffManage];

/// One permission of the catalogue.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PermissionInfo {
    pub key: &'static str,
    /// Area: the part before the first dot (`customer`, `staff`, …).
    pub group: &'static str,
    /// May a role hold it (`platform.*` and the derived former-staff permission may not).
    pub grantable: bool,
    /// Only the owner role may hold it.
    pub owner_only: bool,
    /// The owner role must keep it.
    pub owner_floor: bool,
    /// The feature whose switch removes it, if any.
    pub feature: Option<&'static str>,
}

pub fn catalogue() -> Vec<PermissionInfo> {
    Permission::ALL
        .iter()
        .map(|p| {
            let key = p.as_str();
            PermissionInfo {
                key,
                group: key.split('.').next().unwrap_or(key),
                grantable: !NOT_GRANTABLE.contains(p),
                owner_only: OWNER_ONLY.contains(p),
                owner_floor: OWNER_FLOOR.contains(p),
                feature: kernel::feature::owning(*p).map(kernel::Feature::key),
            }
        })
        .collect()
}

/// The guardrails of every role edit. Returns the permissions, parsed and deduplicated.
pub fn check_grants(role: SystemRole, names: &[String]) -> Result<Vec<Permission>, AppError> {
    let mut set = BTreeSet::new();
    for name in names {
        let permission = Permission::from_str(name).map_err(|_| {
            AppError::Validation(vec![FieldError::new("permissions", "unknown_permission")])
        })?;
        set.insert(permission);
    }
    if set.iter().any(|p| NOT_GRANTABLE.contains(p)) {
        return Err(AppError::Unprocessable {
            code: "role.not_grantable",
        });
    }
    if role != SystemRole::Owner && set.iter().any(|p| OWNER_ONLY.contains(p)) {
        return Err(AppError::Unprocessable {
            code: "role.owner_only_permission",
        });
    }
    if role == SystemRole::Owner {
        let missing: Vec<&str> = OWNER_FLOOR
            .iter()
            .filter(|p| !set.contains(p))
            .map(|p| p.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(AppError::UnprocessableDetails {
                code: "role.owner_floor",
                details: json!({"missing": missing}),
            });
        }
    }
    Ok(set.into_iter().collect())
}

fn role_of(key: &str) -> Result<SystemRole, AppError> {
    SystemRole::from_key(key).ok_or(AppError::NotFound)
}

fn names(permissions: &[Permission]) -> Vec<String> {
    permissions.iter().map(|p| p.as_str().to_owned()).collect()
}

/// A default template, or one spa's role.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleGrants {
    pub key: &'static str,
    pub name: &'static str,
    pub permissions: Vec<String>,
    /// Spa roles only: `true` = follows the template (a template edit changes it too).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follows_template: Option<bool>,
    /// Template edits only: how many spas changed with it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_spas: Option<i32>,
}

/// Body of the role edits.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleEdit {
    pub permissions: Vec<String>,
    /// 3–500 characters.
    pub reason: String,
}

pub async fn templates(pool: &PgPool) -> Result<Vec<RoleGrants>, AppError> {
    let rows = sqlx::query!(
        "SELECT role_key, permission FROM app.role_template_permission ORDER BY role_key, permission"
    )
    .fetch_all(pool)
    .await?;
    Ok(SystemRole::ALL
        .iter()
        .map(|role| RoleGrants {
            key: role.key(),
            name: role.display_name(),
            permissions: rows
                .iter()
                .filter(|r| r.role_key == role.key())
                .map(|r| r.permission.clone())
                .collect(),
            follows_template: None,
            updated_spas: None,
        })
        .collect())
}

/// `PUT /admin/role-templates/{key}`: the new default, applied to every spa that follows it.
pub async fn set_template(
    pool: &PgPool,
    op: &OperatorSession,
    key: &str,
    edit: &RoleEdit,
) -> Result<RoleGrants, AppError> {
    let role = role_of(key)?;
    let permissions = names(&check_grants(role, &edit.permissions)?);
    let reason = Reason {
        reason: edit.reason.clone(),
    };
    let reason = reason.checked()?;
    let mut tx = pool.begin().await?;
    let updated = sqlx::query_scalar!(
        r#"SELECT app.platform_apply_role_template($1, $2, $3) AS "updated!""#,
        &op.token_hash[..],
        role.key(),
        &permissions
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(gate_refusal)?;
    audit::record(
        &mut tx,
        op.operator_id,
        op.meta.ip,
        PlatformEntry {
            action: "platform.role_template.set",
            tenant: None,
            target_type: Some("role_template"),
            target_id: None,
            detail: Some(
                json!({"role": role.key(), "permissions": permissions, "updated_spas": updated}),
            ),
            reason: Some(reason),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(RoleGrants {
        key: role.key(),
        name: role.display_name(),
        permissions,
        follows_template: None,
        updated_spas: Some(updated),
    })
}

async fn spa_roles(tx: &mut TenantTx) -> Result<Vec<RoleGrants>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        r#"SELECT r.key, r.follows_template,
                  COALESCE(array_agg(rp.permission ORDER BY rp.permission)
                           FILTER (WHERE rp.permission IS NOT NULL), '{}') AS "permissions!"
           FROM app.role r
           LEFT JOIN app.role_permission rp ON rp.tenant_id = r.tenant_id AND rp.role_id = r.id
           WHERE r.tenant_id = $1 AND r.is_system
           GROUP BY r.id"#,
        tenant
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(SystemRole::ALL
        .iter()
        .filter_map(|role| {
            rows.iter()
                .find(|r| r.key == role.key())
                .map(|r| RoleGrants {
                    key: role.key(),
                    name: role.display_name(),
                    permissions: r.permissions.clone(),
                    follows_template: Some(r.follows_template),
                    updated_spas: None,
                })
        })
        .collect())
}

/// `GET /admin/tenants/{id}/roles`.
pub async fn of_tenant(
    pool: &PgPool,
    op: &OperatorSession,
    tenant: Uuid,
) -> Result<Vec<RoleGrants>, AppError> {
    crate::tenants::get(pool, op, tenant).await?;
    let mut tx = TenantTx::begin(pool, TenantId::from_uuid(tenant)).await?;
    let roles = spa_roles(&mut tx).await?;
    tx.commit().await?;
    Ok(roles)
}

/// `PUT /admin/tenants/{id}/roles/{key}` (own grants for this spa) or, with `None`,
/// `POST …/reset` (back to the template).
pub async fn set_for_tenant(
    pool: &PgPool,
    op: &OperatorSession,
    tenant: Uuid,
    key: &str,
    edit: Option<&RoleEdit>,
    reason: &Reason,
) -> Result<RoleGrants, AppError> {
    let role = role_of(key)?;
    let reason = reason.checked()?;
    let permissions = match edit {
        Some(edit) => Some(names(&check_grants(role, &edit.permissions)?)),
        None => None,
    };
    let mut tx = TenantTx::begin(pool, TenantId::from_uuid(tenant)).await?;
    // The write checks the operator's step-up itself (0041); no row = no such spa or role.
    let Some(changed) = sqlx::query!(
        r#"SELECT role_id AS "role_id!", before AS "before!", after AS "after!"
           FROM app.platform_set_tenant_role($1, $2, $3, $4)"#,
        &op.token_hash[..],
        tenant,
        role.key(),
        permissions.as_ref()
    )
    .fetch_optional(tx.conn())
    .await
    .map_err(gate_refusal)?
    else {
        return Err(AppError::NotFound);
    };
    let (role_id, before, after) = (changed.role_id, changed.before, changed.after);
    let (spa_action, platform_action) = if edit.is_some() {
        ("role.permissions_changed", "platform.role.set")
    } else {
        ("role.permissions_reset", "platform.role.reset")
    };
    record_platform(
        &mut tx,
        op.operator_id,
        op.meta.ip,
        AuditEntry {
            action: spa_action,
            entity_type: "role",
            entity_id: role_id,
            before: Some(json!({"permissions": before})),
            after: Some(json!({"permissions": after})),
            reason: Some(reason),
        },
    )
    .await?;
    audit::record(
        tx.conn(),
        op.operator_id,
        op.meta.ip,
        PlatformEntry {
            action: platform_action,
            tenant: Some(tenant),
            target_type: Some("role"),
            target_id: Some(role_id),
            detail: Some(json!({"role": role.key(), "before": before, "after": after})),
            reason: Some(reason),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(RoleGrants {
        key: role.key(),
        name: role.display_name(),
        permissions: after,
        follows_template: Some(edit.is_none()),
        updated_spas: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(ps: &[Permission]) -> Vec<String> {
        names(ps)
    }

    #[test]
    fn the_default_roles_pass_their_own_guardrails() {
        for role in SystemRole::ALL {
            assert!(
                check_grants(role, &keys(&role.default_permissions())).is_ok(),
                "{role:?}"
            );
        }
    }

    #[test]
    fn the_owner_floor_is_kept() {
        let mut owner = SystemRole::Owner.default_permissions();
        owner.retain(|p| *p != Permission::StaffManage);
        match check_grants(SystemRole::Owner, &keys(&owner)) {
            Err(AppError::UnprocessableDetails { code, details }) => {
                assert_eq!(code, "role.owner_floor");
                assert_eq!(details["missing"], json!(["staff.manage"]));
            }
            other => panic!("expected role.owner_floor, got {other:?}"),
        }
    }

    #[test]
    fn staff_management_stays_with_the_owner() {
        let mut manager = SystemRole::Manager.default_permissions();
        manager.push(Permission::StaffManage);
        assert!(matches!(
            check_grants(SystemRole::Manager, &keys(&manager)),
            Err(AppError::Unprocessable {
                code: "role.owner_only_permission"
            })
        ));
    }

    #[test]
    fn derived_and_platform_permissions_are_never_granted() {
        for p in NOT_GRANTABLE {
            assert!(matches!(
                check_grants(SystemRole::Technician, &[p.as_str().to_owned()]),
                Err(AppError::Unprocessable {
                    code: "role.not_grantable"
                })
            ));
        }
        assert!(matches!(
            check_grants(SystemRole::Technician, &["customer.delete_all".to_owned()]),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn the_catalogue_lists_every_permission_once() {
        let catalogue = catalogue();
        assert_eq!(catalogue.len(), Permission::ALL.len());
        let appointment = catalogue
            .iter()
            .find(|p| p.key == "appointment.manage")
            .map(|p| (p.group, p.feature));
        assert_eq!(appointment, Some(("appointment", Some("appointments"))));
    }
}
