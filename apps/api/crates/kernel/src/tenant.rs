//! Facts about the current spa that use cases need inside their `TenantTx`: its business day,
//! its main branch and policy settings such as the manager discount cap.

use crate::business_date::BusinessDate;
use crate::db::TenantTx;
use crate::error::AppError;
use crate::ids::BranchId;

/// Manager discount cap when the tenant has not configured one (spec R1-1 D10).
pub const DEFAULT_MANAGER_DISCOUNT_MAX_PCT: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantProfile {
    pub business_date: BusinessDate,
    /// Branch new records belong to. R1 spas have one branch; per-user branch scope comes with spec #4.
    pub branch: BranchId,
    /// Highest discount, in percent of the subtotal, allowed without `sale.discount_unlimited`.
    pub manager_discount_max_pct: u32,
}

/// Reads the tenant of `tx` (RLS lets a tenant see only its own row).
pub async fn profile(tx: &mut TenantTx) -> Result<TenantProfile, AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        r#"SELECT t.timezone,
                  (t.settings ->> 'manager_discount_max_pct') AS manager_discount_max_pct,
                  (SELECT b.id FROM app.branch b WHERE b.tenant_id = t.id AND b.active
                   ORDER BY b.created_at, b.id LIMIT 1) AS branch_id
           FROM app.tenant t WHERE t.id = $1"#,
        tenant
    )
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    let tz = BusinessDate::parse_tz(&row.timezone)
        .map_err(|e| AppError::Internal(format!("tenant {tenant}: {e}")))?;
    let branch = row
        .branch_id
        .map(BranchId::from_uuid)
        .ok_or_else(|| AppError::Internal(format!("tenant {tenant} has no active branch")))?;
    Ok(TenantProfile {
        business_date: BusinessDate::today(tz),
        branch,
        manager_discount_max_pct: parse_pct(row.manager_discount_max_pct.as_deref()),
    })
}

/// A missing or malformed setting falls back to the spec default rather than to "unlimited".
fn parse_pct(raw: Option<&str>) -> u32 {
    raw.and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|p| *p <= 100)
        .unwrap_or(DEFAULT_MANAGER_DISCOUNT_MAX_PCT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discount_cap_defaults_safely() {
        assert_eq!(parse_pct(None), 10);
        assert_eq!(parse_pct(Some("15")), 15);
        assert_eq!(parse_pct(Some("abc")), 10);
        assert_eq!(parse_pct(Some("150")), 10);
        assert_eq!(parse_pct(Some("0")), 0);
    }
}
