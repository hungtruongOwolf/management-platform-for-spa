//! Platform admin console (ADR-0018): the operator principal (its own accounts, sessions, cookie
//! and MFA), and what operators do across spas — list, create and suspend spas, with a platform
//! audit trail of every action.
//!
//! This crate is the only place whose SQL names `app.platform_*`. Cross-tenant work goes through
//! the owner-defined functions of migration 0038, each of which checks the operator's session
//! hash itself; work inside one spa runs in that spa's `TenantTx` (its RLS). Nothing here reads
//! customer, visit, note or pay rows.

pub mod audit;
pub mod auth;
pub mod http;
pub mod operator;
pub mod tenants;

/// Map a refusal of a platform DB function (`platform_gate`: SQLSTATE 42501) to a 401: the Rust
/// extractor already checked the session, so this only fires when the session ended in between
/// (or a code path forgot the extractor — which is the point of the gate).
pub(crate) fn gate_refusal(error: sqlx::Error) -> kernel::AppError {
    let refused = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "42501");
    if refused {
        kernel::AppError::Unauthenticated
    } else {
        kernel::AppError::Database(error)
    }
}
