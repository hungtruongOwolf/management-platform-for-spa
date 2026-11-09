-- 0049: the spa overview for one spa (audit 2026-11-09, ticket "tenants::get builds the whole
-- overview to check one spa exists").
--
-- `GET /admin/tenants/{id}` and every per-spa console route (features, roles, the spa's audit)
-- used to call the overview of EVERY spa (five counting subqueries per spa) and keep one row.
-- `p_tenant` filters inside the function, so one spa costs one spa's counts. NULL (the default)
-- keeps the list behaviour, so `platform_tenant_overview($1)` callers are unchanged.
--
-- Only 0038 defined this function; the argument list changes, so it is dropped and re-created
-- (CREATE OR REPLACE cannot add a parameter) with the same result columns and the same grants.

DROP FUNCTION app.platform_tenant_overview(bytea);

CREATE FUNCTION app.platform_tenant_overview(p_token_hash bytea, p_tenant uuid DEFAULT NULL)
RETURNS TABLE (
  id uuid,
  name text,
  slug text,
  status text,
  created_at timestamptz,
  staff_count bigint,
  customer_count bigint,
  visits_30d bigint,
  owner_count bigint,
  last_activity_at timestamptz
)
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
BEGIN
  PERFORM app.platform_gate(p_token_hash, false);
  RETURN QUERY
  SELECT t.id, t.name, t.slug, t.status, t.created_at,
    (SELECT count(*) FROM app.staff st WHERE st.tenant_id = t.id AND st.status = 'active'),
    (SELECT count(*) FROM app.customer c WHERE c.tenant_id = t.id AND c.anonymized_at IS NULL),
    (SELECT count(*) FROM app.visit v
      WHERE v.tenant_id = t.id AND v.status = 'completed'
        AND v.business_date >= (now() AT TIME ZONE t.timezone)::date - 30),
    (SELECT count(DISTINCT m.id) FROM app.membership m
      JOIN app.user_account u ON u.id = m.user_id AND u.status = 'active'
      JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
      JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
      WHERE m.tenant_id = t.id AND m.status = 'active' AND rp.permission = 'staff.manage'),
    (SELECT max(a.at) FROM app.audit_log a WHERE a.tenant_id = t.id)
  FROM app.tenant t
  WHERE p_tenant IS NULL OR t.id = p_tenant
  ORDER BY t.created_at DESC, t.id;
END
$$;
REVOKE ALL ON FUNCTION app.platform_tenant_overview(bytea, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_tenant_overview(bytea, uuid) TO app_rw;
