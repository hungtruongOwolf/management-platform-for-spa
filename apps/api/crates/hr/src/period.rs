//! Payroll months: open or closed, and who works in one.
//!
//! Locking: every write of a month's records takes the month's `payroll_period` row `FOR SHARE`
//! and refuses a closed month; the close takes it `FOR UPDATE`. So a write either commits before
//! the close computes the payslips, or waits and then sees the month closed — never lands after
//! a payslip was frozen. The `reject_closed_period` trigger is the backstop.

use crate::month::Month;
use crate::repo;
use chrono::{DateTime, NaiveDate, Utc};
use kernel::{AppError, TenantTx};
use org::profile::Profile;

/// `409 payroll.period_closed`: the month's records are read-only; correct in a later month.
pub const fn closed() -> AppError {
    AppError::Conflict {
        code: "payroll.period_closed",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodInfo {
    pub closed: bool,
    pub closed_at: Option<DateTime<Utc>>,
    pub closed_by_name: Option<String>,
}

pub async fn info(tx: &mut TenantTx, month: Month) -> Result<PeriodInfo, AppError> {
    Ok(match repo::period(tx, month.first_day()).await? {
        None => PeriodInfo {
            closed: false,
            closed_at: None,
            closed_by_name: None,
        },
        Some(p) => PeriodInfo {
            closed: p.status == "closed",
            closed_at: p.closed_at,
            closed_by_name: p.closed_by_name,
        },
    })
}

/// Locks the month for a write of its records; `409 payroll.period_closed` if it is closed.
pub async fn require_open(tx: &mut TenantTx, month: Month) -> Result<(), AppError> {
    if repo::lock_period(tx, month.first_day(), false).await? == "closed" {
        Err(closed())
    } else {
        Ok(())
    }
}

/// Whether `p` worked at the spa on at least one day of `month`.
pub fn employed_in(p: &Profile, month: Month) -> bool {
    p.employment.overlaps(month.first_day(), month.last_day())
}

/// Today in the spa's time zone.
pub async fn today(tx: &mut TenantTx) -> Result<NaiveDate, AppError> {
    Ok(kernel::tenant::profile(tx).await?.business_date.as_naive())
}
