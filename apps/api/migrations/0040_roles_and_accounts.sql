-- 0040: platform admin console, part C (ADR-0018 D5): role templates edited by the operator, and
-- the cross-spa account views behind "Tài khoản".
--
-- Role templates: what a new spa's three system roles grant. Seeded from the code defaults
-- (identity::roles::SystemRole::default_permissions, checked by a test on a fresh database). A
-- spa's system role either follows its template (`follows_template`) — then a template edit is
-- applied to it in the same transaction — or the operator gave it its own grants for that spa.
-- app_rw may read templates (seeding a new spa) but writes them only through
-- `app.platform_apply_role_template`, gated like every platform function (0038).
CREATE TABLE app.role_template_permission (
  role_key    text NOT NULL CHECK (role_key IN ('owner', 'manager', 'technician')),
  permission  text NOT NULL CHECK (permission ~ '^[a-z_]+(\.[a-z_]+)+$'),
  PRIMARY KEY (role_key, permission)
);
ALTER TABLE app.role_template_permission ENABLE ROW LEVEL SECURITY;
CREATE POLICY runtime_read ON app.role_template_permission FOR SELECT TO app_rw USING (true);
GRANT SELECT ON app.role_template_permission TO app_rw;

INSERT INTO app.role_template_permission (role_key, permission)
SELECT 'owner', p FROM unnest(ARRAY[
  'catalog.read', 'catalog.manage', 'staff.read_basic', 'staff.manage', 'staff.read', 'staff.read_own',
  'attendance.record', 'evaluation.record', 'payroll.read', 'payroll.close',
  'customer.read', 'customer.write', 'customer.health_notes.read', 'customer.health_notes.write',
  'customer.anonymize', 'customer.follow_up', 'package.sell', 'package.extend_expiry',
  'package.cancel_refund', 'package.use_expired_override', 'opening_balance.import',
  'visit.record', 'visit.read', 'visit.backdate', 'appointment.manage', 'appointment.settings',
  'payment.collect', 'sale.read', 'sale.discount', 'sale.discount_unlimited',
  'record.void_same_day', 'record.void_closed_day', 'report.read', 'report.revenue.read',
  'report.long_range', 'audit.read', 'support_grant.manage'
]) AS p
UNION ALL
SELECT 'manager', p FROM unnest(ARRAY[
  'catalog.read', 'staff.read_basic', 'customer.read', 'customer.write',
  'customer.health_notes.read', 'customer.health_notes.write', 'customer.follow_up',
  'package.sell', 'package.use_expired_override', 'visit.record', 'visit.read',
  'appointment.manage', 'payment.collect', 'sale.read', 'sale.discount', 'record.void_same_day',
  'report.read', 'report.revenue.read', 'attendance.record', 'evaluation.record',
  'staff.read_own', 'staff.read'
]) AS p
UNION ALL
SELECT 'technician', p FROM unnest(ARRAY['catalog.read', 'staff.read_basic', 'staff.read_own']) AS p;

-- Derived, never granted (ADR-0014): every effective-permission computation already dropped it
-- from owners' roles, so removing the stored copy changes nothing at runtime.
DELETE FROM app.role_permission WHERE permission IN ('staff.read_own_past', 'platform.tenant.manage');

-- Existing spas: a system role follows its template when it grants exactly the template.
ALTER TABLE app.role ADD COLUMN follows_template boolean NOT NULL DEFAULT true;
UPDATE app.role r SET follows_template = (
  SELECT coalesce(array_agg(rp.permission ORDER BY rp.permission), '{}')
  FROM app.role_permission rp WHERE rp.tenant_id = r.tenant_id AND rp.role_id = r.id
) = (
  SELECT coalesce(array_agg(t.permission ORDER BY t.permission), '{}')
  FROM app.role_template_permission t WHERE t.role_key = r.key
)
WHERE r.is_system;
UPDATE app.role SET follows_template = false WHERE NOT is_system;

-- Replaces a template and applies it to every spa whose system role follows it, in one statement
-- of the caller's transaction. The spa's own audit trail gets a row per changed spa
-- (`platform_admin`). Returns how many spas changed. Guardrails (owner floor, owner-only and
-- non-grantable permissions) are checked by the platform crate before the call.
CREATE FUNCTION app.platform_apply_role_template(p_token_hash bytea, p_role_key text, p_permissions text[])
RETURNS integer
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
DECLARE
  v_operator uuid;
  v_role record;
  v_before text[];
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
    IF v_before IS DISTINCT FROM (SELECT coalesce(array_agg(p ORDER BY p), '{}') FROM unnest(p_permissions) AS p) THEN
      DELETE FROM app.role_permission WHERE tenant_id = v_role.tenant_id AND role_id = v_role.id;
      INSERT INTO app.role_permission (tenant_id, role_id, permission)
      SELECT v_role.tenant_id, v_role.id, p FROM unnest(p_permissions) AS p;
      INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type,
                                 entity_id, before, after)
      VALUES (gen_random_uuid(), v_role.tenant_id, v_operator, 'platform_admin',
              'role.permissions_changed', 'role', v_role.id,
              jsonb_build_object('permissions', to_jsonb(v_before), 'source', 'template'),
              jsonb_build_object('permissions', to_jsonb(p_permissions)));
      v_count := v_count + 1;
    END IF;
  END LOOP;
  RETURN v_count;
END
$$;
REVOKE ALL ON FUNCTION app.platform_apply_role_template(bytea, text, text[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_apply_role_template(bytea, text, text[]) TO app_rw;

-- The spas of some accounts: what "Tài khoản" shows and what the last-owner rule needs. Reads
-- only those users' memberships, their spas' names and status, and how many other active owners
-- each of those spas has. No customer, visit or pay row.
CREATE FUNCTION app.platform_account_memberships(p_token_hash bytea, p_users uuid[])
RETURNS TABLE (
  user_id uuid,
  tenant_id uuid,
  tenant_name text,
  tenant_status text,
  membership_status text,
  role_keys text[],
  is_owner boolean,
  other_active_owners bigint
)
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
BEGIN
  PERFORM app.platform_gate(p_token_hash, false);
  RETURN QUERY
  SELECT m.user_id, m.tenant_id, t.name, t.status, m.status,
    coalesce((SELECT array_agg(r.key ORDER BY r.key) FROM app.membership_role mr
              JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
              WHERE mr.tenant_id = m.tenant_id AND mr.membership_id = m.id), '{}'::text[]),
    EXISTS (SELECT 1 FROM app.membership_role mr
            JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
            WHERE mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
              AND rp.permission = 'staff.manage'),
    (SELECT count(DISTINCT m2.id) FROM app.membership m2
       JOIN app.user_account u2 ON u2.id = m2.user_id AND u2.status = 'active'
       JOIN app.membership_role mr2 ON mr2.tenant_id = m2.tenant_id AND mr2.membership_id = m2.id
       JOIN app.role_permission rp2 ON rp2.tenant_id = mr2.tenant_id AND rp2.role_id = mr2.role_id
     WHERE m2.tenant_id = m.tenant_id AND m2.status = 'active' AND m2.user_id <> m.user_id
       AND rp2.permission = 'staff.manage')
  FROM app.membership m
  JOIN app.tenant t ON t.id = m.tenant_id
  WHERE m.user_id = ANY (p_users)
  ORDER BY m.user_id, t.name;
END
$$;
REVOKE ALL ON FUNCTION app.platform_account_memberships(bytea, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_account_memberships(bytea, uuid[]) TO app_rw;
