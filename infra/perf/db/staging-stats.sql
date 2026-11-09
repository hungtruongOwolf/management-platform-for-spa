-- Read-only catalog and statistics snapshot of schema `app` (db review 2026-11).
-- Safe on any environment: SELECTs on catalogs and pg_stat_* views only.

\echo '== server'
SELECT version();
SELECT name, setting, unit FROM pg_settings
WHERE name IN ('shared_buffers', 'work_mem', 'random_page_cost', 'effective_cache_size',
               'max_connections', 'track_io_timing', 'default_transaction_isolation', 'jit',
               'data_checksums', 'pg_stat_statements.track', 'pg_stat_statements.max')
ORDER BY name;
SELECT extname, extversion, n.nspname AS schema
FROM pg_extension e JOIN pg_namespace n ON n.oid = e.extnamespace ORDER BY 1;
SELECT stats_reset, xact_commit, xact_rollback, blks_read, blks_hit,
       round(100.0 * blks_hit / nullif(blks_hit + blks_read, 0), 2) AS hit_pct
FROM pg_stat_database WHERE datname = current_database();
SELECT pg_size_pretty(pg_database_size(current_database())) AS db_size;

\echo '== objects in app'
SELECT c.relkind, count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'app' GROUP BY 1 ORDER BY 1;

\echo '== tables: size, rows, scans, writes'
SELECT c.relname,
       s.n_live_tup AS live, s.n_dead_tup AS dead,
       pg_size_pretty(pg_table_size(c.oid)) AS heap_toast,
       pg_size_pretty(pg_indexes_size(c.oid)) AS indexes,
       pg_total_relation_size(c.oid) AS total_bytes,
       s.seq_scan, s.seq_tup_read, s.idx_scan,
       s.n_tup_ins AS ins, s.n_tup_upd AS upd, s.n_tup_hot_upd AS hot_upd, s.n_tup_del AS del
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid
WHERE n.nspname = 'app' AND c.relkind = 'r'
ORDER BY total_bytes DESC, c.relname;

\echo '== indexes: size and scans'
SELECT s.relname AS table, s.indexrelname AS index, s.idx_scan, s.idx_tup_read,
       pg_relation_size(s.indexrelid) AS bytes,
       i.indisunique AS uniq
FROM pg_stat_user_indexes s JOIN pg_index i ON i.indexrelid = s.indexrelid
WHERE s.schemaname = 'app'
ORDER BY s.idx_scan, bytes DESC;

\echo '== TOAST per table'
SELECT c.relname, pg_size_pretty(pg_relation_size(c.reltoastrelid)) AS toast
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'app' AND c.relkind = 'r' AND c.reltoastrelid <> 0
  AND pg_relation_size(c.reltoastrelid) > 0
ORDER BY pg_relation_size(c.reltoastrelid) DESC;
