//! Feature switches per spa (spec US5, ADR-0018 D4). The registry is code (`kernel::feature`);
//! a spa's switches are rows of `app.tenant_feature`, written only through the gated
//! `app.platform_set_feature`. Takes effect on the spa members' next request.

use crate::audit::{self, PlatformEntry};
use crate::auth::OperatorSession;
use crate::gate_refusal;
use crate::tenants::Reason;
use chrono::{DateTime, Utc};
use kernel::audit::{AuditEntry, record_platform};
use kernel::{AppError, Feature, TenantId, TenantTx};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

/// One feature of the registry.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FeatureInfo {
    pub key: &'static str,
    pub name: &'static str,
    /// Permissions a spa loses while it is off.
    #[schema(value_type = Vec<String>)]
    pub permissions: Vec<&'static str>,
}

pub fn registry() -> Vec<FeatureInfo> {
    Feature::ALL
        .iter()
        .map(|f| FeatureInfo {
            key: f.key(),
            name: f.name(),
            permissions: f.permissions().iter().map(|p| p.as_str()).collect(),
        })
        .collect()
}

/// A feature's state in one spa.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TenantFeature {
    pub key: &'static str,
    pub name: &'static str,
    pub enabled: bool,
    /// When an operator last switched it (`null` = never: on by default).
    pub updated_at: Option<DateTime<Utc>>,
}

/// `GET /admin/tenants/{id}/features`: every registry feature, on unless a row says otherwise.
pub async fn of_tenant(
    pool: &PgPool,
    op: &OperatorSession,
    tenant: Uuid,
) -> Result<Vec<TenantFeature>, AppError> {
    // Existence through the operator's own gated list (also proves the session to the DB).
    crate::tenants::get(pool, op, tenant).await?;
    let mut tx = TenantTx::begin(pool, TenantId::from_uuid(tenant)).await?;
    let rows = sqlx::query!(
        "SELECT feature, enabled, updated_at FROM app.tenant_feature WHERE tenant_id = $1",
        tenant
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Feature::ALL
        .iter()
        .map(|f| {
            let row = rows.iter().find(|r| r.feature == f.key());
            TenantFeature {
                key: f.key(),
                name: f.name(),
                enabled: row.is_none_or(|r| r.enabled),
                updated_at: row.map(|r| r.updated_at),
            }
        })
        .collect())
}

/// Body of `PUT /admin/tenants/{id}/features/{key}`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureSwitch {
    pub enabled: bool,
    /// 3–500 characters.
    pub reason: String,
}

/// Switches a feature on or off for one spa; audited on both sides.
pub async fn set(
    pool: &PgPool,
    op: &OperatorSession,
    tenant: Uuid,
    key: &str,
    input: &FeatureSwitch,
) -> Result<TenantFeature, AppError> {
    let feature = Feature::from_key(key).ok_or(AppError::NotFound)?;
    let reason = Reason {
        reason: input.reason.clone(),
    };
    let reason = reason.checked()?;
    let mut tx = TenantTx::begin(pool, TenantId::from_uuid(tenant)).await?;
    let Some(previous) = sqlx::query_scalar!(
        r#"SELECT previous AS "previous!" FROM app.platform_set_feature($1, $2, $3, $4)"#,
        &op.token_hash[..],
        tenant,
        feature.key(),
        input.enabled
    )
    .fetch_optional(tx.conn())
    .await
    .map_err(gate_refusal)?
    else {
        return Err(AppError::NotFound);
    };
    record_platform(
        &mut tx,
        op.operator_id,
        op.meta.ip,
        AuditEntry {
            action: "feature.changed",
            entity_type: "tenant",
            entity_id: tenant,
            before: Some(json!({"feature": feature.key(), "enabled": previous})),
            after: Some(json!({"feature": feature.key(), "enabled": input.enabled})),
            reason: Some(reason),
        },
    )
    .await?;
    audit::record(
        tx.conn(),
        op.operator_id,
        op.meta.ip,
        PlatformEntry {
            action: "platform.feature.set",
            tenant: Some(tenant),
            target_type: Some("feature"),
            target_id: None,
            detail: Some(
                json!({"feature": feature.key(), "before": previous, "after": input.enabled}),
            ),
            reason: Some(reason),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(TenantFeature {
        key: feature.key(),
        name: feature.name(),
        enabled: input.enabled,
        updated_at: Some(Utc::now()),
    })
}
