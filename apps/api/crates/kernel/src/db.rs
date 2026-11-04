//! Tenant-scoped transactions. Every use case runs inside exactly one `TenantTx`, which pins
//! `app.tenant_id` for row-level security and bounds statement time. Network calls never happen
//! while a `TenantTx` is open.

use crate::error::AppError;
use crate::ids::TenantId;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

pub struct TenantTx {
    tx: Transaction<'static, Postgres>,
    tenant: TenantId,
}

impl TenantTx {
    pub async fn begin(pool: &PgPool, tenant: TenantId) -> Result<Self, AppError> {
        let mut tx = pool.begin().await?;
        sqlx::query!(
            "SELECT set_config('app.tenant_id', $1, true) AS tenant, set_config('statement_timeout', '5s', true) AS timeout",
            tenant.to_string()
        )
        .fetch_one(&mut *tx)
        .await?;
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
