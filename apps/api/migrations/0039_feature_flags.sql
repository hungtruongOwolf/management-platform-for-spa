-- 0039: per-spa feature switches (ADR-0018 D4, spec platform-admin US5).
--
-- A row turns a feature off (or back on) for one spa; no row = on, so existing spas keep every
-- feature and nothing changes until an operator switches one. The registry of features (their
-- permissions and routes) lives in code: kernel::feature.
--
-- app_rw may only READ the switches of the spa in its RLS scope: a spa can never turn a feature
-- back on, even through a bug. Writes go through `app.platform_set_feature`, which checks the
-- operator's session hash itself (`app.platform_gate`, 0038) and needs a recent step-up.
CREATE TABLE app.tenant_feature (
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  feature     text NOT NULL CHECK (feature ~ '^[a-z_]{2,40}$'),
  enabled     boolean NOT NULL,
  updated_at  timestamptz NOT NULL DEFAULT now(),
  updated_by  uuid NOT NULL REFERENCES app.platform_operator (id),
  PRIMARY KEY (tenant_id, feature)
);
ALTER TABLE app.tenant_feature ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.tenant_feature
  USING (tenant_id = app.current_tenant());
GRANT SELECT ON app.tenant_feature TO app_rw;                  -- no write: operator function only

-- The previous value (true when the spa had no row) or no row when the spa does not exist.
CREATE FUNCTION app.platform_set_feature(p_token_hash bytea, p_tenant uuid, p_feature text, p_enabled boolean)
RETURNS TABLE (previous boolean)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
DECLARE
  v_operator uuid;
  v_previous boolean;
BEGIN
  v_operator := app.platform_gate(p_token_hash, true);
  PERFORM 1 FROM app.tenant t WHERE t.id = p_tenant;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  SELECT f.enabled INTO v_previous FROM app.tenant_feature f
  WHERE f.tenant_id = p_tenant AND f.feature = p_feature FOR UPDATE;
  INSERT INTO app.tenant_feature (tenant_id, feature, enabled, updated_by)
  VALUES (p_tenant, p_feature, p_enabled, v_operator)
  ON CONFLICT (tenant_id, feature)
  DO UPDATE SET enabled = EXCLUDED.enabled, updated_at = now(), updated_by = EXCLUDED.updated_by;
  RETURN QUERY SELECT coalesce(v_previous, true);
END
$$;
REVOKE ALL ON FUNCTION app.platform_set_feature(bytea, uuid, text, boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_set_feature(bytea, uuid, text, boolean) TO app_rw;

-- app.session_actor (0034, ADR-0017) also returns the spa's switched-off features, so a request
-- still resolves its caller in one statement. Every other column and condition is unchanged; the
-- new subquery reads only the session's own active spa.
DROP FUNCTION app.session_actor(bytea);
CREATE FUNCTION app.session_actor(p_token_hash bytea)
RETURNS TABLE (
  session_id uuid,
  user_id uuid,
  active_tenant_id uuid,
  last_seen_at timestamptz,
  absolute_expires_at timestamptz,
  recent_auth_at timestamptz,
  mfa_pending boolean,
  mfa_enrolled boolean,
  must_change_password boolean,
  member boolean,
  permissions text[],
  former_staff boolean,
  timezone text,
  manager_discount_max_pct text,
  branch_id uuid,
  disabled_features text[]
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = app, pg_temp
AS $$
  SELECT s.id, s.user_id, s.active_tenant_id, s.last_seen_at, s.absolute_expires_at,
         s.recent_auth_at, s.mfa_pending, u.mfa_enabled_at IS NOT NULL, u.must_change_password,
         m.id IS NOT NULL,
         CASE WHEN m.id IS NULL THEN '{}'::text[] ELSE coalesce(
           (SELECT array_agg(DISTINCT rp.permission ORDER BY rp.permission)
            FROM app.membership_role mr
            JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
            WHERE mr.tenant_id = m.tenant_id AND mr.membership_id = m.id),
           '{}'::text[]) END,
         m.id IS NOT NULL AND EXISTS (
           SELECT 1 FROM app.staff st
           WHERE st.tenant_id = m.tenant_id AND st.user_id = m.user_id AND st.status = 'left'),
         t.timezone,
         t.settings ->> 'manager_discount_max_pct',
         (SELECT b.id FROM app.branch b WHERE b.tenant_id = t.id AND b.active
          ORDER BY b.created_at, b.id LIMIT 1),
         coalesce(
           (SELECT array_agg(f.feature ORDER BY f.feature) FROM app.tenant_feature f
            WHERE f.tenant_id = t.id AND NOT f.enabled),
           '{}'::text[])
  FROM app.session s
  JOIN app.user_account u ON u.id = s.user_id AND u.status = 'active'
  LEFT JOIN app.tenant t ON t.id = s.active_tenant_id AND t.status = 'active'
  LEFT JOIN app.membership m
    ON m.tenant_id = t.id AND m.user_id = s.user_id AND m.status = 'active'
  WHERE s.token_hash = p_token_hash
    AND s.revoked_at IS NULL
    AND now() < s.idle_expires_at
    AND now() < s.absolute_expires_at
$$;
REVOKE ALL ON FUNCTION app.session_actor(bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.session_actor(bytea) TO app_rw;
