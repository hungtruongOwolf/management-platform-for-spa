-- 0013: management / oversight views (memory/wiki/product/specs/management-views.md).
-- Depends on 0002 (role, role_permission), 0005/0007/0009 (sale, payment, visit, refund).

-- EM decision 2026-11-05 (memory/raw/2026-11-05-em-management-views-logo-ui.md): the paper books
-- are a one-off migration the team runs, not a front-desk task. The manager role loses the import;
-- the owner keeps it for the transitional HTTP route, the team uses `app import-paper-book`.
DELETE FROM app.role_permission rp
USING app.role r
WHERE r.tenant_id = rp.tenant_id AND r.id = rp.role_id
  AND r.is_system AND r.key = 'manager'
  AND rp.permission = 'opening_balance.import';

-- New report permissions for every existing system owner and manager role (new spas get them
-- from identity::roles::SystemRole::default_permissions). `report.revenue.read` for the manager
-- is the default the spec proposes (open question for the EM): she is the cashier.
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, p.permission
FROM app.role r
CROSS JOIN (VALUES ('report.read'), ('report.revenue.read')) AS p (permission)
WHERE r.is_system AND r.key IN ('owner', 'manager')
ON CONFLICT DO NOTHING;

-- ------------------------------------------------------------------ derived read models (views)
-- security_invoker: the view runs with the caller's rights, so app_rw still sees only its own
-- tenant through the tables' RLS (a plain view would run as its owner and bypass RLS).

-- Derived package status, the one SQL definition of packages::domain::PackageStatus::derive
-- (precedence cancelled → exhausted → expired → active; never stored).
CREATE FUNCTION app.package_status(cancelled_at timestamptz, sessions_remaining bigint,
                                   expires_on date, today date)
RETURNS text LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT CASE
    WHEN cancelled_at IS NOT NULL THEN 'cancelled'
    WHEN sessions_remaining <= 0 THEN 'exhausted'
    WHEN expires_on < today THEN 'expired'
    ELSE 'active'
  END
$$;

-- A package with its session counters summed over its items.
CREATE VIEW app.customer_package_state WITH (security_invoker = true) AS
SELECT cp.tenant_id, cp.id, cp.customer_id, cp.plan_id, cp.sale_id, cp.name_snapshot,
       cp.starts_on, cp.expires_on, cp.cancelled_at, cp.created_at,
       i.sessions_total, i.sessions_remaining
FROM app.customer_package cp
CROSS JOIN LATERAL (
  SELECT coalesce(sum(it.sessions_total), 0)::bigint AS sessions_total,
         coalesce(sum(it.sessions_remaining), 0)::bigint AS sessions_remaining
  FROM app.customer_package_item it
  WHERE it.tenant_id = cp.tenant_id AND it.customer_package_id = cp.id
) i;

-- The paper "sổ theo dõi doanh thu hằng ngày", derived from the append-only money tables: one
-- row per sale (money received for it), per voided sale on the day of its reversals (negative),
-- and per refund (negative). Summing a day's rows gives that day's net cash movement.
-- `seq` orders the book inside a day: the UUIDv7 the API minted for the sale, the latest
-- reversal or the refund, i.e. the order the cashier recorded them (wall-clock columns mix
-- the API's and the database's clocks).
-- No GROUP BY over a non-key column in the sale arm (LATERAL instead), so a caller's
-- `tenant_id` / `business_date` filter is pushed into every arm and hits the day indexes.
CREATE VIEW app.revenue_book WITH (security_invoker = true) AS
SELECT s.tenant_id, s.id, s.id AS seq, 'sale'::text AS entry, s.business_date,
       s.created_at AS at, s.customer_id, s.id AS sale_id, NULL::uuid AS customer_package_id,
       d.sale_kind, d.one_off, d.package,
       pay.bank_transfer, pay.cash, (s.total - pay.received)::bigint AS unpaid,
       s.discount_reason AS note, s.status = 'voided' AS voided
FROM app.sale s
CROSS JOIN LATERAL (
  SELECT coalesce(sum(p.amount) FILTER (WHERE p.method = 'bank_transfer'), 0)::bigint AS bank_transfer,
         coalesce(sum(p.amount) FILTER (WHERE p.method = 'cash'), 0)::bigint AS cash,
         coalesce(sum(p.amount), 0)::bigint AS received
  FROM app.payment p
  WHERE p.tenant_id = s.tenant_id AND p.sale_id = s.id AND p.kind = 'payment'
) pay
CROSS JOIN LATERAL (
  SELECT CASE WHEN bool_or(sl.line_type = 'package') THEN 'package' ELSE 'one_off' END AS sale_kind,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'service') AS one_off,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'package') AS package
  FROM app.sale_line sl
  WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
) d
UNION ALL
SELECT v.tenant_id, v.sale_id, v.seq, 'void', v.business_date, v.at, s.customer_id, s.id, NULL,
       d.sale_kind, d.one_off, d.package,
       -v.bank_transfer, -v.cash, 0::bigint, s.void_reason, true
FROM (
  SELECT p.tenant_id, p.sale_id, p.business_date, max(p.paid_at) AS at,
         (array_agg(p.id ORDER BY p.id DESC))[1] AS seq,
         coalesce(sum(p.amount) FILTER (WHERE p.method = 'bank_transfer'), 0)::bigint AS bank_transfer,
         coalesce(sum(p.amount) FILTER (WHERE p.method = 'cash'), 0)::bigint AS cash
  FROM app.payment p
  WHERE p.kind = 'reversal'
  GROUP BY p.tenant_id, p.business_date, p.sale_id
) v
JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.id = v.sale_id
CROSS JOIN LATERAL (
  SELECT CASE WHEN bool_or(sl.line_type = 'package') THEN 'package' ELSE 'one_off' END AS sale_kind,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'service') AS one_off,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'package') AS package
  FROM app.sale_line sl
  WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
) d
UNION ALL
SELECT r.tenant_id, r.id, r.id, 'refund', r.business_date, r.created_at, r.customer_id, NULL,
       r.customer_package_id, 'package', NULL, cp.name_snapshot,
       CASE WHEN r.method = 'bank_transfer' THEN -r.amount ELSE 0 END,
       CASE WHEN r.method = 'cash' THEN -r.amount ELSE 0 END,
       0::bigint, r.reason, false
FROM app.refund r
JOIN app.customer_package cp ON cp.tenant_id = r.tenant_id AND cp.id = r.customer_package_id;

GRANT SELECT ON app.customer_package_state, app.revenue_book TO app_rw;
GRANT EXECUTE ON FUNCTION app.package_status(timestamptz, bigint, date, date) TO app_rw;

-- ----------------------------------------------------------------------------------- indexes
-- Reports read one spa across all its branches by business day. The existing day indexes lead
-- with branch_id, so a tenant-wide range could not use them as a range scan.
CREATE INDEX sale_tenant_day_idx ON app.sale (tenant_id, business_date);
CREATE INDEX payment_tenant_day_idx ON app.payment (tenant_id, business_date);
CREATE INDEX visit_tenant_day_idx ON app.visit (tenant_id, business_date);
CREATE INDEX refund_tenant_day_idx ON app.refund (tenant_id, business_date);
-- Per-package usage (the paper package page) and "last used on".
CREATE INDEX visit_line_package_idx ON app.visit_line (tenant_id, customer_package_id)
  WHERE customer_package_id IS NOT NULL;
-- Customer timeline / summary: refunds of one customer.
CREATE INDEX refund_customer_idx ON app.refund (tenant_id, customer_id);
