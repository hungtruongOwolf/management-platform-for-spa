//! Tenant-scoped transactions. Every use case runs inside exactly one `TenantTx`, which pins
//! `app.tenant_id` for row-level security and bounds statement time. Network calls never happen
//! while a `TenantTx` is open.

use crate::error::AppError;
use crate::ids::TenantId;
use crate::permission::Actor;
use crate::tenant::TenantFacts;
use chrono::{DateTime, Utc};
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
    /// The spa's settings when the request already read them with its permissions (db review A4).
    facts: Option<TenantFacts>,
    /// The instant every audit entry of this transaction carries (ADR-0023): set up front only by
    /// a back-filling actor ([`Actor::at_clock`]); otherwise fixed by the database clock when the
    /// first entry is written, after the use case's writes.
    pub(crate) clock: Option<DateTime<Utc>>,
    /// Entries recorded but not written yet ([`crate::audit`]): [`Self::commit`] or
    /// [`Self::write_audit`] writes them.
    pub(crate) audit: Vec<crate::audit::Pending>,
}

impl TenantTx {
    pub async fn begin(pool: &PgPool, tenant: TenantId) -> Result<Self, AppError> {
        let tx = pool.begin_with(begin_statement(tenant)).await?;
        Ok(Self {
            tx,
            tenant,
            facts: None,
            clock: None,
            audit: Vec::new(),
        })
    }

    /// The transaction of a request's use case: the actor's spa, and the spa settings the actor
    /// carries, so `kernel::tenant::profile` needs no query. A back-filling actor's clock
    /// ([`Actor::at_clock`], `app seed-demo`) stamps the audit entries; a request's never does.
    pub async fn for_actor(pool: &PgPool, actor: &Actor) -> Result<Self, AppError> {
        let mut tx = Self::begin(pool, actor.tenant_id).await?;
        tx.facts = actor.tenant_facts().copied();
        tx.clock = actor.clock();
        Ok(tx)
    }

    /// Writes the audit entries recorded so far. [`Self::commit`] does it; a query that reads this
    /// transaction's own entries (a reply's "Đã sửa") calls it first, and only once the use case's
    /// writes are done: the first write fixes the instant every entry of the transaction carries
    /// (ADR-0023), so it must come after every row lock the use case takes.
    pub async fn write_audit(&mut self) -> Result<(), AppError> {
        crate::audit::write_pending(self).await
    }

    /// The spa settings carried in from the actor, if any.
    pub const fn facts(&self) -> Option<&TenantFacts> {
        self.facts.as_ref()
    }

    pub const fn tenant(&self) -> TenantId {
        self.tenant
    }

    /// The connection to run queries on. Repositories still pass `tenant()` explicitly in every query.
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    /// Writes the pending audit entries, then commits.
    pub async fn commit(mut self) -> Result<(), AppError> {
        self.write_audit().await?;
        self.tx.commit().await.map_err(AppError::from)
    }
}
