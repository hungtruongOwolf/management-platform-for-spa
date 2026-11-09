//! Money paid on paper, booked as ordinary sales (EM 2026-11-05: "cứ tính là tiền"). The app
//! only records money; paper money is the same kind of record, so a paper-book package's paid
//! amount becomes a sale + payment on the **paper date**. From then on it is revenue of that day
//! like any other sale: the ledger, the daily / range reports, the customer's history and
//! "Đã chi" all include it, and the UI cannot tell it apart.
//!
//! The sale carries `source = 'paper_book'` (never in a DTO) for audit and re-run idempotency:
//! a package is tied to at most one sale (`customer_package.sale_id`, trigger 0018), so booking
//! again is a no-op. Back-dating is what the owner's migration does; only holders of
//! `opening_balance.import` (the owner) reach it.

use chrono::{TimeZone, Utc};
use crm::FieldCipher;
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, Permission, TenantTx};
use packages::opening_balance::{self, OpeningBalanceCreate, PaperPayment};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::repo;

/// Books `payment` as the sale of paper-book package `package_id`, inside the caller's
/// transaction. `Ok(None)` = the package already has its sale (nothing booked twice).
///
/// Errors: unknown package → `404`; an app-sold package → `409 package.not_paper_book`; a
/// cancelled one → `409 package.cancelled`; `paid_on` after today → `422 paid_on in_future`.
pub async fn book_in(
    tx: &mut TenantTx,
    actor: &Actor,
    package_id: Uuid,
    payment: &PaperPayment,
) -> Result<Option<Uuid>, AppError> {
    actor.require(Permission::OpeningBalanceImport)?;
    actor.require(Permission::PaymentCollect)?;
    let package = packages::service::lock_paper_package(tx, package_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !package.from_paper_book {
        return Err(AppError::Conflict {
            code: "package.not_paper_book",
        });
    }
    if package.sale_id.is_some() {
        return Ok(None);
    }
    if package.cancelled {
        return Err(AppError::Conflict {
            code: "package.cancelled",
        });
    }
    let profile = kernel::tenant::profile(tx).await?;
    if payment.paid_on > profile.business_date.as_naive() {
        return Err(AppError::Validation(vec![kernel::FieldError::new(
            "paid_on",
            "in_future",
        )]));
    }
    // Noon of the paper day in the spa's time zone: the record's instants fall on its day.
    let at = profile
        .timezone
        .from_local_datetime(&payment.paid_on.and_hms_opt(12, 0, 0).unwrap_or_default())
        .earliest()
        .map_or_else(Utc::now, |t| t.with_timezone(&Utc));
    let amount = payment.amount.as_i64();
    let sale_id = Uuid::now_v7();
    repo::insert_paper_sale(
        tx,
        &repo::PaperSaleInsert {
            id: sale_id,
            branch_id: profile.branch.as_uuid(),
            customer_id: package.customer_id,
            business_date: payment.paid_on,
            amount,
            plan_id: package.plan_id,
            customer_package_id: package_id,
            description: &package.name,
            method: payment.method.as_str(),
            at,
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    if !packages::service::attach_paper_sale(tx, package_id, sale_id).await? {
        return Err(AppError::Conflict {
            code: "package.not_paper_book",
        });
    }
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "sale.paper_book",
            entity_type: "sale",
            entity_id: sale_id,
            before: None,
            after: Some(json!({
                "source": "paper_book", "customer_package_id": package_id,
                "business_date": payment.paid_on, "amount": amount,
                "method": payment.method.as_str(),
            })),
            reason: None,
        },
    )
    .await?;
    Ok(Some(sale_id))
}

/// Books the paper money of an already imported package in its own transaction (the CLI's
/// "add paid amount" run). `Ok(None)` = it already had its sale.
pub async fn book(
    pool: &PgPool,
    actor: &Actor,
    package_id: Uuid,
    payment: &PaperPayment,
) -> Result<Option<Uuid>, AppError> {
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let booked = book_in(&mut tx, actor, package_id, payment).await?;
    tx.commit().await?;
    Ok(booked)
}

/// `POST /opening-balances` and `app import-paper-book`: the opening-balance use case, then the
/// paper money of each package, in one transaction under one Idempotency-Key. Returns the HTTP
/// status and body (also stored for replays).
pub async fn import_opening_balance(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &OpeningBalanceCreate,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::OpeningBalanceImport)?;
    if input.packages.iter().any(|p| p.paid_amount != 0) {
        actor.require(Permission::PaymentCollect)?;
    }
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let imported = opening_balance::import_in(&mut tx, cipher, actor, input, today).await?;
    for (package_id, payment) in &imported.packages {
        if let Some(payment) = payment {
            book_in(&mut tx, actor, *package_id, payment).await?;
        }
    }
    let dto = opening_balance::dto(&mut tx, &imported, today).await?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}
