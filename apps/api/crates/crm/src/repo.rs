//! Customer SQL. Every query is tenant-scoped explicitly; RLS is the second layer.

use chrono::{DateTime, NaiveDate, Utc};
use kernel::{AppError, TenantTx};
use uuid::Uuid;

pub struct CustomerRow {
    pub id: Uuid,
    /// `None` = anonymous walk-in profile.
    pub full_name: Option<String>,
    pub phone_e164: Option<String>,
    pub gender: Option<String>,
    pub birth_year: Option<i16>,
    pub birth_date: Option<NaiveDate>,
    pub address: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
    pub health_note_ct: Option<Vec<u8>>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

pub struct CustomerFields<'a> {
    pub full_name: Option<&'a str>,
    pub search_text: &'a str,
    pub phone_e164: Option<&'a str>,
    pub gender: Option<&'a str>,
    pub birth_year: Option<i16>,
    pub birth_date: Option<NaiveDate>,
    pub address: Option<&'a str>,
    pub source: Option<&'a str>,
    pub note: Option<&'a str>,
}

/// Unique violation on the live phone index → id of the profile that already has this number.
async fn phone_owner(tx: &mut TenantTx, phone: &str) -> Result<Option<Uuid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        "SELECT id FROM app.customer WHERE tenant_id = $1 AND phone_e164 = $2 AND anonymized_at IS NULL",
        tenant,
        phone
    )
    .fetch_optional(tx.conn())
    .await?)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|c| c == "23505")
}

pub async fn insert(
    tx: &mut TenantTx,
    id: Uuid,
    f: &CustomerFields<'_>,
    health_note_ct: Option<&[u8]>,
) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    // Savepoint so a duplicate phone can be reported with the existing id inside the same transaction.
    sqlx::query("SAVEPOINT customer_insert")
        .execute(tx.conn())
        .await?;
    let result = sqlx::query!(
        r#"INSERT INTO app.customer (id, tenant_id, full_name, search_text, phone_e164, gender, birth_year, address, source, note, health_note_ct, birth_date)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
        id,
        tenant,
        f.full_name,
        f.search_text,
        f.phone_e164,
        f.gender,
        f.birth_year,
        f.address,
        f.source,
        f.note,
        health_note_ct,
        f.birth_date
    )
    .execute(tx.conn())
    .await;
    match result {
        Ok(_) => Ok(()),
        Err(e) if is_unique_violation(&e) => {
            sqlx::query("ROLLBACK TO SAVEPOINT customer_insert")
                .execute(tx.conn())
                .await?;
            let Some(phone) = f.phone_e164 else {
                return Err(AppError::Database(e));
            };
            let existing = phone_owner(tx, phone).await?.ok_or(AppError::Database(e))?;
            Err(AppError::ConflictWith {
                code: "customer.phone_taken",
                existing_id: existing,
            })
        }
        Err(e) => Err(AppError::Database(e)),
    }
}

/// Optimistic full update of profile fields (not the health note). `Ok(false)` = id/version mismatch.
pub async fn update(
    tx: &mut TenantTx,
    id: Uuid,
    expected_version: i64,
    f: &CustomerFields<'_>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query("SAVEPOINT customer_update")
        .execute(tx.conn())
        .await?;
    let result = sqlx::query!(
        r#"UPDATE app.customer
           SET full_name = $4, search_text = $5, phone_e164 = $6, gender = $7, birth_year = $8, address = $9,
               source = $10, note = $11, birth_date = $12, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3 AND anonymized_at IS NULL"#,
        tenant,
        id,
        expected_version,
        f.full_name,
        f.search_text,
        f.phone_e164,
        f.gender,
        f.birth_year,
        f.address,
        f.source,
        f.note,
        f.birth_date
    )
    .execute(tx.conn())
    .await;
    match result {
        Ok(done) => Ok(done.rows_affected() == 1),
        Err(e) if is_unique_violation(&e) => {
            sqlx::query("ROLLBACK TO SAVEPOINT customer_update")
                .execute(tx.conn())
                .await?;
            let Some(phone) = f.phone_e164 else {
                return Err(AppError::Database(e));
            };
            let existing = phone_owner(tx, phone).await?.ok_or(AppError::Database(e))?;
            Err(AppError::ConflictWith {
                code: "customer.phone_taken",
                existing_id: existing,
            })
        }
        Err(e) => Err(AppError::Database(e)),
    }
}

pub async fn set_health_note(
    tx: &mut TenantTx,
    id: Uuid,
    expected_version: i64,
    ct: Option<&[u8]>,
) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    let done = sqlx::query!(
        r#"UPDATE app.customer SET health_note_ct = $4, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND version = $3 AND anonymized_at IS NULL"#,
        tenant,
        id,
        expected_version,
        ct
    )
    .execute(tx.conn())
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn get(tx: &mut TenantTx, id: Uuid) -> Result<Option<CustomerRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query_as!(
        CustomerRow,
        r#"SELECT id, full_name, phone_e164, gender, birth_year, birth_date, address, source, note, health_note_ct, version, created_at
           FROM app.customer WHERE tenant_id = $1 AND id = $2 AND anonymized_at IS NULL"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?;
    Ok(row)
}

pub struct SummaryRow {
    pub id: Uuid,
    pub full_name: Option<String>,
    pub phone_e164: Option<String>,
    pub gender: Option<String>,
}

/// Phone substring search (digits of the national number, e.g. "905123").
pub async fn search_phone(
    tx: &mut TenantTx,
    digits_pattern: &str,
    limit: i64,
) -> Result<Vec<SummaryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        SummaryRow,
        r#"SELECT id, full_name, phone_e164, gender FROM app.customer
           WHERE tenant_id = $1 AND anonymized_at IS NULL AND phone_e164 LIKE $2
           ORDER BY full_name LIMIT $3"#,
        tenant,
        digits_pattern,
        limit
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Accent-insensitive name search; names starting with the query rank first.
pub async fn search_name(
    tx: &mut TenantTx,
    contains: &str,
    starts_with: &str,
    limit: i64,
) -> Result<Vec<SummaryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        SummaryRow,
        r#"SELECT id, full_name, phone_e164, gender FROM app.customer
           WHERE tenant_id = $1 AND anonymized_at IS NULL AND search_text LIKE $2
           ORDER BY (search_text LIKE $3) DESC, full_name LIMIT $4"#,
        tenant,
        contains,
        starts_with,
        limit
    )
    .fetch_all(tx.conn())
    .await?)
}

/// The most recently touched named profiles. Anonymous walk-in profiles stay out: a list of
/// "Khách vãng lai" rows tells the desk nothing (they are reached from their visit instead).
pub async fn recent(tx: &mut TenantTx, limit: i64) -> Result<Vec<SummaryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        SummaryRow,
        r#"SELECT id, full_name, phone_e164, gender FROM app.customer
           WHERE tenant_id = $1 AND anonymized_at IS NULL AND full_name IS NOT NULL
           ORDER BY updated_at DESC LIMIT $2"#,
        tenant,
        limit
    )
    .fetch_all(tx.conn())
    .await?)
}

pub async fn exists(tx: &mut TenantTx, id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        "SELECT 1 AS one FROM app.customer WHERE tenant_id = $1 AND id = $2 AND anonymized_at IS NULL",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?
    .is_some())
}

/// Whether any package (open or closed) belongs to this customer.
pub async fn owns_packages(tx: &mut TenantTx, id: Uuid) -> Result<bool, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM app.customer_package
                          WHERE tenant_id = $1 AND customer_id = $2) AS "owns!""#,
        tenant,
        id
    )
    .fetch_one(tx.conn())
    .await?)
}

pub struct BriefRow {
    pub full_name: Option<String>,
    pub gender: Option<String>,
}

pub async fn brief(tx: &mut TenantTx, id: Uuid) -> Result<Option<BriefRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        BriefRow,
        "SELECT full_name, gender FROM app.customer WHERE tenant_id = $1 AND id = $2 AND anonymized_at IS NULL",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

/// The live customer holding this phone in the caller's tenant (duplicate check while typing).
pub async fn phone_holder(
    tx: &mut TenantTx,
    phone_e164: &str,
) -> Result<Option<(Uuid, Option<String>)>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        "SELECT id, full_name FROM app.customer WHERE tenant_id = $1 AND phone_e164 = $2 AND anonymized_at IS NULL",
        tenant,
        phone_e164
    )
    .fetch_optional(tx.conn())
    .await?
    .map(|r| (r.id, r.full_name)))
}

// ------------------------------------------------------------------------------ follow-ups

pub struct FollowUpInsert<'a> {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub contacted_on: NaiveDate,
    pub channel: &'a str,
    pub outcome: &'a str,
    pub note: Option<&'a str>,
    pub next_follow_up_on: Option<NaiveDate>,
    pub created_by: Uuid,
}

pub struct FollowUpRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub contacted_on: NaiveDate,
    pub channel: String,
    pub outcome: String,
    pub note: Option<String>,
    pub next_follow_up_on: Option<NaiveDate>,
    pub created_by: Uuid,
    pub by_name: String,
    pub created_at: DateTime<Utc>,
}

pub async fn insert_follow_up(tx: &mut TenantTx, f: &FollowUpInsert<'_>) -> Result<(), AppError> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.customer_follow_up
             (id, tenant_id, customer_id, contacted_on, channel, outcome, note, next_follow_up_on, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        f.id,
        tenant,
        f.customer_id,
        f.contacted_on,
        f.channel,
        f.outcome,
        f.note,
        f.next_follow_up_on,
        f.created_by
    )
    .execute(tx.conn())
    .await?;
    Ok(())
}

pub async fn follow_up(tx: &mut TenantTx, id: Uuid) -> Result<Option<FollowUpRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        FollowUpRow,
        r#"SELECT f.id, f.customer_id, f.contacted_on, f.channel, f.outcome, f.note,
                  f.next_follow_up_on, f.created_by, u.full_name AS by_name, f.created_at
           FROM app.customer_follow_up f
           JOIN app.user_account u ON u.id = f.created_by
           WHERE f.tenant_id = $1 AND f.id = $2"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}

/// A customer's follow-ups, newest contact first (then newest recorded).
pub async fn follow_ups(
    tx: &mut TenantTx,
    customer_id: Uuid,
    limit: i64,
) -> Result<Vec<FollowUpRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        FollowUpRow,
        r#"SELECT f.id, f.customer_id, f.contacted_on, f.channel, f.outcome, f.note,
                  f.next_follow_up_on, f.created_by, u.full_name AS by_name, f.created_at
           FROM app.customer_follow_up f
           JOIN app.user_account u ON u.id = f.created_by
           WHERE f.tenant_id = $1 AND f.customer_id = $2
           ORDER BY f.contacted_on DESC, f.created_at DESC, f.id DESC
           LIMIT $3"#,
        tenant,
        customer_id,
        limit
    )
    .fetch_all(tx.conn())
    .await?)
}
