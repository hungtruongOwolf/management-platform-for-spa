-- 0052: "ai ghi, ai sửa" on each record (ADR-0022, spec record-history) and the "Nhật ký thao tác"
-- screen behind a feature switch that is OFF by default.
--
-- Deploy order (architecture/record-history §8): migrate, then deploy the new binary in the same
-- session. The previous binary keeps working on this schema (session_actor only gains a last
-- column, read by name), except two console writes: a spa role edit may answer
-- `422 unknown_permission` and switching a feature of a spa without a row answers 500. Operators
-- write nothing in the console during that gap.

-- ---------------------------------------------------------------- the permission (D1, D3)
-- `record.history.read` is spa-local: the role template decides only a NEW spa's initial value
-- (owner on, manager off; never a technician: platform::roles::NOT_FOR_TECHNICIAN). Applying a
-- template, editing a spa's role and "Về mẫu" keep each spa's own value (functions below); only
-- app.platform_set_record_history changes it. Rust mirrors the list (platform::roles::SPA_LOCAL);
-- a test compares the two.
CREATE FUNCTION app.spa_local_permissions() RETURNS text[]
LANGUAGE sql IMMUTABLE SET search_path = app, pg_temp AS $$
  SELECT ARRAY['record.history.read']::text[]
$$;
REVOKE ALL ON FUNCTION app.spa_local_permissions() FROM PUBLIC;

-- Shown by default to every spa's owner, existing and new. Idempotent.
INSERT INTO app.role_template_permission (role_key, permission)
VALUES ('owner', 'record.history.read')
ON CONFLICT DO NOTHING;

INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'record.history.read'
FROM app.role r
WHERE r.is_system AND r.key = 'owner'
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------- role functions keep it (D1)
-- 0040's template apply, now spa-local aware: each spa that follows the template gets
-- (template − spa-local) ∪ (its own spa-local grants). The spa's audit row records that set (0040
-- logged the template itself, which differs from what the spa got once a spa-local grant exists).
CREATE OR REPLACE FUNCTION app.platform_apply_role_template(p_token_hash bytea, p_role_key text, p_permissions text[])
RETURNS integer
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
DECLARE
  v_operator uuid;
  v_role record;
  v_before text[];
  v_after text[];
  v_count integer := 0;
BEGIN
  v_operator := app.platform_gate(p_token_hash, true);
  DELETE FROM app.role_template_permission WHERE role_key = p_role_key;
  INSERT INTO app.role_template_permission (role_key, permission)
  SELECT p_role_key, p FROM unnest(p_permissions) AS p;
  FOR v_role IN
    SELECT r.tenant_id, r.id FROM app.role r
    WHERE r.is_system AND r.key = p_role_key AND r.follows_template
    FOR UPDATE
  LOOP
    SELECT coalesce(array_agg(rp.permission ORDER BY rp.permission), '{}') INTO v_before
    FROM app.role_permission rp WHERE rp.tenant_id = v_role.tenant_id AND rp.role_id = v_role.id;
    SELECT coalesce(array_agg(DISTINCT p ORDER BY p), '{}') INTO v_after
    FROM (
      SELECT p FROM unnest(p_permissions) AS p
      WHERE NOT p = ANY (app.spa_local_permissions())
      UNION
      SELECT p FROM unnest(v_before) AS p
      WHERE p = ANY (app.spa_local_permissions())
    ) s(p);
    IF v_before IS DISTINCT FROM v_after THEN
      DELETE FROM app.role_permission WHERE tenant_id = v_role.tenant_id AND role_id = v_role.id;
      INSERT INTO app.role_permission (tenant_id, role_id, permission)
      SELECT v_role.tenant_id, v_role.id, p FROM unnest(v_after) AS p;
      INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type,
                                 entity_id, before, after)
      VALUES (gen_random_uuid(), v_role.tenant_id, v_operator, 'platform_admin',
              'role.permissions_changed', 'role', v_role.id,
              jsonb_build_object('permissions', to_jsonb(v_before), 'source', 'template'),
              jsonb_build_object('permissions', to_jsonb(v_after)));
      v_count := v_count + 1;
    END IF;
  END LOOP;
  RETURN v_count;
END
$$;

-- 0041's spa role edit, now spa-local aware: the spa-local permissions in the list sent (or in the
-- template, for "Về mẫu") are ignored and the role's current ones kept. `follows_template` as
-- before: a reset follows the template again, an own list does not.
CREATE OR REPLACE FUNCTION app.platform_set_tenant_role(
  p_token_hash bytea, p_tenant uuid, p_role_key text, p_permissions text[])
RETURNS TABLE (role_id uuid, before text[], after text[])
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
DECLARE
  v_role uuid;
  v_before text[];
  v_wanted text[];
  v_after text[];
BEGIN
  PERFORM app.platform_gate(p_token_hash, true);
  SELECT r.id INTO v_role FROM app.role r
  WHERE r.tenant_id = p_tenant AND r.key = p_role_key AND r.is_system
  FOR UPDATE;
  IF v_role IS NULL THEN
    RETURN;
  END IF;
  SELECT coalesce(array_agg(rp.permission ORDER BY rp.permission), '{}') INTO v_before
  FROM app.role_permission rp WHERE rp.tenant_id = p_tenant AND rp.role_id = v_role;
  IF p_permissions IS NULL THEN
    SELECT coalesce(array_agg(t.permission), '{}') INTO v_wanted
    FROM app.role_template_permission t WHERE t.role_key = p_role_key;
  ELSE
    v_wanted := p_permissions;
  END IF;
  SELECT coalesce(array_agg(DISTINCT p ORDER BY p), '{}') INTO v_after
  FROM (
    SELECT p FROM unnest(v_wanted) AS p
    WHERE NOT p = ANY (app.spa_local_permissions())
    UNION
    SELECT p FROM unnest(v_before) AS p
    WHERE p = ANY (app.spa_local_permissions())
  ) s(p);
  DELETE FROM app.role_permission rp WHERE rp.tenant_id = p_tenant AND rp.role_id = v_role;
  INSERT INTO app.role_permission (tenant_id, role_id, permission)
  SELECT p_tenant, v_role, p FROM unnest(v_after) AS p;
  UPDATE app.role r SET follows_template = (p_permissions IS NULL)
  WHERE r.tenant_id = p_tenant AND r.id = v_role;
  RETURN QUERY SELECT v_role, v_before, v_after;
END
$$;

-- ---------------------------------------------------------------- the switch (D2)
-- The only writer of `record.history.read` on a spa's role. Permission and roles are fixed here,
-- not parameters: this function cannot grant anything else, nor reach a technician. Locks the
-- role row; adds or removes exactly one grant; never touches `follows_template`. Returns the
-- role's permissions before / after (the spa's audit row needs them); no row = no such spa / role.
-- before = after (already in that state): the platform crate answers 409 and rolls back.
CREATE FUNCTION app.platform_set_record_history(
  p_token_hash bytea, p_tenant uuid, p_role text, p_visible boolean)
RETURNS TABLE (role_id uuid, before text[], after text[])
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
DECLARE
  v_role uuid;
  v_before text[];
  v_after text[];
BEGIN
  PERFORM app.platform_gate(p_token_hash, true);
  IF p_role IS NULL OR p_role NOT IN ('owner', 'manager') THEN
    RAISE EXCEPTION 'record history is switched for owner or manager only' USING ERRCODE = '22023';
  END IF;
  SELECT r.id INTO v_role FROM app.role r
  WHERE r.tenant_id = p_tenant AND r.key = p_role AND r.is_system
  FOR UPDATE;
  IF v_role IS NULL THEN
    RETURN;
  END IF;
  SELECT coalesce(array_agg(rp.permission ORDER BY rp.permission), '{}') INTO v_before
  FROM app.role_permission rp WHERE rp.tenant_id = p_tenant AND rp.role_id = v_role;
  IF p_visible THEN
    INSERT INTO app.role_permission (tenant_id, role_id, permission)
    VALUES (p_tenant, v_role, 'record.history.read')
    ON CONFLICT DO NOTHING;
  ELSE
    DELETE FROM app.role_permission rp
    WHERE rp.tenant_id = p_tenant AND rp.role_id = v_role AND rp.permission = 'record.history.read';
  END IF;
  SELECT coalesce(array_agg(rp.permission ORDER BY rp.permission), '{}') INTO v_after
  FROM app.role_permission rp WHERE rp.tenant_id = p_tenant AND rp.role_id = v_role;
  RETURN QUERY SELECT v_role, v_before, v_after;
END
$$;
REVOKE ALL ON FUNCTION app.platform_set_record_history(bytea, uuid, text, boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_set_record_history(bytea, uuid, text, boolean) TO app_rw;

-- ---------------------------------------------------------------- feature defaults (D5)
-- A feature's state without a row is now the code's default (kernel::feature, `default_enabled`):
-- "Nhật ký thao tác" is off unless an operator switched it on. So the request path needs the rows
-- that switch a feature ON too: session_actor gains a last column, `enabled_features`. Every
-- other column and condition is 0039's. DROP loses the grants: they are given again below.
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
  disabled_features text[],
  enabled_features text[]
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
           '{}'::text[]),
         coalesce(
           (SELECT array_agg(f.feature ORDER BY f.feature) FROM app.tenant_feature f
            WHERE f.tenant_id = t.id AND f.enabled),
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

-- 0039's switch, now returning NULL as `previous` when the spa had no row (0039 answered true):
-- the platform crate applies the feature's default. Same signature: grants and search_path stay.
CREATE OR REPLACE FUNCTION app.platform_set_feature(p_token_hash bytea, p_tenant uuid, p_feature text, p_enabled boolean)
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
  RETURN QUERY SELECT v_previous;
END
$$;

-- ---------------------------------------------------------------- indexes (architecture §6)
-- The "Đã sửa" chip of a list row (`edited`): one probe per row into the few audit rows that can
-- make a visit / booking "edited". Queries repeat this exact action list as literals, so the
-- generic plan of a prepared statement still proves the predicate.
CREATE INDEX audit_log_edit_idx ON app.audit_log (tenant_id, entity_type, entity_id)
  WHERE action IN ('visit.update', 'appointment.update');

-- One staff-book cell (staff member × day, wave 2): a save of the book writes many days at one
-- instant on the staff member, so the cell's history filters on the row's own date.
CREATE INDEX audit_log_staff_day_idx
  ON app.audit_log (tenant_id, entity_id, (coalesce(after ->> 'date', before ->> 'date')))
  WHERE action IN ('attendance.record', 'evaluation.daily_review');
