-- 0011: security review r1 hardening. Depends on 0002 (session).

-- H1: a session row must state when the user last proved the password. With DEFAULT now(), any
-- code path that issues a token (switch-tenant rotation) silently got a fresh re-auth window.
-- Without a default, forgetting the column is a NOT NULL violation instead of a privilege grant.
ALTER TABLE app.session ALTER COLUMN recent_auth_at DROP DEFAULT;

-- M3: a suspended spa grants no permissions (identity::repo::member_permissions). MFA, though,
-- must not lapse while a spa is suspended: an owner who signed in password-only during the
-- suspension would otherwise hold a full owner session the moment the spa is reactivated.
-- So the MFA requirement looks at active memberships regardless of the spa's status.
CREATE OR REPLACE FUNCTION app.user_requires_mfa(p_user_id uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT EXISTS (
    SELECT 1
    FROM app.membership m
    JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
    JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
    WHERE m.user_id = p_user_id AND m.status = 'active'
      AND (r.requires_mfa OR EXISTS (
        SELECT 1 FROM app.role_permission rp
        WHERE rp.tenant_id = r.tenant_id AND rp.role_id = r.id AND rp.permission = 'platform.tenant.manage'))
  )
$$;
