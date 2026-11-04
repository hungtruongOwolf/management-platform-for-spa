//! Customer SQL. Every query is tenant-scoped explicitly; RLS is the second layer.

use chrono::{DateTime, Utc};
use kernel::{AppError, TenantTx};
use uuid::Uuid;

pub struct CustomerRow {
    pub id: Uuid,
    pub full_name: String,
    pub phone_e164: String,
    pub gender: Option<String>,
    pub birth_year: Option<i16>,
    pub address: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
    pub health_note_ct: Option<Vec<u8>>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

pub struct CustomerFields<'a> {
    pub full_name: &'a str,
    pub search_text: &'a str,
    pub phone_e164: &'a str,
    pub gender: Option<&'a str>,
    pub birth_year: Option<i16>,
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
        r#"INSERT INTO app.customer (id, tenant_id, full_name, search_text, phone_e164, gender, birth_year, address, source, note, health_note_ct)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
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
        health_note_ct
    )
    .execute(tx.conn())
    .await;
    match result {
        Ok(_) => Ok(()),
        Err(e) if is_unique_violation(&e) => {
            sqlx::query("ROLLBACK TO SAVEPOINT customer_insert")
                .execute(tx.conn())
                .await?;
            let existing = phone_owner(tx, f.phone_e164)
                .await?
                .ok_or(AppError::Database(e))?;
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
               source = $10, note = $11, version = version + 1, updated_at = now()
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
        f.note
    )
    .execute(tx.conn())
    .await;
    match result {
        Ok(done) => Ok(done.rows_affected() == 1),
        Err(e) if is_unique_violation(&e) => {
            sqlx::query("ROLLBACK TO SAVEPOINT customer_update")
                .execute(tx.conn())
                .await?;
            let existing = phone_owner(tx, f.phone_e164)
                .await?
                .ok_or(AppError::Database(e))?;
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
        r#"SELECT id, full_name, phone_e164, gender, birth_year, address, source, note, health_note_ct, version, created_at
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
    pub full_name: String,
    pub phone_e164: String,
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

pub async fn recent(tx: &mut TenantTx, limit: i64) -> Result<Vec<SummaryRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        SummaryRow,
        r#"SELECT id, full_name, phone_e164, gender FROM app.customer
           WHERE tenant_id = $1 AND anonymized_at IS NULL ORDER BY updated_at DESC LIMIT $2"#,
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
