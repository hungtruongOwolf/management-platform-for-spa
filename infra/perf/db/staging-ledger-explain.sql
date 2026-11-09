-- Read-only: plan of the ledger statement (GET /reports/ledger, this month) on staging, as app_rw
-- with the RLS tenant set when the owner may assume that role (db review 2026-11). SELECT only.
SELECT id AS tenant FROM app.tenant ORDER BY created_at LIMIT 1 \gset
SELECT pg_has_role(current_user, 'app_rw', 'USAGE') AS can_be_app_rw \gset
\if :can_be_app_rw
SET LOCAL ROLE app_rw;
\endif
SELECT set_config('app.tenant_id', :'tenant', true) IS NOT NULL AS tenant_set, current_user;
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT b.id, b.seq, b.entry, b.business_date, b.at, b.customer_id, app.customer_label(c.full_name),
       c.gender, b.sale_kind,
       (SELECT string_agg(DISTINCT app.customer_label(o.full_name), ', ')
        FROM app.customer_package cp JOIN app.customer o ON o.tenant_id = cp.tenant_id AND o.id = cp.customer_id
        WHERE cp.tenant_id = b.tenant_id AND cp.sale_id = b.sale_id AND cp.customer_id <> b.customer_id),
       b.one_off, b.package, b.bank_transfer, b.cash, b.unpaid, b.note, b.voided, b.visit_id, b.upsell_by
FROM app.revenue_book b
JOIN app.customer c ON c.tenant_id = b.tenant_id AND c.id = b.customer_id
WHERE b.tenant_id = :'tenant' AND b.business_date BETWEEN date_trunc('month', current_date)::date AND current_date
ORDER BY b.business_date, b.seq
LIMIT 200;
