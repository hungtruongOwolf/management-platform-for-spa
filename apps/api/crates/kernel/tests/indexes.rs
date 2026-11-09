//! Index hygiene of migration 0033 (db review 2026-11 B1): the eight indexes no statement can use,
//! or that duplicate another, are gone; the indexes that answer the same lookups, and every
//! constraint-backed index, are still there.

use sqlx::PgPool;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn index_def(pool: &PgPool, name: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT pg_get_indexdef(i.indexrelid) FROM pg_index i
         JOIN pg_class c ON c.oid = i.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'app' AND c.relname = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
}

const DROPPED: [&str; 8] = [
    "customer_search_trgm",
    "customer_phone_trgm",
    "visit_line_visit_idx",
    "sale_line_sale_idx",
    "visit_day_idx",
    "sale_day_idx",
    "payment_day_idx",
    "refund_day_idx",
];

/// What takes over each dropped index's lookups, with the leading columns it must keep.
const KEPT: [(&str, &str); 7] = [
    (
        "visit_line_tenant_id_visit_id_position_key",
        "(tenant_id, visit_id, \"position\")",
    ),
    (
        "sale_line_tenant_id_sale_id_position_key",
        "(tenant_id, sale_id, \"position\")",
    ),
    ("visit_tenant_day_idx", "(tenant_id, business_date)"),
    ("sale_tenant_day_idx", "(tenant_id, business_date)"),
    ("payment_tenant_day_idx", "(tenant_id, business_date)"),
    ("refund_tenant_day_idx", "(tenant_id, business_date)"),
    ("customer_phone_uq", "(tenant_id, phone_e164)"),
];

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn redundant_indexes_are_gone_and_their_replacements_stay(owner: PgPool) -> TestResult {
    for name in DROPPED {
        assert_eq!(index_def(&owner, name).await?, None, "{name} is dropped");
    }
    for (name, columns) in KEPT {
        let def = index_def(&owner, name).await?.unwrap_or_default();
        assert!(
            def.contains(columns),
            "{name} still serves {columns}: {def:?}"
        );
    }
    // Constraint-backed indexes (unique / exclusion / FK targets) are never part of the cleanup.
    let constraint_indexes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint c JOIN pg_namespace n ON n.oid = c.connamespace
         WHERE n.nspname = 'app' AND c.contype IN ('p', 'u', 'x') AND c.conindid = 0",
    )
    .fetch_one(&owner)
    .await?;
    assert_eq!(
        constraint_indexes, 0,
        "every key constraint keeps its index"
    );
    Ok(())
}
