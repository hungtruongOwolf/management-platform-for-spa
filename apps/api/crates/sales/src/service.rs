//! Sell packages (US3). One transaction creates the sale, its lines, the customer packages with
//! their items, the payments, the audit entry and the idempotency record — or nothing at all.
//! Visits (S7) record their one-off services through [`record_service_sale`] in their own transaction.

use crate::domain::{self, DiscountAuthority, DiscountError, PaymentMethod};
use crate::dto::{
    DaySaleDto, DiscountCreate, PaymentCreate, PaymentDto, PaymentKindDto, PaymentMethodDto,
    SaleCheckoutCreate, SaleCheckoutDto, SaleCreate, SaleDto, SaleLineDto, SaleLineTypeDto,
    SaleStatusDto,
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

/// One package of a sale, after shape validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLine {
    pub plan_id: Uuid,
    /// `None` = take the plan's; `Some(None)` = explicitly no gift.
    pub gift_note: Option<Option<String>>,
}

/// A package sale after shape validation; amounts are still unchecked against the catalog.
/// The one shape of `POST /sales` and of a package sold in a visit (`package_sales[]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageOrder {
    /// Who pays (`sale.customer_id`).
    pub customer_id: Uuid,
    /// Who the packages belong to (`customer_package.customer_id`); default = the payer (US13).
    pub beneficiary_customer_id: Option<Uuid>,
    /// "Người giới thiệu / chốt": any active staff member (owner, manager, technician), or `None`
    /// = "khách tự chọn" (nobody is credited).
    pub closer_staff_id: Option<Uuid>,
    pub lines: Vec<PackageLine>,
    pub discount: Money,
    pub discount_reason: Option<String>,
}

struct Order {
    package: PackageOrder,
    payments: Vec<Payment>,
}

fn trimmed(s: Option<&String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// Shape rules of a gift note (`None` = the plan's, empty = none). Problems go to `errors`.
pub fn parse_gift_note(
    note: Option<&String>,
    field: &str,
    errors: &mut Vec<FieldError>,
) -> Option<Option<String>> {
    let gift = note.map(|g| trimmed(Some(g)));
    if gift
        .as_ref()
        .and_then(Option::as_ref)
        .is_some_and(|g| g.chars().count() > MAX_GIFT_NOTE)
    {
        errors.push(FieldError::new(field, "too_long"));
    }
    gift
}

/// Shape rules of a package discount (`amount` ≥ 0, reason ≤ 500; a reason without an amount is
/// dropped). `prefix` is the field path, e.g. `discount` or `package_sales[0].discount`.
pub fn parse_package_discount(
    input: Option<&DiscountCreate>,
    prefix: &str,
    errors: &mut Vec<FieldError>,
) -> (Money, Option<String>) {
    match input {
        None => (Money::ZERO, None),
        Some(d) => {
            let amount = Money::vnd(d.amount).unwrap_or_else(|_| {
                errors.push(FieldError::new(format!("{prefix}.amount"), "negative"));
                Money::ZERO
            });
            let reason = trimmed(d.reason.as_ref());
            if reason
                .as_ref()
                .is_some_and(|r| r.chars().count() > MAX_REASON)
            {
                errors.push(FieldError::new(format!("{prefix}.reason"), "too_long"));
            }
            // A reason without a discount means nothing; drop it rather than store noise.
            (amount, reason.filter(|_| amount > Money::ZERO))
        }
    }
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
        .map(|(i, l)| PackageLine {
            plan_id: l.package_plan_id,
            gift_note: parse_gift_note(
                l.gift_note.as_ref(),
                &format!("lines[{i}].gift_note"),
                &mut errors,
            ),
        })
        .collect();
    let (discount, discount_reason) =
        parse_package_discount(input.discount.as_ref(), "discount", &mut errors);
    let payments = parse_payments(&input.payments, &mut errors);

    if errors.is_empty() {
        Ok(Order {
            package: PackageOrder {
                customer_id: input.customer_id,
                beneficiary_customer_id: input.beneficiary_customer_id,
                closer_staff_id: input.closer_staff_id,
                lines,
                discount,
                discount_reason,
            },
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

/// `POST /sales` (`package.sell`, plus `payment.collect` when money is taken, as on `POST /visits`,
/// and `sale.discount` / `sale.discount_unlimited` for a discount).
/// Returns the HTTP status and body to send, which are also stored under the idempotency key:
/// retrying with the same key and body replays them without creating anything (AC3.7).
pub async fn create_sale(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &SaleCreate,
) -> Result<(u16, Value), AppError> {
    create_sale_as_of(pool, actor, key, request_hash, input, Utc::now()).await
}

/// [`create_sale`] on the business day of `now` instead of the server clock, with every other
/// rule unchanged. Operator tooling only (`app seed-demo` back-fills demo history); no route
/// reaches it — `POST /sales` calls [`create_sale`].
pub async fn create_sale_as_of(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &SaleCreate,
    now: DateTime<Utc>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::PackageSell)?;
    let order = parse(input)?;
    if !order.payments.is_empty() {
        actor.require(Permission::PaymentCollect)?;
    }

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let tenant = kernel::tenant::profile_as_of(&mut tx, now).await?;
    let today = tenant.business_date.as_naive();
    let priced = price_package_sale(
        &mut tx,
        actor,
        &order.package,
        tenant.manager_discount_max_pct,
    )
    .await?;
    let sold = record_package_sale(
        &mut tx,
        actor,
        &priced,
        SaleAt {
            business_date: today,
            branch_id: tenant.branch.as_uuid(),
            visit_id: None,
        },
        &order.payments,
    )
    .await?;

    let dto = load(&mut tx, sold.sale_id, today)
        .await?
        .ok_or_else(|| AppError::Internal("sale vanished".into()))?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// Most packages one checkout sells: the same cap as `package_sales[]` of a visit.
pub const MAX_CHECKOUT_PACKAGES: usize = 3;

/// Field errors of package `k` of a checkout name `package_sales[k].…` (as in a visit).
fn in_checkout_package(k: usize, e: AppError) -> AppError {
    match e {
        AppError::Validation(errors) => AppError::Validation(
            errors
                .into_iter()
                .map(|f| FieldError::new(format!("package_sales[{k}].{}", f.field), f.code))
                .collect(),
        ),
        other => other,
    }
}

/// Shape rules of `POST /sales/checkout`: 1–3 packages, each like a package of a visit.
fn parse_checkout(
    input: &SaleCheckoutCreate,
) -> Result<(Vec<PackageOrder>, Vec<Payment>), AppError> {
    let mut errors = Vec::new();
    if input.package_sales.is_empty() || input.package_sales.len() > MAX_CHECKOUT_PACKAGES {
        errors.push(FieldError::new("package_sales", "count_out_of_range"));
    }
    let orders = input
        .package_sales
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let prefix = format!("package_sales[{k}]");
            let (discount, discount_reason) = parse_package_discount(
                p.discount.as_ref(),
                &format!("{prefix}.discount"),
                &mut errors,
            );
            PackageOrder {
                customer_id: input.customer_id,
                beneficiary_customer_id: p.beneficiary_customer_id,
                closer_staff_id: p.closer_staff_id,
                lines: vec![PackageLine {
                    plan_id: p.package_plan_id,
                    gift_note: parse_gift_note(
                        p.gift_note.as_ref(),
                        &format!("{prefix}.gift_note"),
                        &mut errors,
                    ),
                }],
                discount,
                discount_reason,
            }
        })
        .collect();
    let payments = parse_payments(&input.payments, &mut errors);
    if errors.is_empty() {
        Ok((orders, payments))
    } else {
        Err(AppError::Validation(errors))
    }
}

/// `POST /sales/checkout` (`package.sell`; `payment.collect` when money is taken; Idempotency-Key):
/// a Quầy cart of packages only (EM 2026-11-07 item 4: chị Lan buys two packages, one for herself and one
/// for her mother → one save, one payment step). Every package is priced and checked like a package of a
/// visit (its own beneficiary, closer and discount, the manager cap per sale), the payments are
/// split over the sales by [`allocate_payments`] (cash first, in cart order), and every sale is
/// written in **one transaction**: one refusal saves nothing. Each sale keeps its own closer, so
/// commission attribution (ADR-0010) stays per sale.
pub async fn create_checkout(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &SaleCheckoutCreate,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::PackageSell)?;
    let (orders, payments) = parse_checkout(input)?;
    if !payments.is_empty() {
        actor.require(Permission::PaymentCollect)?;
    }

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let tenant = kernel::tenant::profile(&mut tx).await?;
    let today = tenant.business_date.as_naive();
    let mut priced = Vec::with_capacity(orders.len());
    for (k, order) in orders.iter().enumerate() {
        priced.push(
            price_package_sale(&mut tx, actor, order, tenant.manager_discount_max_pct)
                .await
                .map_err(|e| in_checkout_package(k, e))?,
        );
    }
    let dues: Vec<Money> = priced.iter().map(PricedPackageSale::total).collect();
    let split = allocate_payments(&dues, &payments).ok_or(AppError::Unprocessable {
        code: "payment.amount_mismatch",
    })?;
    let at = SaleAt {
        business_date: today,
        branch_id: tenant.branch.as_uuid(),
        visit_id: None,
    };
    let mut sales = Vec::with_capacity(priced.len());
    let mut total = Money::ZERO;
    for (sale, paid) in priced.iter().zip(&split) {
        let sold = record_package_sale(&mut tx, actor, sale, at, paid).await?;
        total = Money::sum([total, sold.total]).map_err(money_err)?;
        sales.push(
            load(&mut tx, sold.sale_id, today)
                .await?
                .ok_or_else(|| AppError::Internal("sale vanished".into()))?,
        );
    }
    let body = serde_json::to_value(SaleCheckoutDto {
        sales,
        total: total.as_i64(),
    })
    .map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// A package sale priced from the catalog inside the caller's transaction: every rule checked,
/// nothing written yet (so a visit can price all its sales before splitting the payments).
#[derive(Debug, Clone)]
pub struct PricedPackageSale {
    order: PackageOrder,
    beneficiary: Uuid,
    closer: Option<Uuid>,
    plans: Vec<catalog::service::PlanSnapshot>,
    line_totals: Vec<Money>,
    subtotal: Money,
    total: Money,
}

impl PricedPackageSale {
    /// What the customer pays for this sale (after discount).
    pub const fn total(&self) -> Money {
        self.total
    }

    /// Who the packages belong to.
    pub const fn beneficiary(&self) -> Uuid {
        self.beneficiary
    }

    /// Whether one of the packages sold covers `service_id` (a plan item).
    pub fn covers(&self, service_id: Uuid) -> bool {
        self.plans
            .iter()
            .any(|p| p.items.iter().any(|i| i.service_id == service_id))
    }

    /// Names of the plans sold, for audit and reasons ("Chuyển sang gói …").
    pub fn plan_names(&self) -> Vec<&str> {
        self.plans.iter().map(|p| p.name.as_str()).collect()
    }
}

fn discount_error(e: DiscountError) -> AppError {
    match e {
        DiscountError::NotAllowed => AppError::forbidden(),
        DiscountError::LimitExceeded => AppError::Forbidden {
            code: "discount.limit_exceeded",
        },
    }
}

/// Prices a package sale (the caller holds `package.sell`): payer and beneficiary exist here
/// (`404`), the closer is an active staff member (`422 staff.inactive`) or nobody, every plan is
/// active (`422 inactive`), the discount follows D10 for this sale alone (Q-U9: the manager cap
/// applies per sale, separately from a visit's one-off discount).
pub async fn price_package_sale(
    tx: &mut TenantTx,
    actor: &Actor,
    order: &PackageOrder,
    manager_discount_max_pct: u32,
) -> Result<PricedPackageSale, AppError> {
    let payer = crm::service::brief_in(tx, order.customer_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let beneficiary = order.beneficiary_customer_id.unwrap_or(order.customer_id);
    let owner = if beneficiary == payer.id {
        payer
    } else {
        crm::service::brief_in(tx, beneficiary)
            .await?
            .ok_or(AppError::NotFound)?
    };
    // A package is kept, used and refunded over months: its owner must be someone the spa can
    // tell apart (an anonymous walk-in may still pay for a named person).
    owner.require_named()?;
    let closer = match order.closer_staff_id {
        Some(id) => Some(org::service::require_active(tx, id).await?.id),
        None => None,
    };

    // Every amount comes from the catalog, read and locked inside this transaction.
    let mut plans = Vec::with_capacity(order.lines.len());
    for (i, line) in order.lines.iter().enumerate() {
        let plan = catalog::service::plan_snapshot(tx, line.plan_id)
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
        authority(actor, manager_discount_max_pct),
    )
    .map_err(discount_error)?;
    let total = subtotal.checked_sub(order.discount).map_err(money_err)?;
    Ok(PricedPackageSale {
        order: order.clone(),
        beneficiary,
        closer,
        plans,
        line_totals,
        subtotal,
        total,
    })
}

/// Where and when a sale is booked.
#[derive(Debug, Clone, Copy)]
pub struct SaleAt {
    pub business_date: NaiveDate,
    pub branch_id: Uuid,
    /// The visit it was made in, if any (`sale.visit_id`).
    pub visit_id: Option<Uuid>,
}

/// A package created by a sale: its id and, per item, `(item id, service id)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoldPackage {
    pub customer_package_id: Uuid,
    pub items: Vec<(Uuid, Uuid)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedPackageSale {
    pub sale_id: Uuid,
    pub total: Money,
    pub packages: Vec<SoldPackage>,
}

/// Writes a priced package sale inside the caller's transaction: the sale (payer, closer,
/// `visit_id`), its package lines, the beneficiary's packages with their items, the payments
/// (exactly the total, `422 payment.amount_mismatch`; taking money needs `payment.collect`) and
/// the audit entry `sale.create`.
pub async fn record_package_sale(
    tx: &mut TenantTx,
    actor: &Actor,
    priced: &PricedPackageSale,
    at: SaleAt,
    payments: &[Payment],
) -> Result<RecordedPackageSale, AppError> {
    let paid = Money::sum(payments.iter().map(|p| p.amount)).map_err(money_err)?;
    if paid != priced.total {
        return Err(AppError::Unprocessable {
            code: "payment.amount_mismatch",
        });
    }
    if !payments.is_empty() {
        actor.require(Permission::PaymentCollect)?;
    }
    let order = &priced.order;
    let today = at.business_date;
    let sale_id = Uuid::now_v7();
    repo::insert_sale(
        tx,
        &SaleInsert {
            id: sale_id,
            branch_id: at.branch_id,
            customer_id: order.customer_id,
            business_date: today,
            subtotal: priced.subtotal.as_i64(),
            discount_amount: order.discount.as_i64(),
            discount_reason: order.discount_reason.as_deref(),
            total: priced.total.as_i64(),
            amount_paid: paid.as_i64(),
            closer_staff_id: priced.closer,
            visit_id: at.visit_id,
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;

    let shares = domain::allocate_discount(&priced.line_totals, order.discount);
    let mut packages = Vec::with_capacity(priced.plans.len());
    for (position, ((plan, line), share)) in
        (0_i32..).zip(priced.plans.iter().zip(&order.lines).zip(shares))
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
        // Collected in full (R1), so what was paid is also the refund cap.
        let price_paid = plan.price.checked_sub(share).map_err(money_err)?;
        let package = packages::service::create(
            tx,
            actor,
            &NewPackage {
                customer_id: priced.beneficiary,
                plan_id: Some(plan.id),
                name: plan.name.clone(),
                gift_note: line
                    .gift_note
                    .clone()
                    .unwrap_or_else(|| plan.gift_note.clone()),
                sold_price: price_paid,
                origin: Origin::Sale {
                    sale_id,
                    closer_staff_id: priced.closer,
                },
                starts_on: today,
                expires_on: expiry(today, plan.validity_months)?,
                items,
            },
        )
        .await?;
        repo::insert_package_line(
            tx,
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
        packages.push(SoldPackage {
            customer_package_id: package.id,
            items: package.items.iter().map(|i| (i.id, i.service_id)).collect(),
        });
    }
    insert_payments(tx, actor, sale_id, at, payments).await?;
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "sale.create",
            entity_type: "sale",
            entity_id: sale_id,
            before: None,
            after: Some(json!({
                "customer_id": order.customer_id, "closer_staff_id": priced.closer,
                "beneficiary_customer_id": priced.beneficiary, "visit_id": at.visit_id,
                "plans": priced.plans.iter().map(|p| json!({"plan_id": p.id, "price": p.price.as_i64()})).collect::<Vec<_>>(),
                "subtotal": priced.subtotal.as_i64(), "discount_amount": order.discount.as_i64(),
                "total": priced.total.as_i64(),
                "payments": payments.iter().map(|p| json!({"method": p.method.as_str(), "amount": p.amount.as_i64()})).collect::<Vec<_>>(),
            })),
            reason: order.discount_reason.as_deref(),
        },
    )
    .await?;
    Ok(RecordedPackageSale {
        sale_id,
        total: priced.total,
        packages,
    })
}

async fn insert_payments(
    tx: &mut TenantTx,
    actor: &Actor,
    sale_id: Uuid,
    at: SaleAt,
    payments: &[Payment],
) -> Result<(), AppError> {
    for p in payments {
        repo::insert_payment(
            tx,
            &PaymentInsert {
                branch_id: at.branch_id,
                sale_id,
                business_date: at.business_date,
                method: p.method.as_str(),
                amount: p.amount.as_i64(),
                reference: p.reference.as_deref(),
                created_by: actor.user_id.as_uuid(),
            },
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn load(
    tx: &mut TenantTx,
    id: Uuid,
    today: NaiveDate,
) -> Result<Option<SaleDto>, AppError> {
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
        .map(|p| {
            payment_dto(
                p.id,
                &p.kind,
                p.reversal_of,
                &p.method,
                p.amount,
                p.reference,
                p.paid_at,
            )
        })
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
        void_reason: s.void_reason,
        voided_at: s.voided_at,
        closer_staff_id: s.closer_staff_id,
        closer_name: s.closer_name,
        visit_id: s.visit_id,
        created_at: s.created_at,
        lines,
        payments,
        customer_packages,
    }))
}

/// Most package sales one day list returns (a busy day sells a handful).
const DAY_SALES_MAX: i64 = 200;

/// `GET /sales?date=` (`sale.read` + `customer.read`: the list names the customer): the package
/// sales of one business day saved at the desk without a visit (F6, live test 2026-11-08), newest
/// first, each with what `GET /sales/{id}` returns. A visit's sales are shown with the visit
/// (`GET /visits`), and paper-book imports are not desk sales.
pub async fn for_day(
    pool: &PgPool,
    actor: &Actor,
    day: Option<NaiveDate>,
) -> Result<Vec<DaySaleDto>, AppError> {
    actor.require(Permission::SaleRead)?;
    actor.require(Permission::CustomerRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let heads = repo::day_sales(&mut tx, day.unwrap_or(today), DAY_SALES_MAX).await?;
    let mut out = Vec::with_capacity(heads.len());
    for h in heads {
        let sale = load(&mut tx, h.id, today)
            .await?
            .ok_or_else(|| AppError::Internal("sale vanished".into()))?;
        out.push(DaySaleDto {
            sale,
            customer_name: h.customer_name,
            customer_gender: crm::http::gender_dto(h.customer_gender.as_deref()),
            beneficiary_name: h.beneficiary_name,
        });
    }
    tx.commit().await?;
    Ok(out)
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
    reversal_of: Option<Uuid>,
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
        reversal_of,
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

/// How a one-off discount was asked for; the amount is always computed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscountAsk {
    /// 1–100 % of the subtotal, rounded down to the đồng.
    Percent(u32),
    Amount(Money),
}

/// A validated discount request on one-off services (shape only; limits are checked against the
/// subtotal and the caller's authority in [`record_service_sale`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDiscount {
    pub ask: DiscountAsk,
    pub reason: String,
}

/// Shape rules for a visit's `discount`: exactly one of `percent` / `amount`, a reason (D10).
/// Problems are appended to `errors` with the `discount.` prefix.
pub fn parse_service_discount(
    input: &crate::dto::ServiceDiscountCreate,
    errors: &mut Vec<FieldError>,
) -> Option<ServiceDiscount> {
    let ask = match (input.percent, input.amount) {
        (Some(p), None) if (1..=100).contains(&p) => Some(DiscountAsk::Percent(p)),
        (Some(_), None) => {
            errors.push(FieldError::new("discount.percent", "out_of_range"));
            None
        }
        (None, Some(a)) => match Money::vnd(a) {
            Ok(m) if m > Money::ZERO => Some(DiscountAsk::Amount(m)),
            _ => {
                errors.push(FieldError::new("discount.amount", "must_be_positive"));
                None
            }
        },
        (Some(_), Some(_)) => {
            errors.push(FieldError::new("discount", "percent_or_amount"));
            None
        }
        (None, None) => {
            errors.push(FieldError::new("discount", "required"));
            None
        }
    };
    let reason = trimmed(input.reason.as_ref());
    if reason
        .as_ref()
        .is_some_and(|r| r.chars().count() > MAX_REASON)
    {
        errors.push(FieldError::new("discount.reason", "too_long"));
    }
    let ask = ask?;
    match reason {
        Some(reason) => Some(ServiceDiscount { ask, reason }),
        None => {
            // Same code as a package sale without a reason (AC3.5).
            errors.push(FieldError::new("discount.reason", "required"));
            None
        }
    }
}

/// The amount of `ask` on `subtotal`: a percent rounds down, so a manager at the cap is never
/// pushed one đồng over it.
pub fn discount_amount(subtotal: Money, ask: DiscountAsk) -> Result<Money, AppError> {
    match ask {
        DiscountAsk::Amount(m) => Ok(m),
        DiscountAsk::Percent(p) => {
            let v = i128::from(subtotal.as_i64()) * i128::from(p) / 100;
            Money::vnd(
                i64::try_from(v).map_err(|_| money_err(kernel::money::MoneyError::Overflow))?,
            )
            .map_err(money_err)
        }
    }
}

/// A sale of one-off services, e.g. the paid part of a visit. Always collected in full (R1).
#[derive(Debug, Clone, Copy)]
pub struct ServiceSale<'a> {
    pub customer_id: Uuid,
    pub business_date: NaiveDate,
    pub branch_id: Uuid,
    /// The visit whose lines this sale charges (`sale.visit_id`).
    pub visit_id: Option<Uuid>,
    pub lines: &'a [ServiceLine],
    pub payments: &'a [Payment],
    pub discount: Option<&'a ServiceDiscount>,
    /// The spa's cap for callers without `sale.discount_unlimited` (`TenantProfile`).
    pub manager_discount_max_pct: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSaleRecorded {
    pub sale_id: Uuid,
    pub subtotal: Money,
    pub discount: Money,
    pub total: Money,
    /// Each line's share of the discount, in the order of `ServiceSale::lines`; sums to `discount`.
    pub line_discounts: Vec<Money>,
}

/// One-off lines priced with their discount (D10), before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServicePricing {
    pub line_totals: Vec<Money>,
    pub subtotal: Money,
    pub discount: Money,
    pub total: Money,
}

/// Prices one-off lines and checks their discount against the caller's authority (D10:
/// `sale.discount` up to the spa's cap, `sale.discount_unlimited` beyond). Pure apart from the
/// actor's permissions, so a visit can know every sale's total before splitting its payments.
pub fn price_services(
    actor: &Actor,
    lines: &[ServiceLine],
    discount: Option<&ServiceDiscount>,
    manager_discount_max_pct: u32,
) -> Result<ServicePricing, AppError> {
    let line_totals = lines
        .iter()
        .map(|l| l.unit_price.checked_mul(l.qty))
        .collect::<Result<Vec<_>, _>>()
        .map_err(money_err)?;
    let subtotal = Money::sum(line_totals.iter().copied()).map_err(money_err)?;
    let discount = match discount {
        None => Money::ZERO,
        Some(d) => {
            let amount = discount_amount(subtotal, d.ask)?;
            if amount > subtotal || amount == Money::ZERO {
                return Err(AppError::Validation(vec![FieldError::new(
                    "discount.amount",
                    "exceeds_subtotal",
                )]));
            }
            domain::check_discount(subtotal, amount, authority(actor, manager_discount_max_pct))
                .map_err(discount_error)?;
            amount
        }
    };
    let total = subtotal.checked_sub(discount).map_err(money_err)?;
    Ok(ServicePricing {
        line_totals,
        subtotal,
        discount,
        total,
    })
}

/// Records the sale of one-off services inside the caller's transaction: sale, `service` lines,
/// append-only payments and an audit entry. An optional discount follows D10 like a package sale
/// (see [`price_services`]). Payments must add up exactly to the server-computed total after
/// discount (`422 payment.amount_mismatch`); taking money needs `payment.collect`.
/// No lines → no sale (`Ok(None)`), and then any payment or discount is rejected.
pub async fn record_service_sale(
    tx: &mut TenantTx,
    actor: &Actor,
    sale: ServiceSale<'_>,
) -> Result<Option<ServiceSaleRecorded>, AppError> {
    let paid = Money::sum(sale.payments.iter().map(|p| p.amount)).map_err(money_err)?;
    let ServicePricing {
        line_totals,
        subtotal,
        discount,
        total,
    } = price_services(
        actor,
        sale.lines,
        sale.discount,
        sale.manager_discount_max_pct,
    )?;
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
            subtotal: subtotal.as_i64(),
            discount_amount: discount.as_i64(),
            discount_reason: sale.discount.map(|d| d.reason.as_str()),
            total: total.as_i64(),
            amount_paid: paid.as_i64(),
            closer_staff_id: None,
            visit_id: sale.visit_id,
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
    insert_payments(
        tx,
        actor,
        sale_id,
        SaleAt {
            business_date: sale.business_date,
            branch_id: sale.branch_id,
            visit_id: sale.visit_id,
        },
        sale.payments,
    )
    .await?;
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "sale.create",
            entity_type: "sale",
            entity_id: sale_id,
            before: None,
            after: Some(json!({
                "customer_id": sale.customer_id, "kind": "services", "visit_id": sale.visit_id,
                "lines": sale.lines.iter().map(|l| json!({
                    "service_id": l.service_id, "qty": l.qty, "unit_price": l.unit_price.as_i64(),
                })).collect::<Vec<_>>(),
                "subtotal": subtotal.as_i64(), "discount_amount": discount.as_i64(),
                "total": total.as_i64(),
                "payments": sale.payments.iter().map(|p| json!({"method": p.method.as_str(), "amount": p.amount.as_i64()})).collect::<Vec<_>>(),
            })),
            reason: sale.discount.map(|d| d.reason.as_str()),
        },
    )
    .await?;
    Ok(Some(ServiceSaleRecorded {
        sale_id,
        subtotal,
        discount,
        total,
        line_discounts: domain::allocate_discount(&line_totals, discount),
    }))
}

// Payments of several sales, read inside the caller's transaction (`sale_id → payments`, oldest
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
            p.reversal_of,
            &p.method,
            p.amount,
            p.reference,
            p.paid_at,
        )?);
    }
    Ok(out)
}

/// Splits one checkout's payments over its sales (spec §10.4.2). `dues[k]` is what sale `k` costs,
/// in checkout order: the visit's one-off sale first, then the package sales in request order.
/// Cash pays the sales in that order first, the rest is bank transfer; a sale due 0 gets nothing.
/// Each sale then has payments of its own, so voiding one sale reverses exactly its money.
/// `None` when the payments do not add up to the dues (`422 payment.amount_mismatch`).
pub fn allocate_payments(dues: &[Money], payments: &[Payment]) -> Option<Vec<Vec<Payment>>> {
    let due: i128 = dues.iter().map(|d| i128::from(d.as_i64())).sum();
    let paid: i128 = payments.iter().map(|p| i128::from(p.amount.as_i64())).sum();
    if due != paid {
        return None;
    }
    let mut ordered: Vec<(Payment, i64)> = payments
        .iter()
        .map(|p| (p.clone(), p.amount.as_i64()))
        .collect();
    ordered.sort_by_key(|(p, _)| p.method != PaymentMethod::Cash);
    let mut source = ordered.into_iter().peekable();
    let mut out = Vec::with_capacity(dues.len());
    for d in dues {
        let mut need = d.as_i64();
        let mut parts: Vec<Payment> = Vec::new();
        while need > 0 {
            let (payment, left) = source.peek_mut()?;
            let take = need.min(*left);
            parts.push(Payment {
                method: payment.method,
                amount: Money::vnd(take).ok()?,
                reference: payment.reference.clone(),
            });
            need -= take;
            *left -= take;
            if *left == 0 {
                source.next();
            }
        }
        out.push(parts);
    }
    Some(out)
}

/// Money a void just handed back, per method (cash first): the payments of `payments` that now
/// have a reversal. Used when the voided sale is recorded again (convert to package), so the money
/// already paid keeps its own method instead of being re-entered with the method of the top-up.
pub fn reversed_by_method(payments: &[PaymentDto]) -> Vec<Payment> {
    let reversed: HashSet<Uuid> = payments
        .iter()
        .filter(|p| p.kind == PaymentKindDto::Reversal)
        .filter_map(|p| p.reversal_of)
        .collect();
    let mut out: Vec<Payment> = Vec::new();
    for p in payments
        .iter()
        .filter(|p| p.kind == PaymentKindDto::Payment && reversed.contains(&p.id))
    {
        let method = match p.method {
            PaymentMethodDto::Cash => PaymentMethod::Cash,
            PaymentMethodDto::BankTransfer => PaymentMethod::BankTransfer,
        };
        let Ok(amount) = Money::vnd(p.amount) else {
            continue;
        };
        match out.iter_mut().find(|c| c.method == method) {
            Some(c) => {
                c.amount = Money::vnd(c.amount.as_i64() + amount.as_i64()).unwrap_or(c.amount)
            }
            None => out.push(Payment {
                method,
                amount,
                reference: p.reference.clone(),
            }),
        }
    }
    out.sort_by_key(|p| p.method != PaymentMethod::Cash);
    out
}

/// The payments of a checkout that re-records money already paid (`carried`, e.g. a one-off
/// visit converted to a package) plus what the customer hands over now (`top_up`).
///
/// The carried money counts first, in its own method, up to `due`; whatever of it exceeds `due`
/// stays given back (the void's reversal). The top-up is added on top, merged per method (one
/// payment per method). `allocate_payments` then checks the sum: the top-up must be exactly
/// `due − carried` (none when the carried money already covers `due`).
pub fn with_carried(carried: &[Payment], top_up: &[Payment], due: Money) -> Vec<Payment> {
    let mut left = due.as_i64();
    let mut out: Vec<Payment> = Vec::new();
    for c in carried {
        let take = c.amount.as_i64().min(left);
        if take <= 0 {
            break;
        }
        left -= take;
        out.push(Payment {
            method: c.method,
            amount: Money::vnd(take).unwrap_or(c.amount),
            reference: c.reference.clone(),
        });
    }
    for t in top_up {
        match out.iter_mut().find(|p| p.method == t.method) {
            Some(p) => {
                p.amount = Money::vnd(p.amount.as_i64().saturating_add(t.amount.as_i64()))
                    .unwrap_or(p.amount);
                if t.reference.is_some() {
                    p.reference.clone_from(&t.reference);
                }
            }
            None => out.push(t.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vnd(v: i64) -> Money {
        Money::vnd(v).unwrap_or_default()
    }

    fn pay(method: PaymentMethod, amount: i64) -> Payment {
        Payment {
            method,
            amount: vnd(amount),
            reference: None,
        }
    }

    #[test]
    fn one_method_pays_every_sale_with_that_method() {
        let split = allocate_payments(
            &[Money::ZERO, vnd(3_500_000)],
            &[pay(PaymentMethod::Cash, 3_500_000)],
        );
        assert_eq!(
            split,
            Some(vec![vec![], vec![pay(PaymentMethod::Cash, 3_500_000)]])
        );
    }

    #[test]
    fn cash_pays_the_one_off_sale_first_then_packages_in_order() {
        // AC9.2: one-off 100.000đ + package 3.500.000đ, paid TM 600.000đ + CK 3.000.000đ.
        let transfer = Payment {
            reference: Some("VCB 123".into()),
            ..pay(PaymentMethod::BankTransfer, 3_000_000)
        };
        let split = allocate_payments(
            &[vnd(100_000), vnd(3_500_000)],
            &[transfer.clone(), pay(PaymentMethod::Cash, 600_000)],
        );
        assert_eq!(
            split,
            Some(vec![
                vec![pay(PaymentMethod::Cash, 100_000)],
                vec![pay(PaymentMethod::Cash, 500_000), transfer],
            ])
        );
    }

    #[test]
    fn transfer_carries_over_to_later_sales() {
        let split = allocate_payments(
            &[vnd(200_000), vnd(1_000_000), vnd(2_000_000)],
            &[
                pay(PaymentMethod::Cash, 100_000),
                pay(PaymentMethod::BankTransfer, 3_100_000),
            ],
        );
        assert_eq!(
            split,
            Some(vec![
                vec![
                    pay(PaymentMethod::Cash, 100_000),
                    pay(PaymentMethod::BankTransfer, 100_000)
                ],
                vec![pay(PaymentMethod::BankTransfer, 1_000_000)],
                vec![pay(PaymentMethod::BankTransfer, 2_000_000)],
            ])
        );
    }

    #[test]
    fn payments_must_add_up_to_the_dues() {
        assert_eq!(
            allocate_payments(&[vnd(100_000)], &[pay(PaymentMethod::Cash, 99_999)]),
            None
        );
        assert_eq!(
            allocate_payments(&[vnd(100_000)], &[pay(PaymentMethod::Cash, 100_001)]),
            None
        );
        assert_eq!(allocate_payments(&[Money::ZERO], &[]), Some(vec![vec![]]));
    }

    fn dto(
        id: u128,
        kind: PaymentKindDto,
        of: Option<u128>,
        method: PaymentMethodDto,
        amount: i64,
    ) -> PaymentDto {
        PaymentDto {
            id: Uuid::from_u128(id),
            kind,
            reversal_of: of.map(Uuid::from_u128),
            method,
            amount,
            reference: None,
            paid_at: DateTime::<Utc>::UNIX_EPOCH,
        }
    }

    #[test]
    fn reversed_money_keeps_its_method() {
        use PaymentKindDto::{Payment as P, Reversal as R};
        use PaymentMethodDto::{BankTransfer as Ck, Cash as Tm};
        let payments = [
            dto(1, P, None, Ck, 20_000),
            dto(2, P, None, Tm, 99_000),
            dto(3, P, None, Tm, 20_000),
            dto(4, R, Some(1), Ck, 20_000),
            dto(5, R, Some(2), Tm, 99_000),
            dto(6, R, Some(3), Tm, 20_000),
            // Not reversed: not handed back, so not carried.
            dto(7, P, None, Ck, 5_000),
        ];
        assert_eq!(
            reversed_by_method(&payments),
            vec![
                pay(PaymentMethod::Cash, 119_000),
                pay(PaymentMethod::BankTransfer, 20_000)
            ]
        );
    }

    #[test]
    fn carried_money_counts_first_and_the_top_up_adds_on_top() {
        // Live test 2026-11-07: 119.000đ paid cash, converted to a 739.000đ package, top-up by
        // transfer → TM 119.000đ + CK 620.000đ (not −119.000đ TM + 739.000đ CK).
        let carried = [pay(PaymentMethod::Cash, 119_000)];
        let top_up = [pay(PaymentMethod::BankTransfer, 620_000)];
        let all = with_carried(&carried, &top_up, vnd(739_000));
        assert_eq!(
            all,
            vec![
                pay(PaymentMethod::Cash, 119_000),
                pay(PaymentMethod::BankTransfer, 620_000)
            ]
        );
        assert!(allocate_payments(&[Money::ZERO, vnd(739_000)], &all).is_some());
        // Same method: one payment.
        let cash_top_up = [pay(PaymentMethod::Cash, 620_000)];
        assert_eq!(
            with_carried(&carried, &cash_top_up, vnd(739_000)),
            vec![pay(PaymentMethod::Cash, 739_000)]
        );
        // The old money covers more than the new total: only `due` is carried, no top-up.
        let both = [
            pay(PaymentMethod::Cash, 100_000),
            pay(PaymentMethod::BankTransfer, 50_000),
        ];
        assert_eq!(
            with_carried(&both, &[], vnd(120_000)),
            vec![
                pay(PaymentMethod::Cash, 100_000),
                pay(PaymentMethod::BankTransfer, 20_000)
            ]
        );
        // A top-up that is not exactly the rest does not add up.
        let wrong = with_carried(
            &carried,
            &[pay(PaymentMethod::BankTransfer, 739_000)],
            vnd(739_000),
        );
        assert_eq!(allocate_payments(&[vnd(739_000)], &wrong), None);
    }
}
