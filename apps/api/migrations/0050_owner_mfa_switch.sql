-- 0050: a per-spa switch "owners must use MFA" on the platform console (ADR-0020, amends ADR-0004;
-- EM request 2026-11-09).
--
-- Until now MFA was hard-wired for owners: the system `owner` role has `requires_mfa` (CHECK
-- `role_owner_requires_mfa`, 0006) and app.user_requires_mfa looked at roles only. Now the spa
-- carries a policy, ON by default for every spa (existing and new), and a `requires_mfa` role only
-- counts in a spa whose policy is ON. The role-level CHECK stays: the policy is the spa's, the
-- role flag is what the policy applies to.
--
-- app_rw keeps SELECT only on app.tenant (0001): the switch is written by
-- app.platform_set_owner_mfa alone, which checks the operator's step-up itself.

ALTER TABLE app.tenant ADD COLUMN owner_mfa_required boolean NOT NULL DEFAULT true;

-- ---------------------------------------------------------------- MFA requirement (0011, 0041)
-- Strictest wins: a user needs MFA when ANY active membership holds a `requires_mfa` role in a spa
-- whose policy is ON. Still regardless of the spa's status (ADR-0004 M3): a suspended spa's policy
-- counts, so a password-only session opened during a suspension never becomes a non-MFA owner
-- session when the spa reopens. Same signature, so the grants and the pinned search_path stay.
CREATE OR REPLACE FUNCTION app.user_requires_mfa(p_user_id uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT EXISTS (
    SELECT 1
    FROM app.membership m
    JOIN app.tenant t ON t.id = m.tenant_id
    JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
    JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
    WHERE m.user_id = p_user_id AND m.status = 'active' AND r.requires_mfa
      AND t.owner_mfa_required
  )
$$;

-- ---------------------------------------------------------------- the switch (ADR-0020)
-- Sets the spa's policy. No row = no such spa. Setting the current value changes nothing and
-- returns it as `previous` (the platform crate answers 409 `tenant.owner_mfa_unchanged`).
--
-- Turning it ON ends every session of the spa's members whose MFA requirement flips to true with
-- it (the same rule as ADR-0004 L4): none of them passed a second factor under the old policy, so
-- none may keep working under the new one. Members who already needed MFA through another spa
-- keep their sessions (those passed MFA). Turning it OFF ends nothing; stored TOTP secrets and
-- recovery codes stay, so turning it back ON reuses the same authenticator.
--
-- Each affected member is locked (advisory, in user-id order, the same lock in both directions)
-- before their requirement is read: a concurrent switch of another spa they belong to commits
-- first, and is then seen, so "flips to true" is decided on committed state.
CREATE FUNCTION app.platform_set_owner_mfa(p_token_hash bytea, p_tenant uuid, p_required boolean)
RETURNS TABLE (previous boolean, revoked bigint)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
DECLARE
  v_previous boolean;
  v_user uuid;
  v_flipping uuid[] := '{}';
  v_revoked bigint := 0;
BEGIN
  PERFORM app.platform_gate(p_token_hash, true);
  SELECT t.owner_mfa_required INTO v_previous FROM app.tenant t WHERE t.id = p_tenant FOR UPDATE;
  IF NOT FOUND THEN
    RETURN;
  END IF;
  IF v_previous = p_required THEN
    RETURN QUERY SELECT v_previous, 0::bigint;
    RETURN;
  END IF;
  FOR v_user IN
    SELECT DISTINCT m.user_id
    FROM app.membership m
    JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
    JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
    WHERE m.tenant_id = p_tenant AND m.status = 'active' AND r.requires_mfa
    ORDER BY m.user_id
  LOOP
    PERFORM pg_advisory_xact_lock(hashtextextended('app.user_mfa:' || v_user::text, 0));
    -- Read after the lock (a new snapshot per statement): the policy is still the old one here.
    IF p_required AND NOT app.user_requires_mfa(v_user) THEN
      v_flipping := v_flipping || v_user;
    END IF;
  END LOOP;
  UPDATE app.tenant t SET owner_mfa_required = p_required WHERE t.id = p_tenant;
  IF p_required THEN
    UPDATE app.session s SET revoked_at = now()
    WHERE s.user_id = ANY (v_flipping) AND s.revoked_at IS NULL;
    GET DIAGNOSTICS v_revoked = ROW_COUNT;
  END IF;
  RETURN QUERY SELECT v_previous, v_revoked;
END
$$;
REVOKE ALL ON FUNCTION app.platform_set_owner_mfa(bytea, uuid, boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_set_owner_mfa(bytea, uuid, boolean) TO app_rw;

-- ---------------------------------------------------------------- the console's overview (0049)
-- Also returns the policy, so the spa list can flag the spas that are OFF. Same body and grants as
-- 0049 plus one column; a result-column change needs DROP + CREATE.
DROP FUNCTION app.platform_tenant_overview(bytea, uuid);

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
  last_activity_at timestamptz,
  owner_mfa_required boolean
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
    (SELECT max(a.at) FROM app.audit_log a WHERE a.tenant_id = t.id),
    t.owner_mfa_required
  FROM app.tenant t
  WHERE p_tenant IS NULL OR t.id = p_tenant
  ORDER BY t.created_at DESC, t.id;
END
$$;
REVOKE ALL ON FUNCTION app.platform_tenant_overview(bytea, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_tenant_overview(bytea, uuid) TO app_rw;
