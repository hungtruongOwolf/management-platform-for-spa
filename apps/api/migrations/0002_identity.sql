-- 0002: identity — global accounts and sessions, per-tenant memberships, roles as data.

-- ---------------------------------------------------------------- global (not tenant-scoped)
-- A person may work at several spas: accounts are global, access is granted per tenant via membership.
CREATE TABLE app.user_account (
  id              uuid PRIMARY KEY,
  phone_e164      text NOT NULL UNIQUE CHECK (phone_e164 ~ '^\+84[0-9]{9}$'),
  full_name       text NOT NULL CHECK (length(btrim(full_name)) BETWEEN 1 AND 200),
  password_hash   text NOT NULL CHECK (password_hash LIKE '$argon2id$%'),
  status          text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
  failed_logins   integer NOT NULL DEFAULT 0 CHECK (failed_logins >= 0),
  locked_until    timestamptz,
  created_at      timestamptz NOT NULL DEFAULT now(),
  updated_at      timestamptz NOT NULL DEFAULT now()
);

-- Opaque sessions: only the SHA-256 of the cookie token is stored.
CREATE TABLE app.session (
  id                   uuid PRIMARY KEY,
  token_hash           bytea NOT NULL UNIQUE CHECK (length(token_hash) = 32),
  user_id              uuid NOT NULL REFERENCES app.user_account (id),
  active_tenant_id     uuid REFERENCES app.tenant (id),
  created_at           timestamptz NOT NULL DEFAULT now(),
  last_seen_at         timestamptz NOT NULL DEFAULT now(),
  idle_expires_at      timestamptz NOT NULL,
  absolute_expires_at  timestamptz NOT NULL,
  recent_auth_at       timestamptz NOT NULL DEFAULT now(),
  ip                   inet,
  user_agent           text,
  revoked_at           timestamptz,
  CHECK (idle_expires_at <= absolute_expires_at)
);
CREATE INDEX session_user_idx ON app.session (user_id) WHERE revoked_at IS NULL;

-- ---------------------------------------------------------------- tenant-scoped
CREATE TABLE app.membership (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  user_id     uuid NOT NULL REFERENCES app.user_account (id),
  status      text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, user_id)
);

CREATE TABLE app.role (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  key         text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_]{1,39}$'),
  name        text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 100),
  is_system   boolean NOT NULL DEFAULT false,
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, key)
);

CREATE TABLE app.role_permission (
  tenant_id   uuid NOT NULL,
  role_id     uuid NOT NULL,
  permission  text NOT NULL CHECK (permission ~ '^[a-z_]+(\.[a-z_]+)+$'),
  PRIMARY KEY (tenant_id, role_id, permission),
  FOREIGN KEY (tenant_id, role_id) REFERENCES app.role (tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE app.membership_role (
  tenant_id      uuid NOT NULL,
  membership_id  uuid NOT NULL,
  role_id        uuid NOT NULL,
  PRIMARY KEY (tenant_id, membership_id, role_id),
  FOREIGN KEY (tenant_id, membership_id) REFERENCES app.membership (tenant_id, id) ON DELETE CASCADE,
  FOREIGN KEY (tenant_id, role_id) REFERENCES app.role (tenant_id, id) ON DELETE CASCADE
);

-- Login happens before any tenant is chosen, so listing a user's tenants needs this narrow,
-- owner-defined function instead of weakening RLS on membership.
CREATE FUNCTION app.tenants_of_user(p_user_id uuid)
RETURNS TABLE (tenant_id uuid, tenant_name text)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT t.id, t.name
  FROM app.membership m
  JOIN app.tenant t ON t.id = m.tenant_id
  WHERE m.user_id = p_user_id AND m.status = 'active' AND t.status = 'active'
  ORDER BY t.name
$$;
REVOKE ALL ON FUNCTION app.tenants_of_user(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.tenants_of_user(uuid) TO app_rw;

-- ---------------------------------------------------------------- privileges & RLS
-- Global identity tables are reached only through the identity crate (token-hash / phone lookups).
GRANT SELECT, INSERT, UPDATE ON app.user_account TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.session TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.membership TO app_rw;
GRANT SELECT, INSERT, UPDATE, DELETE ON app.role TO app_rw;
GRANT SELECT, INSERT, DELETE ON app.role_permission TO app_rw;
GRANT SELECT, INSERT, DELETE ON app.membership_role TO app_rw;

ALTER TABLE app.membership ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.membership
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());

ALTER TABLE app.role ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.role
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());

ALTER TABLE app.role_permission ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.role_permission
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());

ALTER TABLE app.membership_role ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.membership_role
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
