//! Visit use cases (US4). Recording a visit is one transaction: idempotency claim, technician
//! checks, server-side pricing, atomic session consumption, the sale of the one-off lines with its
//! payments, the visit and its lines, the audit trail and the stored response — or nothing at all.
//!
//! The session note is health data: encrypted at rest (AAD = tenant + visit), written only with
//! `customer.health_notes.write`, returned only on a single visit and only with
//! `customer.health_notes.read`, never stored in the idempotent response nor in audit.

use crate::domain::{self, CheckInError};
use crate::dto::{
    VisitCreate, VisitDto, VisitLineDto, VisitLineSourceDto, VisitStatusDto, VisitUpdate,
};
use crate::repo::{self, LineInsert, LineRow, VisitInsert, VisitRow};
use catalog::{PriceLookupError, PriceVariant};
use chrono::{DateTime, NaiveDate, Utc};
use kernel::audit::{self, AuditEntry, REDACTED};
use kernel::crypto::FieldCipher;
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Money, Permission, TenantTx};
use packages::{Consume, Consumed};
use sales::service::{Payment, ServiceLine, ServiceSale};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const MAX_LINES: usize = 20;
pub const MAX_QTY: u32 = 10;
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
    staff_id: Uuid,
    qty: u32,
    price_variant: Option<PriceVariant>,
    override_reason: Option<String>,
}

/// The request after shape validation; nothing is checked against the database yet.
struct Order {
    customer_id: Uuid,
    check_in_at: Option<DateTime<Utc>>,
    note: Option<String>,
    lines: Vec<LineOrder>,
    payments: Vec<Payment>,
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

fn parse(actor: &Actor, input: &VisitCreate) -> Result<Order, AppError> {
    let mut errors = Vec::new();
    if input.lines.is_empty() || input.lines.len() > MAX_LINES {
        errors.push(FieldError::new("lines", "count_out_of_range"));
    }
    let mut lines = Vec::with_capacity(input.lines.len());
    for (i, l) in input.lines.iter().enumerate() {
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
        if source == Source::OneOff {
            if l.customer_package_item_id.is_some() {
                errors.push(FieldError::new(
                    format!("lines[{i}].customer_package_item_id"),
                    "not_allowed",
                ));
            }
            if override_reason.is_some() {
                errors.push(FieldError::new(
                    format!("lines[{i}].expiry_override_reason"),
                    "not_allowed",
                ));
            }
        }
        lines.push(LineOrder {
            service_id: l.service_id,
            source,
            item_id: l.customer_package_item_id,
            staff_id,
            qty,
            price_variant: l.price_variant,
            override_reason,
        });
    }
    let note = match clean_note(input.note.as_ref()) {
        Ok(n) => n,
        Err(AppError::Validation(e)) => {
            errors.extend(e);
            None
        }
        Err(e) => return Err(e),
    };
    let payments = sales::service::parse_payments(&input.payments, &mut errors);
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    if note.is_some() {
        actor.require(Permission::CustomerHealthNotesWrite)?;
    }
    if lines.iter().any(|l| l.override_reason.is_some()) {
        actor.require(Permission::PackageUseExpiredOverride)?;
    }
    Ok(Order {
        customer_id: input.customer_id,
        check_in_at: input.check_in_at,
        note,
        lines,
        payments,
    })
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
}

/// `POST /visits` (`visit.record`; `payment.collect` when money is taken,
/// `customer.health_notes.write` for a note, `package.use_expired_override` for an override).
/// Returns the HTTP status and body, also stored under the idempotency key: a retry with the same
/// key and body replays them without consuming anything twice.
pub async fn record(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    input: &VisitCreate,
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
    let tenant = kernel::tenant::profile(&mut tx).await?;
    let today = tenant.business_date.as_naive();
    let check_in_at = domain::check_in_at(order.check_in_at, Utc::now(), tenant.timezone, today)
        .map_err(|e| AppError::Unprocessable {
            code: match e {
                CheckInError::InFuture => "visit.check_in_in_future",
                CheckInError::NotToday => "visit.check_in_not_today",
            },
        })?;
    let customer = crm::service::brief_in(&mut tx, order.customer_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let mut verified = HashSet::new();
    for line in &order.lines {
        if verified.insert(line.staff_id) {
            org::service::require_active(&mut tx, line.staff_id).await?;
        }
    }

    // Every amount comes from the catalog or the package, read inside this transaction.
    let mut priced = Vec::with_capacity(order.lines.len());
    for (i, line) in order.lines.iter().enumerate() {
        priced.push(match line.source {
            Source::OneOff => {
                let service = catalog::service::service_snapshot(&mut tx, line.service_id)
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
            Source::Package => {
                let item_id = match line.item_id {
                    Some(id) => id,
                    None => packages::service::default_item(
                        &mut tx,
                        customer.id,
                        line.service_id,
                        today,
                    )
                    .await?
                    .ok_or(AppError::Unprocessable {
                        code: "package.item_not_found",
                    })?,
                };
                Priced::Package { item_id }
            }
        });
    }

    // Consume in item order so two multi-line visits racing for the same items lock rows in the
    // same order and cannot deadlock.
    let mut package_lines: Vec<(usize, Uuid)> = priced
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match p {
            Priced::Package { item_id } => Some((i, *item_id)),
            Priced::OneOff { .. } => None,
        })
        .collect();
    package_lines.sort_by_key(|(i, item)| (*item, *i));
    let mut consumed: HashMap<usize, Consumed> = HashMap::new();
    for (i, item_id) in package_lines {
        let line = &order.lines[i];
        let c = packages::service::consume_session(
            &mut tx,
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
        consumed.insert(i, c);
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
            Priced::Package { .. } => None,
        })
        .collect();
    let branch = tenant.branch.as_uuid();
    let sale = sales::service::record_service_sale(
        &mut tx,
        actor,
        ServiceSale {
            customer_id: customer.id,
            business_date: today,
            branch_id: branch,
            lines: &service_lines,
            payments: &order.payments,
        },
    )
    .await?;

    let visit_id = Uuid::now_v7();
    let note_ct = encrypt_note(cipher, &tx, visit_id, order.note.as_deref())?;
    repo::insert_visit(
        &mut tx,
        &VisitInsert {
            id: visit_id,
            branch_id: branch,
            customer_id: customer.id,
            business_date: today,
            check_in_at,
            note_ct: note_ct.as_deref(),
            sale_id: sale.map(|s| s.sale_id),
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    let mut audit_lines = Vec::with_capacity(order.lines.len());
    let mut overrides = Vec::new();
    for (position, (i, (line, p))) in (0_i32..).zip(order.lines.iter().zip(&priced).enumerate()) {
        let qty = i32::try_from(line.qty).map_err(|_| AppError::Internal("qty".into()))?;
        let insert = match p {
            Priced::OneOff {
                name,
                variant,
                unit_price,
            } => LineInsert {
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
                    .map_err(|_| AppError::Internal("line value overflow".into()))?
                    .as_i64(),
                expiry_override_reason: None,
            },
            Priced::Package { item_id } => {
                let c = consumed
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
                }
            }
        };
        audit_lines.push(json!({
            "service_id": insert.service_id, "source": insert.source,
            "customer_package_item_id": insert.customer_package_item_id, "staff_id": insert.staff_id,
            "qty": insert.qty, "price_variant": insert.price_variant,
            "unit_price": insert.unit_price, "line_value": insert.line_value,
        }));
        repo::insert_line(&mut tx, &insert).await?;
    }
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "visit.create",
            entity_type: "visit",
            entity_id: visit_id,
            before: None,
            after: Some(json!({
                "customer_id": customer.id, "check_in_at": check_in_at,
                "sale_id": sale.map(|s| s.sale_id), "total": sale.map_or(0, |s| s.total.as_i64()),
                "note": order.note.as_ref().map(|_| REDACTED), "lines": audit_lines,
            })),
            reason: None,
        },
    )
    .await?;
    for (package_id, item_id, reason) in overrides {
        audit::record(
            &mut tx,
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

    // The stored response never carries the note: the idempotency table is not encrypted.
    let dto = load(&mut tx, cipher, actor, Some(visit_id), None, false)
        .await?
        .pop()
        .ok_or_else(|| AppError::Internal("visit vanished".into()))?;
    let body = serde_json::to_value(&dto).map_err(|e| AppError::Internal(e.to_string()))?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
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
        customer_package_id: l.customer_package_id,
        customer_package_item_id: l.customer_package_item_id,
        package_name: l.package_name,
        sessions_left_after: l.sessions_left_after,
        sessions_total: l.sessions_total,
        staff_id: l.staff_id,
        staff_name: l.staff_name,
        qty: l.qty,
        unit_price: l.unit_price,
        line_value: l.line_value,
        expiry_override_reason: l.expiry_override_reason,
    })
}

/// Visits with their lines and payments in three queries (no N+1). `with_note` decrypts the
/// note for callers holding `customer.health_notes.read`; otherwise the key is omitted.
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
    let sale_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.sale_id).collect();
    let mut payments = sales::service::payments_of(tx, &sale_ids).await?;
    let read_note = with_note && actor.can(Permission::CustomerHealthNotesRead);
    let tenant = tx.tenant().as_uuid();
    rows.into_iter()
        .map(|r| {
            let lines = lines.remove(&r.id).unwrap_or_default();
            let total = lines
                .iter()
                .filter(|l| l.source == VisitLineSourceDto::OneOff)
                .map(|l| l.line_value)
                .sum();
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
            Ok(VisitDto {
                status: if r.status == "voided" {
                    VisitStatusDto::Voided
                } else {
                    VisitStatusDto::Completed
                },
                payments: r
                    .sale_id
                    .and_then(|s| payments.remove(&s))
                    .unwrap_or_default(),
                void_reason: r.void_reason,
                voided_at: r.voided_at,
                id: r.id,
                customer_id: r.customer_id,
                customer_name: r.customer_name,
                business_date: r.business_date,
                check_in_at: r.check_in_at,
                note,
                sale_id: r.sale_id,
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
/// first. Notes are never listed.
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
    let visits = load(&mut tx, cipher, actor, None, Some(day), false).await?;
    tx.commit().await?;
    Ok(visits)
}

/// `PATCH /visits/{id}` (`visit.record`, If-Match): correct the note or a line's technician.
/// Sessions and money are never touched here (voids are S8). Only today's visits, unless the
/// caller may correct closed days (`record.void_closed_day`) and re-authenticated recently (🔒).
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

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let (business_date, status, current, _) = repo::lock_visit(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if status == "voided" {
        return Err(AppError::Conflict {
            code: "visit.voided",
        });
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    // Same 🔒 rule as voids: a closed day needs `record.void_closed_day` and a recent re-auth.
    kernel::corrections::authorize_closed_day(actor, business_date, today)?;
    if current != version {
        return Err(AppError::StaleVersion);
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
    if !repo::update_visit(&mut tx, id, version, note_ct.as_ref().map(Option::as_deref)).await? {
        return Err(AppError::StaleVersion);
    }
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "visit.update",
            entity_type: "visit",
            entity_id: id,
            before: Some(json!({"version": version})),
            after: Some(json!({
                "version": version + 1, "staff": staff_changes,
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

/// `POST /visits/{id}/void` (`record.void_same_day`; a closed day needs `record.void_closed_day`
/// and a recent re-auth; Idempotency-Key): the visit stays in history as `voided` with its reason,
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
    let reason = kernel::corrections::reason(reason)?;

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    let visit = repo::lock_for_void(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if visit.status != "completed" {
        return Err(AppError::Conflict {
            code: "visit.voided",
        });
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    kernel::corrections::authorize_void(actor, visit.business_date, today)?;
    if !repo::mark_voided(&mut tx, id, &reason, actor.user_id.as_uuid()).await? {
        return Err(AppError::Conflict {
            code: "visit.voided",
        });
    }

    let mut restored = Vec::new();
    for line in repo::package_lines(&mut tx, id).await? {
        let qty = u32::try_from(line.qty).map_err(|_| AppError::Internal("qty".into()))?;
        let r = packages::lifecycle::restore_sessions(&mut tx, line.customer_package_item_id, qty)
            .await?;
        restored.push(json!({
            "customer_package_id": line.customer_package_id,
            "customer_package_item_id": line.customer_package_item_id,
            "qty": line.qty, "sessions_remaining": r.sessions_remaining,
        }));
    }
    let reversals = match visit.sale_id {
        Some(sale_id) => {
            sales::voids::void_in(&mut tx, actor, sale_id, &reason, today)
                .await?
                .reversals
        }
        None => Vec::new(),
    };
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "visit.void",
            entity_type: "visit",
            entity_id: id,
            before: Some(json!({"status": "completed", "business_date": visit.business_date})),
            after: Some(json!({
                "status": "voided", "restored": restored, "sale_id": visit.sale_id,
                "reversed": reversals.iter().map(|(_, _, amount)| amount).sum::<i64>(),
            })),
            reason: Some(&reason),
        },
    )
    .await?;

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
