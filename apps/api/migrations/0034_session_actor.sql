-- 0034: one statement resolves a request's caller (db review 2026-11 A3, ADR-0017): the live
-- session, its account flags, the active membership's permissions, the former-staff flag and the
-- spa's settings. It replaces the session lookup plus a separate permission transaction
-- (BEGIN + set_config, SELECT, COMMIT): three round trips fewer on every authenticated request.
--
-- SECURITY DEFINER because the caller's spa is only known from the session row, so no
-- `app.tenant_id` can be set yet and the RLS policies on membership / role / staff / branch would
-- hide everything. The function does by hand what RLS would: every row it reads belongs to the
-- session's own `active_tenant_id`, and the membership is the session's own user's. It returns
-- nothing about anyone else. The input is the SHA-256 of the cookie secret (never the secret):
-- whoever can pass it already holds that session.
--
-- The rules are the ones of the two statements it replaces (identity::repo::live_session and
-- member_access), unchanged: no row = no live session (revoked, idle or absolutely expired, or
-- the account is disabled); `member` = an active membership of an active spa; `former_staff` =
-- the linked staff profile has left (ADR-0014). Rust keeps the decisions (MFA pending, temporary
-- password, no active spa, not a member, former staff's derived permissions).

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
  branch_id uuid
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
          ORDER BY b.created_at, b.id LIMIT 1)
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
