-- 0035: housekeeping the review found missing (db review 2026-11 C1, C2, F10; EM 2026-11-09 Q-DB4).
--
-- * Sessions were never purged: a revoked or expired session kept its IP and user agent for
--   ever. app.purge_sessions deletes those ended more than `older_than` ago (the job passes 30 days,
--   EM Q-DB4). app_rw has no DELETE on app.session; like purge_idempotency_records, this owner
--   function does the cross-tenant housekeeping so the runtime role never needs more.
-- * The stored copies the schema keeps on purpose (amount_paid, branch / visit links, session
--   counts, staff status, search text, business dates) had no scheduled check.
--   app.invariant_violations counts the rows where a copy drifted, per check, across all spas,
--   and returns counts only: no customer data leaves the function.
-- * On a scale-to-zero host every new instance ran the hourly purge at start (217 runs in 10 days on
--   staging). app.claim_maintenance lets one instance per interval run a job, whatever the number
--   of instances.
-- audit_log keeps every row: no retention (EM Q-DB4).

CREATE TABLE app.maintenance_run (
  job          text PRIMARY KEY CHECK (job ~ '^[a-z_]{1,40}$'),
  last_run_at  timestamptz NOT NULL
);
-- Only the owner functions below touch it; app_rw gets no privilege on the table, and RLS with no
-- policy denies every other role even if one were granted by mistake.
REVOKE ALL ON app.maintenance_run FROM PUBLIC;
ALTER TABLE app.maintenance_run ENABLE ROW LEVEL SECURITY;

-- True when the caller should run `job` now: it has not run within `every`. Atomic (one upsert),
-- so of two instances starting together exactly one gets true.
CREATE FUNCTION app.claim_maintenance(p_job text, p_every interval) RETURNS boolean
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  WITH claimed AS (
    INSERT INTO app.maintenance_run AS r (job, last_run_at) VALUES (p_job, now())
    ON CONFLICT (job) DO UPDATE SET last_run_at = now()
      WHERE r.last_run_at < now() - p_every
    RETURNING 1
  )
  SELECT EXISTS (SELECT 1 FROM claimed)
$$;

-- Deletes sessions revoked, or past their idle expiry (which is never after the absolute one),
-- more than `older_than` ago. A live session is never touched.
CREATE FUNCTION app.purge_sessions(older_than interval) RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  WITH deleted AS (
    DELETE FROM app.session
    WHERE (revoked_at IS NOT NULL AND revoked_at < now() - older_than)
       OR idle_expires_at < now() - older_than
    RETURNING 1
  )
  SELECT count(*) FROM deleted
$$;

-- One row per check, with the number of rows that break it (0 = clean). The same checks as
-- infra/perf/db/invariants.sql.
CREATE FUNCTION app.invariant_violations()
RETURNS TABLE (check_name text, violations bigint)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT 'sale_amount_paid', count(*) FROM app.sale s
  LEFT JOIN LATERAL (
    SELECT sum(amount) FILTER (WHERE kind = 'payment') AS paid,
           sum(amount) FILTER (WHERE kind = 'reversal') AS reversed
    FROM app.payment WHERE tenant_id = s.tenant_id AND sale_id = s.id) p ON true
  WHERE (s.status = 'open' AND s.amount_paid <> coalesce(p.paid, 0) - coalesce(p.reversed, 0))
     OR (s.status = 'voided' AND (s.amount_paid <> 0 OR coalesce(p.paid, 0) <> coalesce(p.reversed, 0)))
  UNION ALL
  SELECT 'payment_branch', count(*) FROM app.payment p
  JOIN app.sale s ON s.tenant_id = p.tenant_id AND s.id = p.sale_id WHERE p.branch_id <> s.branch_id
  UNION ALL
  SELECT 'visit_sale_link', (
    SELECT count(*) FROM app.visit v JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.id = v.sale_id
    WHERE s.visit_id IS DISTINCT FROM v.id) + (
    SELECT count(*) FROM app.visit_line l JOIN app.sale s ON s.tenant_id = l.tenant_id AND s.id = l.sale_id
    WHERE s.visit_id IS DISTINCT FROM l.visit_id)
  UNION ALL
  SELECT 'voided_visit_open_sale', count(*) FROM app.visit v
  JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.visit_id = v.id
  WHERE v.status = 'voided' AND s.status = 'open'
  UNION ALL
  SELECT 'sessions_remaining', count(*) FROM app.customer_package_item i
  JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
  LEFT JOIN LATERAL (
    SELECT sum(l.qty) AS used FROM app.visit_line l
    JOIN app.visit v ON v.tenant_id = l.tenant_id AND v.id = l.visit_id
    WHERE l.tenant_id = i.tenant_id AND l.customer_package_item_id = i.id AND v.status = 'completed') u ON true
  WHERE p.origin = 'sale' AND p.cancelled_at IS NULL
    AND i.sessions_remaining <> i.sessions_total - coalesce(u.used, 0)
  UNION ALL
  SELECT 'cancelled_package_sessions', count(*) FROM app.customer_package_item i
  JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
  WHERE p.cancelled_at IS NOT NULL AND i.sessions_remaining > 0
  UNION ALL
  SELECT 'package_sale_copy', count(*) FROM (
    SELECT 1 FROM app.sale s
    JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id AND cp.origin = 'sale'
    GROUP BY s.tenant_id, s.id, s.total, s.closer_staff_id
    HAVING sum(cp.sold_price) <> s.total
        OR NOT bool_and(cp.closer_staff_id IS NOT DISTINCT FROM s.closer_staff_id)) x
  UNION ALL
  SELECT 'staff_mirrors_employment', count(*) FROM app.staff s
  JOIN LATERAL (
    SELECT started_on, ended_on FROM app.staff_employment
    WHERE tenant_id = s.tenant_id AND staff_id = s.id
    ORDER BY coalesce(started_on, '-infinity'::date) DESC, created_at DESC LIMIT 1) e ON true
  WHERE (s.status = 'left') <> (e.ended_on IS NOT NULL)
     OR s.left_on IS DISTINCT FROM e.ended_on OR s.joined_on IS DISTINCT FROM e.started_on
  UNION ALL
  SELECT 'customer_search_text', count(*) FROM app.customer
  WHERE search_text <> coalesce(app.search_fold(full_name), '')
  UNION ALL
  SELECT 'business_date_local_day', (
    SELECT count(*) FROM app.visit v JOIN app.tenant t ON t.id = v.tenant_id
    WHERE v.business_date <> (v.check_in_at AT TIME ZONE t.timezone)::date) + (
    SELECT count(*) FROM app.appointment a JOIN app.tenant t ON t.id = a.tenant_id
    WHERE a.business_date <> (a.starts_at AT TIME ZONE t.timezone)::date)
  UNION ALL
  SELECT 'appointment_visit_customer', count(*) FROM app.appointment a
  JOIN app.visit v ON v.tenant_id = a.tenant_id AND v.id = a.visit_id
  WHERE a.customer_id IS DISTINCT FROM v.customer_id
$$;

REVOKE ALL ON FUNCTION app.claim_maintenance(text, interval) FROM PUBLIC;
REVOKE ALL ON FUNCTION app.purge_sessions(interval) FROM PUBLIC;
REVOKE ALL ON FUNCTION app.invariant_violations() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.claim_maintenance(text, interval) TO app_rw;
GRANT EXECUTE ON FUNCTION app.purge_sessions(interval) TO app_rw;
GRANT EXECUTE ON FUNCTION app.invariant_violations() TO app_rw;
