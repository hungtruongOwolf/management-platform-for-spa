-- 0003: catalog — service categories, services with price variants, package plans.

CREATE TABLE app.service_category (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  name        text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 120),
  sort_order  integer NOT NULL DEFAULT 0,
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, name)
);

CREATE TABLE app.service (
  id            uuid PRIMARY KEY,
  tenant_id     uuid NOT NULL,
  category_id   uuid NOT NULL,
  name          text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
  description   text,
  duration_min  integer CHECK (duration_min IS NULL OR duration_min BETWEEN 1 AND 600), -- NULL = "theo tư vấn"
  recommended   boolean NOT NULL DEFAULT false,
  active        boolean NOT NULL DEFAULT true,
  sort_order    integer NOT NULL DEFAULT 0,
  version       bigint NOT NULL DEFAULT 1,
  created_at    timestamptz NOT NULL DEFAULT now(),
  updated_at    timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, name),
  FOREIGN KEY (tenant_id, category_id) REFERENCES app.service_category (tenant_id, id)
);
CREATE INDEX service_category_idx ON app.service (tenant_id, category_id, sort_order);

-- One flat price ('all') or gender-specific prices; a missing gender = not offered to that gender.
-- "Exactly 'all' or only genders" is enforced in the catalog service (spans rows).
CREATE TABLE app.service_price (
  tenant_id   uuid NOT NULL,
  service_id  uuid NOT NULL,
  variant     text NOT NULL CHECK (variant IN ('all', 'female', 'male')),
  amount      bigint NOT NULL CHECK (amount >= 0),
  PRIMARY KEY (tenant_id, service_id, variant),
  FOREIGN KEY (tenant_id, service_id) REFERENCES app.service (tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE app.package_plan (
  id               uuid PRIMARY KEY,
  tenant_id        uuid NOT NULL REFERENCES app.tenant (id),
  name             text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 200),
  price            bigint NOT NULL CHECK (price >= 0),
  validity_months  integer CHECK (validity_months IS NULL OR validity_months BETWEEN 1 AND 120),
  gift_note        text,
  active           boolean NOT NULL DEFAULT true,
  version          bigint NOT NULL DEFAULT 1,
  created_at       timestamptz NOT NULL DEFAULT now(),
  updated_at       timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id)
);
CREATE UNIQUE INDEX package_plan_active_name_uq ON app.package_plan (tenant_id, name) WHERE active;

CREATE TABLE app.package_plan_item (
  id              uuid PRIMARY KEY,
  tenant_id       uuid NOT NULL,
  plan_id         uuid NOT NULL,
  service_id      uuid NOT NULL,
  sessions        integer NOT NULL CHECK (sessions BETWEEN 1 AND 500),
  bonus_sessions  integer NOT NULL DEFAULT 0 CHECK (bonus_sessions BETWEEN 0 AND 500),
  UNIQUE (tenant_id, plan_id, service_id),
  FOREIGN KEY (tenant_id, plan_id) REFERENCES app.package_plan (tenant_id, id) ON DELETE CASCADE,
  FOREIGN KEY (tenant_id, service_id) REFERENCES app.service (tenant_id, id)
);

-- ---------------------------------------------------------------- privileges & RLS
GRANT SELECT, INSERT, UPDATE ON app.service_category TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.service TO app_rw;              -- no DELETE: deactivate instead (AC1.6)
GRANT SELECT, INSERT, UPDATE, DELETE ON app.service_price TO app_rw; -- price variants are replaced as a set
GRANT SELECT, INSERT, UPDATE ON app.package_plan TO app_rw;
GRANT SELECT, INSERT, UPDATE, DELETE ON app.package_plan_item TO app_rw;

ALTER TABLE app.service_category ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.service_category
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.service ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.service
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.service_price ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.service_price
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.package_plan ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.package_plan
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.package_plan_item ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.package_plan_item
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
