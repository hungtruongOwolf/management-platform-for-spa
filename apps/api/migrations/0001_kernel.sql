-- 0001: schema, tenant isolation primitives, tenant/branch, audit log, idempotency.
-- Runs as the schema owner. Requires cluster role `app_rw` (infra/postgres-init/01-roles.sql).

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE SCHEMA app;
REVOKE ALL ON SCHEMA app FROM PUBLIC;
GRANT USAGE ON SCHEMA app TO app_rw;

-- Tenant of the current transaction, set by kernel::TenantTx via set_config('app.tenant_id', …, true).
-- NULL when unset, so every RLS policy below matches nothing by default (deny by default).
CREATE FUNCTION app.current_tenant() RETURNS uuid
LANGUAGE sql STABLE AS $$
  SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

-- ---------------------------------------------------------------- tenant / branch
CREATE TABLE app.tenant (
  id          uuid PRIMARY KEY,
  name        text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
  slug        text NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9]([a-z0-9-]{0,38}[a-z0-9])?$'),
  timezone    text NOT NULL DEFAULT 'Asia/Ho_Chi_Minh',
  status      text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
  settings    jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(settings) = 'object'),
  created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE app.branch (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  name        text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
  address     text,
  active      boolean NOT NULL DEFAULT true,
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, name)
);

-- ---------------------------------------------------------------- audit log (append-only)
CREATE TABLE app.audit_log (
  id             uuid PRIMARY KEY,
  tenant_id      uuid REFERENCES app.tenant (id),
  actor_user_id  uuid,
  actor_kind     text NOT NULL CHECK (actor_kind IN ('user', 'platform_admin', 'system')),
  action         text NOT NULL CHECK (action ~ '^[a-z_]+(\.[a-z_]+)+$'),
  entity_type    text NOT NULL,
  entity_id      uuid NOT NULL,
  before         jsonb,
  after          jsonb,
  reason         text,
  ip             inet,
  at             timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_entity_idx ON app.audit_log (tenant_id, entity_type, entity_id, at);

-- ---------------------------------------------------------------- idempotency
-- A retried request with the same key blocks on the primary key until the first transaction
-- finishes, then replays the stored response; if the first one rolled back, the retry runs fresh.
CREATE TABLE app.idempotency_record (
  tenant_id        uuid NOT NULL REFERENCES app.tenant (id),
  actor_user_id    uuid NOT NULL,
  key              uuid NOT NULL,
  request_hash     bytea NOT NULL CHECK (length(request_hash) = 32),
  response_status  smallint CHECK (response_status BETWEEN 100 AND 599),
  response_body    jsonb,
  created_at       timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, actor_user_id, key)
);
CREATE INDEX idempotency_record_created_idx ON app.idempotency_record (created_at);

-- Cross-tenant housekeeping runs through this owner-defined function, so app_rw never needs BYPASSRLS.
CREATE FUNCTION app.purge_idempotency_records(older_than interval) RETURNS bigint
LANGUAGE sql SECURITY DEFINER SET search_path = app, pg_temp AS $$
  WITH deleted AS (
    DELETE FROM app.idempotency_record WHERE created_at < now() - older_than RETURNING 1
  )
  SELECT count(*) FROM deleted
$$;
REVOKE ALL ON FUNCTION app.purge_idempotency_records(interval) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.purge_idempotency_records(interval) TO app_rw;

-- ---------------------------------------------------------------- privileges & row-level security
GRANT SELECT ON app.tenant TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.branch TO app_rw;
GRANT SELECT, INSERT ON app.audit_log TO app_rw;                 -- no UPDATE/DELETE: append-only
GRANT SELECT, INSERT, UPDATE ON app.idempotency_record TO app_rw; -- no DELETE: purge via function
GRANT EXECUTE ON FUNCTION app.current_tenant() TO app_rw;

ALTER TABLE app.tenant ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_self ON app.tenant
  USING (id = app.current_tenant());

ALTER TABLE app.branch ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.branch
  USING (tenant_id = app.current_tenant())
  WITH CHECK (tenant_id = app.current_tenant());

ALTER TABLE app.audit_log ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.audit_log
  USING (tenant_id = app.current_tenant())
  WITH CHECK (tenant_id = app.current_tenant());

ALTER TABLE app.idempotency_record ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.idempotency_record
  USING (tenant_id = app.current_tenant())
  WITH CHECK (tenant_id = app.current_tenant());
