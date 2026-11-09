-- Read-only invariant checks over the copies the schema stores on purpose (db review 2026-11):
-- every row returned is drift between a stored value and the data it copies. Zero rows everywhere
-- = clean. Safe on any environment (SELECT only); run as the schema owner (sees every spa).
--
--   bash infra/perf/db/staging-readonly.sh infra/perf/db/invariants.sql

\echo '== 1. sale.amount_paid = payments − reversals (open sales) / 0 (voided sales)'
SELECT s.tenant_id, s.id, s.status, s.amount_paid, coalesce(p.paid, 0) AS paid, coalesce(p.reversed, 0) AS reversed
FROM app.sale s
LEFT JOIN LATERAL (
  SELECT sum(amount) FILTER (WHERE kind = 'payment') AS paid, sum(amount) FILTER (WHERE kind = 'reversal') AS reversed
  FROM app.payment WHERE tenant_id = s.tenant_id AND sale_id = s.id) p ON true
WHERE (s.status = 'open' AND s.amount_paid <> coalesce(p.paid, 0) - coalesce(p.reversed, 0))
   OR (s.status = 'voided' AND (s.amount_paid <> 0 OR coalesce(p.paid, 0) <> coalesce(p.reversed, 0)))
LIMIT 20;

\echo '== 2. payment.branch_id = sale.branch_id'
SELECT p.tenant_id, p.id FROM app.payment p JOIN app.sale s ON s.tenant_id = p.tenant_id AND s.id = p.sale_id
WHERE p.branch_id <> s.branch_id LIMIT 20;

\echo '== 3. visit.sale_id <-> sale.visit_id (two-way link) and visit_line.sale_id belongs to the visit'
SELECT v.tenant_id, v.id AS visit_id, v.sale_id FROM app.visit v JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.id = v.sale_id
WHERE s.visit_id IS DISTINCT FROM v.id
UNION ALL
SELECT l.tenant_id, l.visit_id, l.sale_id FROM app.visit_line l JOIN app.sale s ON s.tenant_id = l.tenant_id AND s.id = l.sale_id
WHERE s.visit_id IS DISTINCT FROM l.visit_id
LIMIT 20;

\echo '== 4. a voided visit has no open sale'
SELECT v.tenant_id, v.id, s.id AS open_sale FROM app.visit v JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.visit_id = v.id
WHERE v.status = 'voided' AND s.status = 'open' LIMIT 20;

\echo '== 5. sessions_remaining = total − sessions used by completed visits (sold, not cancelled packages)'
SELECT i.tenant_id, i.id, i.sessions_total, i.sessions_remaining, coalesce(u.used, 0) AS used
FROM app.customer_package_item i
JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
LEFT JOIN LATERAL (
  SELECT sum(l.qty) AS used FROM app.visit_line l JOIN app.visit v ON v.tenant_id = l.tenant_id AND v.id = l.visit_id
  WHERE l.tenant_id = i.tenant_id AND l.customer_package_item_id = i.id AND v.status = 'completed') u ON true
WHERE p.origin = 'sale' AND p.cancelled_at IS NULL AND i.sessions_remaining <> i.sessions_total - coalesce(u.used, 0)
LIMIT 20;

\echo '== 6. a cancelled package has no session left'
SELECT i.tenant_id, i.id FROM app.customer_package_item i JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
WHERE p.cancelled_at IS NOT NULL AND i.sessions_remaining > 0 LIMIT 20;

\echo '== 7. customer_package copies its sale: same closer; sold prices add up to the sale total (after discount)'
SELECT s.tenant_id, s.id AS sale_id, s.total, sum(cp.sold_price) AS sold_prices,
       bool_and(cp.closer_staff_id IS NOT DISTINCT FROM s.closer_staff_id) AS same_closer
FROM app.sale s
JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id AND cp.origin = 'sale'
GROUP BY s.tenant_id, s.id, s.total, s.closer_staff_id
HAVING sum(cp.sold_price) <> s.total OR NOT bool_and(cp.closer_staff_id IS NOT DISTINCT FROM s.closer_staff_id)
LIMIT 20;

\echo '== 8. staff.status / joined_on / left_on mirror the latest employment period (ADR-0013)'
SELECT s.tenant_id, s.id, s.status, s.joined_on, s.left_on, e.started_on, e.ended_on
FROM app.staff s
JOIN LATERAL (SELECT started_on, ended_on FROM app.staff_employment
              WHERE tenant_id = s.tenant_id AND staff_id = s.id
              ORDER BY coalesce(started_on, '-infinity'::date) DESC, created_at DESC LIMIT 1) e ON true
WHERE (s.status = 'left') <> (e.ended_on IS NOT NULL)
   OR s.left_on IS DISTINCT FROM e.ended_on
   OR s.joined_on IS DISTINCT FROM e.started_on
LIMIT 20;

\echo '== 9. customer.search_text = app.search_fold(full_name)'
SELECT tenant_id, id FROM app.customer
WHERE search_text <> coalesce(app.search_fold(full_name), '') LIMIT 20;

\echo '== 10. business_date = local day of the instant (visit, appointment)'
SELECT v.tenant_id, v.id, v.business_date, (v.check_in_at AT TIME ZONE t.timezone)::date AS local_day
FROM app.visit v JOIN app.tenant t ON t.id = v.tenant_id
WHERE v.business_date <> (v.check_in_at AT TIME ZONE t.timezone)::date
UNION ALL
SELECT a.tenant_id, a.id, a.business_date, (a.starts_at AT TIME ZONE t.timezone)::date
FROM app.appointment a JOIN app.tenant t ON t.id = a.tenant_id
WHERE a.business_date <> (a.starts_at AT TIME ZONE t.timezone)::date
LIMIT 20;

\echo '== 11. a done appointment points at a visit of the same customer'
SELECT a.tenant_id, a.id, a.customer_id, v.customer_id AS visit_customer
FROM app.appointment a JOIN app.visit v ON v.tenant_id = a.tenant_id AND v.id = a.visit_id
WHERE a.customer_id IS DISTINCT FROM v.customer_id LIMIT 20;
