-- Experiments behind the db review findings (2026-11), on the seeded throwaway database only.
-- Each block compares the statement the API runs today with a proposed rewrite or index, as the
-- runtime role app_rw with the RLS tenant set. Nothing here is applied to the schema: indexes are
-- created inside a transaction that is rolled back.

\set ON_ERROR_STOP 1
\pset pager off
SELECT id AS tenant FROM app.tenant WHERE slug = 'spa-bench' \gset
SELECT customer_id AS busy_customer FROM app.visit WHERE tenant_id = :'tenant' GROUP BY customer_id ORDER BY count(*) DESC LIMIT 1 \gset
-- Planner settings of the Supabase staging instance (see explain_hot.py).
SET random_page_cost = 1.1;
SET effective_cache_size = '384MB';
SET work_mem = '2184kB';

\echo '===== A. customer search: GIN trigram as the owner (no RLS) vs app_rw (RLS)'
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT id, full_name, phone_e164, gender FROM app.customer
WHERE tenant_id = :'tenant' AND anonymized_at IS NULL AND search_text LIKE '%nguyen thi lan%'
ORDER BY (search_text LIKE 'nguyen thi lan%') DESC, full_name LIMIT 20;
SET ROLE app_rw;
SELECT set_config('app.tenant_id', :'tenant', false) AS tenant_setting;
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT id, full_name, phone_e164, gender FROM app.customer
WHERE tenant_id = :'tenant' AND anonymized_at IS NULL AND search_text LIKE '%nguyen thi lan%'
ORDER BY (search_text LIKE 'nguyen thi lan%') DESC, full_name LIMIT 20;

\echo '===== B. customer history, sale branch: OR over two relations (today) vs UNION of two index lookups'
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT s.id FROM app.sale s
WHERE s.tenant_id = :'tenant' AND (s.customer_id = :'busy_customer' OR EXISTS (
        SELECT 1 FROM app.customer_package cp
        WHERE cp.tenant_id = s.tenant_id AND cp.sale_id = s.id AND cp.customer_id = :'busy_customer'))
  AND NOT EXISTS (SELECT 1 FROM app.visit v WHERE v.tenant_id = s.tenant_id AND v.sale_id = s.id)
  AND NOT EXISTS (SELECT 1 FROM app.visit_line l WHERE l.tenant_id = s.tenant_id AND l.sale_id = s.id);
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT s.id FROM app.sale s
WHERE s.tenant_id = :'tenant' AND s.id IN (
        SELECT x.id FROM app.sale x WHERE x.tenant_id = :'tenant' AND x.customer_id = :'busy_customer'
        UNION
        SELECT cp.sale_id FROM app.customer_package cp
        WHERE cp.tenant_id = :'tenant' AND cp.customer_id = :'busy_customer' AND cp.sale_id IS NOT NULL)
  AND NOT EXISTS (SELECT 1 FROM app.visit v WHERE v.tenant_id = s.tenant_id AND v.sale_id = s.id)
  AND NOT EXISTS (SELECT 1 FROM app.visit_line l WHERE l.tenant_id = s.tenant_id AND l.sale_id = s.id);

\echo '===== C. generic plan of an optional-parameter statement (what a cached prepared plan could become)'
PREPARE vday(uuid, uuid, date) AS
SELECT v.id FROM app.visit v WHERE v.tenant_id = $1 AND ($2::uuid IS NULL OR v.id = $2)
  AND ($3::date IS NULL OR v.business_date = $3) ORDER BY v.check_in_at DESC, v.id DESC LIMIT 500;
SET plan_cache_mode = force_generic_plan;
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF) EXECUTE vday(:'tenant', NULL, current_date);
SET plan_cache_mode = auto;
EXECUTE vday(:'tenant', NULL, current_date); EXECUTE vday(:'tenant', NULL, current_date);
EXECUTE vday(:'tenant', NULL, current_date); EXECUTE vday(:'tenant', NULL, current_date);
EXECUTE vday(:'tenant', NULL, current_date); EXECUTE vday(:'tenant', NULL, current_date);
\echo '-- after 6 executions in auto mode (custom unless the generic plan is about as cheap):'
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF) EXECUTE vday(:'tenant', NULL, current_date);
DEALLOCATE vday;

\echo '===== D. one statement for session + membership + permissions (proposal P2, draft as a plain query)'
RESET ROLE;
SELECT encode(token_hash, 'hex') AS token FROM app.session WHERE revoked_at IS NULL ORDER BY created_at DESC LIMIT 1 \gset
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT s.id, s.user_id, s.active_tenant_id, s.last_seen_at, s.absolute_expires_at, s.recent_auth_at,
       s.mfa_pending, u.mfa_enabled_at IS NOT NULL AS mfa_enrolled, u.must_change_password,
       t.timezone, t.settings ->> 'manager_discount_max_pct' AS discount_cap,
       (SELECT b.id FROM app.branch b WHERE b.tenant_id = t.id AND b.active ORDER BY b.created_at, b.id LIMIT 1) AS branch_id,
       m.id IS NOT NULL AS member,
       coalesce((SELECT array_agg(DISTINCT rp.permission) FROM app.membership_role mr
                 JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
                 WHERE mr.tenant_id = m.tenant_id AND mr.membership_id = m.id), '{}') AS permissions,
       EXISTS (SELECT 1 FROM app.staff st WHERE st.tenant_id = m.tenant_id AND st.user_id = m.user_id AND st.status = 'left') AS former_staff
FROM app.session s
JOIN app.user_account u ON u.id = s.user_id AND u.status = 'active'
LEFT JOIN app.tenant t ON t.id = s.active_tenant_id AND t.status = 'active'
LEFT JOIN app.membership m ON m.tenant_id = t.id AND m.user_id = s.user_id AND m.status = 'active'
WHERE s.token_hash = decode(:'token', 'hex') AND s.revoked_at IS NULL
  AND now() < s.idle_expires_at AND now() < s.absolute_expires_at;

\echo '===== E. sizes of indexes the review proposes to drop (bytes)'
SELECT c.relname AS index, pg_relation_size(c.oid) AS bytes, s.idx_scan
FROM pg_class c JOIN pg_stat_user_indexes s ON s.indexrelid = c.oid
WHERE c.relname IN ('customer_search_trgm', 'customer_phone_trgm', 'visit_line_visit_idx', 'sale_line_sale_idx',
                    'visit_day_idx', 'sale_day_idx', 'payment_day_idx', 'refund_day_idx')
ORDER BY 1;

\echo '===== F. plans once the redundant indexes are gone (dropped inside a transaction, rolled back)'
SELECT array_agg(id)::text AS day_visits FROM app.visit WHERE tenant_id = :'tenant' AND business_date = current_date \gset
SELECT id AS branch FROM app.branch WHERE tenant_id = :'tenant' \gset
BEGIN;
DROP INDEX app.visit_line_visit_idx;
DROP INDEX app.sale_line_sale_idx;
DROP INDEX app.visit_day_idx;
DROP INDEX app.sale_day_idx;
DROP INDEX app.payment_day_idx;
DROP INDEX app.refund_day_idx;
DROP INDEX app.customer_search_trgm;
DROP INDEX app.customer_phone_trgm;
SET LOCAL ROLE app_rw;
SELECT set_config('app.tenant_id', :'tenant', true) AS tenant_setting;
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT l.id FROM app.visit_line l WHERE l.tenant_id = :'tenant' AND l.visit_id = ANY(:'day_visits'::uuid[]) ORDER BY l.visit_id, l.position;
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT s.id, string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
FROM app.sale s JOIN app.sale_line sl ON sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
WHERE s.tenant_id = :'tenant' AND s.business_date = current_date GROUP BY s.id;
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT v.id FROM app.visit v WHERE v.tenant_id = :'tenant' AND v.branch_id = :'branch'
  AND v.business_date BETWEEN current_date - 6 AND current_date ORDER BY v.check_in_at, v.id LIMIT 2000;
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT id FROM app.customer WHERE tenant_id = :'tenant' AND anonymized_at IS NULL AND search_text LIKE '%nguyen thi lan%'
ORDER BY (search_text LIKE 'nguyen thi lan%') DESC, full_name LIMIT 20;
ROLLBACK;
