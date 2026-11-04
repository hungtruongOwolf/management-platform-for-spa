//! Shared kernel: identifiers, money and dates, errors, permissions, tenant-scoped transactions,
//! audit and idempotency. Every bounded-context crate depends on this crate and nothing else shared.

pub mod audit;
pub mod business_date;
pub mod db;
pub mod error;
pub mod http;
pub mod idempotency;
pub mod ids;
pub mod money;
pub mod permission;
#[cfg(feature = "testing")]
pub mod testing;

pub use business_date::BusinessDate;
pub use db::TenantTx;
pub use error::{AppError, FieldError};
pub use ids::{BranchId, TenantId, UserId};
pub use money::Money;
pub use permission::{Actor, Permission};

/// Embedded migrations, shared by the `app migrate` command and integration tests.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
