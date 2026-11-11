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
    /// The instant audit entries written in this transaction carry; `None` = the database clock.
    clock: Option<DateTime<Utc>>,
}

impl TenantTx {
    pub async fn begin(pool: &PgPool, tenant: TenantId) -> Result<Self, AppError> {
        let tx = pool.begin_with(begin_statement(tenant)).await?;
        Ok(Self {
            tx,
            tenant,
            facts: None,
            clock: None,
        })
    }

    /// The transaction of a request's use case: the actor's spa, and the spa settings the actor
    /// carries, so `kernel::tenant::profile` needs no query.
    pub async fn for_actor(pool: &PgPool, actor: &Actor) -> Result<Self, AppError> {
        let mut tx = Self::begin(pool, actor.tenant_id).await?;
        tx.facts = actor.tenant_facts().copied();
        tx.clock = actor.clock();
        Ok(tx)
    }

    /// The transaction of a use case that runs on the clock `now` (the `*_as_of` forms, ADR-0021):
    /// its audit entries are stamped `now`, so history back-filled by `app seed-demo` reads as
    /// written on its own day. HTTP handlers pass the server clock.
    pub async fn for_actor_at(
        pool: &PgPool,
        actor: &Actor,
        now: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let mut tx = Self::for_actor(pool, actor).await?;
        tx.clock = Some(now);
        Ok(tx)
    }

    /// The instant this transaction's audit entries carry (`None`: the database clock).
    pub const fn clock(&self) -> Option<DateTime<Utc>> {
        self.clock
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

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await.map_err(AppError::from)
    }
}
