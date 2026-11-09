-- 0033: drop 8 indexes no statement of the API can use, or that duplicate another index (db review
-- 2026-11 B1, memory/wiki/architecture/db-review-2026-11.md F5/F6; EM 2026-11-09 Q-DB3).
--
-- * customer_search_trgm, customer_phone_trgm: GIN trigram. `LIKE` / `%` are not LEAKPROOF, so
--   under RLS (the API runs as app_rw) Postgres never turns the search into an index condition:
--   0 scans on staging, a sequential scan of the spa's customers either way (1–1.5 ms at 8 000).
-- * visit_line_visit_idx, sale_line_sale_idx: (tenant_id, x_id) is a prefix of the unique
--   (tenant_id, x_id, position) index, which answers the same lookups.
-- * visit_day_idx, sale_day_idx, payment_day_idx, refund_day_idx: lead with branch_id; 0013 added
--   (tenant_id, business_date) twins that every day/range query uses.
--
-- Nothing here enforces an invariant: no constraint, unique index or FK target is touched.
-- DROP INDEX takes an ACCESS EXCLUSIVE lock for milliseconds; lock_timeout keeps the migration
-- from queueing behind a long query (it fails instead and can simply be run again).
--
-- Rollback (one statement per `-- no-transaction` migration, because CONCURRENTLY cannot run in a
-- transaction block):
--   CREATE INDEX CONCURRENTLY customer_search_trgm ON app.customer USING gin (search_text gin_trgm_ops);
--   CREATE INDEX CONCURRENTLY customer_phone_trgm ON app.customer USING gin (phone_e164 gin_trgm_ops);
--   CREATE INDEX CONCURRENTLY visit_line_visit_idx ON app.visit_line (tenant_id, visit_id);
--   CREATE INDEX CONCURRENTLY sale_line_sale_idx ON app.sale_line (tenant_id, sale_id);
--   CREATE INDEX CONCURRENTLY visit_day_idx ON app.visit (tenant_id, branch_id, business_date, check_in_at DESC);
--   CREATE INDEX CONCURRENTLY sale_day_idx ON app.sale (tenant_id, branch_id, business_date);
--   CREATE INDEX CONCURRENTLY payment_day_idx ON app.payment (tenant_id, branch_id, business_date);
--   CREATE INDEX CONCURRENTLY refund_day_idx ON app.refund (tenant_id, branch_id, business_date);

SET LOCAL lock_timeout = '3s';

DROP INDEX app.customer_search_trgm;
DROP INDEX app.customer_phone_trgm;
DROP INDEX app.visit_line_visit_idx;
DROP INDEX app.sale_line_sale_idx;
DROP INDEX app.visit_day_idx;
DROP INDEX app.sale_day_idx;
DROP INDEX app.payment_day_idx;
DROP INDEX app.refund_day_idx;
