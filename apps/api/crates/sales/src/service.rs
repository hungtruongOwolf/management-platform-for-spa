//! Sell packages (US3). One transaction creates the sale, its lines, the customer packages with
//! their items, the payments, the audit entry and the idempotency record — or nothing at all.
//! Visits (S7) record their one-off services through [`record_service_sale`] in their own transaction.

use crate::domain::{self, DiscountAuthority, DiscountError, PaymentMethod};
use crate::dto::{
    PaymentCreate, PaymentDto, PaymentKindDto, PaymentMethodDto, SaleCreate, SaleDto, SaleLineDto,
    SaleLineTypeDto, SaleStatusDto,
};
use crate::repo::{self, PackageLineInsert, PaymentInsert, SaleInsert, ServiceLineInsert};
use chrono::{DateTime, Months, NaiveDate, Utc};
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx};
use packages::http::CustomerPackageDto;
use packages::{NewItem, NewPackage, Origin};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const MAX_LINES: usize = 10;
pub const MAX_PAYMENTS: usize = 2;
const MAX_REASON: usize = 500;
const MAX_REFERENCE: usize = 200;
const MAX_GIFT_NOTE: usize = 500;

/// One validated payment of a sale (see [`parse_payments`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payment {
    pub method: PaymentMethod,
    pub amount: Money,
    pub reference: Option<String>,
}

struct Line {
    plan_id: Uuid,
    /// `None` = take the plan's; `Some(None)` = explicitly no gift.
    gift_note: Option<Option<String>>,
}

/// The request after shape validation; amounts are still unchecked against the catalog.
struct Order {
    customer_id: Uuid,
    closer_staff_id: Uuid,
    lines: Vec<Line>,
    discount: Money,
    discount_reason: Option<String>,
    payments: Vec<Payment>,
}

fn trimmed(s: Option<&String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

fn parse(input: &SaleCreate) -> Result<Order, AppError> {
    let mut errors = Vec::new();
    if input.lines.is_empty() || input.lines.len() > MAX_LINES {
        errors.push(FieldError::new("lines", "count_out_of_range"));
    }
    let lines = input
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let gift = l.gift_note.as_ref().map(|g| trimmed(Some(g)));
            if gift
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|g| g.chars().count() > MAX_GIFT_NOTE)
            {
                errors.push(FieldError::new(format!("lines[{i}].gift_note"), "too_long"));
            }
            Line {
                plan_id: l.package_plan_id,
                gift_note: gift,
            }
        })
        .collect();

    let (discount, discount_reason) = match &input.discount {
        None => (Money::ZERO, None),
        Some(d) => {
            let amount = Money::vnd(d.amount).unwrap_or_else(|_| {
                errors.push(FieldError::new("discount.amount", "negative"));
                Money::ZERO
            });
            let reason = trimmed(d.reason.as_ref());
            if reason
                .as_ref()
                .is_some_and(|r| r.chars().count() > MAX_REASON)
            {
                errors.push(FieldError::new("discount.reason", "too_long"));
            }
            // A reason without a discount means nothing; drop it rather than store noise.
            (amount, reason.filter(|_| amount > Money::ZERO))
        }
    };

    let payments = parse_payments(&input.payments, &mut errors);

    if errors.is_empty() {
        Ok(Order {
            customer_id: input.customer_id,
            closer_staff_id: input.closer_staff_id,
            lines,
            discount,
            discount_reason,
            payments,
        })
    } else {
        Err(AppError::Validation(errors))
    }
}

/// Shape rules for `payments[]` shared by package sales and visits: 0–2 entries, one per method,
/// each amount > 0, short reference. Problems are appended to `errors`; whether the payments add up
/// to what is due is checked later, against the server-computed total.
pub fn parse_payments(input: &[PaymentCreate], errors: &mut Vec<FieldError>) -> Vec<Payment> {
    if input.len() > MAX_PAYMENTS {
        errors.push(FieldError::new("payments", "count_out_of_range"));
    }
    let mut methods = HashSet::new();
    input
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let method = match p.method {
                PaymentMethodDto::Cash => PaymentMethod::Cash,
                PaymentMethodDto::BankTransfer => PaymentMethod::BankTransfer,
            };
            if !methods.insert(method) {
                errors.push(FieldError::new(
                    format!("payments[{i}].method"),
                    "duplicate",
                ));
            }
            let amount = match Money::vnd(p.amount) {
                Ok(a) if a > Money::ZERO => a,
                _ => {
                    errors.push(FieldError::new(
                        format!("payments[{i}].amount"),
                        "must_be_positive",
                    ));
                    Money::ZERO
                }
            };
            let reference = trimmed(p.reference.as_ref());
            if reference
                .as_ref()
                .is_some_and(|r| r.chars().count() > MAX_REFERENCE)
            {
                errors.push(FieldError::new(
                    format!("payments[{i}].reference"),
                    "too_long",
                ));
            }
            Payment {
                method,
                amount,
                reference,
            }
        })
        .collect()
}

fn authority(actor: &Actor, cap_pct: u32) -> DiscountAuthority {
    if actor.can(Permission::SaleDiscountUnlimited) {
        DiscountAuthority::Unlimited
    } else if actor.can(Permission::SaleDiscount) {
        DiscountAuthority::UpToPercent(cap_pct)
    } else {
        DiscountAuthority::None
    }
}

fn money_err(_: kernel::money::MoneyError) -> AppError {
    AppError::Validation(vec![FieldError::new("total", "out_of_range")])
}

fn expiry(
    starts_on: NaiveDate,
    validity_months: Option<i32>,
) -> Result<Option<NaiveDate>, AppError> {
    validity_months
        .map(|m| {
            u32::try_from(m)
                .ok()
                .and_then(|m| starts_on.checked_add_months(Months::new(m)))
                .ok_or_else(|| AppError::Internal(format!("validity of {m} months")))
        })
        .transpose()
}

fn session_count(v: i32) -> Result<u32, AppError> {
    u32::try_from(v).map_err(|_| AppError::Internal("negative session count in plan".into()))
}

/// `POST /sales` (`package.sell`, plus `sale.discount` / `sale.discount_unlimited` for a discount).
/// Returns the HTTP status and body to send, which are also stored under the idempotency key:
/// retrying with the same key and body replays them without creating anything (AC3.7).
pub async fn create_sale(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &SaleCreate,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::PackageSell)?;
    let order = parse(input)?;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let tenant = kernel::tenant::profile(&mut tx).await?;
    let today = tenant.business_date.as_naive();

    if !crm::service::exists_in(&mut tx, order.customer_id).await? {
        return Err(AppError::NotFound);
    }
    let closer = org::service::require_active(&mut tx, order.closer_staff_id).await?;

    // Every amount comes from the catalog, read and locked inside this transaction.
    let mut plans = Vec::with_capacity(order.lines.len());
    for (i, line) in order.lines.iter().enumerate() {
        let plan = catalog::service::plan_snapshot(&mut tx, line.plan_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if !plan.active || plan.items.is_empty() {
            return Err(AppError::Validation(vec![FieldError::new(
                format!("lines[{i}].package_plan_id"),
                "inactive",
            )]));
        }
        plans.push(plan);
    }
    let line_totals: Vec<Money> = plans.iter().map(|p| p.price).collect();
    let subtotal = Money::sum(line_totals.iter().copied()).map_err(money_err)?;

    if order.discount > subtotal {
        return Err(AppError::Validation(vec![FieldError::new(
            "discount.amount",
            "exceeds_subtotal",
        )]));
    }
    if order.discount > Money::ZERO && order.discount_reason.is_none() {
        return Err(AppError::Unprocessable {
            code: "discount.reason_required",
        });
    }
    domain::check_discount(
        subtotal,
        order.discount,
        authority(actor, tenant.manager_discount_max_pct),
    )
    .map_err(|e| match e {
        DiscountError::NotAllowed => AppError::forbidden(),
        DiscountError::LimitExceeded => AppError::Forbidden {
            code: "discount.limit_exceeded",
        },
    })?;
    let total = subtotal.checked_sub(order.discount).map_err(money_err)?;
    let paid = Money::sum(order.payments.iter().map(|p| p.amount)).map_err(money_err)?;
    if paid != total {
        return Err(AppError::Unprocessable {
            code: "payment.amount_mismatch",
        });
    }

    let sale_id = Uuid::now_v7();
    let branch = tenant.branch.as_uuid();
    repo::insert_sale(
        &mut tx,
        &SaleInsert {
            id: sale_id,
            branch_id: branch,
            customer_id: order.customer_id,
            business_date: today,
            subtotal: subtotal.as_i64(),
            discount_amount: order.discount.as_i64(),
            discount_reason: order.discount_reason.as_deref(),
            total: total.as_i64(),
            amount_paid: paid.as_i64(),
            closer_staff_id: Some(closer.id),
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;

    let shares = domain::allocate_discount(&line_totals, order.discount);
    for (position, ((plan, line), share)) in
        (0_i32..).zip(plans.iter().zip(&order.lines).zip(shares))
    {
        let items = plan
            .items
            .iter()
            .map(|i| {
                let purchased = session_count(i.sessions)?;
                let bonus = session_count(i.bonus_sessions)?;
                Ok(NewItem {
                    service_id: i.service_id,
                    name: i.service_name.clone(),
                    sessions_purchased: purchased,
                    sessions_bonus: bonus,
                    sessions_remaining: purchased + bonus,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let package = packages::service::create(
            &mut tx,
            actor,
            &NewPackage {
                customer_id: order.customer_id,
                plan_id: Some(plan.id),
                name: plan.name.clone(),
                gift_note: line
                    .gift_note
                    .clone()
                    .unwrap_or_else(|| plan.gift_note.clone()),
                sold_price: plan.price.checked_sub(share).map_err(money_err)?,
                origin: Origin::Sale {
                    sale_id,
                    closer_staff_id: closer.id,
                },
                starts_on: today,
                expires_on: expiry(today, plan.validity_months)?,
                items,
            },
        )
        .await?;
        repo::insert_package_line(
            &mut tx,
            &PackageLineInsert {
                sale_id,
                position,
                package_plan_id: plan.id,
                customer_package_id: package.id,
                description: &plan.name,
                unit_price: plan.price.as_i64(),
            },
        )
        .await?;
    }
    for p in &order.payments {
        repo::insert_payment(
            &mut tx,
            &PaymentInsert {
                branch_id: branch,
                sale_id,
                business_date: today,
                method: p.method.as_str(),
                amount: p.amount.as_i64(),
                reference: p.reference.as_deref(),
                created_by: actor.user_id.as_uuid(),
            },
        )
        .await?;
    }
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "sale.create",
            entity_type: "sale",
            entity_id: sale_id,
            before: None,
            after: Some(json!({
                "customer_id": order.customer_id, "closer_staff_id": closer.id,
                "plans": plans.iter().map(|p| json!({"plan_id": p.id, "price": p.price.as_i64()})).collect::<Vec<_>>(),
                "subtotal": subtotal.as_i64(), "discount_amount": order.discount.as_i64(), "total": total.as_i64(),
                "payments": order.payments.iter().map(|p| json!({"method": p.method.as_str(), "amount": p.amount.as_i64()})).collect::<Vec<_>>(),
            })),
            reason: order.discount_reason.as_deref(),
        },
    )
    .await?;

    let dto = load(&mut tx, sale_id, today)
        .await?
        .ok_or_else(|| AppError::Internal("sale vanished".into()))?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

async fn load(tx: &mut TenantTx, id: Uuid, today: NaiveDate) -> Result<Option<SaleDto>, AppError> {
    let Some(s) = repo::sale(tx, id).await? else {
        return Ok(None);
    };
    let lines = repo::lines(tx, id)
        .await?
        .into_iter()
        .map(|l| SaleLineDto {
            id: l.id,
            line_type: if l.line_type == "service" {
                SaleLineTypeDto::Service
            } else {
                SaleLineTypeDto::Package
            },
            package_plan_id: l.package_plan_id,
            customer_package_id: l.customer_package_id,
            service_id: l.service_id,
            description: l.description_snapshot,
            qty: l.qty,
            unit_price: l.unit_price,
            line_total: l.line_total,
        })
        .collect();
    let payments = repo::payments(tx, id)
        .await?
        .into_iter()
        .map(|p| payment_dto(p.id, &p.kind, &p.method, p.amount, p.reference, p.paid_at))
        .collect::<Result<Vec<_>, AppError>>()?;
    let customer_packages = packages::service::of_sale(tx, id, today)
        .await?
        .into_iter()
        .map(CustomerPackageDto::from)
        .collect();
    Ok(Some(SaleDto {
        id: s.id,
        customer_id: s.customer_id,
        business_date: s.business_date,
        subtotal: s.subtotal,
        discount_amount: s.discount_amount,
        discount_reason: s.discount_reason,
        total: s.total,
        amount_paid: s.amount_paid,
        status: if s.status == "voided" {
            SaleStatusDto::Voided
        } else {
            SaleStatusDto::Open
        },
        closer_staff_id: s.closer_staff_id,
        closer_name: s.closer_name,
        created_at: s.created_at,
        lines,
        payments,
        customer_packages,
    }))
}

/// `GET /sales/{id}` (`sale.read`). Another spa's sale is `404`.
pub async fn get_sale(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<SaleDto, AppError> {
    actor.require(Permission::SaleRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let sale = load(&mut tx, id, today).await?;
    tx.commit().await?;
    sale.ok_or(AppError::NotFound)
}

fn payment_dto(
    id: Uuid,
    kind: &str,
    method: &str,
    amount: i64,
    reference: Option<String>,
    paid_at: DateTime<Utc>,
) -> Result<PaymentDto, AppError> {
    let method = match PaymentMethod::parse(method) {
        Some(PaymentMethod::Cash) => PaymentMethodDto::Cash,
        Some(PaymentMethod::BankTransfer) => PaymentMethodDto::BankTransfer,
        None => return Err(AppError::Internal(format!("payment method {method}"))),
    };
    Ok(PaymentDto {
        id,
        kind: if kind == "reversal" {
            PaymentKindDto::Reversal
        } else {
            PaymentKindDto::Payment
        },
        method,
        amount,
        reference,
        paid_at,
    })
}

// ---------------------------------------------------------------- one-off services (S7 visits)

/// A one-off service line priced by the caller from the catalog, inside the same transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLine {
    pub service_id: Uuid,
    /// Service name at this moment (snapshot).
    pub description: String,
    pub qty: u32,
    pub unit_price: Money,
}

/// A sale of one-off services, e.g. the paid part of a visit. Always collected in full (R1).
#[derive(Debug, Clone, Copy)]
pub struct ServiceSale<'a> {
    pub customer_id: Uuid,
    pub business_date: NaiveDate,
    pub branch_id: Uuid,
    pub lines: &'a [ServiceLine],
    pub payments: &'a [Payment],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceSaleRecorded {
    pub sale_id: Uuid,
    pub total: Money,
}

/// Records the sale of one-off services inside the caller's transaction: sale, `service` lines,
/// append-only payments and an audit entry. Payments must add up exactly to the server-computed
/// total (`422 payment.amount_mismatch`); taking money needs `payment.collect`.
/// No lines → no sale (`Ok(None)`), and then any payment is a mismatch.
pub async fn record_service_sale(
    tx: &mut TenantTx,
    actor: &Actor,
    sale: ServiceSale<'_>,
) -> Result<Option<ServiceSaleRecorded>, AppError> {
    let paid = Money::sum(sale.payments.iter().map(|p| p.amount)).map_err(money_err)?;
    let line_totals = sale
        .lines
        .iter()
        .map(|l| l.unit_price.checked_mul(l.qty))
        .collect::<Result<Vec<_>, _>>()
        .map_err(money_err)?;
    let total = Money::sum(line_totals.iter().copied()).map_err(money_err)?;
    if paid != total {
        return Err(AppError::Unprocessable {
            code: "payment.amount_mismatch",
        });
    }
    if sale.lines.is_empty() {
        return Ok(None);
    }
    if !sale.payments.is_empty() {
        actor.require(Permission::PaymentCollect)?;
    }

    let sale_id = Uuid::now_v7();
    repo::insert_sale(
        tx,
        &SaleInsert {
            id: sale_id,
            branch_id: sale.branch_id,
            customer_id: sale.customer_id,
            business_date: sale.business_date,
            subtotal: total.as_i64(),
            discount_amount: 0,
            discount_reason: None,
            total: total.as_i64(),
            amount_paid: paid.as_i64(),
            closer_staff_id: None,
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    for (position, (line, line_total)) in (0_i32..).zip(sale.lines.iter().zip(&line_totals)) {
        repo::insert_service_line(
            tx,
            &ServiceLineInsert {
                sale_id,
                position,
                service_id: line.service_id,
                description: &line.description,
                qty: i32::try_from(line.qty)
                    .map_err(|_| AppError::Internal("qty overflow".into()))?,
                unit_price: line.unit_price.as_i64(),
                line_total: line_total.as_i64(),
            },
        )
        .await?;
    }
    for p in sale.payments {
        repo::insert_payment(
            tx,
            &PaymentInsert {
                branch_id: sale.branch_id,
                sale_id,
                business_date: sale.business_date,
                method: p.method.as_str(),
                amount: p.amount.as_i64(),
                reference: p.reference.as_deref(),
                created_by: actor.user_id.as_uuid(),
            },
        )
        .await?;
    }
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "sale.create",
            entity_type: "sale",
            entity_id: sale_id,
            before: None,
            after: Some(json!({
                "customer_id": sale.customer_id, "kind": "services",
                "lines": sale.lines.iter().map(|l| json!({
                    "service_id": l.service_id, "qty": l.qty, "unit_price": l.unit_price.as_i64(),
                })).collect::<Vec<_>>(),
                "total": total.as_i64(),
                "payments": sale.payments.iter().map(|p| json!({"method": p.method.as_str(), "amount": p.amount.as_i64()})).collect::<Vec<_>>(),
            })),
            reason: None,
        },
    )
    .await?;
    Ok(Some(ServiceSaleRecorded { sale_id, total }))
}

/// Payments of several sales, read inside the caller's transaction (`sale_id → payments`, oldest
/// first). The caller authorizes.
pub async fn payments_of(
    tx: &mut TenantTx,
    sale_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<PaymentDto>>, AppError> {
    let mut out: HashMap<Uuid, Vec<PaymentDto>> = HashMap::new();
    if sale_ids.is_empty() {
        return Ok(out);
    }
    for p in repo::payments_of(tx, sale_ids).await? {
        out.entry(p.sale_id).or_default().push(payment_dto(
            p.id,
            &p.kind,
            &p.method,
            p.amount,
            p.reference,
            p.paid_at,
        )?);
    }
    Ok(out)
}
