-- 0038: platform admin console, part A (ADR-0018): the operator principal, its sessions and audit,
-- and the narrow cross-tenant functions for the spa list, creating a spa and suspending one.
--
-- The operator is NOT a spa member: own account table, own session table, own cookie. Nothing in
-- the spa API reads these tables (only the `platform` crate's SQL names `app.platform_*`).
--
-- No silent RLS bypass: no role gets BYPASSRLS and no business-table policy is widened. What has to
-- cross tenants goes through owner-defined SECURITY DEFINER functions with a pinned search_path and
-- static SQL. Each of them takes the SHA-256 of the operator's cookie secret first and calls
-- `app.platform_gate`, which raises unless that hash is a live, MFA-complete operator session (and,
-- for writes, one that re-authenticated within 5 minutes). A Rust bug that calls one of them from a
-- spa route, or forgets the operator check, is refused by the database. They return counts and
-- metadata only: no customer, visit, note, pay or password column ever leaves them.

-- ---------------------------------------------------------------- operator identity (global)
CREATE TABLE app.platform_operator (
  id              uuid PRIMARY KEY,
  username        text NOT NULL UNIQUE CHECK (username ~ '^[a-z0-9][a-z0-9._-]{2,39}$'),
  full_name       text NOT NULL CHECK (length(btrim(full_name)) BETWEEN 1 AND 200),
  password_hash   text NOT NULL CHECK (password_hash LIKE '$argon2id$%'),
  status          text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
  failed_logins   integer NOT NULL DEFAULT 0 CHECK (failed_logins >= 0),
  locked_until    timestamptz,
  -- AES-256-GCM (kernel::crypto::FieldCipher), AAD `platform_operator.mfa:<id>`: a ciphertext
  -- copied from a spa account (AAD = user id) or another operator does not decrypt.
  mfa_secret_ct   bytea CHECK (mfa_secret_ct IS NULL OR length(mfa_secret_ct) > 30),
  mfa_enabled_at  timestamptz,
  mfa_last_step   bigint CHECK (mfa_last_step IS NULL OR mfa_last_step >= 0),
  created_at      timestamptz NOT NULL DEFAULT now(),
  updated_at      timestamptz NOT NULL DEFAULT now(),
  CHECK (mfa_enabled_at IS NULL OR mfa_secret_ct IS NOT NULL)
);

CREATE TABLE app.platform_recovery_code (
  id           uuid PRIMARY KEY,
  operator_id  uuid NOT NULL REFERENCES app.platform_operator (id),
  code_hash    bytea NOT NULL UNIQUE CHECK (length(code_hash) = 32),
  created_at   timestamptz NOT NULL DEFAULT now(),
  used_at      timestamptz
);
CREATE INDEX platform_recovery_code_operator_idx
  ON app.platform_recovery_code (operator_id) WHERE used_at IS NULL;

-- Opaque sessions: only the SHA-256 of the cookie token is stored. Short-lived (idle 30 min,
-- absolute 8 h, set by the platform crate); `recent_auth_at` has no default so every insert states
-- whether the token is a fresh authentication (same rule as app.session, security review H1).
CREATE TABLE app.platform_session (
  id                   uuid PRIMARY KEY,
  token_hash           bytea NOT NULL UNIQUE CHECK (length(token_hash) = 32),
  operator_id          uuid NOT NULL REFERENCES app.platform_operator (id),
  created_at           timestamptz NOT NULL DEFAULT now(),
  last_seen_at         timestamptz NOT NULL DEFAULT now(),
  idle_expires_at      timestamptz NOT NULL,
  absolute_expires_at  timestamptz NOT NULL,
  recent_auth_at       timestamptz NOT NULL,
  mfa_pending          boolean NOT NULL,
  ip                   inet,
  user_agent           text,
  revoked_at           timestamptz,
  CHECK (idle_expires_at <= absolute_expires_at)
);
CREATE INDEX platform_session_operator_idx ON app.platform_session (operator_id) WHERE revoked_at IS NULL;

-- Every operator action, in the same transaction as the action. Append-only (no UPDATE/DELETE
-- grant). `detail` never carries a password, hash, secret or full phone number.
CREATE TABLE app.platform_audit_log (
  id           uuid PRIMARY KEY,
  operator_id  uuid NOT NULL REFERENCES app.platform_operator (id),
  action       text NOT NULL CHECK (action ~ '^platform(\.[a-z_]+)+$'),
  tenant_id    uuid REFERENCES app.tenant (id),
  target_type  text CHECK (target_type ~ '^[a-z_]{2,40}$'),
  target_id    uuid,
  detail       jsonb CHECK (detail IS NULL OR jsonb_typeof(detail) = 'object'),
  reason       text CHECK (reason IS NULL OR length(btrim(reason)) BETWEEN 1 AND 500),
  ip           inet,
  at           timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX platform_audit_log_at_idx ON app.platform_audit_log (at DESC, id DESC);

-- The spa audit trail read by support (metadata only) and "last activity" in the spa list.
CREATE INDEX audit_log_tenant_at_idx ON app.audit_log (tenant_id, at DESC);

-- ---------------------------------------------------------------- privileges & RLS
-- Reached only through the platform crate, like user_account/session through identity. RLS with a
-- policy for app_rw only: any other role (Supabase anon/authenticated, should a grant slip) sees
-- nothing.
GRANT SELECT, INSERT, UPDATE ON app.platform_operator TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.platform_session TO app_rw;
GRANT SELECT, INSERT, UPDATE, DELETE ON app.platform_recovery_code TO app_rw;
GRANT SELECT, INSERT ON app.platform_audit_log TO app_rw;           -- append-only

ALTER TABLE app.platform_operator ENABLE ROW LEVEL SECURITY;
CREATE POLICY runtime_only ON app.platform_operator TO app_rw USING (true) WITH CHECK (true);
ALTER TABLE app.platform_recovery_code ENABLE ROW LEVEL SECURITY;
CREATE POLICY runtime_only ON app.platform_recovery_code TO app_rw USING (true) WITH CHECK (true);
ALTER TABLE app.platform_session ENABLE ROW LEVEL SECURITY;
CREATE POLICY runtime_only ON app.platform_session TO app_rw USING (true) WITH CHECK (true);
ALTER TABLE app.platform_audit_log ENABLE ROW LEVEL SECURITY;
CREATE POLICY runtime_only ON app.platform_audit_log TO app_rw USING (true) WITH CHECK (true);

-- ---------------------------------------------------------------- the gate
-- The operator behind a cookie hash, or insufficient_privilege. Not granted to anyone: only the
-- definer functions below (running as the owner) call it.
CREATE FUNCTION app.platform_gate(p_token_hash bytea, p_write boolean)
RETURNS uuid
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
DECLARE
  v_operator uuid;
BEGIN
  SELECT s.operator_id INTO v_operator
  FROM app.platform_session s
  JOIN app.platform_operator o ON o.id = s.operator_id AND o.status = 'active'
  WHERE s.token_hash = p_token_hash
    AND s.revoked_at IS NULL
    AND NOT s.mfa_pending
    AND now() < s.idle_expires_at
    AND now() < s.absolute_expires_at
    AND (NOT p_write OR s.recent_auth_at > now() - interval '5 minutes');
  IF v_operator IS NULL THEN
    RAISE EXCEPTION 'a live platform operator session is required'
      USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN v_operator;
END
$$;
REVOKE ALL ON FUNCTION app.platform_gate(bytea, boolean) FROM PUBLIC;

-- ---------------------------------------------------------------- spa list (counts only)
CREATE FUNCTION app.platform_tenant_overview(p_token_hash bytea)
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
  ORDER BY t.created_at DESC, t.id;
END
$$;
REVOKE ALL ON FUNCTION app.platform_tenant_overview(bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_tenant_overview(bytea) TO app_rw;

-- ---------------------------------------------------------------- create a spa
-- Only the tenant row: app_rw may not INSERT into app.tenant. The caller runs this inside a
-- TenantTx for the new id, so the branch, roles and owner membership that follow are ordinary
-- inserts under that spa's RLS, in the same transaction (nothing half-created on failure).
CREATE FUNCTION app.platform_insert_tenant(p_token_hash bytea, p_id uuid, p_name text, p_slug text)
RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
BEGIN
  PERFORM app.platform_gate(p_token_hash, true);
  INSERT INTO app.tenant (id, name, slug) VALUES (p_id, p_name, p_slug);
END
$$;
REVOKE ALL ON FUNCTION app.platform_insert_tenant(bytea, uuid, text, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_insert_tenant(bytea, uuid, text, text) TO app_rw;

-- ---------------------------------------------------------------- suspend / unsuspend
-- No row = no such spa. Suspending ends every open session of the spa's members (and any session
-- that has the spa active), so nobody keeps working in it; data is untouched.
CREATE FUNCTION app.platform_set_tenant_status(p_token_hash bytea, p_tenant uuid, p_status text)
RETURNS TABLE (previous text, revoked integer)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = app, pg_temp AS $$
#variable_conflict use_column
DECLARE
  v_previous text;
  v_revoked integer := 0;
BEGIN
  PERFORM app.platform_gate(p_token_hash, true);
  SELECT t.status INTO v_previous FROM app.tenant t WHERE t.id = p_tenant FOR UPDATE;
  IF v_previous IS NULL THEN
    RETURN;
  END IF;
  IF v_previous <> p_status THEN
    UPDATE app.tenant t SET status = p_status WHERE t.id = p_tenant;
    IF p_status = 'suspended' THEN
      UPDATE app.session s SET revoked_at = now()
      WHERE s.revoked_at IS NULL
        AND (s.active_tenant_id = p_tenant
             OR s.user_id IN (SELECT m.user_id FROM app.membership m WHERE m.tenant_id = p_tenant));
      GET DIAGNOSTICS v_revoked = ROW_COUNT;
    END IF;
  END IF;
  RETURN QUERY SELECT v_previous, v_revoked;
END
$$;
REVOKE ALL ON FUNCTION app.platform_set_tenant_status(bytea, uuid, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.platform_set_tenant_status(bytea, uuid, text) TO app_rw;

-- ---------------------------------------------------------------- sign-in into a suspended spa
-- After a correct password only (identity::service::login): true when the account's only active
-- memberships are in suspended spas, so the person learns why they cannot get in
-- (`auth.spa_suspended`). Same class as tenants_of_user: reads that user's memberships only.
CREATE FUNCTION app.user_spa_suspended(p_user_id uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT EXISTS (
           SELECT 1 FROM app.membership m JOIN app.tenant t ON t.id = m.tenant_id
           WHERE m.user_id = p_user_id AND m.status = 'active' AND t.status = 'suspended')
     AND NOT EXISTS (
           SELECT 1 FROM app.membership m JOIN app.tenant t ON t.id = m.tenant_id
           WHERE m.user_id = p_user_id AND m.status = 'active' AND t.status = 'active')
$$;
REVOKE ALL ON FUNCTION app.user_spa_suspended(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.user_spa_suspended(uuid) TO app_rw;
