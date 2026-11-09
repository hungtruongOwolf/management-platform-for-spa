//! Customer use cases (US2). Health notes: encrypted at rest, decrypted only for
//! `customer.health_notes.read`, written only with `customer.health_notes.write`, redacted in audit.

use crate::cipher::{FieldCipher, customer_aad};
use crate::repo::{self, CustomerFields, CustomerRow, SummaryRow};
use crate::text::{clean_name, like_escape, search_key};
use chrono::{Datelike, NaiveDate};
use kernel::audit::{self, AuditEntry, REDACTED};
use kernel::{Actor, AppError, FieldError, Permission, PhoneE164, TenantTx};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

pub const SEARCH_LIMIT: i64 = 20;
/// Longest search query: a full name or a phone number fits many times over.
pub const MAX_QUERY_CHARS: usize = 100;
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
    /// `None`/blank = a profile without a name: an anonymous walk-in ("Khách vãng lai").
    pub full_name: Option<String>,
    /// `None`/blank = no phone (decision E: optional on every profile).
    pub phone: Option<String>,
    pub gender: Option<Gender>,
    pub birth_year: Option<i16>,
    /// Date of birth (birthday promotions later). Sets `birth_year` when that is left out.
    pub birth_date: Option<NaiveDate>,
    pub address: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
}

/// How a profile without a name reads everywhere (lists, visits, reports, payroll). SQL read
/// models use its twin `app.customer_label(full_name)` (migration 0023).
pub const WALK_IN_LABEL: &str = "Khách vãng lai";

/// The name a customer is shown by: the recorded name, or [`WALK_IN_LABEL`].
pub fn label(full_name: Option<&str>) -> &str {
    full_name.unwrap_or(WALK_IN_LABEL)
}

/// Oldest plausible age for a date of birth.
const MAX_AGE_YEARS: i32 = 120;

/// A customer as the caller may see it: `health_note` is `None` when the caller lacks permission
/// (the field is then omitted from the response entirely, AC2.6).
pub struct CustomerView {
    pub row: CustomerRow,
    pub health_note: Option<Option<String>>,
}

struct Clean {
    full_name: Option<String>,
    search_text: String,
    phone: Option<PhoneE164>,
    birth_year: Option<i16>,
    address: Option<String>,
    source: Option<String>,
    note: Option<String>,
}

impl Clean {
    fn fields<'a>(&'a self, input: &'a ProfileInput) -> CustomerFields<'a> {
        CustomerFields {
            full_name: self.full_name.as_deref(),
            search_text: &self.search_text,
            phone_e164: self.phone.as_ref().map(PhoneE164::as_str),
            gender: input.gender.map(Gender::as_str),
            birth_year: self.birth_year,
            birth_date: input.birth_date,
            address: self.address.as_deref(),
            source: self.source.as_deref(),
            note: self.note.as_deref(),
        }
    }
}

fn optional(text: Option<&String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// Field rules of a profile. `today` is the spa's business day (a birth date is never after it).
fn validate(input: &ProfileInput, today: NaiveDate) -> Result<Clean, AppError> {
    let mut errors = Vec::new();
    let full_name = input
        .full_name
        .as_deref()
        .map(clean_name)
        .filter(|n| !n.is_empty());
    if full_name.as_ref().is_some_and(|n| n.chars().count() > 200) {
        errors.push(FieldError::new("full_name", "too_long"));
    }
    let phone = match input
        .phone
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        None => None,
        Some(raw) => {
            let parsed = PhoneE164::parse(raw).ok();
            if parsed.is_none() {
                errors.push(FieldError::new("phone", "invalid_format"));
            }
            parsed
        }
    };
    if input
        .birth_year
        .is_some_and(|y| !(1900..=2100).contains(&y))
    {
        errors.push(FieldError::new("birth_year", "out_of_range"));
    }
    let mut birth_year = input.birth_year;
    if let Some(born) = input.birth_date {
        let floor = today
            .with_year(today.year() - MAX_AGE_YEARS)
            .unwrap_or(today);
        if born > today {
            errors.push(FieldError::new("birth_date", "in_future"));
        } else if born < floor {
            errors.push(FieldError::new("birth_date", "too_old"));
        }
        let year = i16::try_from(born.year()).ok();
        match input.birth_year {
            None => birth_year = year,
            Some(y) if Some(y) != year => {
                errors.push(FieldError::new("birth_year", "conflicts_with_birth_date"));
            }
            Some(_) => {}
        }
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
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    Ok(Clean {
        // A nameless profile has no search key: no name search can match it.
        search_text: full_name.as_deref().map(search_key).unwrap_or_default(),
        full_name,
        phone,
        birth_year,
        address,
        source,
        note,
    })
}

async fn spa_today(tx: &mut TenantTx) -> Result<NaiveDate, AppError> {
    Ok(kernel::tenant::profile(tx).await?.business_date.as_naive())
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

/// What a customer audit row keeps of a profile (ADR-0019 D13, Nghị định 13/2023): which personal
/// fields are filled, never their values. The trail outlives an anonymised profile (`audit_log`
/// has no retention), so a name, phone or birth date written here would never go away. Migration
/// 0047 rewrote the rows written before this shape.
fn audit_presence(row: &CustomerRow) -> serde_json::Value {
    json!({
        "has_name": row.full_name.is_some(), "has_phone": row.phone_e164.is_some(),
        "gender": row.gender, "has_birth_year": row.birth_year.is_some(),
        "has_birth_date": row.birth_date.is_some(), "has_health_note": row.health_note_ct.is_some(),
    })
}

/// Profile fields an update changed, named like the API fields (their values are not recorded).
fn changed_fields(before: &CustomerRow, after: &CustomerRow) -> Vec<&'static str> {
    [
        ("full_name", before.full_name != after.full_name),
        ("phone", before.phone_e164 != after.phone_e164),
        ("gender", before.gender != after.gender),
        ("birth_year", before.birth_year != after.birth_year),
        ("birth_date", before.birth_date != after.birth_date),
        ("address", before.address != after.address),
        ("source", before.source != after.source),
        ("note", before.note != after.note),
    ]
    .into_iter()
    .filter_map(|(field, changed)| changed.then_some(field))
    .collect()
}

pub async fn create(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    input: ProfileInput,
    health_note: Option<&str>,
) -> Result<CustomerView, AppError> {
    actor.require(Permission::CustomerWrite)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
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
    let c = validate(&input, spa_today(tx).await?)?;
    let id = Uuid::now_v7();
    let ct = health_note
        .as_deref()
        .map(|note| cipher.encrypt(note, &customer_aad(actor.tenant_id.as_uuid(), id)))
        .transpose()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    repo::insert(tx, id, &c.fields(&input), ct.as_deref()).await?;
    let row = repo::get(tx, id).await?.ok_or(AppError::NotFound)?;
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "customer.create",
            entity_type: "customer",
            entity_id: id,
            before: None,
            after: Some(audit_presence(&row)),
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let c = validate(&input, spa_today(&mut tx).await?)?;
    let before = repo::get(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    // A package owner stays someone the spa can tell apart (same rule as selling one).
    if c.full_name.is_none() && repo::owns_packages(&mut tx, id).await? {
        return Err(AppError::Unprocessable {
            code: "customer.name_required",
        });
    }
    if !repo::update(&mut tx, id, expected_version, &c.fields(&input)).await? {
        return Err(if repo::exists(&mut tx, id).await? {
            AppError::StaleVersion
        } else {
            AppError::NotFound
        });
    }
    let after = repo::get(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let mut after_audit = audit_presence(&after);
    after_audit["changed"] = json!(changed_fields(&before, &after));
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "customer.update",
            entity_type: "customer",
            entity_id: id,
            before: Some(audit_presence(&before)),
            after: Some(after_audit),
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
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

/// What another context needs to price and label a visit: name and recorded gender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerBrief {
    pub id: Uuid,
    /// `None` = an anonymous walk-in profile (shown as [`WALK_IN_LABEL`]).
    pub full_name: Option<String>,
    pub gender: Option<Gender>,
}

impl CustomerBrief {
    /// Who may own a package: someone the spa can tell apart later (EM 2026-11-07). An anonymous
    /// walk-in gets a name on the profile first.
    pub fn require_named(&self) -> Result<(), AppError> {
        if self.full_name.is_some() {
            Ok(())
        } else {
            Err(AppError::Unprocessable {
                code: "customer.name_required",
            })
        }
    }
}

/// Records the gender the desk picked for a profile without one (F2: the Nữ / Nam chosen to price
/// a visit), inside the caller's transaction. Needs `customer.write`; without it, or when the
/// profile already has a gender, nothing changes (never an overwrite). Audited as a profile edit.
pub async fn fill_gender_in(
    tx: &mut TenantTx,
    actor: &Actor,
    id: Uuid,
    gender: Gender,
) -> Result<(), AppError> {
    if !actor.can(Permission::CustomerWrite) || !repo::fill_gender(tx, id, gender.as_str()).await? {
        return Ok(());
    }
    audit::record(
        tx,
        actor,
        AuditEntry {
            action: "customer.update",
            entity_type: "customer",
            entity_id: id,
            before: Some(json!({ "gender": null })),
            after: Some(json!({ "gender": gender.as_str(), "changed": ["gender"] })),
            reason: Some("gender picked to price a visit"),
        },
    )
    .await
}

/// A live customer of the caller's tenant, read inside the caller's transaction (`None` = not
/// here, which callers answer with `404`). The caller authorizes.
pub async fn brief_in(tx: &mut TenantTx, id: Uuid) -> Result<Option<CustomerBrief>, AppError> {
    Ok(repo::brief(tx, id).await?.map(|r| CustomerBrief {
        id,
        gender: r.gender.as_deref().and_then(Gender::parse),
        full_name: r.full_name,
    }))
}

/// The live customer holding `phone` in the caller's tenant, inside the caller's transaction
/// (e.g. a booking for a "newcomer" who already has a record). The caller authorizes.
pub async fn phone_holder_in(
    tx: &mut TenantTx,
    phone: &PhoneE164,
) -> Result<Option<(Uuid, String)>, AppError> {
    Ok(repo::phone_holder(tx, phone.as_str())
        .await?
        .map(|(id, name)| (id, label(name.as_deref()).to_owned())))
}

/// Result of a duplicate-phone check while the number is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhoneCheck {
    /// Not (yet) a valid Vietnamese mobile number: nothing to look up.
    Invalid,
    Free,
    Taken {
        id: Uuid,
        full_name: String,
    },
}

/// `GET /customers/phone-check?phone=` (`customer.read`): whether a live customer of the caller's
/// spa already has this number, normalised exactly like create (`PhoneE164::parse`). A partial
/// number answers `Invalid` instead of an error so the form can call it on every keystroke.
/// Reveals no more than `GET /customers?q=<digits>` already does to the same caller.
pub async fn phone_check(
    pool: &PgPool,
    actor: &Actor,
    phone: Option<&str>,
) -> Result<PhoneCheck, AppError> {
    actor.require(Permission::CustomerRead)?;
    let raw = phone.map(str::trim).unwrap_or_default();
    if raw.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::Validation(vec![FieldError::new(
            "phone", "too_long",
        )]));
    }
    let Ok(phone) = PhoneE164::parse(raw) else {
        return Ok(PhoneCheck::Invalid);
    };
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let holder = phone_holder_in(&mut tx, &phone).await?;
    tx.commit().await?;
    Ok(match holder {
        Some((id, full_name)) => PhoneCheck::Taken { id, full_name },
        None => PhoneCheck::Free,
    })
}

/// AC2.1: ≥ 3 digits → phone substring; ≥ 2 letters → accent-insensitive name; empty → most recent.
pub async fn search(
    pool: &PgPool,
    actor: &Actor,
    query: Option<&str>,
) -> Result<Vec<SummaryRow>, AppError> {
    actor.require(Permission::CustomerRead)?;
    let query = query.map(str::trim).unwrap_or_default();
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::Validation(vec![FieldError::new("q", "too_long")]));
    }
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let rows = if query.is_empty() {
        repo::recent(&mut tx, SEARCH_LIMIT).await?
    } else if query.chars().all(|c| {
        c.is_ascii_digit() || c.is_whitespace() || matches!(c, '.' | '-' | '+' | '(' | ')')
    }) {
        let digits: String = query.chars().filter(char::is_ascii_digit).collect();
        // Search the national number. A whole number in any form the phone check accepts
        // (`84905…` included) is its national part; otherwise drop a leading trunk 0, or a
        // country code 84 written after '+' (a bare "849…" may be part of a number).
        let whole = PhoneE164::parse(query).ok();
        let national = match &whole {
            Some(phone) => phone.as_str().strip_prefix("+84").unwrap_or(phone.as_str()),
            None => digits
                .strip_prefix("84")
                .filter(|_| query.trim_start().starts_with('+'))
                .or_else(|| digits.strip_prefix('0'))
                .unwrap_or(&digits),
        };
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
