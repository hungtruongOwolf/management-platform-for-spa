//! Staff SQL. Tenant-scoped explicitly; RLS is the second layer.

use kernel::{AppError, TenantTx};
use uuid::Uuid;

pub struct StaffRow {
    pub id: Uuid,
    pub full_name: String,
    pub position: String,
    pub status: String,
}

pub async fn list(tx: &mut TenantTx, status: Option<&str>) -> Result<Vec<StaffRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        StaffRow,
        r#"SELECT id, full_name, position, status FROM app.staff
           WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2)
           ORDER BY status, full_name"#,
        tenant,
        status
    )
    .fetch_all(tx.conn())
    .await?)
}

/// Locks the row (`FOR SHARE`) so the staff member cannot be marked as left while the caller's
/// transaction records them as closer or technician.
pub async fn get_for_share(tx: &mut TenantTx, id: Uuid) -> Result<Option<StaffRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query_as!(
        StaffRow,
        r#"SELECT id, full_name, position, status FROM app.staff
           WHERE tenant_id = $1 AND id = $2 FOR SHARE"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?)
}
