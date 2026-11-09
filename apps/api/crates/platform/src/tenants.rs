//! Spas as the operator sees them (spec US2–US4): counts and metadata, never the spa's data.
//! Creating a spa runs in one `TenantTx` of the new spa: the tenant row through the gated DB
//! function (app_rw may not insert tenants), everything else under that spa's RLS.

use crate::audit::{self, PlatformEntry};
use crate::auth::OperatorSession;
use crate::gate_refusal;
use chrono::{DateTime, Utc};
use identity::account::TemporaryPassword;
use identity::roles::SystemRole;
use kernel::audit::{AuditEntry, record_platform};
use kernel::{AppError, FieldError, PhoneE164, TenantId, TenantTx};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    Active,
    Suspended,
}

impl TenantStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }

    fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            other => Err(AppError::Internal(format!("tenant status {other:?}"))),
        }
    }
}

/// One spa in the console's list: metadata and counts only (ADR-0003, ADR-0018 D3).
#[derive(Debug, Clone, Serialize, PartialEq, Eq, ToSchema)]
pub struct TenantRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: TenantStatus,
    pub created_at: DateTime<Utc>,
    /// Staff profiles still working.
    pub staff_count: i64,
    /// Customer profiles not anonymised.
    pub customer_count: i64,
    /// Completed (not voided) visits in the last 30 days.
    pub visits_30d: i64,
    /// Active logins holding `staff.manage`.
    pub owner_count: i64,
    /// Latest entry of the spa's audit trail.
    pub last_activity_at: Option<DateTime<Utc>>,
}

pub async fn list(pool: &PgPool, op: &OperatorSession) -> Result<Vec<TenantRow>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", name AS "name!", slug AS "slug!", status AS "status!",
                  created_at AS "created_at!", staff_count AS "staff_count!",
                  customer_count AS "customer_count!", visits_30d AS "visits_30d!",
                  owner_count AS "owner_count!", last_activity_at
           FROM app.platform_tenant_overview($1)"#,
        &op.token_hash[..]
    )
    .fetch_all(pool)
    .await
    .map_err(gate_refusal)?;
    rows.into_iter()
        .map(|r| {
            Ok(TenantRow {
                id: r.id,
                name: r.name,
                slug: r.slug,
                status: TenantStatus::parse(&r.status)?,
                created_at: r.created_at,
                staff_count: r.staff_count,
                customer_count: r.customer_count,
                visits_30d: r.visits_30d,
                owner_count: r.owner_count,
                last_activity_at: r.last_activity_at,
            })
        })
        .collect()
}

pub async fn get(pool: &PgPool, op: &OperatorSession, id: Uuid) -> Result<TenantRow, AppError> {
    list(pool, op)
        .await?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or(AppError::NotFound)
}

/// Body of `POST /admin/tenants`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewTenant {
    pub name: String,
    /// `^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$`, unique.
    pub slug: String,
    /// Default "Cơ sở chính".
    #[serde(default)]
    pub branch_name: Option<String>,
    pub owner: NewOwner,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewOwner {
    /// Vietnamese mobile number that has no login yet (one person, one spa: ADR-0016).
    pub phone: String,
    pub full_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedOwner {
    pub user_id: Uuid,
    pub phone: String,
    /// Shown this once (`Cache-Control: no-store`); the owner must replace it at first sign-in.
    pub temporary_password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedTenant {
    pub tenant: TenantRow,
    pub owner: CreatedOwner,
}

struct Valid {
    name: String,
    slug: String,
    branch: String,
    phone: PhoneE164,
    owner_name: String,
}

fn text_ok(value: &str, max: usize) -> bool {
    let n = value.trim().chars().count();
    (1..=max).contains(&n)
}

fn slug_ok(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    let edge = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    (1..=40).contains(&bytes.len())
        && bytes.first().is_some_and(edge)
        && bytes.last().is_some_and(edge)
        && bytes.iter().all(|b| edge(b) || *b == b'-')
}

fn validate(input: &NewTenant) -> Result<Valid, AppError> {
    let mut errors = Vec::new();
    if !text_ok(&input.name, 200) {
        errors.push(FieldError::new("name", "required"));
    }
    if !slug_ok(&input.slug) {
        errors.push(FieldError::new("slug", "invalid_format"));
    }
    let branch = input
        .branch_name
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or("Cơ sở chính")
        .to_owned();
    if !text_ok(&branch, 200) {
        errors.push(FieldError::new("branch_name", "too_long"));
    }
    if !text_ok(&input.owner.full_name, 200) {
        errors.push(FieldError::new("owner.full_name", "required"));
    }
    let phone = PhoneE164::parse(&input.owner.phone);
    if phone.is_err() {
        errors.push(FieldError::new("owner.phone", "invalid_format"));
    }
    match phone {
        Ok(phone) if errors.is_empty() => Ok(Valid {
            name: input.name.trim().to_owned(),
            slug: input.slug.clone(),
            branch,
            phone,
            owner_name: input.owner.full_name.trim().to_owned(),
        }),
        _ => Err(AppError::Validation(errors)),
    }
}

fn unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
}

/// `POST /admin/tenants`: the spa, its default branch and system roles, and its first owner with
/// a temporary password (ADR-0016) — one transaction, all or nothing.
pub async fn create(
    pool: &PgPool,
    op: &OperatorSession,
    input: &NewTenant,
) -> Result<CreatedTenant, AppError> {
    let valid = validate(input)?;
    let temporary = TemporaryPassword::generate().map_err(|e| AppError::Internal(e.to_string()))?;
    let hashed = identity::account::hash("temporary_password", temporary.expose()).await?;

    let tenant = TenantId::new();
    let tenant_id = tenant.as_uuid();
    let mut tx = TenantTx::begin(pool, tenant).await?;
    sqlx::query!(
        "SELECT app.platform_insert_tenant($1, $2, $3, $4)",
        &op.token_hash[..],
        tenant_id,
        valid.name,
        valid.slug
    )
    .execute(tx.conn())
    .await
    .map_err(|e| {
        if unique_violation(&e) {
            AppError::Conflict {
                code: "tenant.slug_taken",
            }
        } else {
            gate_refusal(e)
        }
    })?;
    sqlx::query!(
        "INSERT INTO app.branch (id, tenant_id, name) VALUES ($1, $2, $3)",
        Uuid::now_v7(),
        tenant_id,
        valid.branch
    )
    .execute(tx.conn())
    .await?;
    let mut owner_role = None;
    for role in SystemRole::ALL {
        let role_id = Uuid::now_v7();
        sqlx::query!(
            "INSERT INTO app.role (id, tenant_id, key, name, is_system, requires_mfa)
             VALUES ($1, $2, $3, $4, true, $5)",
            role_id,
            tenant_id,
            role.key(),
            role.display_name(),
            role.requires_mfa()
        )
        .execute(tx.conn())
        .await?;
        let permissions: Vec<String> = role
            .default_permissions()
            .iter()
            .map(|p| p.as_str().to_owned())
            .collect();
        sqlx::query!(
            "INSERT INTO app.role_permission (tenant_id, role_id, permission)
             SELECT $1, $2, unnest($3::text[])",
            tenant_id,
            role_id,
            &permissions
        )
        .execute(tx.conn())
        .await?;
        if role == SystemRole::Owner {
            owner_role = Some(role_id);
        }
    }
    let owner_role = owner_role.ok_or(AppError::Internal("no owner role".to_owned()))?;

    let user_id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO app.user_account (id, phone_e164, full_name, password_hash, must_change_password)
         VALUES ($1, $2, $3, $4, true)",
        user_id,
        valid.phone.as_str(),
        valid.owner_name,
        hashed
    )
    .execute(tx.conn())
    .await
    .map_err(|e| {
        if unique_violation(&e) {
            AppError::Conflict {
                code: "account.phone_taken",
            }
        } else {
            AppError::Database(e)
        }
    })?;
    let membership = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO app.membership (id, tenant_id, user_id) VALUES ($1, $2, $3)",
        membership,
        tenant_id,
        user_id
    )
    .execute(tx.conn())
    .await?;
    sqlx::query!(
        "INSERT INTO app.membership_role (tenant_id, membership_id, role_id) VALUES ($1, $2, $3)",
        tenant_id,
        membership,
        owner_role
    )
    .execute(tx.conn())
    .await?;

    record_platform(
        &mut tx,
        op.operator_id,
        op.meta.ip,
        AuditEntry {
            action: "tenant.created",
            entity_type: "tenant",
            entity_id: tenant_id,
            before: None,
            after: Some(json!({"name": valid.name, "slug": valid.slug, "owner_user_id": user_id})),
            reason: None,
        },
    )
    .await?;
    audit::record(
        tx.conn(),
        op.operator_id,
        op.meta.ip,
        PlatformEntry {
            action: "platform.tenant.create",
            tenant: Some(tenant_id),
            target_type: Some("tenant"),
            target_id: Some(tenant_id),
            detail: Some(json!({
                "slug": valid.slug,
                "owner_user_id": user_id,
                "owner_phone": audit::phone_tail(valid.phone.as_str()),
            })),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;

    Ok(CreatedTenant {
        tenant: TenantRow {
            id: tenant_id,
            name: valid.name,
            slug: valid.slug,
            status: TenantStatus::Active,
            created_at: Utc::now(),
            staff_count: 0,
            customer_count: 0,
            visits_30d: 0,
            owner_count: 1,
            last_activity_at: None,
        },
        owner: CreatedOwner {
            user_id,
            phone: valid.phone.display_national(),
            temporary_password: temporary.expose().to_owned(),
        },
    })
}

/// Body of suspend / unsuspend (and every other operator write): why, for the audit trail.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Reason {
    /// 3–500 characters.
    pub reason: String,
}

impl Reason {
    pub fn checked(&self) -> Result<&str, AppError> {
        let reason = self.reason.trim();
        if (3..=500).contains(&reason.chars().count()) {
            Ok(reason)
        } else {
            Err(AppError::Validation(vec![FieldError::new(
                "reason", "required",
            )]))
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusChanged {
    pub tenant: TenantRow,
    /// Sessions of the spa's members ended by a suspension.
    pub revoked_sessions: i32,
}

/// Suspend or reactivate a spa (spec US4). Data is untouched; a suspension ends the members'
/// sessions and blocks their sign-in (`auth.spa_suspended`).
pub async fn set_status(
    pool: &PgPool,
    op: &OperatorSession,
    id: Uuid,
    status: TenantStatus,
    reason: &Reason,
) -> Result<StatusChanged, AppError> {
    let reason = reason.checked()?;
    let mut tx = TenantTx::begin(pool, TenantId::from_uuid(id)).await?;
    let Some(changed) = sqlx::query!(
        r#"SELECT previous AS "previous!", revoked AS "revoked!"
           FROM app.platform_set_tenant_status($1, $2, $3)"#,
        &op.token_hash[..],
        id,
        status.as_str()
    )
    .fetch_optional(tx.conn())
    .await
    .map_err(gate_refusal)?
    else {
        return Err(AppError::NotFound);
    };
    if changed.previous == status.as_str() {
        return Err(AppError::Conflict {
            code: "tenant.status_unchanged",
        });
    }
    let (spa_action, platform_action) = match status {
        TenantStatus::Suspended => ("tenant.suspended", "platform.tenant.suspend"),
        TenantStatus::Active => ("tenant.unsuspended", "platform.tenant.unsuspend"),
    };
    record_platform(
        &mut tx,
        op.operator_id,
        op.meta.ip,
        AuditEntry {
            action: spa_action,
            entity_type: "tenant",
            entity_id: id,
            before: Some(json!({"status": changed.previous})),
            after: Some(json!({"status": status.as_str()})),
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
            tenant: Some(id),
            target_type: Some("tenant"),
            target_id: Some(id),
            detail: Some(json!({"revoked_sessions": changed.revoked})),
            reason: Some(reason),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(StatusChanged {
        tenant: get(pool, op, id).await?,
        revoked_sessions: changed.revoked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(slug: &str, phone: &str) -> NewTenant {
        NewTenant {
            name: "Spa Mới".into(),
            slug: slug.into(),
            branch_name: None,
            owner: NewOwner {
                phone: phone.into(),
                full_name: "Chủ Mới".into(),
            },
        }
    }

    #[test]
    fn slugs_follow_the_tenant_check() {
        for ok in ["a", "spa-1", "vien-phuc"] {
            assert!(slug_ok(ok), "{ok}");
        }
        for bad in ["", "-spa", "spa-", "Spa", "spa_1", &"a".repeat(41)] {
            assert!(!slug_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_new_spa_needs_a_valid_slug_name_and_owner_phone() {
        assert!(validate(&input("spa-moi", "0905111222")).is_ok());
        let Err(AppError::Validation(errors)) = validate(&input("Bad Slug", "12")) else {
            panic!("expected validation errors");
        };
        let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, ["slug", "owner.phone"]);
    }

    #[test]
    fn a_reason_is_required() {
        assert!(
            Reason {
                reason: "  ab ".into()
            }
            .checked()
            .is_err()
        );
        assert_eq!(
            Reason {
                reason: " Chưa thanh toán ".into()
            }
            .checked()
            .ok(),
            Some("Chưa thanh toán")
        );
    }
}
