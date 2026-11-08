-- 0032: logins created from the staff roster, temporary passwords, forgotten-password requests
-- (EM 2026-11-08, ADR-0016).
--
-- * `user_account.must_change_password`: set when the owner creates a login or resets a staff
--   password (a temporary password, shown once). Every session of such an account reaches only
--   "change password" and logout until the person sets their own (`identity::service::resolve`).
-- * `app.password_reset_request`: "Quên mật khẩu?" on the login screen (option A): the owner of
--   the spa sees the request and resets the password in the app. One open request per login per
--   spa; it expires after 24 h; a reset or "Bỏ qua" closes it. Tenant-scoped, RLS like every
--   tenant table. Written by the public endpoint under the requested spa's tenant context.
ALTER TABLE app.user_account
  ADD COLUMN must_change_password boolean NOT NULL DEFAULT false,
  ADD COLUMN password_changed_at  timestamptz;

CREATE TABLE app.password_reset_request (
  id            uuid PRIMARY KEY,
  tenant_id     uuid NOT NULL REFERENCES app.tenant (id),
  user_id       uuid NOT NULL REFERENCES app.user_account (id),
  requested_at  timestamptz NOT NULL DEFAULT now(),
  expires_at    timestamptz NOT NULL,
  closed_at     timestamptz,
  -- `reset` (the owner set a temporary password), `dismissed` ("Bỏ qua") or `expired` (24 h).
  outcome       text CHECK (outcome IN ('reset', 'dismissed', 'expired')),
  closed_by     uuid REFERENCES app.user_account (id),
  CHECK ((closed_at IS NULL) = (outcome IS NULL)),
  CHECK (expires_at > requested_at),
  UNIQUE (tenant_id, id)
);
-- At most one open request per login and spa (a repeated request refreshes nothing).
CREATE UNIQUE INDEX password_reset_request_open_idx
  ON app.password_reset_request (tenant_id, user_id) WHERE closed_at IS NULL;

ALTER TABLE app.password_reset_request ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.password_reset_request
  USING (tenant_id = app.current_tenant())
  WITH CHECK (tenant_id = app.current_tenant());
GRANT SELECT, INSERT, UPDATE ON app.password_reset_request TO app_rw;
-- "At most one request per login and spa per hour": a repeated or dismissed request does not
-- reach the owner again right away.
CREATE INDEX password_reset_request_recent_idx
  ON app.password_reset_request (tenant_id, user_id, requested_at);
