//! Visit use cases (US4). Recording a visit is one transaction: idempotency claim, technician
//! checks, server-side pricing, the packages sold in the checkout, atomic session consumption, the
//! sale of the one-off lines with its payments, the visit and its lines, the audit trail and the
//! stored response — or nothing at all.
//!
//! Round 6 (spec §10): one checkout sells packages too (`package_sales`, one sale per package,
//! linked by `sale.visit_id`), a line may use today's session from a package sold in the same
//! request, services and packages can be added to a saved visit (`add`), and a paid one-off visit
//! can count today as the first session of a new package (`convert`).
//!
//! The session note is health data: encrypted at rest (AAD = tenant + visit), written only with
//! `customer.health_notes.write`, returned (on a visit and in the day list) only with
//! `customer.health_notes.read`, never stored in the idempotent response nor in audit.

use crate::domain::{self, CheckInError};
use crate::dto::{
    VisitConversion, VisitCorrected, VisitCreate, VisitDto, VisitLineCreate, VisitLineDto,
    VisitLineSourceDto, VisitPackageSaleCreate, VisitPackageSaleDto, VisitStatusDto, VisitUpdate,
};
use crate::repo::{self, LineInsert, LineRow, VisitInsert, VisitRow};
use catalog::{PriceLookupError, PriceVariant};
use chrono::{DateTime, NaiveDate, Utc};
use kernel::audit::{self, AuditEntry, REDACTED};
use kernel::crypto::FieldCipher;
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx};
use packages::{Consume, Consumed};
use sales::service::{
    DiscountAsk, PackageLine, PackageOrder, Payment, PricedPackageSale, SaleAt, ServiceDiscount,
    ServiceLine, ServiceSale,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const MAX_LINES: usize = 20;
pub const MAX_QTY: u32 = 10;
/// Packages sold in one checkout (spec §10.4.1).
pub const MAX_PACKAGE_SALES: usize = 3;
/// Companions (notes only) on one visit or booking.
pub const MAX_COMPANIONS: usize = 5;
const MAX_COMPANION_CHARS: usize = 100;
const MAX_REF_CHARS: usize = 20;
const MAX_NOTE: usize = 2000;
const MAX_REASON: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Package,
    OneOff,
}

impl Source {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::OneOff => "one_off",
        }
    }
}

struct LineOrder {
    service_id: Uuid,
    source: Source,
    item_id: Option<Uuid>,
    /// Index into `Order::package_sales`: today's session comes from that new package.
    new_package: Option<usize>,
    staff_id: Uuid,
    /// One-off lines: "người giới thiệu" (upsell credit); `None` = "khách tự chọn".
    referrer: Option<Uuid>,
    qty: u32,
    price_variant: Option<PriceVariant>,
    override_reason: Option<String>,
}

/// A package sold in the checkout, after shape validation.
struct PackageSaleOrder {
    /// The beneficiary as asked (`None` = the visit's customer).
    beneficiary: Option<Uuid>,
    closer_staff_id: Option<Uuid>,
    line: PackageLine,
    discount: Money,
    discount_reason: Option<String>,
}

/// The request after shape validation; nothing is checked against the database yet.
pub(crate) struct Order {
    customer_id: Uuid,
    check_in_at: Option<DateTime<Utc>>,
    note: Option<String>,
    lines: Vec<LineOrder>,
    /// New money handed over in this request (its method is the desk's choice).
    payments: Vec<Payment>,
    /// Money already paid for the visit this one replaces, just reversed by its void ("Tính buổi
    /// hôm nay vào gói"): recorded again in its own method before `payments`, up to the new total.
    carried: Vec<Payment>,
    discount: Option<ServiceDiscount>,
    package_sales: Vec<PackageSaleOrder>,
    companions: Vec<String>,
    /// Set by "Sửa lượt": the voided visit this one replaces, whose business day it keeps.
    correction: Option<Replaces>,
}

/// The visit a correction replaces (already voided and authorized by the caller).
#[derive(Debug, Clone, Copy)]
struct Replaces {
    visit_id: Uuid,
    business_date: NaiveDate,
}

/// A saved visit that an addition appends to (locked, current and authorized by the caller).
#[derive(Debug, Clone, Copy)]
struct Existing {
    id: Uuid,
    customer_id: Uuid,
    business_date: NaiveDate,
    next_position: i32,
}

fn trimmed(s: Option<&String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

fn clean_note(note: Option<&String>) -> Result<Option<String>, AppError> {
    let note = trimmed(note);
    if note.as_ref().is_some_and(|n| n.chars().count() > MAX_NOTE) {
        return Err(AppError::Validation(vec![FieldError::new(
            "note", "too_long",
        )]));
    }
    Ok(note)
}

/// Shape rules of companion notes: 0–5, each 1–100 characters after trimming. Shared with
/// bookings (`appointments`).
pub(crate) fn parse_companions(input: &[String], errors: &mut Vec<FieldError>) -> Vec<String> {
    if input.len() > MAX_COMPANIONS {
        errors.push(FieldError::new("companions", "count_out_of_range"));
    }
    input
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let c = c.trim();
            if c.is_empty() || c.chars().count() > MAX_COMPANION_CHARS {
                errors.push(FieldError::new(format!("companions[{i}]"), "invalid"));
                None
            } else {
                Some(c.to_owned())
            }
        })
        .collect()
}

/// Shape rules of `package_sales[]`; returns the sales and each `ref` → index.
fn parse_package_sales(
    input: &[VisitPackageSaleCreate],
    errors: &mut Vec<FieldError>,
) -> (Vec<PackageSaleOrder>, HashMap<String, usize>) {
    if input.len() > MAX_PACKAGE_SALES {
        errors.push(FieldError::new("package_sales", "count_out_of_range"));
    }
    let mut refs = HashMap::new();
    let sales = input
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let prefix = format!("package_sales[{k}]");
            if let Some(r) = p.package_ref.as_deref().map(str::trim) {
                if r.is_empty() || r.chars().count() > MAX_REF_CHARS {
                    errors.push(FieldError::new(format!("{prefix}.ref"), "invalid"));
                } else if refs.insert(r.to_owned(), k).is_some() {
                    errors.push(FieldError::new(format!("{prefix}.ref"), "duplicate"));
                }
            }
            let (discount, discount_reason) = sales::service::parse_package_discount(
                p.discount.as_ref(),
                &format!("{prefix}.discount"),
                errors,
            );
            PackageSaleOrder {
                beneficiary: p.beneficiary_customer_id,
                closer_staff_id: p.closer_staff_id,
                line: PackageLine {
                    plan_id: p.package_plan_id,
                    gift_note: sales::service::parse_gift_note(
                        p.gift_note.as_ref(),
                        &format!("{prefix}.gift_note"),
                        errors,
                    ),
                },
                discount,
                discount_reason,
            }
        })
        .collect();
    (sales, refs)
}

fn parse_lines(
    input: &[VisitLineCreate],
    refs: &HashMap<String, usize>,
    errors: &mut Vec<FieldError>,
) -> Vec<LineOrder> {
    let mut lines = Vec::with_capacity(input.len());
    for (i, l) in input.iter().enumerate() {
        let source = match l.source {
            VisitLineSourceDto::Package => Source::Package,
            VisitLineSourceDto::OneOff => Source::OneOff,
        };
        let staff_id = l.staff_id.unwrap_or_else(|| {
            errors.push(FieldError::new(format!("lines[{i}].staff_id"), "required"));
            Uuid::nil()
        });
        let qty = l.qty.unwrap_or(1);
        if qty == 0 || qty > MAX_QTY {
            errors.push(FieldError::new(format!("lines[{i}].qty"), "out_of_range"));
        }
        let override_reason = trimmed(l.expiry_override_reason.as_ref());
        if override_reason
            .as_ref()
            .is_some_and(|r| r.chars().count() > MAX_REASON)
        {
            errors.push(FieldError::new(
                format!("lines[{i}].expiry_override_reason"),
                "too_long",
            ));
        }
        let new_package = match l.package_ref.as_deref().map(str::trim) {
            None => None,
            Some(r) => match refs.get(r) {
                Some(k) => Some(*k),
                None => {
                    errors.push(FieldError::new(
                        format!("lines[{i}].package_ref"),
                        "unknown",
                    ));
                    None
                }
            },
        };
        let mut not_allowed = |field: &str| {
            errors.push(FieldError::new(
                format!("lines[{i}].{field}"),
                "not_allowed",
            ));
        };
        match source {
            Source::OneOff => {
                if l.customer_package_item_id.is_some() {
                    not_allowed("customer_package_item_id");
                }
                if override_reason.is_some() {
                    not_allowed("expiry_override_reason");
                }
                if l.package_ref.is_some() {
                    not_allowed("package_ref");
                }
            }
            Source::Package => {
                // A package line was sold with its package: that sale's closer has the credit.
                if l.referrer_staff_id.is_some() {
                    not_allowed("referrer_staff_id");
                }
                if l.package_ref.is_some() && l.customer_package_item_id.is_some() {
                    not_allowed("customer_package_item_id");
                }
                if l.package_ref.is_some() && override_reason.is_some() {
                    not_allowed("expiry_override_reason");
                }
            }
        }
        lines.push(LineOrder {
            service_id: l.service_id,
            source,
            item_id: l.customer_package_item_id,
            new_package,
            staff_id,
            referrer: l.referrer_staff_id,
            qty,
            price_variant: l.price_variant,
            override_reason,
        });
    }
    lines
}

/// Shape validation of a visit and the permissions its content needs (note, expiry override,
/// packages sold). `min_lines` is 1 for a visit, 0 for an addition (which may only sell a package).
fn parse_order(actor: &Actor, input: &VisitCreate, min_lines: usize) -> Result<Order, AppError> {
    let mut errors = Vec::new();
    if input.lines.len() < min_lines || input.lines.len() > MAX_LINES {
        errors.push(FieldError::new("lines", "count_out_of_range"));
    }
    let (package_sales, refs) = parse_package_sales(&input.package_sales, &mut errors);
    let lines = parse_lines(&input.lines, &refs, &mut errors);
    let note = match clean_note(input.note.as_ref()) {
        Ok(n) => n,
        Err(AppError::Validation(e)) => {
            errors.extend(e);
            None
        }
        Err(e) => return Err(e),
    };
    let companions = parse_companions(&input.companions, &mut errors);
    let payments = sales::service::parse_payments(&input.payments, &mut errors);
    let discount = input
        .discount
        .as_ref()
        .and_then(|d| sales::service::parse_service_discount(d, &mut errors));
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    if note.is_some() {
        actor.require(Permission::CustomerHealthNotesWrite)?;
    }
    if lines.iter().any(|l| l.override_reason.is_some()) {
        actor.require(Permission::PackageUseExpiredOverride)?;
    }
    if !package_sales.is_empty() {
        actor.require(Permission::PackageSell)?;
    }
    Ok(Order {
        customer_id: input.customer_id,
        check_in_at: input.check_in_at,
        note,
        lines,
        payments,
        carried: Vec::new(),
        discount,
        package_sales,
        companions,
        correction: None,
    })
}

/// Shape validation of `POST /visits` (and of an appointment's completion, a correction).
pub(crate) fn parse(actor: &Actor, input: &VisitCreate) -> Result<Order, AppError> {
    parse_order(actor, input, 1)
}

/// Associated data for a visit note: a ciphertext copied to another visit or spa fails to decrypt.
fn visit_aad(tenant: Uuid, visit: Uuid) -> Vec<u8> {
    kernel::crypto::record_aad("visit.note", &[tenant, visit])
}

fn encrypt_note(
    cipher: &FieldCipher,
    tx: &TenantTx,
    visit: Uuid,
    note: Option<&str>,
) -> Result<Option<Vec<u8>>, AppError> {
    note.map(|n| cipher.encrypt(n, &visit_aad(tx.tenant().as_uuid(), visit)))
        .transpose()
        .map_err(|e| AppError::Internal(format!("visit note of {visit}: {e}")))
}

fn line_error(i: usize, field: &str, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(format!("lines[{i}].{field}"), code)])
}

/// Field errors of package sale `k` name `package_sales[k].…`.
fn in_package_sale(k: usize, e: AppError) -> AppError {
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

/// A line once priced (one-off) or matched to a package item (package).
enum Priced {
    OneOff {
        name: String,
        variant: PriceVariant,
        unit_price: Money,
    },
    Package {
        item_id: Uuid,
    },
    /// Today's session of a package sold in this checkout (its item is known once it is written).
    NewPackage {
        sale: usize,
    },
}

/// `POST /visits` (`visit.record`; `payment.collect` when money is taken,
/// `customer.health_notes.write` for a note, `package.use_expired_override` for an override,
/// `package.sell` when packages are sold). Returns the HTTP status and body, also stored under the
/// idempotency key: a retry with the same key and body replays them without consuming anything
/// twice.
pub async fn record(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &VisitCreate,
) -> Result<(u16, Value), AppError> {
    record_as_of(pool, cipher, actor, key, request_hash, input, Utc::now()).await
}

/// [`record`] with `now` as the server clock: the visit lands on the business day of `now`, and
/// the check-in rules (not in the future, on that day) are checked against it. Operator tooling
/// only (`app seed-demo` back-fills demo history); no route reaches it — `POST /visits` calls
/// [`record`].
pub async fn record_as_of(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &VisitCreate,
    now: DateTime<Utc>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::VisitRecord)?;
    let order = parse(actor, input)?;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let visit_id = record_in(&mut tx, cipher, actor, order, now).await?;
    // The stored response never carries the note: the idempotency table is not encrypted.
    let dto = load_one(&mut tx, cipher, actor, visit_id, false).await?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// The body of `POST /visits` inside the caller's transaction (the caller holds `visit.record`
/// and has claimed its idempotency key). Also how a booked appointment becomes a visit: the same
/// rules, nothing copied. Returns the new visit's id.
pub(crate) async fn record_in(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    order: Order,
    now: DateTime<Utc>,
) -> Result<Uuid, AppError> {
    checkout(tx, cipher, actor, order, now, None).await
}

/// What a checkout sells, priced and checked, before anything is written.
struct Priced2 {
    lines: Vec<Priced>,
    packages: Vec<PricedPackageSale>,
    service_lines: Vec<ServiceLine>,
    /// Payments per sale: `[0]` the one-off sale, then one per package sale.
    split: Vec<Vec<Payment>>,
}

/// Prices every line and package of a checkout inside the transaction and splits the payments
/// over the sales (`422 payment.amount_mismatch` when they do not add up).
async fn price_checkout(
    tx: &mut TenantTx,
    actor: &Actor,
    order: &Order,
    customer: &crm::service::CustomerBrief,
    today: NaiveDate,
    manager_discount_max_pct: u32,
) -> Result<Priced2, AppError> {
    let mut packages = Vec::with_capacity(order.package_sales.len());
    for (k, p) in order.package_sales.iter().enumerate() {
        let priced = sales::service::price_package_sale(
            tx,
            actor,
            &PackageOrder {
                customer_id: customer.id,
                beneficiary_customer_id: p.beneficiary,
                closer_staff_id: p.closer_staff_id,
                lines: vec![p.line.clone()],
                discount: p.discount,
                discount_reason: p.discount_reason.clone(),
            },
            manager_discount_max_pct,
        )
        .await
        .map_err(|e| in_package_sale(k, e))?;
        packages.push(priced);
    }

    // Every amount comes from the catalog or the package, read inside this transaction.
    let mut priced = Vec::with_capacity(order.lines.len());
    for (i, line) in order.lines.iter().enumerate() {
        priced.push(match (line.source, line.new_package) {
            (Source::OneOff, _) => {
                let service = catalog::service::service_snapshot(tx, line.service_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                if !service.active {
                    return Err(line_error(i, "service_id", "inactive"));
                }
                let gender = domain::pricing_gender(customer.gender, line.price_variant)
                    .map_err(|_| line_error(i, "price_variant", "conflicts_with_customer"))?;
                let (variant, unit_price) =
                    service.prices.price_for(gender).map_err(|e| match e {
                        PriceLookupError::GenderRequired => AppError::Unprocessable {
                            code: "price.gender_required",
                        },
                        PriceLookupError::NotOffered => AppError::Unprocessable {
                            code: "price.variant_not_available",
                        },
                    })?;
                Priced::OneOff {
                    name: service.name,
                    variant,
                    unit_price,
                }
            }
            (Source::Package, Some(k)) => {
                let sale = &packages[k];
                // Packages are not shared (D5): one bought for someone else is used by them.
                if sale.beneficiary() != customer.id {
                    return Err(line_error(i, "package_ref", "not_beneficiary"));
                }
                if !sale.covers(line.service_id) {
                    return Err(line_error(i, "service_id", "not_in_package"));
                }
                Priced::NewPackage { sale: k }
            }
            (Source::Package, None) => {
                let item_id = match line.item_id {
                    Some(id) => id,
                    None => {
                        packages::service::default_item(tx, customer.id, line.service_id, today)
                            .await?
                            .ok_or(AppError::Unprocessable {
                                code: "package.item_not_found",
                            })?
                    }
                };
                Priced::Package { item_id }
            }
        });
    }

    let service_lines: Vec<ServiceLine> = order
        .lines
        .iter()
        .zip(&priced)
        .filter_map(|(line, p)| match p {
            Priced::OneOff {
                name, unit_price, ..
            } => Some(ServiceLine {
                service_id: line.service_id,
                description: name.clone(),
                qty: line.qty,
                unit_price: *unit_price,
            }),
            Priced::Package { .. } | Priced::NewPackage { .. } => None,
        })
        .collect();
    let services = sales::service::price_services(
        actor,
        &service_lines,
        order.discount.as_ref(),
        manager_discount_max_pct,
    )?;
    let dues: Vec<Money> = std::iter::once(services.total)
        .chain(packages.iter().map(PricedPackageSale::total))
        .collect();
    let due = Money::sum(dues.iter().copied()).map_err(|_| AppError::Unprocessable {
        code: "payment.amount_mismatch",
    })?;
    let payments = sales::service::with_carried(&order.carried, &order.payments, due);
    let split =
        sales::service::allocate_payments(&dues, &payments).ok_or(AppError::Unprocessable {
            code: "payment.amount_mismatch",
        })?;
    if !order.payments.is_empty() {
        actor.require(Permission::PaymentCollect)?;
    }
    Ok(Priced2 {
        lines: priced,
        packages,
        service_lines,
        split,
    })
}

/// One checkout: a new visit (`existing` = `None`) or lines and sales added to a saved one.
/// Order of writes: package sales (so their items exist) → sessions consumed in item order → the
/// one-off sale → the visit (new only) and its lines → audit. Any failure rolls back everything.
#[allow(clippy::too_many_lines)]
async fn checkout(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    order: Order,
    now: DateTime<Utc>,
    existing: Option<Existing>,
) -> Result<Uuid, AppError> {
    let tenant = kernel::tenant::profile_as_of(tx, now).await?;
    let recorded_on = tenant.business_date.as_naive();
    let (visit_id, customer_id, today, check_in_at, backdated) = match existing {
        Some(e) => (e.id, e.customer_id, e.business_date, None, false),
        None => {
            // A correction stays on the replaced visit's day; the caller already authorized that
            // day (closed-day rule), so it needs no `visit.backdate` of its own.
            let service = domain::service_time(
                order.check_in_at,
                now,
                tenant.timezone,
                recorded_on,
                order.correction.is_some() || actor.can(Permission::VisitBackdate),
            )
            .map_err(|e: CheckInError| AppError::Unprocessable { code: e.code() })?;
            if let Some(replaced) = order.correction
                && service.business_date != replaced.business_date
            {
                return Err(AppError::Validation(vec![FieldError::new(
                    "check_in_at",
                    "not_visit_day",
                )]));
            }
            (
                Uuid::now_v7(),
                order.customer_id,
                service.business_date,
                Some(service.at),
                service.backdated,
            )
        }
    };
    // Everything below belongs to the day of the service: package expiry, the sale, payments.
    // Except a package sold by a later correction ("convert to package" on another day): its
    // money changes hands today, like the reversal of the one-off it replaces (Q-E1).
    let package_day = if order.correction.is_some() {
        recorded_on
    } else {
        today
    };
    let customer = crm::service::brief_in(tx, customer_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut verified = HashSet::new();
    for line in &order.lines {
        for staff in std::iter::once(line.staff_id).chain(line.referrer) {
            if verified.insert(staff) {
                org::service::require_active(tx, staff).await?;
            }
        }
    }

    let Priced2 {
        lines: priced,
        packages,
        service_lines,
        mut split,
    } = price_checkout(
        tx,
        actor,
        &order,
        &customer,
        today,
        tenant.manager_discount_max_pct,
    )
    .await?;
    let branch = tenant.branch.as_uuid();
    // The Nữ / Nam picked to price this visit becomes the profile's gender when it has none (F2).
    let picked = domain::gender_to_record(
        customer.gender,
        order
            .lines
            .iter()
            .filter(|l| l.source == Source::OneOff)
            .map(|l| l.price_variant),
    );
    if let Some(gender) = picked {
        crm::service::fill_gender_in(tx, actor, customer_id, gender).await?;
    }

    // Package sales first: today's session may come from one of them.
    let mut sold = Vec::with_capacity(packages.len());
    for (k, priced_sale) in packages.iter().enumerate() {
        let payments = std::mem::take(&mut split[k + 1]);
        sold.push(
            sales::service::record_package_sale(
                tx,
                actor,
                priced_sale,
                SaleAt {
                    business_date: package_day,
                    branch_id: branch,
                    visit_id: Some(visit_id),
                },
                &payments,
            )
            .await
            .map_err(|e| in_package_sale(k, e))?,
        );
    }
    // Each line's package item: the chosen one, or the new package's item for the service.
    let item_of = |i: usize, p: &Priced| -> Result<Option<Uuid>, AppError> {
        Ok(match p {
            Priced::Package { item_id } => Some(*item_id),
            Priced::NewPackage { sale } => Some(
                sold[*sale]
                    .packages
                    .iter()
                    .flat_map(|pk| pk.items.iter())
                    .find(|(_, service)| *service == order.lines[i].service_id)
                    .map(|(item, _)| *item)
                    .ok_or_else(|| line_error(i, "service_id", "not_in_package"))?,
            ),
            Priced::OneOff { .. } => None,
        })
    };
    let mut package_lines: Vec<(usize, Uuid)> = Vec::new();
    for (i, p) in priced.iter().enumerate() {
        if let Some(item) = item_of(i, p)? {
            package_lines.push((i, item));
        }
    }
    // Consume in item order so two multi-line visits racing for the same items lock rows in the
    // same order and cannot deadlock.
    package_lines.sort_by_key(|(i, item)| (*item, *i));
    // Packages before items, as voids and cancellations lock them (no deadlock with either).
    let items: Vec<Uuid> = package_lines.iter().map(|(_, item)| *item).collect();
    if !items.is_empty() {
        packages::service::lock_for_consumption(tx, &items).await?;
    }
    let mut consumed: HashMap<usize, (Uuid, Consumed)> = HashMap::new();
    for (i, item_id) in package_lines {
        let line = &order.lines[i];
        let c = packages::service::consume_session(
            tx,
            Consume {
                item_id,
                customer_id: customer.id,
                qty: line.qty,
                today,
                allow_expired: line.override_reason.is_some(),
            },
        )
        .await?;
        if c.service_id != line.service_id {
            return Err(line_error(i, "service_id", "not_in_package"));
        }
        consumed.insert(i, (item_id, c));
    }

    let sale = sales::service::record_service_sale(
        tx,
        actor,
        ServiceSale {
            customer_id: customer.id,
            business_date: today,
            branch_id: branch,
            visit_id: Some(visit_id),
            lines: &service_lines,
            payments: &split[0],
            discount: order.discount.as_ref(),
            manager_discount_max_pct: tenant.manager_discount_max_pct,
        },
    )
    .await?;
    // Discount share of each one-off line, in line order (same order as `service_lines`).
    let mut shares = sale
        .as_ref()
        .map(|s| s.line_discounts.clone())
        .unwrap_or_default()
        .into_iter();

    let first_position = match existing {
        Some(e) => e.next_position,
        None => {
            let note_ct = encrypt_note(cipher, tx, visit_id, order.note.as_deref())?;
            repo::insert_visit(
                tx,
                &VisitInsert {
                    id: visit_id,
                    branch_id: branch,
                    customer_id: customer.id,
                    business_date: today,
                    check_in_at: check_in_at
                        .ok_or_else(|| AppError::Internal("check-in time".into()))?,
                    note_ct: note_ct.as_deref(),
                    sale_id: sale.as_ref().map(|s| s.sale_id),
                    created_by: actor.user_id.as_uuid(),
                    corrects_visit_id: order.correction.map(|c| c.visit_id),
                    companions: &order.companions,
                },
            )
            .await?;
            0
        }
    };
    let mut audit_lines = Vec::with_capacity(order.lines.len());
    let mut overrides = Vec::new();
    for (position, (i, (line, p))) in
        (first_position..).zip(order.lines.iter().zip(&priced).enumerate())
    {
        let qty = i32::try_from(line.qty).map_err(|_| AppError::Internal("qty".into()))?;
        let insert = match p {
            Priced::OneOff {
                name,
                variant,
                unit_price,
            } => {
                let share = shares.next().unwrap_or(Money::ZERO);
                LineInsert {
                    visit_id,
                    position,
                    service_id: line.service_id,
                    service_name: name,
                    source: Source::OneOff.as_str(),
                    customer_package_id: None,
                    customer_package_item_id: None,
                    sessions_left_after: None,
                    staff_id: line.staff_id,
                    qty,
                    price_variant: Some(variant.as_str()),
                    unit_price: unit_price.as_i64(),
                    line_value: unit_price
                        .checked_mul(line.qty)
                        .and_then(|v| v.checked_sub(share))
                        .map_err(|_| AppError::Internal("line value overflow".into()))?
                        .as_i64(),
                    expiry_override_reason: None,
                    discount_amount: share.as_i64(),
                    sale_id: sale.as_ref().map(|s| s.sale_id),
                    referrer_staff_id: line.referrer,
                }
            }
            Priced::Package { .. } | Priced::NewPackage { .. } => {
                let (item_id, c) = consumed
                    .get(&i)
                    .ok_or_else(|| AppError::Internal("package line not consumed".into()))?;
                // The reason is kept only when it was actually needed.
                let reason = line.override_reason.as_deref().filter(|_| c.used_expired);
                if let Some(reason) = reason {
                    overrides.push((c.customer_package_id, *item_id, reason));
                }
                LineInsert {
                    visit_id,
                    position,
                    service_id: line.service_id,
                    service_name: &c.service_name,
                    source: Source::Package.as_str(),
                    customer_package_id: Some(c.customer_package_id),
                    customer_package_item_id: Some(*item_id),
                    sessions_left_after: Some(c.sessions_remaining),
                    staff_id: line.staff_id,
                    qty,
                    price_variant: None,
                    unit_price: c.per_session_value.as_i64(),
                    line_value: c.value.as_i64(),
                    expiry_override_reason: reason,
                    discount_amount: 0,
                    sale_id: None,
                    referrer_staff_id: None,
                }
            }
        };
        audit_lines.push(json!({
            "service_id": insert.service_id, "source": insert.source,
            "customer_package_item_id": insert.customer_package_item_id, "staff_id": insert.staff_id,
            "referrer_staff_id": insert.referrer_staff_id,
            "qty": insert.qty, "price_variant": insert.price_variant,
            "unit_price": insert.unit_price, "line_value": insert.line_value,
            "discount_amount": insert.discount_amount,
        }));
        repo::insert_line(tx, &insert).await?;
    }
    let package_sale_ids: Vec<Uuid> = sold.iter().map(|s| s.sale_id).collect();
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: if existing.is_some() {
                "visit.addition"
            } else {
                "visit.create"
            },
            entity_type: "visit",
            entity_id: visit_id,
            before: None,
            after: Some(json!({
                "customer_id": customer.id, "check_in_at": check_in_at,
                "business_date": today, "backdated": backdated, "recorded_on": recorded_on,
                "sale_id": sale.as_ref().map(|s| s.sale_id),
                "subtotal": sale.as_ref().map_or(0, |s| s.subtotal.as_i64()),
                "discount_amount": sale.as_ref().map_or(0, |s| s.discount.as_i64()),
                "total": sale.as_ref().map_or(0, |s| s.total.as_i64()),
                "package_sale_ids": package_sale_ids,
                "companions": order.companions.len(),
                "note": order.note.as_ref().map(|_| REDACTED), "lines": audit_lines,
                "corrects_visit_id": order.correction.map(|c| c.visit_id),
            })),
            reason: order.discount.as_ref().map(|d| d.reason.as_str()),
        },
    )
    .await?;
    for (package_id, item_id, reason) in overrides {
        audit::record(
            tx,
            actor,
            AuditEntry {
                action: "customer_package.use_expired_override",
                entity_type: "customer_package",
                entity_id: package_id,
                before: None,
                after: Some(json!({"visit_id": visit_id, "customer_package_item_id": item_id})),
                reason: Some(reason),
            },
        )
        .await?;
    }
    Ok(visit_id)
}

/// One visit read inside the caller's transaction (it must exist).
pub(crate) async fn load_one(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Uuid,
    with_note: bool,
) -> Result<VisitDto, AppError> {
    load(tx, cipher, actor, Some(id), None, with_note)
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal("visit vanished".into()))
}

fn price_variant(v: Option<&str>) -> Result<Option<PriceVariant>, AppError> {
    v.map(|s| {
        PriceVariant::parse(s).ok_or_else(|| AppError::Internal(format!("price variant {s}")))
    })
    .transpose()
}

fn line_dto(l: LineRow) -> Result<VisitLineDto, AppError> {
    Ok(VisitLineDto {
        source: if l.source == "package" {
            VisitLineSourceDto::Package
        } else {
            VisitLineSourceDto::OneOff
        },
        price_variant: price_variant(l.price_variant.as_deref())?,
        id: l.id,
        service_id: l.service_id,
        service_name: l.service_name_snapshot,
        category_name: l.category_name,
        customer_package_id: l.customer_package_id,
        customer_package_item_id: l.customer_package_item_id,
        package_name: l.package_name,
        sessions_left_after: l.sessions_left_after,
        sessions_total: l.sessions_total,
        staff_id: l.staff_id,
        staff_name: l.staff_name,
        qty: l.qty,
        unit_price: l.unit_price,
        discount_amount: l.discount_amount,
        line_value: l.line_value,
        expiry_override_reason: l.expiry_override_reason,
        sale_id: l.sale_id,
        referrer_staff_id: l.referrer_staff_id,
        referrer_name: l.referrer_name,
    })
}

/// Visits with their lines, payments and package sales in a fixed number of queries (no N+1).
/// `with_note` decrypts the note for callers holding `customer.health_notes.read`; otherwise the
/// key is omitted.
async fn load(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Option<Uuid>,
    day: Option<NaiveDate>,
    with_note: bool,
) -> Result<Vec<VisitDto>, AppError> {
    let rows: Vec<VisitRow> = repo::visits(tx, id, day).await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut lines: HashMap<Uuid, Vec<VisitLineDto>> = HashMap::new();
    for l in repo::lines(tx, &ids).await? {
        lines.entry(l.visit_id).or_default().push(line_dto(l)?);
    }
    let service_sales = repo::service_sales_of(tx, &ids).await?;
    let package_rows = repo::package_sales(tx, &ids).await?;
    let sale_ids: Vec<Uuid> = service_sales
        .iter()
        .map(|(_, s)| *s)
        .chain(package_rows.iter().map(|p| p.sale_id))
        .collect();
    let mut payments = sales::service::payments_of(tx, &sale_ids).await?;
    let mut sales_of: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (visit, sale) in service_sales {
        sales_of.entry(visit).or_default().push(sale);
    }
    let mut package_sales: HashMap<Uuid, Vec<VisitPackageSaleDto>> = HashMap::new();
    for p in package_rows {
        package_sales
            .entry(p.visit_id)
            .or_default()
            .push(VisitPackageSaleDto {
                payments: payments.remove(&p.sale_id).unwrap_or_default(),
                sale_id: p.sale_id,
                voided: p.status == "voided",
                business_date: p.business_date,
                description: p.description,
                customer_package_ids: p.customer_package_ids,
                total: p.total,
                closer_staff_id: p.closer_staff_id,
                closer_name: p.closer_name,
                beneficiary_customer_id: p.beneficiary_customer_id,
                beneficiary_name: p.beneficiary_name,
            });
    }
    let read_note = with_note && actor.can(Permission::CustomerHealthNotesRead);
    let tenant = tx.tenant().as_uuid();
    rows.into_iter()
        .map(|r| {
            let lines = lines.remove(&r.id).unwrap_or_default();
            let one_off = lines
                .iter()
                .filter(|l| l.source == VisitLineSourceDto::OneOff);
            let total: i64 = one_off.clone().map(|l| l.line_value).sum();
            let discount_amount: i64 = one_off.map(|l| l.discount_amount).sum();
            let note = if read_note {
                Some(
                    r.note_ct
                        .as_deref()
                        .map(|ct| cipher.decrypt(ct, &visit_aad(tenant, r.id)))
                        .transpose()
                        .map_err(|e| AppError::Internal(format!("visit note of {}: {e}", r.id)))?,
                )
            } else {
                None
            };
            let mut visit_sales = sales_of.remove(&r.id).unwrap_or_default();
            visit_sales.sort_unstable();
            visit_sales.dedup();
            let mut visit_payments: Vec<_> = visit_sales
                .iter()
                .flat_map(|s| payments.remove(s).unwrap_or_default())
                .collect();
            visit_payments.sort_by_key(|p| (p.paid_at, p.id));
            Ok(VisitDto {
                status: if r.status == "voided" {
                    VisitStatusDto::Voided
                } else {
                    VisitStatusDto::Completed
                },
                payments: visit_payments,
                package_sales: package_sales.remove(&r.id).unwrap_or_default(),
                companions: r.companions,
                void_reason: r.void_reason,
                voided_at: r.voided_at,
                corrects_visit_id: r.corrects_visit_id,
                corrected_by_visit_id: r.corrected_by_visit_id,
                id: r.id,
                customer_id: r.customer_id,
                customer_name: r.customer_name,
                customer_gender: crm::http::gender_dto(r.customer_gender.as_deref()),
                business_date: r.business_date,
                check_in_at: r.check_in_at,
                note,
                sale_id: r.sale_id,
                subtotal: total + discount_amount,
                discount_amount,
                discount_reason: r.discount_reason,
                total,
                lines,
                version: r.version,
                created_at: r.created_at,
            })
        })
        .collect()
}
/// `GET /visits/{id}` (`visit.read`). Another spa's visit is `404`.
pub async fn get(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Uuid,
) -> Result<VisitDto, AppError> {
    actor.require(Permission::VisitRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let visit = load(&mut tx, cipher, actor, Some(id), None, true)
        .await?
        .pop();
    tx.commit().await?;
    visit.ok_or(AppError::NotFound)
}

/// `GET /visits?date=` (`visit.read`): the visits of one business day (default today), latest
/// first. Each carries its session note only for callers holding `customer.health_notes.read`
/// (EM 2026-11-07: the note is seen in the day list, not only on the visit).
pub async fn for_day(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    day: Option<NaiveDate>,
) -> Result<Vec<VisitDto>, AppError> {
    actor.require(Permission::VisitRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let day = match day {
        Some(d) => d,
        None => kernel::tenant::profile(&mut tx)
            .await?
            .business_date
            .as_naive(),
    };
    let visits = load(&mut tx, cipher, actor, None, Some(day), true).await?;
    tx.commit().await?;
    Ok(visits)
}

/// Authorize before reporting what is wrong with a request (F17, live test 2026-11-08): a void or
/// correction refused by the body (missing reason, bad line…) first answers the questions any
/// caller would hear for a valid body — is the visit here (`404`), may this caller change its day
/// (`403 day.locked`) — and only then `refused`. Runs on the error path only, so a valid request
/// pays no extra read. The caller has checked the route's permissions.
pub async fn authorized_then(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    refused: AppError,
) -> AppError {
    let check = async {
        let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
        let visit = repo::lock(&mut tx, id).await?.ok_or(AppError::NotFound)?;
        let today = kernel::tenant::profile(&mut tx)
            .await?
            .business_date
            .as_naive();
        kernel::corrections::authorize_void(actor, visit.business_date, today)?;
        tx.commit().await?;
        Ok::<(), AppError>(())
    };
    match check.await {
        Ok(()) => refused,
        Err(denied) => denied,
    }
}

/// Only a visit that is still the current version may change: a voided one is `409 visit.voided`,
/// one replaced by "Sửa lượt" is `409 visit.corrected` (the desk opens the newest version).
fn ensure_current(v: &repo::LockedVisit) -> Result<(), AppError> {
    match (v.status.as_str(), v.corrected_by) {
        ("completed", _) => Ok(()),
        (_, Some(_)) => Err(AppError::Conflict {
            code: "visit.corrected",
        }),
        _ => Err(AppError::Conflict {
            code: "visit.voided",
        }),
    }
}

fn decrypt_note(
    cipher: &FieldCipher,
    tx: &TenantTx,
    visit: Uuid,
    ct: Option<&[u8]>,
) -> Result<Option<String>, AppError> {
    ct.map(|ct| cipher.decrypt(ct, &visit_aad(tx.tenant().as_uuid(), visit)))
        .transpose()
        .map_err(|e| AppError::Internal(format!("visit note of {visit}: {e}")))
}

/// `PATCH /visits/{id}` (`visit.record`, If-Match): "Sửa lượt" without money or sessions — the
/// note, a line's technician, the service time within the visit's business day (US6b). Only
/// today's visits, unless the caller may correct closed days (`record.void_closed_day`, the
/// owner; no re-auth).
pub async fn update(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Uuid,
    version: i64,
    input: &VisitUpdate,
) -> Result<VisitDto, AppError> {
    actor.require(Permission::VisitRecord)?;
    let note = input
        .note
        .as_ref()
        .map(|n| clean_note(n.as_ref()))
        .transpose()?;
    if note.is_some() {
        actor.require(Permission::CustomerHealthNotesWrite)?;
    }
    if input.lines.len() > MAX_LINES {
        return Err(AppError::Validation(vec![FieldError::new(
            "lines",
            "count_out_of_range",
        )]));
    }
    let companions = match &input.companions {
        None => None,
        Some(c) => {
            let mut errors = Vec::new();
            let c = parse_companions(c, &mut errors);
            if !errors.is_empty() {
                return Err(AppError::Validation(errors));
            }
            Some(c)
        }
    };

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let visit = repo::lock(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    ensure_current(&visit)?;
    let profile = kernel::tenant::profile(&mut tx).await?;
    // Same rule as voids: a closed day needs `record.void_closed_day`.
    kernel::corrections::authorize_closed_day(
        actor,
        visit.business_date,
        profile.business_date.as_naive(),
    )?;
    if visit.version != version {
        return Err(AppError::StaleVersion);
    }
    let moved = input.check_in_at.filter(|at| *at != visit.check_in_at);
    if let Some(at) = moved {
        domain::moved_check_in(at, Utc::now(), profile.timezone, visit.business_date).map_err(
            |e| match e {
                domain::MoveError::InFuture => AppError::Unprocessable {
                    code: "visit.check_in_in_future",
                },
                domain::MoveError::OtherDay => {
                    AppError::Validation(vec![FieldError::new("check_in_at", "not_visit_day")])
                }
            },
        )?;
    }

    let mut staff_changes = Vec::with_capacity(input.lines.len());
    for (i, change) in input.lines.iter().enumerate() {
        org::service::require_active(&mut tx, change.staff_id).await?;
        let before = repo::set_line_staff(&mut tx, id, change.id, change.staff_id)
            .await?
            .ok_or_else(|| line_error(i, "id", "not_found"))?;
        staff_changes.push(json!({"line_id": change.id, "from": before, "to": change.staff_id}));
    }
    let note_ct = match &note {
        Some(n) => Some(encrypt_note(cipher, &tx, id, n.as_deref())?),
        None => None,
    };
    if !repo::update_visit(
        &mut tx,
        id,
        version,
        note_ct.as_ref().map(Option::as_deref),
        moved,
        companions.as_deref(),
    )
    .await?
    {
        return Err(AppError::StaleVersion);
    }
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "visit.update",
            entity_type: "visit",
            entity_id: id,
            before: Some(json!({
                "version": version,
                "check_in_at": moved.map(|_| visit.check_in_at),
            })),
            after: Some(json!({
                "version": version + 1, "staff": staff_changes, "check_in_at": moved,
                "companions": companions.as_ref().map(Vec::len),
                "note": note.as_ref().map(|n| if n.is_some() { REDACTED } else { "cleared" }),
            })),
            reason: None,
        },
    )
    .await?;
    let visit = load(&mut tx, cipher, actor, Some(id), None, true)
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal("visit vanished".into()))?;
    tx.commit().await?;
    Ok(visit)
}

/// `POST /visits/{id}/void` (`record.void_same_day`; a closed day needs `record.void_closed_day`;
/// Idempotency-Key): the visit stays in history as `voided` with its reason,
/// every session it took goes back to its package (AC6.1), and the sale of its one-off lines is
/// voided with one reversal per payment. One transaction, so either all of it happens or nothing.
///
/// Two voids of the same visit serialize on the visit row: the second sees `voided` and gets
/// `409 visit.voided`, so sessions are never given back twice.
pub async fn void(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    id: Uuid,
    reason: Option<&str>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::RecordVoidSameDay)?;
    let reason = match kernel::corrections::reason(reason) {
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
    let visit = repo::lock(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    ensure_current(&visit)?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    kernel::corrections::authorize_void(actor, visit.business_date, today)?;
    void_locked(&mut tx, actor, id, &visit, &reason, today).await?;

    // Like `POST /visits`, the stored response never carries the note.
    let dto = load(&mut tx, cipher, actor, Some(id), None, false)
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal("visit vanished".into()))?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 200, &body).await?;
    tx.commit().await?;
    Ok((200, body))
}

/// The void of a visit the caller has locked, checked current and authorized, inside its
/// transaction: `completed` → `voided` with the reason, every session it took back to its package,
/// its one-off sale voided with one reversal per payment (booked on `today`), audit `visit.void`.
/// The one body of both `POST /visits/{id}/void` and "Sửa lượt".
async fn void_locked(
    tx: &mut TenantTx,
    actor: &Actor,
    id: Uuid,
    visit: &repo::LockedVisit,
    reason: &str,
    today: NaiveDate,
) -> Result<(), AppError> {
    if !repo::mark_voided(tx, id, reason, actor.user_id.as_uuid()).await? {
        return Err(AppError::Conflict {
            code: "visit.voided",
        });
    }
    let mut restored = Vec::new();
    for line in repo::package_lines(tx, id).await? {
        let qty = u32::try_from(line.qty).map_err(|_| AppError::Internal("qty".into()))?;
        let r =
            packages::lifecycle::restore_sessions(tx, line.customer_package_item_id, qty).await?;
        restored.push(json!({
            "customer_package_id": line.customer_package_id,
            "customer_package_item_id": line.customer_package_item_id,
            "qty": line.qty, "sessions_remaining": r.sessions_remaining,
        }));
    }
    // Every one-off sale of the visit (its first sale and each addition). Packages sold in the
    // visit stay: the package is the customer's, voided on its own (US6 on the sale) if needed.
    let service_sales = repo::open_service_sales(tx, id).await?;
    let mut reversed = 0_i64;
    for sale_id in &service_sales {
        reversed += sales::voids::void_in(tx, actor, *sale_id, reason, today)
            .await?
            .reversals
            .iter()
            .map(|(_, _, amount)| amount)
            .sum::<i64>();
    }
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "visit.void",
            entity_type: "visit",
            entity_id: id,
            before: Some(json!({"status": "completed", "business_date": visit.business_date})),
            after: Some(json!({
                "status": "voided", "restored": restored, "sale_id": visit.sale_id,
                "sale_ids": service_sales, "reversed": reversed,
            })),
            reason: Some(reason),
        },
    )
    .await?;
    Ok(())
}

/// What the old visit keeps as its void reason.
const CORRECTION_PREFIX: &str = "Sửa lượt: ";

/// The reason of a correction, prefixed so every list and the revenue book read "Sửa lượt: …".
fn correction_reason(input: Option<&str>) -> Result<String, AppError> {
    let reason = format!("{CORRECTION_PREFIX}{}", kernel::corrections::reason(input)?);
    if reason.chars().count() > kernel::corrections::MAX_REASON {
        return Err(AppError::Validation(vec![FieldError::new(
            "reason", "too_long",
        )]));
    }
    Ok(reason)
}

/// One "Sửa lượt" request that changes money or sessions (`POST /visits/{id}/correct`).
pub struct Correction<'a> {
    /// The visit being corrected.
    pub id: Uuid,
    /// Its version as the desk saw it (`If-Match`).
    pub version: i64,
    pub reason: Option<&'a str>,
    /// Absent = keep the old note; `Some(None)` = no note; `Some(Some(_))` = a new note.
    pub note: Option<Option<String>>,
    /// The visit as it should have been recorded (`customer_id`, lines, payments, discount, time).
    pub visit: &'a VisitCreate,
}

/// `POST /visits/{id}/correct` (`visit.record` + `record.void_same_day`; a closed day needs
/// `record.void_closed_day`; If-Match + Idempotency-Key): "Sửa lượt" for a change of money or
/// sessions (US6b). One transaction: the old visit is voided by the very code of
/// `POST /visits/{id}/void` (sessions back, payments reversed), then the corrected visit is
/// recorded by the very code of `POST /visits` (same pricing, permissions and package rules) on
/// the old visit's business day, linked by `corrects_visit_id`; an appointment completed by the
/// old visit moves to the new one. Either all of it happens or nothing.
///
/// Two corrections of one visit serialize on the visit row: the second sees it voided and gets
/// `409 visit.corrected`, so sessions and money move exactly once.
pub async fn correct(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    request: Correction<'_>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::VisitRecord)?;
    actor.require(Permission::RecordVoidSameDay)?;
    let id = request.id;
    let checked = (|| {
        let reason = correction_reason(request.reason)?;
        let new_note = request
            .note
            .as_ref()
            .map(|n| clean_note(n.as_ref()))
            .transpose()?;
        if new_note.is_some() {
            actor.require(Permission::CustomerHealthNotesWrite)?;
        }
        Ok((reason, new_note, parse(actor, request.visit)?))
    })();
    let (reason, new_note, order) = match checked {
        Ok(c) => c,
        Err(refused) => return Err(authorized_then(pool, actor, id, refused).await),
    };

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let started = begin_correction(&mut tx, cipher, actor, id, request.version, &reason).await?;
    // (A later "convert to package" records its package sale here, between the two steps.)
    let done = finish_correction(&mut tx, cipher, actor, started, order, new_note, &reason).await?;
    let body = serde_json::to_value(done).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// A correction after its first step: the old visit is voided, nothing recorded yet.
pub(crate) struct StartedCorrection {
    id: Uuid,
    old: repo::LockedVisit,
    /// The old note, decrypted for re-encryption under the new visit.
    old_note: Option<String>,
}

/// Step 1 of every correction (reusable inside one transaction, e.g. by a future
/// "convert to package" that sells its package between the steps): lock the visit, check it is
/// the current version, the closed-day rule and `If-Match`, then void it exactly as
/// `POST /visits/{id}/void` does. The caller holds the permissions and the idempotency claim.
pub(crate) async fn begin_correction(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Uuid,
    version: i64,
    reason: &str,
) -> Result<StartedCorrection, AppError> {
    let old = repo::lock(tx, id).await?.ok_or(AppError::NotFound)?;
    ensure_current(&old)?;
    let today = kernel::tenant::profile(tx).await?.business_date.as_naive();
    kernel::corrections::authorize_void(actor, old.business_date, today)?;
    if old.version != version {
        return Err(AppError::StaleVersion);
    }
    let old_note = decrypt_note(cipher, tx, id, old.note_ct.as_deref())?;
    void_locked(tx, actor, id, &old, reason, today).await?;
    Ok(StartedCorrection { id, old, old_note })
}

/// Step 2: record the corrected visit with the rules of `POST /visits` on the old visit's day,
/// linked by `corrects_visit_id`; move a completed appointment to it; audit `visit.correct`.
/// `new_note`: `None` = keep the old note, `Some(n)` = replace it.
pub(crate) async fn finish_correction(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    started: StartedCorrection,
    mut order: Order,
    new_note: Option<Option<String>>,
    reason: &str,
) -> Result<VisitCorrected, AppError> {
    let StartedCorrection { id, old, old_note } = started;
    // Like `POST /visits`, the stored response never carries a note.
    let before = load_one(tx, cipher, actor, id, false).await?;
    // The desk changes only what was wrong: time, note and companions default to the old visit's.
    // The note is copied here (re-encrypted for the new visit), so it never travels through the
    // browser.
    order.check_in_at = order.check_in_at.or(Some(old.check_in_at));
    order.note = new_note.unwrap_or(old_note);
    if order.companions.is_empty() {
        order.companions.clone_from(&before.companions);
    }
    order.correction = Some(Replaces {
        visit_id: id,
        business_date: old.business_date,
    });
    let new_id = record_in(tx, cipher, actor, order, Utc::now()).await?;
    let appointment = crate::appointments::repo::move_to_correction(tx, id, new_id).await?;

    // Re-read: the old visit now shows the visit that replaced it.
    let voided = load_one(tx, cipher, actor, id, false).await?;
    let visit = load_one(tx, cipher, actor, new_id, false).await?;
    let difference = visit.total - voided.total;
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "visit.correct",
            entity_type: "visit",
            entity_id: id,
            before: Some(json!({
                "visit_id": id, "customer_id": voided.customer_id, "total": voided.total,
            })),
            after: Some(json!({
                "visit_id": new_id, "customer_id": visit.customer_id, "total": visit.total,
                "difference": difference, "appointment_id": appointment,
            })),
            reason: Some(reason),
        },
    )
    .await?;
    Ok(VisitCorrected {
        voided,
        visit,
        difference,
    })
}

// ------------------------------------------------------------ round 6: additions, conversion

/// Upsell on another day changes that month's commissions (Q-U4): refused once its payroll is
/// closed (`409 payroll.period_closed`). Takes the month `FOR SHARE`, so a concurrent close
/// waits for this transaction.
async fn require_payroll_open(tx: &mut TenantTx, day: NaiveDate) -> Result<(), AppError> {
    hr::period::require_open(tx, hr::month::Month::of(day)).await
}

/// `POST /visits/{id}/additions` (`visit.record`, plus what the content needs as on
/// `POST /visits`; another day needs `record.void_closed_day`; Idempotency-Key): "Thêm dịch vụ
/// vào lượt" (US12). The new lines join the visit, each new sale (one-off, packages) is a sale of
/// its own linked to it; existing lines and sales are never touched and no reason is asked (a
/// sale, not a correction). The visit's version moves on.
pub async fn add(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    id: Uuid,
    input: &VisitCreate,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::VisitRecord)?;
    let order = parse_order(actor, input, 0)?;
    if order.lines.is_empty() && order.package_sales.is_empty() {
        return Err(AppError::Validation(vec![FieldError::new(
            "lines",
            "count_out_of_range",
        )]));
    }

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let visit = repo::lock(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    ensure_current(&visit)?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    kernel::corrections::authorize_closed_day(actor, visit.business_date, today)?;
    require_payroll_open(&mut tx, visit.business_date).await?;
    let next_position = repo::next_position(&mut tx, id).await?;
    checkout(
        &mut tx,
        cipher,
        actor,
        order,
        Utc::now(),
        Some(Existing {
            id,
            customer_id: visit.customer_id,
            business_date: visit.business_date,
            next_position,
        }),
    )
    .await?;
    repo::bump_version(&mut tx, id).await?;
    let dto = load_one(&mut tx, cipher, actor, id, false).await?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// One "Tính buổi hôm nay vào gói" request (`POST /visits/{id}/convert-to-package`).
pub struct Conversion<'a> {
    pub id: Uuid,
    /// The visit's version as the desk saw it (`If-Match`).
    pub version: i64,
    pub input: &'a VisitConversion,
}

/// Prefix of the void reason a conversion leaves on the old visit.
const CONVERSION_PREFIX: &str = "Chuyển sang gói ";

/// `POST /visits/{id}/convert-to-package` (`visit.record` + `package.sell` +
/// `record.void_same_day`, `payment.collect` for money; another day needs
/// `record.void_closed_day`; a month whose payroll is closed is `409 payroll.period_closed`;
/// If-Match + Idempotency-Key): today's one-off service becomes session 1 of a package bought
/// now (US11, Q-U1 — the desk's explicit choice, never automatic).
///
/// One correction (US6b machinery): the old visit is voided ("Chuyển sang gói …", its one-off
/// payments reversed on today's books, sessions given back), the package is sold and the visit is
/// recorded again on its own day — the one-off lines the package covers now take its sessions,
/// every other line is recorded unchanged (same technician, referrer, discount share).
/// The money already paid for the visit is recorded again in its own method (a cash payment stays
/// cash on the day's books); the request's `payments` are only the top-up, in the method the desk
/// chose. `difference` in the response = what the customer hands over now ("Khách đưa thêm").
pub async fn convert(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    request: Conversion<'_>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::VisitRecord)?;
    actor.require(Permission::RecordVoidSameDay)?;
    actor.require(Permission::PackageSell)?;
    let mut errors = Vec::new();
    let (mut package_sales, _) = parse_package_sales(
        std::slice::from_ref(&request.input.package_sale),
        &mut errors,
    );
    let payments = sales::service::parse_payments(&request.input.payments, &mut errors);
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let id = request.id;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let plan_id = request.input.package_sale.package_plan_id;
    let plan = catalog::service::plan_snapshot(&mut tx, plan_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let covered: HashSet<Uuid> = plan.items.iter().map(|i| i.service_id).collect();
    let reason: String = format!("{CONVERSION_PREFIX}{}", plan.name)
        .chars()
        .take(kernel::corrections::MAX_REASON)
        .collect();

    let started = begin_correction(&mut tx, cipher, actor, id, request.version, &reason).await?;
    let old = load_one(&mut tx, cipher, actor, id, false).await?;
    // What the customer already paid for the visit keeps its method (cash stays cash); the request's
    // payments are only the top-up ("Khách đưa thêm").
    let carried = sales::service::reversed_by_method(&old.payments);
    require_payroll_open(&mut tx, old.business_date).await?;
    if package_sales[0]
        .beneficiary
        .is_some_and(|b| b != old.customer_id)
    {
        return Err(AppError::Validation(vec![FieldError::new(
            "package_sale.beneficiary_customer_id",
            "not_allowed",
        )]));
    }

    // The old visit, line by line: covered one-off lines take the new package's session.
    let keep_variant = old.customer_gender.is_none();
    let mut converted = 0_usize;
    let mut kept_discount = 0_i64;
    let lines: Vec<LineOrder> = old
        .lines
        .iter()
        .map(|l| {
            let qty = u32::try_from(l.qty).unwrap_or(1);
            match l.source {
                VisitLineSourceDto::OneOff if covered.contains(&l.service_id) => {
                    converted += 1;
                    LineOrder {
                        service_id: l.service_id,
                        source: Source::Package,
                        item_id: None,
                        new_package: Some(0),
                        staff_id: l.staff_id,
                        referrer: None,
                        qty,
                        price_variant: None,
                        override_reason: None,
                    }
                }
                VisitLineSourceDto::OneOff => {
                    kept_discount += l.discount_amount;
                    LineOrder {
                        service_id: l.service_id,
                        source: Source::OneOff,
                        item_id: None,
                        new_package: None,
                        staff_id: l.staff_id,
                        referrer: l.referrer_staff_id,
                        qty,
                        price_variant: l.price_variant.filter(|_| keep_variant),
                        override_reason: None,
                    }
                }
                VisitLineSourceDto::Package => LineOrder {
                    service_id: l.service_id,
                    source: Source::Package,
                    item_id: l.customer_package_item_id,
                    new_package: None,
                    staff_id: l.staff_id,
                    referrer: None,
                    qty,
                    price_variant: None,
                    override_reason: l.expiry_override_reason.clone(),
                },
            }
        })
        .collect();
    if converted == 0 {
        return Err(AppError::Unprocessable {
            code: "visit.nothing_to_convert",
        });
    }
    // The one-off lines kept keep their share of the old discount, with its reason.
    let discount = Money::vnd(kept_discount)
        .ok()
        .filter(|d| *d > Money::ZERO)
        .map(|amount| ServiceDiscount {
            ask: DiscountAsk::Amount(amount),
            reason: old
                .discount_reason
                .clone()
                .unwrap_or_else(|| reason.clone()),
        });
    let order = Order {
        customer_id: old.customer_id,
        check_in_at: None,
        note: None,
        lines,
        payments,
        carried,
        discount,
        package_sales: vec![package_sales.remove(0)],
        companions: old.companions.clone(),
        correction: None,
    };
    let mut done = finish_correction(&mut tx, cipher, actor, started, order, None, &reason).await?;
    // What the customer hands over now: the new package plus the one-off lines kept, minus what
    // the old visit had collected for its one-off lines (reversed).
    done.difference += done
        .visit
        .package_sales
        .iter()
        .map(|p| p.total)
        .sum::<i64>();
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "visit.convert_to_package",
            entity_type: "visit",
            entity_id: id,
            before: Some(json!({"visit_id": id, "total": old.total})),
            after: Some(json!({
                "visit_id": done.visit.id, "package_plan_id": plan_id,
                "package_sale_ids": done.visit.package_sales.iter().map(|p| p.sale_id).collect::<Vec<_>>(),
                "converted_lines": converted, "difference": done.difference,
            })),
            reason: Some(&reason),
        },
    )
    .await?;
    let body = serde_json::to_value(done).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}
