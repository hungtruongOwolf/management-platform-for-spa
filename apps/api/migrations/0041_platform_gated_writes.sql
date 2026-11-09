-- 0041: every platform console write goes through a function that checks the operator's step-up
-- itself (ADR-0018 D3, lead review of PR C). Until now the account actions (lock / unlock / reset
-- MFA / reset password / end sessions) and a spa's own role grants were plain app_rw writes: the
-- step-up was enforced only by the Rust `Operator` extractor, and the database merely checked a
-- live session (`platform_gate(token, false)`) when the memberships were read. Now each of those
-- writes runs inside a definer function that first calls `app.platform_gate(p_token_hash, true)`,
-- like the other writes of 0038–0040: a code path that forgets the step-up is refused (42501).
--
-- The owner floor of account locks (ADR-0018 D5: an active spa keeps at least one active owner)
-- moves into the function too, under a per-spa advisory lock: two operators locking the last two
-- owners of a spa at the same moment can no longer both pass the check.
--
-- Also: operator sessions are purged like spa sessions (0035 C1 covered app.session only), and
-- app.user_requires_mfa drops its dead `platform.tenant.manage` branch.

-- ---------------------------------------------------------------- account actions (US8)
-- `p_action`: lock | unlock | reset_mfa | reset_password (with `p_password_hash`, an Argon2id
-- PHC string of a temporary password) | revoke_sessions. Returns the sessions it ended. The
-- caller has already checked the account exists and the action changes something (409s); this
-- function owns the security-relevant part: the step-up, the owner floor, the writes.
CREATE FUNCTION app.platform_account_action(
  p_token_hash bytea, p_user uuid, p_action text, p_password_hash text)
RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
DECLARE
  v_tenant uuid;
  v_revoked bigint := 0;
BEGIN
  PERFORM app.platform_gate(p_token_hash, true);
  IF p_action = 'lock' THEN
    -- One lock per spa the account belongs to, in a fixed order (no deadlock between two locks).
    FOR v_tenant IN
      SELECT m.tenant_id FROM app.membership m WHERE m.user_id = p_user ORDER BY m.tenant_id
    LOOP
      PERFORM pg_advisory_xact_lock(hashtextextended('app.owner_floor:' || v_tenant::text, 0));
    END LOOP;
    -- Read after the locks: a concurrent lock of another owner has committed by now.
    IF EXISTS (
      SELECT 1 FROM app.membership m
      JOIN app.tenant t ON t.id = m.tenant_id AND t.status = 'active'
      WHERE m.user_id = p_user AND m.status = 'active'
        AND EXISTS (SELECT 1 FROM app.membership_role mr
                    JOIN app.role_permission rp ON rp.tenant_id = mr.tenant_id AND rp.role_id = mr.role_id
                    WHERE mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
                      AND rp.permission = 'staff.manage')
        AND NOT EXISTS (SELECT 1 FROM app.membership m2
                        JOIN app.user_account u2 ON u2.id = m2.user_id AND u2.status = 'active'
                        JOIN app.membership_role mr2 ON mr2.tenant_id = m2.tenant_id AND mr2.membership_id = m2.id
                        JOIN app.role_permission rp2 ON rp2.tenant_id = mr2.tenant_id AND rp2.role_id = mr2.role_id
                        WHERE m2.tenant_id = m.tenant_id AND m2.status = 'active' AND m2.user_id <> p_user
                          AND rp2.permission = 'staff.manage')
    ) THEN
      RAISE EXCEPTION 'the last active owner of an active spa cannot be locked'
        USING ERRCODE = 'check_violation', CONSTRAINT = 'platform_owner_floor';
    END IF;
    UPDATE app.user_account SET status = 'disabled', updated_at = now() WHERE id = p_user;
  ELSIF p_action = 'unlock' THEN
    UPDATE app.user_account
    SET status = 'active', failed_logins = 0, locked_until = NULL, updated_at = now()
    WHERE id = p_user;
    RETURN 0;
  ELSIF p_action = 'reset_mfa' THEN
    UPDATE app.user_account
    SET mfa_secret_ct = NULL, mfa_enabled_at = NULL, mfa_last_step = NULL, updated_at = now()
    WHERE id = p_user;
    DELETE FROM app.mfa_recovery_code WHERE user_id = p_user;
  ELSIF p_action = 'reset_password' THEN
    IF p_password_hash IS NULL OR p_password_hash NOT LIKE '$argon2id$%' THEN
      RAISE EXCEPTION 'reset_password needs an Argon2id hash' USING ERRCODE = 'invalid_parameter_value';
    END IF;
    UPDATE app.user_account
    SET password_hash = p_password_hash, must_change_password = true, failed_logins = 0,
        locked_until = NULL, updated_at = now()
    WHERE id = p_user;
  ELSIF p_action <> 'revoke_sessions' THEN
    RAISE EXCEPTION 'unknown account action %', p_action USING ERRCODE = 'invalid_parameter_value';
  END IF;
  -- Every action but unlock ends the account's sessions.
  UPDATE app.session SET revoked_at = now() WHERE user_id = p_user AND revoked_at IS NULL;
  GET DIAGNOSTICS v_revoked = ROW_COUNT;
  RETURN v_revoked;
END
$$;
REVOKE ALL ON FUNCTION app.platform_account_action(bytea, uuid, text, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_account_action(bytea, uuid, text, text) TO app_rw;

-- ---------------------------------------------------------------- one spa's role (US7)
-- Gives the spa's system role `p_role_key` its own grants (`p_permissions`), or with NULL puts it
-- back on its template (and following it again). Guardrails (owner floor, owner-only and
-- non-grantable permissions) are checked by the platform crate before the call, as for
-- app.platform_apply_role_template. No row = no such spa or role. Returns what the spa's audit
-- entry needs: the role, its grants before and after.
CREATE FUNCTION app.platform_set_tenant_role(
  p_token_hash bytea, p_tenant uuid, p_role_key text, p_permissions text[])
RETURNS TABLE (role_id uuid, before text[], after text[])
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
DECLARE
  v_role uuid;
  v_before text[];
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
    SELECT coalesce(array_agg(t.permission ORDER BY t.permission), '{}') INTO v_after
    FROM app.role_template_permission t WHERE t.role_key = p_role_key;
  ELSE
    SELECT coalesce(array_agg(DISTINCT p ORDER BY p), '{}') INTO v_after FROM unnest(p_permissions) AS p;
  END IF;
  DELETE FROM app.role_permission rp WHERE rp.tenant_id = p_tenant AND rp.role_id = v_role;
  INSERT INTO app.role_permission (tenant_id, role_id, permission)
  SELECT p_tenant, v_role, p FROM unnest(v_after) AS p;
  UPDATE app.role r SET follows_template = (p_permissions IS NULL)
  WHERE r.tenant_id = p_tenant AND r.id = v_role;
  RETURN QUERY SELECT v_role, v_before, v_after;
END
$$;
REVOKE ALL ON FUNCTION app.platform_set_tenant_role(bytea, uuid, text, text[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_set_tenant_role(bytea, uuid, text, text[]) TO app_rw;

-- ---------------------------------------------------------------- MFA requirement (0011)
-- `platform.tenant.manage` can no longer be held by any role: 0040 deleted the stored copies and
-- the platform crate refuses to grant it (`role.not_grantable`); platform administration is the
-- operator principal's (ADR-0018). The branch that made it imply MFA is dead; the rule is the
-- role's `requires_mfa` alone. Same signature, so the grants and the pinned search_path stay.
CREATE OR REPLACE FUNCTION app.user_requires_mfa(p_user_id uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT EXISTS (
    SELECT 1
    FROM app.membership m
    JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
    JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
    WHERE m.user_id = p_user_id AND m.status = 'active' AND r.requires_mfa
  )
$$;

-- ---------------------------------------------------------------- session purge (0035 C1)
-- Same rule for operator sessions, which kept their IP and user agent for ever. One count for both
-- kinds; the hourly job and its claim are unchanged. app_rw has no DELETE on either table.
CREATE OR REPLACE FUNCTION app.purge_sessions(older_than interval) RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  WITH spa AS (
    DELETE FROM app.session
    WHERE (revoked_at IS NOT NULL AND revoked_at < now() - older_than)
       OR idle_expires_at < now() - older_than
    RETURNING 1
  ), operator AS (
    DELETE FROM app.platform_session
    WHERE (revoked_at IS NOT NULL AND revoked_at < now() - older_than)
       OR idle_expires_at < now() - older_than
    RETURNING 1
  )
  SELECT (SELECT count(*) FROM spa) + (SELECT count(*) FROM operator)
$$;
