//! A staff profile's login, looked after by the owner (ADR-0016, spec staff-management §17):
//! create it from the profile (the profile and the login are one person by construction), reset
//! its password, change its role; and the forgotten-password requests of the spa (option A).
//!
//! Every use case needs `staff.manage`; the HTTP layer also asks for a recent re-authentication
//! (create, reset, role). Temporary passwords come back once and are never audited or logged.

use crate::profile::{self, Profile, ProfileView};
use crate::repo;
use identity::account::{self as login, LoginRole, TemporaryPassword};
use kernel::audit::{self, AuditEntry};
use kernel::{Actor, AppError, Permission, TenantTx, UserId};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// A login just created for a profile: the updated profile and the one-time temporary password.
pub struct LoginCreated {
    pub profile: ProfileView,
    pub temporary_password: TemporaryPassword,
}

/// `422 account.owner_protected`: the owner's own login, or any login that manages staff, is
/// never reset or re-roled from the roster.
const fn owner_protected() -> AppError {
    AppError::Unprocessable {
        code: "account.owner_protected",
    }
}

/// The profile and its login, refusing a login the owner may not touch here.
async fn managed_login(
    tx: &mut TenantTx,
    actor: &Actor,
    id: Uuid,
) -> Result<(Profile, UserId), AppError> {
    let p = profile::profile_in(tx, id).await?;
    let user = p.user_id.ok_or(AppError::Conflict {
        code: "staff.no_login",
    })?;
    if user == actor.user_id.as_uuid() || repo::login_manages_staff(tx, user).await? {
        return Err(owner_protected());
    }
    Ok((p, UserId::from_uuid(user)))
}

/// `POST /staff/{id}/login` (`staff.manage`, `If-Match`): a login for a working profile that has
/// none — the phone (unused by any account), the role, a temporary password; linked to the
/// profile in the same transaction. Audit `staff.login_create` (user, role; never the password).
pub async fn create(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    phone: &str,
    role: LoginRole,
) -> Result<LoginCreated, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let before = profile::load_one(&mut tx, id, true).await?;
    profile::fresh(&before, version)?;
    if before.user_id.is_some() {
        return Err(AppError::Conflict {
            code: "staff.account_exists",
        });
    }
    if before.employment.current().is_none() {
        return Err(AppError::Conflict {
            code: "staff.already_left",
        });
    }
    let (user, temporary_password) =
        login::create_login(&mut tx, phone, &before.full_name, role).await?;
    if !repo::set_link(&mut tx, id, version, Some(user.as_uuid())).await? {
        return Err(AppError::StaleVersion);
    }
    let after = profile::reread(&mut tx, actor, id).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.login_create",
            entity_type: "staff",
            entity_id: id,
            before: Some(json!({"user_id": null})),
            after: Some(json!({"user_id": user.as_uuid(), "role": role.system().key()})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(LoginCreated {
        profile: after,
        temporary_password,
    })
}

/// `POST /staff/{id}/login/reset-password` (`staff.manage`): a new temporary password for the
/// profile's login; it must be changed at the next sign-in and every session of it ends; open
/// forgotten-password requests of it close. Never the owner's own login or one that manages
/// staff (`422 account.owner_protected`), never a login that also belongs to another spa
/// (`422 account.other_spa`: the password opens that spa too), never a login locked when its
/// person left (`409 account.locked`: rehiring unlocks it; a reset must not reopen access).
pub async fn reset_password(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
) -> Result<TemporaryPassword, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let (_, user) = managed_login(&mut tx, actor, id).await?;
    if !repo::is_active_member(&mut tx, user.as_uuid()).await? {
        return Err(AppError::Conflict {
            code: "account.locked",
        });
    }
    if login::spa_count(pool, user).await? > 1 {
        return Err(AppError::Unprocessable {
            code: "account.other_spa",
        });
    }
    let temporary = login::reset_password(&mut tx, user).await?;
    login::close_requests_of(&mut tx, user, actor.user_id).await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "staff.login_password_reset",
            entity_type: "staff",
            entity_id: id,
            before: None,
            after: Some(json!({"user_id": user.as_uuid(), "sessions_revoked": true})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(temporary)
}

/// `PUT /staff/{id}/login/role` (`staff.manage`): Quản lý ↔ Kỹ thuật viên for the profile's
/// login in this spa; permissions follow on the login's next request. Not the owner's own login
/// nor one that manages staff; not for someone who has left (`409 staff.already_left`).
pub async fn set_role(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    role: LoginRole,
) -> Result<ProfileView, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let (p, user) = managed_login(&mut tx, actor, id).await?;
    if p.employment.current().is_none() {
        return Err(AppError::Conflict {
            code: "staff.already_left",
        });
    }
    let before = login::role_of(&mut tx, user).await?;
    if before != Some(role) {
        login::set_role(&mut tx, user, role).await?;
        audit::record(
            &mut tx,
            actor,
            AuditEntry {
                action: "staff.login_role",
                entity_type: "staff",
                entity_id: id,
                before: Some(json!({"role": before.map(|r| r.system().key())})),
                after: Some(json!({"role": role.system().key()})),
                reason: None,
            },
        )
        .await?;
    }
    let after = profile::reread(&mut tx, actor, id).await?;
    tx.commit().await?;
    Ok(after)
}

/// An open forgotten-password request, with who asked as the roster knows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetRequest {
    pub id: Uuid,
    pub user_id: Uuid,
    pub full_name: String,
    /// The profile linked to the login, when there is one (its sheet holds "Đặt lại").
    pub staff_id: Option<Uuid>,
    pub requested_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// `GET /staff/password-reset-requests` (`staff.manage`): open, unexpired requests, oldest first.
pub async fn reset_requests(pool: &PgPool, actor: &Actor) -> Result<Vec<ResetRequest>, AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let requests = login::reset_requests(&mut tx).await?;
    let accounts = repo::accounts(&mut tx).await?;
    tx.commit().await?;
    Ok(requests
        .into_iter()
        .filter_map(|r| {
            let account = accounts.iter().find(|a| a.user_id == r.user_id.as_uuid())?;
            Some(ResetRequest {
                id: r.id,
                user_id: account.user_id,
                full_name: account.full_name.clone(),
                staff_id: account.staff_id,
                requested_at: r.requested_at,
                expires_at: r.expires_at,
            })
        })
        .collect())
}

/// `POST /staff/password-reset-requests/{id}/dismiss` (`staff.manage`): "Bỏ qua". `404` for an
/// unknown, closed or another spa's request.
pub async fn dismiss_request(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<(), AppError> {
    actor.require(Permission::StaffManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let user = login::dismiss_request(&mut tx, id, actor.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "account.password_reset_dismissed",
            entity_type: "user",
            entity_id: user.as_uuid(),
            before: None,
            after: Some(json!({"request_id": id})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
