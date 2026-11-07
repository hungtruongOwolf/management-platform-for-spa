//! The package lifecycle after the sale (spec R1-1 US6, US7):
//!
//! - [`restore_sessions`]: put back the sessions of a visit that is being voided (in-transaction);
//! - [`cancel_for_sale`]: cancel the still-unused packages of a sale that is being voided
//!   (in-transaction);
//! - [`extend`] (`POST /customer-packages/{id}/extend`) and [`cancel`]
//!   (`POST /customer-packages/{id}/cancel`, with an optional refund).
//!
//! Lock order, shared with sales and visits: visit/sale row → package rows (by id) → item rows
//! (by id). A concurrent `consume_session` only locks the item row and re-checks
//! `sessions_remaining >= qty`, which a cancellation sets to 0, so it can never take a session
//! from a package cancelled under it.

use crate::domain::RefundMethod;
use crate::http::{CustomerPackageDto, RefundMethodDto};
use crate::repo::{self, ExtensionInsert, RefundInsert};
use crate::service;
use chrono::NaiveDate;
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx, corrections};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

/// Sessions given back to one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Restored {
    pub customer_package_id: Uuid,
    pub sessions_remaining: i32,
    pub sessions_total: i32,
}

/// Re-credits `qty` sessions to an item (a visit that took them is being voided, AC6.1) with an
/// atomic `remaining + qty WHERE remaining + qty <= total`, inside the caller's transaction.
///
/// Errors: the package was cancelled since (e.g. refunded) → `409 package.cancelled`; the item
/// would exceed its total → `409 package.restore_exceeds_total` (never expected: it means the
/// sessions were already given back).
pub async fn restore_sessions(
    tx: &mut TenantTx,
    item_id: Uuid,
    qty: u32,
) -> Result<Restored, AppError> {
    let qty = i32::try_from(qty)
        .ok()
        .filter(|q| *q > 0)
        .ok_or_else(|| AppError::Internal(format!("restore qty {qty}")))?;
    let package = repo::lock_package_of_item(tx, item_id)
        .await?
        .ok_or_else(|| AppError::Internal(format!("package item {item_id} vanished")))?;
    if package.cancelled {
        return Err(AppError::Conflict {
            code: "package.cancelled",
        });
    }
    let row = repo::restore(tx, item_id, qty)
        .await?
        .ok_or(AppError::Conflict {
            code: "package.restore_exceeds_total",
        })?;
    Ok(Restored {
        customer_package_id: package.customer_package_id,
        sessions_remaining: row.sessions_remaining,
        sessions_total: row.sessions_total,
    })
}

/// Cancels every package created by `sale_id`, which is being voided (AC6.4), inside the
/// caller's transaction. Allowed only while no session of them was used: otherwise
/// `409 sale.has_used_sessions` (cancel the package with a refund instead, US7). A package
/// already cancelled (and maybe refunded) → `409 package.cancelled`, so money is never returned
/// twice. Returns the cancelled package ids.
pub async fn cancel_for_sale(
    tx: &mut TenantTx,
    actor: &Actor,
    sale_id: Uuid,
    reason: &str,
) -> Result<Vec<Uuid>, AppError> {
    let packages = repo::lock_packages_of_sale(tx, sale_id).await?;
    if packages.iter().any(|p| p.cancelled) {
        return Err(AppError::Conflict {
            code: "package.cancelled",
        });
    }
    let ids: Vec<Uuid> = packages.iter().map(|p| p.id).collect();
    let items = repo::lock_items(tx, &ids).await?;
    if items
        .iter()
        .any(|i| i.sessions_remaining < i.sessions_total)
    {
        return Err(AppError::Conflict {
            code: "sale.has_used_sessions",
        });
    }
    for id in &ids {
        repo::cancel_package(tx, *id, reason, actor.user_id.as_uuid()).await?;
        audit::record(
            tx,
            actor,
            AuditEntry {
                action: "customer_package.cancel",
                entity_type: "customer_package",
                entity_id: *id,
                before: None,
                after: Some(
                    json!({"via": "sale.void", "sale_id": sale_id, "sessions_remaining": 0}),
                ),
                reason: Some(reason),
            },
        )
        .await?;
    }
    Ok(ids)
}

// ---------------------------------------------------------------- endpoints

/// Body of `POST /customer-packages/{id}/extend` (with `If-Match`).
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageExtend {
    /// The new last day of validity: later than the current one and not in the past.
    pub expires_on: NaiveDate,
    /// Why (required, 1–500 characters); audited.
    pub reason: Option<String>,
}

/// Body of `POST /customer-packages/{id}/cancel`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PackageCancel {
    /// Why (required, 1–500 characters); audited.
    pub reason: Option<String>,
    /// đồng paid back now, 0 = no refund. At most what the customer paid for this package
    /// (`sold_price`), else `422 refund.exceeds_paid`.
    #[serde(default)]
    pub refund_amount: i64,
    /// Required when `refund_amount` > 0.
    pub refund_method: Option<RefundMethodDto>,
}

fn to_body(package: service::CustomerPackage) -> Result<Value, AppError> {
    serde_json::to_value(CustomerPackageDto::from(package))
        .map_err(|e| AppError::Internal(e.to_string()))
}

/// `POST /customer-packages/{id}/extend` (`package.extend_expiry`, If-Match, Idempotency-Key):
/// a new expiry with a reason (AC7.1). An expired package with sessions left becomes active again.
pub async fn extend(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    id: Uuid,
    version: i64,
    input: &PackageExtend,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::PackageExtendExpiry)?;
    let reason = corrections::reason(input.reason.as_deref())?;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let package = repo::lock_package(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if package.cancelled {
        return Err(AppError::Conflict {
            code: "package.cancelled",
        });
    }
    if package.version != version {
        return Err(AppError::StaleVersion);
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let field = |code| AppError::Validation(vec![FieldError::new("expires_on", code)]);
    let Some(current) = package.expires_on else {
        return Err(field("package_has_no_expiry"));
    };
    if input.expires_on <= current {
        return Err(field("not_later"));
    }
    if input.expires_on < today {
        return Err(field("in_past"));
    }
    if !repo::set_expiry(&mut tx, id, version, input.expires_on).await? {
        return Err(AppError::StaleVersion);
    }
    repo::insert_extension(
        &mut tx,
        &ExtensionInsert {
            customer_package_id: id,
            previous_expires_on: current,
            expires_on: input.expires_on,
            reason: &reason,
            business_date: today,
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "customer_package.extend",
            entity_type: "customer_package",
            entity_id: id,
            before: Some(json!({"expires_on": current, "version": version})),
            after: Some(json!({"expires_on": input.expires_on, "version": version + 1})),
            reason: Some(&reason),
        },
    )
    .await?;
    let body = to_body(
        service::load(&mut tx, id, today)
            .await?
            .ok_or_else(|| AppError::Internal("package vanished".into()))?,
    )?;
    idempotency::complete(&mut tx, actor, key, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

struct CancelOrder {
    reason: String,
    refund: Option<(Money, RefundMethod)>,
}

fn parse_cancel(input: &PackageCancel) -> Result<CancelOrder, AppError> {
    let reason = corrections::reason(input.reason.as_deref())?;
    let amount = Money::vnd(input.refund_amount).map_err(|_| {
        AppError::Validation(vec![FieldError::new("refund_amount", "out_of_range")])
    })?;
    let refund = match (amount > Money::ZERO, input.refund_method) {
        (true, Some(m)) => Some((amount, m.into())),
        (true, None) => {
            return Err(AppError::Validation(vec![FieldError::new(
                "refund_method",
                "required",
            )]));
        }
        (false, Some(_)) => {
            return Err(AppError::Validation(vec![FieldError::new(
                "refund_method",
                "not_allowed",
            )]));
        }
        (false, None) => None,
    };
    Ok(CancelOrder { reason, refund })
}

/// `POST /customer-packages/{id}/cancel` (`package.cancel_refund`, owner; Idempotency-Key):
/// cancels the package — remaining sessions → 0, status `cancelled` — and optionally refunds up
/// to what the customer paid for it (AC7.2, AC7.4). The refund is an append-only `refund` row in
/// today's cash book; nothing already recorded is changed.
pub async fn cancel(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    id: Uuid,
    input: &PackageCancel,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::PackageCancelRefund)?;
    let order = parse_cancel(input)?;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let package = repo::lock_package(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if package.cancelled {
        return Err(AppError::Conflict {
            code: "package.cancelled",
        });
    }
    // R1 always collects in full, so what the customer paid for a live package is its sold price
    // (after discount); an opening balance carries the price paid in the paper books. A tighter
    // cap for paper-book packages is deferred (EM decision 2026-11-05).
    let paid = package.sold_price;
    if let Some((amount, _)) = order.refund
        && amount.as_i64() > paid
    {
        return Err(AppError::Unprocessable {
            code: "refund.exceeds_paid",
        });
    }
    let tenant = kernel::tenant::profile(&mut tx).await?;
    let today = tenant.business_date.as_naive();
    let before = repo::lock_items(&mut tx, &[id]).await?;
    let remaining_before: i64 = before.iter().map(|i| i64::from(i.sessions_remaining)).sum();
    repo::cancel_package(&mut tx, id, &order.reason, actor.user_id.as_uuid()).await?;
    if let Some((amount, method)) = order.refund {
        repo::insert_refund(
            &mut tx,
            &RefundInsert {
                branch_id: tenant.branch.as_uuid(),
                customer_id: package.customer_id,
                customer_package_id: id,
                business_date: today,
                method: method.as_str(),
                amount: amount.as_i64(),
                reason: &order.reason,
                created_by: actor.user_id.as_uuid(),
            },
        )
        .await?;
    }
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "customer_package.cancel",
            entity_type: "customer_package",
            entity_id: id,
            before: Some(json!({"sessions_remaining": remaining_before, "version": package.version})),
            after: Some(json!({
                "sessions_remaining": 0, "paid": paid,
                "refund": order.refund.map(|(a, m)| json!({"amount": a.as_i64(), "method": m.as_str()})),
            })),
            reason: Some(&order.reason),
        },
    )
    .await?;
    let body = to_body(
        service::load(&mut tx, id, today)
            .await?
            .ok_or_else(|| AppError::Internal("package vanished".into()))?,
    )?;
    idempotency::complete(&mut tx, actor, key, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(amount: i64, method: Option<RefundMethodDto>, reason: Option<&str>) -> PackageCancel {
        PackageCancel {
            reason: reason.map(str::to_owned),
            refund_amount: amount,
            refund_method: method,
        }
    }

    fn field(r: Result<CancelOrder, AppError>) -> Option<String> {
        match r {
            Err(AppError::Validation(e)) => e.first().map(|f| format!("{}:{}", f.field, f.code)),
            _ => None,
        }
    }

    #[test]
    fn cancel_needs_a_reason_and_a_method_for_any_refund() {
        let cash = Some(RefundMethodDto::Cash);
        assert_eq!(
            field(parse_cancel(&input(0, None, None))).as_deref(),
            Some("reason:required")
        );
        assert_eq!(
            field(parse_cancel(&input(
                300_000,
                None,
                Some("khách chuyển nhà")
            )))
            .as_deref(),
            Some("refund_method:required")
        );
        assert_eq!(
            field(parse_cancel(&input(0, cash, Some("x")))).as_deref(),
            Some("refund_method:not_allowed")
        );
        assert_eq!(
            field(parse_cancel(&input(-1, cash, Some("x")))).as_deref(),
            Some("refund_amount:out_of_range")
        );
        let ok = parse_cancel(&input(300_000, cash, Some(" khách chuyển nhà ")));
        assert!(matches!(
            ok,
            Ok(CancelOrder { ref reason, refund: Some((a, RefundMethod::Cash)) })
                if reason == "khách chuyển nhà" && a.as_i64() == 300_000
        ));
        assert!(matches!(
            parse_cancel(&input(0, None, Some("x"))),
            Ok(CancelOrder { refund: None, .. })
        ));
    }
}
