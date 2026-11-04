-- 0007: visits recorded at the front desk (spec R1-1 US4). A visit groups service lines; each line
-- either consumes sessions of one customer package item or is a one-off service paid in full through
-- the visit's sale. No check-out time (EM 2026-11-04). Voids come with S8.

CREATE TABLE app.visit (
  id             uuid PRIMARY KEY,
  tenant_id      uuid NOT NULL REFERENCES app.tenant (id),
  branch_id      uuid NOT NULL,
  customer_id    uuid NOT NULL,
  business_date  date NOT NULL,
  check_in_at    timestamptz NOT NULL,
  -- Session note incl. reactions after the session: health data, AES-256-GCM (AAD = tenant + visit).
  note_ct        bytea,
  status         text NOT NULL DEFAULT 'completed' CHECK (status IN ('completed', 'voided')),
  void_reason    text CHECK (length(btrim(void_reason)) BETWEEN 1 AND 500),
  voided_at      timestamptz,
  voided_by      uuid,
  -- The sale of the one-off lines; NULL when the visit only consumed package sessions.
  sale_id        uuid,
  created_by     uuid NOT NULL,
  created_at     timestamptz NOT NULL DEFAULT now(),
  updated_at     timestamptz NOT NULL DEFAULT now(),
  version        bigint NOT NULL DEFAULT 1,
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, sale_id),
  FOREIGN KEY (tenant_id, branch_id) REFERENCES app.branch (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_id) REFERENCES app.customer (tenant_id, id),
  FOREIGN KEY (tenant_id, sale_id) REFERENCES app.sale (tenant_id, id),
  CHECK (status <> 'voided' OR (void_reason IS NOT NULL AND voided_at IS NOT NULL AND voided_by IS NOT NULL))
);
CREATE INDEX visit_day_idx ON app.visit (tenant_id, branch_id, business_date, check_in_at DESC);
CREATE INDEX visit_customer_idx ON app.visit (tenant_id, customer_id, check_in_at DESC);

-- unit_price: listed price of a one-off line, per-session value of a package line.
-- line_value: what the line is worth (one-off: qty × unit_price; package: paid sessions taken × value,
-- bonus sessions count 0), the base for commission in R2.
CREATE TABLE app.visit_line (
  id                        uuid PRIMARY KEY,
  tenant_id                 uuid NOT NULL,
  visit_id                  uuid NOT NULL,
  position                  integer NOT NULL CHECK (position >= 0),
  service_id                uuid NOT NULL,
  service_name_snapshot     text NOT NULL CHECK (length(btrim(service_name_snapshot)) BETWEEN 1 AND 200),
  source                    text NOT NULL CHECK (source IN ('package', 'one_off')),
  customer_package_id       uuid,
  customer_package_item_id  uuid,
  -- Sessions left on the item right after this visit (history: "buổi 8/12").
  sessions_left_after       integer CHECK (sessions_left_after >= 0),
  staff_id                  uuid NOT NULL,
  qty                       integer NOT NULL CHECK (qty BETWEEN 1 AND 50),
  price_variant             text CHECK (price_variant IN ('all', 'female', 'male')),
  unit_price                bigint NOT NULL CHECK (unit_price >= 0),
  line_value                bigint NOT NULL CHECK (line_value >= 0),
  -- Set when an expired package was used with `package.use_expired_override` (AC4.6, audited).
  expiry_override_reason    text CHECK (length(btrim(expiry_override_reason)) BETWEEN 1 AND 500),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, visit_id, position),
  FOREIGN KEY (tenant_id, visit_id) REFERENCES app.visit (tenant_id, id),
  FOREIGN KEY (tenant_id, service_id) REFERENCES app.service (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_package_id) REFERENCES app.customer_package (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_package_item_id) REFERENCES app.customer_package_item (tenant_id, id),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id),
  CHECK (
    (source = 'package' AND customer_package_id IS NOT NULL AND customer_package_item_id IS NOT NULL
       AND sessions_left_after IS NOT NULL AND price_variant IS NULL)
    OR
    (source = 'one_off' AND customer_package_id IS NULL AND customer_package_item_id IS NULL
       AND sessions_left_after IS NULL AND price_variant IS NOT NULL AND expiry_override_reason IS NULL
       AND line_value = qty * unit_price)
  )
);
CREATE INDEX visit_line_visit_idx ON app.visit_line (tenant_id, visit_id);
-- AC4.4: visits per technician per day (R2 attendance) and a package's usage history.
CREATE INDEX visit_line_staff_idx ON app.visit_line (tenant_id, staff_id);
CREATE INDEX visit_line_item_idx ON app.visit_line (tenant_id, customer_package_item_id)
  WHERE customer_package_item_id IS NOT NULL;

-- ---------------------------------------------------------------- privileges & RLS
GRANT SELECT, INSERT, UPDATE ON app.visit TO app_rw;            -- note correction, void (S8); never DELETE
GRANT SELECT, INSERT ON app.visit_line TO app_rw;
GRANT UPDATE (staff_id) ON app.visit_line TO app_rw;            -- technician correction only

ALTER TABLE app.visit ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.visit
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.visit_line ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.visit_line
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
