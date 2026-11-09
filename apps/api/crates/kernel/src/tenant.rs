//! Facts about the current spa that use cases need inside their `TenantTx`: its business day,
//! its main branch and policy settings such as the manager discount cap.

use crate::business_date::BusinessDate;
use crate::db::TenantTx;
use crate::error::AppError;
use crate::ids::{BranchId, TenantId};
use chrono::{DateTime, Utc};

/// Manager discount cap when the tenant has not configured one (spec R1-1 D10).
pub const DEFAULT_MANAGER_DISCOUNT_MAX_PCT: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantProfile {
    pub business_date: BusinessDate,
    /// The spa's time zone: which business day an instant (e.g. a visit's check-in) belongs to.
    pub timezone: chrono_tz::Tz,
    /// Branch new records belong to. R1 spas have one branch; per-user branch scope comes with spec #4.
    pub branch: BranchId,
    /// Highest discount, in percent of the subtotal, allowed without `sale.discount_unlimited`.
    pub manager_discount_max_pct: u32,
}

/// The spa's settings behind a [`TenantProfile`], without the clock. Read once per request, in
/// the same statement as the caller's permissions (db review A4), and carried by the `Actor` into
/// every `TenantTx` the request opens, so use cases do not spend a round trip re-reading them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantFacts {
    pub timezone: chrono_tz::Tz,
    pub branch: BranchId,
    pub manager_discount_max_pct: u32,
}

impl TenantFacts {
    /// From the stored row: the time zone name, the raw discount setting, the first active branch.
    pub fn from_row(
        tenant: TenantId,
        timezone: &str,
        manager_discount_max_pct: Option<&str>,
        branch: Option<uuid::Uuid>,
    ) -> Result<Self, AppError> {
        let timezone = BusinessDate::parse_tz(timezone)
            .map_err(|e| AppError::Internal(format!("tenant {tenant}: {e}")))?;
        let branch = branch
            .map(BranchId::from_uuid)
            .ok_or_else(|| AppError::Internal(format!("tenant {tenant} has no active branch")))?;
        Ok(Self {
            timezone,
            branch,
            manager_discount_max_pct: parse_pct(manager_discount_max_pct),
        })
    }

    pub fn profile_at(&self, now: DateTime<Utc>) -> TenantProfile {
        TenantProfile {
            business_date: BusinessDate::of_instant(now, self.timezone),
            timezone: self.timezone,
            branch: self.branch,
            manager_discount_max_pct: self.manager_discount_max_pct,
        }
    }
}

/// Reads the tenant of `tx` (RLS lets a tenant see only its own row); today is the business day
/// of the server clock.
pub async fn profile(tx: &mut TenantTx) -> Result<TenantProfile, AppError> {
    profile_as_of(tx, Utc::now()).await
}

/// Like [`profile`], with today taken from `now` instead of the server clock. Only operator
/// tooling that back-fills history (`app seed-demo`) passes another instant; HTTP use cases
/// always go through [`profile`]. A transaction opened for a request's actor already carries the
/// facts and answers without a query; any other transaction reads them.
pub async fn profile_as_of(
    tx: &mut TenantTx,
    now: DateTime<Utc>,
) -> Result<TenantProfile, AppError> {
    if let Some(facts) = tx.facts() {
        return Ok(facts.profile_at(now));
    }
    Ok(read_facts(tx).await?.profile_at(now))
}

/// The tenant's settings, read inside `tx`.
pub async fn read_facts(tx: &mut TenantTx) -> Result<TenantFacts, AppError> {
    let tenant = tx.tenant();
    let row = sqlx::query!(
        r#"SELECT t.timezone,
                  (t.settings ->> 'manager_discount_max_pct') AS manager_discount_max_pct,
                  (SELECT b.id FROM app.branch b WHERE b.tenant_id = t.id AND b.active
                   ORDER BY b.created_at, b.id LIMIT 1) AS branch_id
           FROM app.tenant t WHERE t.id = $1"#,
        tenant.as_uuid()
    )
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    TenantFacts::from_row(
        tenant,
        &row.timezone,
        row.manager_discount_max_pct.as_deref(),
        row.branch_id,
    )
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

    #[test]
    fn facts_need_a_known_zone_and_an_active_branch() {
        let tenant = TenantId::new();
        let branch = uuid::Uuid::now_v7();
        let facts = TenantFacts::from_row(tenant, "Asia/Ho_Chi_Minh", Some("15"), Some(branch));
        assert_eq!(
            facts.map(|f| (f.branch, f.manager_discount_max_pct)).ok(),
            Some((BranchId::from_uuid(branch), 15))
        );
        assert!(TenantFacts::from_row(tenant, "Mars/Olympus", None, Some(branch)).is_err());
        assert!(TenantFacts::from_row(tenant, "Asia/Ho_Chi_Minh", None, None).is_err());
    }

    #[test]
    fn the_business_day_comes_from_the_clock_not_the_facts() -> Result<(), AppError> {
        let facts = TenantFacts::from_row(
            TenantId::new(),
            "Asia/Ho_Chi_Minh",
            None,
            Some(uuid::Uuid::now_v7()),
        )?;
        // 2026-11-08 18:30 UTC is already 2026-11-09 in Vietnam.
        let late = DateTime::parse_from_rfc3339("2026-11-08T18:30:00Z")
            .map_err(|e| AppError::Internal(e.to_string()))?
            .with_timezone(&Utc);
        assert_eq!(
            facts.profile_at(late).business_date.as_naive().to_string(),
            "2026-11-09"
        );
        Ok(())
    }
}
