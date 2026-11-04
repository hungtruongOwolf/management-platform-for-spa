-- 0006: MFA (TOTP, RFC 6238) for owners and platform admins (ADR-0004). Depends on 0001–0002 only.

-- ---------------------------------------------------------------- which roles require MFA (data, principle #2)
ALTER TABLE app.role ADD COLUMN requires_mfa boolean NOT NULL DEFAULT false;
UPDATE app.role SET requires_mfa = true WHERE is_system AND key = 'owner';
-- The built-in owner role can never be edited into an MFA-free role (ADR-0004).
ALTER TABLE app.role ADD CONSTRAINT role_owner_requires_mfa
  CHECK (NOT (is_system AND key = 'owner') OR requires_mfa);

-- ---------------------------------------------------------------- per-account TOTP state
-- mfa_secret_ct: AES-256-GCM (kernel::crypto::FieldCipher), AAD bound to the account id, so a
--   ciphertext copied onto another account does not decrypt. Written on enroll/start, active only
--   once mfa_enabled_at is set by enroll/confirm.
-- mfa_last_step: the last accepted TOTP time step (unix time / 30); a code for a step ≤ it is a replay.
ALTER TABLE app.user_account
  ADD COLUMN mfa_secret_ct  bytea CHECK (mfa_secret_ct IS NULL OR length(mfa_secret_ct) > 30),
  ADD COLUMN mfa_enabled_at timestamptz,
  ADD COLUMN mfa_last_step  bigint CHECK (mfa_last_step IS NULL OR mfa_last_step >= 0),
  ADD CONSTRAINT user_account_mfa_enabled_has_secret
    CHECK (mfa_enabled_at IS NULL OR mfa_secret_ct IS NOT NULL);

-- Single-use recovery codes: only SHA-256(account id || code) is stored (codes carry 80 random bits).
CREATE TABLE app.mfa_recovery_code (
  id          uuid PRIMARY KEY,
  user_id     uuid NOT NULL REFERENCES app.user_account (id),
  code_hash   bytea NOT NULL UNIQUE CHECK (length(code_hash) = 32),
  created_at  timestamptz NOT NULL DEFAULT now(),
  used_at     timestamptz
);
CREATE INDEX mfa_recovery_code_user_idx ON app.mfa_recovery_code (user_id) WHERE used_at IS NULL;

-- ---------------------------------------------------------------- two-step login
-- A session created after the password but before the second factor. CurrentSession/AuthUser
-- refuse it; only the MFA endpoints (and logout) accept it. Completing MFA revokes it and issues
-- a fresh, full session (token rotation).
ALTER TABLE app.session ADD COLUMN mfa_pending boolean NOT NULL DEFAULT false;

-- Login happens before any tenant is chosen, so (like tenants_of_user) this reads memberships
-- across tenants through a narrow owner-defined function instead of weakening RLS.
-- MFA applies when any active membership in an active spa has a role flagged requires_mfa or
-- holding platform administration.
CREATE FUNCTION app.user_requires_mfa(p_user_id uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT EXISTS (
    SELECT 1
    FROM app.membership m
    JOIN app.tenant t ON t.id = m.tenant_id
    JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
    JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
    WHERE m.user_id = p_user_id AND m.status = 'active' AND t.status = 'active'
      AND (r.requires_mfa OR EXISTS (
        SELECT 1 FROM app.role_permission rp
        WHERE rp.tenant_id = r.tenant_id AND rp.role_id = r.id AND rp.permission = 'platform.tenant.manage'))
  )
$$;
REVOKE ALL ON FUNCTION app.user_requires_mfa(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.user_requires_mfa(uuid) TO app_rw;

-- Sessions opened before MFA existed never passed a second factor: end them for accounts that
-- now require it, so the next request goes through the two-step login.
UPDATE app.session SET revoked_at = now()
WHERE revoked_at IS NULL AND app.user_requires_mfa(user_id);

-- ---------------------------------------------------------------- privileges
-- user_account / session already grant SELECT, INSERT, UPDATE to app_rw (0002); new columns inherit.
GRANT SELECT, INSERT, UPDATE, DELETE ON app.mfa_recovery_code TO app_rw;
