//! Startup check that the API's database role cannot step around row-level security
//! (security review L6). RLS is the second tenant-isolation layer (ADR-0003); it is void for a
//! superuser, a BYPASSRLS role, or the owner of the tables. A misconfigured APP_DATABASE_URL
//! (e.g. the owner credentials pasted in) must stop the process, not quietly disable RLS.

use sqlx::PgPool;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UnsafeRole {
    #[error("runtime database role {0:?} is a superuser")]
    Superuser(String),
    #[error("runtime database role {0:?} has BYPASSRLS")]
    BypassRls(String),
    #[error(
        "runtime database role {role:?} owns {tables} table(s) in schema app (RLS does not apply to owners)"
    )]
    OwnsTables { role: String, tables: i64 },
}

/// Refuses a role that is superuser, has BYPASSRLS, or owns (directly or through role
/// membership) any table in schema `app`.
pub async fn check_runtime_role(pool: &PgPool) -> anyhow::Result<()> {
    let (role, superuser, bypass_rls): (String, bool, bool) = sqlx::query_as(
        "SELECT rolname::text, rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(pool)
    .await?;
    if superuser {
        return Err(UnsafeRole::Superuser(role).into());
    }
    if bypass_rls {
        return Err(UnsafeRole::BypassRls(role).into());
    }
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'app' AND c.relkind IN ('r', 'p')
           AND pg_has_role(current_user, c.relowner, 'USAGE')",
    )
    .fetch_one(pool)
    .await?;
    if tables > 0 {
        return Err(UnsafeRole::OwnsTables { role, tables }.into());
    }
    Ok(())
}
