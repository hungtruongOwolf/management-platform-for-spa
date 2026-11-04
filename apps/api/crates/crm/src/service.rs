//! Customer use cases (US2). Health notes: encrypted at rest, decrypted only for
//! `customer.health_notes.read`, written only with `customer.health_notes.write`, redacted in audit.

use crate::cipher::{FieldCipher, customer_aad};
use crate::repo::{self, CustomerFields, CustomerRow, SummaryRow};
use crate::text::{clean_name, like_escape, search_key};
use kernel::audit::{self, AuditEntry, REDACTED};
use kernel::{Actor, AppError, FieldError, Permission, PhoneE164, TenantTx};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

pub const SEARCH_LIMIT: i64 = 20;
const MAX_HEALTH_NOTE: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Female,
    Male,
}

impl Gender {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "female" => Some(Self::Female),
            "male" => Some(Self::Male),
            _ => None,
        }
    }
}

pub struct ProfileInput {
    pub full_name: String,
    pub phone: String,
    pub gender: Option<Gender>,
    pub birth_year: Option<i16>,
    pub address: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
}

/// A customer as the caller may see it: `health_note` is `None` when the caller lacks permission
/// (the field is then omitted from the response entirely, AC2.6).
pub struct CustomerView {
    pub row: CustomerRow,
    pub health_note: Option<Option<String>>,
}

struct Clean {
    full_name: String,
    search_text: String,
    phone: PhoneE164,
    address: Option<String>,
    source: Option<String>,
    note: Option<String>,
}

fn optional(text: Option<&String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

fn validate(input: &ProfileInput) -> Result<Clean, AppError> {
    let mut errors = Vec::new();
    let full_name = clean_name(&input.full_name);
    match full_name.chars().count() {
        0 => errors.push(FieldError::new("full_name", "required")),
        n if n > 200 => errors.push(FieldError::new("full_name", "too_long")),
        _ => {}
    }
    let phone = PhoneE164::parse(&input.phone);
    if phone.is_err() {
        errors.push(FieldError::new("phone", "invalid_format"));
    }
    if input
        .birth_year
        .is_some_and(|y| !(1900..=2100).contains(&y))
    {
        errors.push(FieldError::new("birth_year", "out_of_range"));
    }
    let (address, source, note) = (
        optional(input.address.as_ref()),
        optional(input.source.as_ref()),
        optional(input.note.as_ref()),
    );
    for (field, value, max) in [
        ("address", &address, 500),
        ("source", &source, 200),
        ("note", &note, 2000),
    ] {
        if value.as_ref().is_some_and(|v| v.chars().count() > max) {
            errors.push(FieldError::new(field, "too_long"));
        }
    }
    match phone {
        Ok(phone) if errors.is_empty() => Ok(Clean {
            search_text: search_key(&full_name),
            full_name,
            phone,
            address,
            source,
            note,
        }),
        _ => Err(AppError::Validation(errors)),
    }
}

fn clean_health_note(note: Option<&str>) -> Result<Option<String>, AppError> {
    let note = note
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned);
    if note
        .as_ref()
        .is_some_and(|n| n.chars().count() > MAX_HEALTH_NOTE)
    {
        return Err(AppError::Validation(vec![FieldError::new(
            "health_note",
            "too_long",
        )]));
    }
    Ok(note)
}

fn view(cipher: &FieldCipher, actor: &Actor, row: CustomerRow) -> Result<CustomerView, AppError> {
    let health_note = if actor.can(Permission::CustomerHealthNotesRead) {
        let aad = customer_aad(actor.tenant_id.as_uuid(), row.id);
        let plain = row
            .health_note_ct
            .as_deref()
            .map(|ct| cipher.decrypt(ct, &aad))
            .transpose()
            .map_err(|e| AppError::Internal(format!("health note of {}: {e}", row.id)))?;
        Some(plain)
    } else {
        None
    };
    Ok(CustomerView { row, health_note })
}

fn audit_snapshot(row: &CustomerRow) -> serde_json::Value {
    json!({
        "full_name": row.full_name, "phone_e164": row.phone_e164, "gender": row.gender,
        "birth_year": row.birth_year, "health_note": row.health_note_ct.as_ref().map(|_| REDACTED),
    })
}

pub async fn create(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    input: ProfileInput,
    health_note: Option<&str>,
) -> Result<CustomerView, AppError> {
    actor.require(Permission::CustomerWrite)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let view = create_in(&mut tx, cipher, actor, input, health_note).await?;
    tx.commit().await?;
    Ok(view)
}

/// Creates a customer inside the caller's transaction, so another use case can create the
/// customer and its own records atomically (S9 opening balance). Checks `customer.write` (and
/// `customer.health_notes.write` when a note is given) itself. A duplicate live phone is
/// `409 customer.phone_taken` with `existing_id`; field errors name the profile fields
/// (`full_name`, `phone`, …).
pub async fn create_in(
    tx: &mut TenantTx,
    cipher: &FieldCipher,
    actor: &Actor,
    input: ProfileInput,
    health_note: Option<&str>,
) -> Result<CustomerView, AppError> {
    actor.require(Permission::CustomerWrite)?;
    let health_note = clean_health_note(health_note)?;
    if health_note.is_some() {
        actor.require(Permission::CustomerHealthNotesWrite)?;
    }
    let c = validate(&input)?;
    let id = Uuid::now_v7();
    let ct = health_note
        .as_deref()
        .map(|note| cipher.encrypt(note, &customer_aad(actor.tenant_id.as_uuid(), id)))
        .transpose()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let fields = CustomerFields {
        full_name: &c.full_name,
        search_text: &c.search_text,
        phone_e164: c.phone.as_str(),
        gender: input.gender.map(Gender::as_str),
        birth_year: input.birth_year,
        address: c.address.as_deref(),
        source: c.source.as_deref(),
        note: c.note.as_deref(),
    };
    repo::insert(tx, id, &fields, ct.as_deref()).await?;
    let row = repo::get(tx, id).await?.ok_or(AppError::NotFound)?;
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "customer.create",
            entity_type: "customer",
            entity_id: id,
            before: None,
            after: Some(audit_snapshot(&row)),
            reason: None,
        },
    )
    .await?;
    view(cipher, actor, row)
}

pub async fn get(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Uuid,
) -> Result<CustomerView, AppError> {
    actor.require(Permission::CustomerRead)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let row = repo::get(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    view(cipher, actor, row)
}

pub async fn update(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Uuid,
    expected_version: i64,
    input: ProfileInput,
) -> Result<CustomerView, AppError> {
    actor.require(Permission::CustomerWrite)?;
    let c = validate(&input)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let before = repo::get(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let fields = CustomerFields {
        full_name: &c.full_name,
        search_text: &c.search_text,
        phone_e164: c.phone.as_str(),
        gender: input.gender.map(Gender::as_str),
        birth_year: input.birth_year,
        address: c.address.as_deref(),
        source: c.source.as_deref(),
        note: c.note.as_deref(),
    };
    if !repo::update(&mut tx, id, expected_version, &fields).await? {
        return Err(if repo::exists(&mut tx, id).await? {
            AppError::StaleVersion
        } else {
            AppError::NotFound
        });
    }
    let after = repo::get(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "customer.update",
            entity_type: "customer",
            entity_id: id,
            before: Some(audit_snapshot(&before)),
            after: Some(audit_snapshot(&after)),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    view(cipher, actor, after)
}

pub async fn set_health_note(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    id: Uuid,
    expected_version: i64,
    note: Option<&str>,
) -> Result<CustomerView, AppError> {
    actor.require(Permission::CustomerHealthNotesWrite)?;
    let note = clean_health_note(note)?;
    let ct = note
        .as_deref()
        .map(|n| cipher.encrypt(n, &customer_aad(actor.tenant_id.as_uuid(), id)))
        .transpose()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    if !repo::set_health_note(&mut tx, id, expected_version, ct.as_deref()).await? {
        return Err(if repo::exists(&mut tx, id).await? {
            AppError::StaleVersion
        } else {
            AppError::NotFound
        });
    }
    let row = repo::get(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "customer.health_note_set",
            entity_type: "customer",
            entity_id: id,
            before: None,
            after: Some(json!({"health_note": if note.is_some() { REDACTED } else { "cleared" }})),
            reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    view(cipher, actor, row)
}

/// Whether a live (not anonymised) customer with this id exists in the caller's tenant, checked
/// inside the caller's transaction. Other contexts use it to answer `404` for foreign ids.
/// The caller authorizes.
pub async fn exists_in(tx: &mut TenantTx, id: Uuid) -> Result<bool, AppError> {
    repo::exists(tx, id).await
}

/// AC2.1: ≥ 3 digits → phone substring; ≥ 2 letters → accent-insensitive name; empty → most recent.
pub async fn search(
    pool: &PgPool,
    actor: &Actor,
    query: Option<&str>,
) -> Result<Vec<SummaryRow>, AppError> {
    actor.require(Permission::CustomerRead)?;
    let query = query.map(str::trim).unwrap_or_default();
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let rows = if query.is_empty() {
        repo::recent(&mut tx, SEARCH_LIMIT).await?
    } else if query.chars().all(|c| {
        c.is_ascii_digit() || c.is_whitespace() || matches!(c, '.' | '-' | '+' | '(' | ')')
    }) {
        let digits: String = query.chars().filter(char::is_ascii_digit).collect();
        // Search the national number: drop a leading trunk 0 or country code 84.
        let national = digits
            .strip_prefix("84")
            .filter(|_| query.trim_start().starts_with('+'))
            .or_else(|| digits.strip_prefix('0'))
            .unwrap_or(&digits);
        if national.len() < 3 {
            return Err(AppError::Validation(vec![FieldError::new(
                "q",
                "too_short",
            )]));
        }
        repo::search_phone(
            &mut tx,
            &format!("+84%{}%", like_escape(national)),
            SEARCH_LIMIT,
        )
        .await?
    } else {
        let key = search_key(query);
        if key.chars().count() < 2 {
            return Err(AppError::Validation(vec![FieldError::new(
                "q",
                "too_short",
            )]));
        }
        let escaped = like_escape(&key);
        repo::search_name(
            &mut tx,
            &format!("%{escaped}%"),
            &format!("{escaped}%"),
            SEARCH_LIMIT,
        )
        .await?
    };
    tx.commit().await?;
    Ok(rows)
}
