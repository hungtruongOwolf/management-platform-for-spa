//! Adding staff before the staff screen exists (spec #4): CLI `app add-staff`, schema-owner pool.

use kernel::{AppError, FieldError};
use sqlx::PgPool;
use uuid::Uuid;

/// Adds an active staff member to the spa's main branch and returns their id.
pub async fn add_staff(
    owner: &PgPool,
    tenant_slug: &str,
    full_name: &str,
    position: &str,
) -> Result<Uuid, AppError> {
    let (full_name, position) = (full_name.trim(), position.trim());
    let mut errors = Vec::new();
    if full_name.is_empty() || full_name.chars().count() > 200 {
        errors.push(FieldError::new("full_name", "invalid"));
    }
    if position.is_empty() || position.chars().count() > 100 {
        errors.push(FieldError::new("position", "invalid"));
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    let branch = sqlx::query!(
        r#"SELECT b.tenant_id, b.id FROM app.branch b JOIN app.tenant t ON t.id = b.tenant_id
           WHERE t.slug = $1 AND b.active ORDER BY b.created_at, b.id LIMIT 1"#,
        tenant_slug
    )
    .fetch_optional(owner)
    .await?
    .ok_or(AppError::NotFound)?;
    let id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO app.staff (id, tenant_id, branch_id, full_name, position) VALUES ($1, $2, $3, $4, $5)",
        id,
        branch.tenant_id,
        branch.id,
        full_name,
        position
    )
    .execute(owner)
    .await?;
    Ok(id)
}
