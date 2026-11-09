//! Tenant-scoped transactions. Every use case runs inside exactly one `TenantTx`, which pins
//! `app.tenant_id` for row-level security and bounds statement time. Network calls never happen
//! while a `TenantTx` is open.

use crate::error::AppError;
use crate::ids::TenantId;
use sqlx::{AssertSqlSafe, PgConnection, PgPool, Postgres, Transaction};

/// A use case never waits on anything outside the database while its transaction is open; a
/// transaction left idle this long (a stuck task, a bug) is ended by Postgres and its locks freed
/// (db review C4). Transaction-local like the other settings, so nothing else is affected.
pub const IDLE_IN_TRANSACTION: &str = "30s";

/// `BEGIN` and the transaction-local settings in one simple-query round trip (perf page §3.4:
/// every request opened two transactions at two round trips each before its first real query).
/// `set_config(..., true)` keeps RLS exactly as before: it lasts until this transaction ends and
/// never leaks to the next user of the pooled connection. If a setting fails, the transaction
/// is aborted and `begin_with` rolls it back. The tenant id is formatted from a typed UUID
/// (hex and hyphens only), so it cannot carry SQL.
fn begin_statement(tenant: TenantId) -> AssertSqlSafe<String> {
    let id: uuid::Uuid = tenant.as_uuid();
    AssertSqlSafe(format!(
        "BEGIN; SELECT set_config('app.tenant_id', '{}', true), \
         set_config('statement_timeout', '5s', true), \
         set_config('idle_in_transaction_session_timeout', '{IDLE_IN_TRANSACTION}', true)",
        id.hyphenated()
    ))
}

pub struct TenantTx {
    tx: Transaction<'static, Postgres>,
    tenant: TenantId,
}

impl TenantTx {
    pub async fn begin(pool: &PgPool, tenant: TenantId) -> Result<Self, AppError> {
        let tx = pool.begin_with(begin_statement(tenant)).await?;
        Ok(Self { tx, tenant })
    }

    pub const fn tenant(&self) -> TenantId {
        self.tenant
    }

    /// The connection to run queries on. Repositories still pass `tenant()` explicitly in every query.
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await.map_err(AppError::from)
    }
}
