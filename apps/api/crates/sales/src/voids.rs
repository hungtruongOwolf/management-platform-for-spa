//! Voiding sales (spec R1-1 US6). Payments are append-only: a void adds one `reversal` row per
//! payment, booked on the day of the void, and moves the sale `open` → `voided` (amount paid → 0).
//! Nothing recorded is ever updated or deleted, so the cash book of every day stays reproducible.
//!
//! - [`void_in`]: the money part, inside the caller's transaction (also used by visit voids);
//! - [`void_sale`]: `POST /sales/{id}/void` for a package sale, which also cancels its packages
//!   while none of their sessions was used (AC6.4).

use crate::repo;
use crate::service::load;
use chrono::NaiveDate;
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, Permission, TenantTx, corrections};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// What a void reversed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoidedSale {
    /// (original payment id, reversal id, amount) per reversed payment.
    pub reversals: Vec<(Uuid, Uuid, i64)>,
}

/// Voids an open sale inside the caller's transaction: `open` → `voided`, then one reversal per
/// payment, then an audit entry with the reason. The caller authorizes (permission, closed day).
///
/// Errors: unknown sale → `404`; already voided → `409 sale.voided`.
pub async fn void_in(
    tx: &mut TenantTx,
    actor: &Actor,
    sale_id: Uuid,
    reason: &str,
    today: NaiveDate,
) -> Result<VoidedSale, AppError> {
    let sale = repo::lock_sale(tx, sale_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if sale.status != "open"
        || !repo::mark_voided(tx, sale_id, reason, actor.user_id.as_uuid()).await?
    {
        return Err(AppError::Conflict {
            code: "sale.voided",
        });
    }
    let mut reversals = Vec::new();
    for payment in repo::unreversed_payments(tx, sale_id).await? {
        let reversal =
            repo::insert_reversal(tx, sale_id, &payment, today, actor.user_id.as_uuid()).await?;
        reversals.push((payment.id, reversal, payment.amount));
    }
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "sale.void",
            entity_type: "sale",
            entity_id: sale_id,
            before: Some(json!({"status": "open", "business_date": sale.business_date})),
            after: Some(json!({
                "status": "voided", "amount_paid": 0,
                "reversals": reversals.iter().map(|(of, id, amount)| json!({
                    "payment_id": of, "reversal_id": id, "amount": amount,
                })).collect::<Vec<_>>(),
            })),
            reason: Some(reason),
        },
    )
    .await?;
    Ok(VoidedSale { reversals })
}

/// `POST /sales/{id}/void` (`record.void_same_day`; a closed day needs `record.void_closed_day`;
/// Idempotency-Key). Only package sales: the sale of a visit's one-off
/// services is voided with its visit (`409 sale.void_via_visit`), so a visit never keeps lines
/// whose money was returned. Any session of its packages used → `409 sale.has_used_sessions`.
/// Authorize before reporting what is wrong with a void request (F17): `404` / `403 day.locked`
/// first, then `refused`. Error path only; the caller has checked `record.void_same_day`.
pub async fn authorized_then(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    refused: AppError,
) -> AppError {
    let check = async {
        let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
        let sale = repo::lock_sale(&mut tx, id)
            .await?
            .ok_or(AppError::NotFound)?;
        let today = kernel::tenant::profile(&mut tx)
            .await?
            .business_date
            .as_naive();
        corrections::authorize_void(actor, sale.business_date, today)?;
        tx.commit().await?;
        Ok::<(), AppError>(())
    };
    match check.await {
        Ok(()) => refused,
        Err(denied) => denied,
    }
}

pub async fn void_sale(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    id: Uuid,
    reason: Option<&str>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::RecordVoidSameDay)?;
    let reason = match corrections::reason(reason) {
        Ok(r) => r,
        Err(invalid) => return Err(authorized_then(pool, actor, id, invalid).await),
    };

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let sale = repo::lock_sale(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if sale.status != "open" {
        return Err(AppError::Conflict {
            code: "sale.voided",
        });
    }
    if !sale.sells_packages {
        return Err(AppError::Conflict {
            code: "sale.void_via_visit",
        });
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    corrections::authorize_void(actor, sale.business_date, today)?;
    packages::lifecycle::cancel_for_sale(&mut tx, actor, id, &reason).await?;
    void_in(&mut tx, actor, id, &reason, today).await?;

    let dto = load(&mut tx, id, today)
        .await?
        .ok_or_else(|| AppError::Internal("sale vanished".into()))?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}
