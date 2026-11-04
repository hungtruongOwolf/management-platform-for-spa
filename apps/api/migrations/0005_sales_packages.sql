-- 0005: staff (minimal org), sales with append-only payments, customer packages (spec R1-1 US3).
-- Money is bigint đồng. Every FK is (tenant_id, x_id) so no row can point into another spa.

-- ---------------------------------------------------------------- org: staff
-- Who can close a sale or perform a service. Leaving = status 'left' + left_on; staff are never deleted.
CREATE TABLE app.staff (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  branch_id   uuid NOT NULL,
  full_name   text NOT NULL CHECK (length(btrim(full_name)) BETWEEN 1 AND 200),
  phone_e164  text CHECK (phone_e164 ~ '^\+84[0-9]{9}$'),
  position    text NOT NULL CHECK (length(btrim(position)) BETWEEN 1 AND 100),
  status      text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'left')),
  joined_on   date,
  left_on     date,
  version     bigint NOT NULL DEFAULT 1,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  FOREIGN KEY (tenant_id, branch_id) REFERENCES app.branch (tenant_id, id),
  CHECK ((status = 'left') = (left_on IS NOT NULL)),
  CHECK (left_on IS NULL OR joined_on IS NULL OR left_on >= joined_on)
);
CREATE INDEX staff_status_idx ON app.staff (tenant_id, status, full_name);

-- ---------------------------------------------------------------- sales
-- R1 always collects in full (EM 2026-11-04: no receivables), so amount_paid = total at creation.
-- The column and its CHECK stay so receivables can be switched on later without a schema change.
CREATE TABLE app.sale (
  id               uuid PRIMARY KEY,
  tenant_id        uuid NOT NULL REFERENCES app.tenant (id),
  branch_id        uuid NOT NULL,
  customer_id      uuid NOT NULL,
  business_date    date NOT NULL,
  subtotal         bigint NOT NULL CHECK (subtotal >= 0),
  discount_amount  bigint NOT NULL DEFAULT 0 CHECK (discount_amount >= 0),
  discount_reason  text CHECK (length(btrim(discount_reason)) BETWEEN 1 AND 500),
  total            bigint NOT NULL,
  amount_paid      bigint NOT NULL,
  status           text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'voided')),
  closer_staff_id  uuid,
  void_reason      text,
  voided_at        timestamptz,
  voided_by        uuid,
  created_by       uuid NOT NULL,
  created_at       timestamptz NOT NULL DEFAULT now(),
  version          bigint NOT NULL DEFAULT 1,
  UNIQUE (tenant_id, id),
  FOREIGN KEY (tenant_id, branch_id) REFERENCES app.branch (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_id) REFERENCES app.customer (tenant_id, id),
  FOREIGN KEY (tenant_id, closer_staff_id) REFERENCES app.staff (tenant_id, id),
  CHECK (total = subtotal - discount_amount AND total >= 0),
  CHECK (discount_amount = 0 OR discount_reason IS NOT NULL),
  CHECK (amount_paid BETWEEN 0 AND total),
  CHECK (status <> 'voided' OR (void_reason IS NOT NULL AND voided_at IS NOT NULL))
);
CREATE INDEX sale_customer_idx ON app.sale (tenant_id, customer_id, created_at DESC);
CREATE INDEX sale_day_idx ON app.sale (tenant_id, branch_id, business_date);

-- ---------------------------------------------------------------- packages
-- Plan name / price / sessions are snapshotted at sale time (AC1.2): later catalog edits never change them.
-- Status is derived (cancelled → exhausted → expired → active), never stored.
CREATE TABLE app.customer_package (
  id                uuid PRIMARY KEY,
  tenant_id         uuid NOT NULL REFERENCES app.tenant (id),
  customer_id       uuid NOT NULL,
  plan_id           uuid,
  name_snapshot     text NOT NULL CHECK (length(btrim(name_snapshot)) BETWEEN 1 AND 200),
  gift_note         text CHECK (length(gift_note) <= 500),
  sold_price        bigint NOT NULL CHECK (sold_price >= 0),
  origin            text NOT NULL CHECK (origin IN ('sale', 'opening_balance')),
  sale_id           uuid,
  closer_staff_id   uuid,
  starts_on         date NOT NULL,
  expires_on        date,
  cancelled_at      timestamptz,
  cancel_reason     text,
  created_by        uuid NOT NULL,
  created_at        timestamptz NOT NULL DEFAULT now(),
  updated_at        timestamptz NOT NULL DEFAULT now(),
  version           bigint NOT NULL DEFAULT 1,
  UNIQUE (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_id) REFERENCES app.customer (tenant_id, id),
  FOREIGN KEY (tenant_id, plan_id) REFERENCES app.package_plan (tenant_id, id),
  FOREIGN KEY (tenant_id, sale_id) REFERENCES app.sale (tenant_id, id),
  FOREIGN KEY (tenant_id, closer_staff_id) REFERENCES app.staff (tenant_id, id),
  CHECK (origin <> 'sale' OR (sale_id IS NOT NULL AND closer_staff_id IS NOT NULL)),
  CHECK (origin <> 'opening_balance' OR sale_id IS NULL),
  CHECK (expires_on IS NULL OR expires_on >= starts_on),
  CHECK (cancelled_at IS NULL OR cancel_reason IS NOT NULL)
);
CREATE INDEX customer_package_customer_idx ON app.customer_package (tenant_id, customer_id, created_at DESC);
CREATE INDEX customer_package_sale_idx ON app.customer_package (tenant_id, sale_id) WHERE sale_id IS NOT NULL;

-- Sessions are consumed with an atomic `remaining - qty WHERE remaining >= qty`; the CHECK is the backstop.
-- per_session_value = sold price after discount ÷ purchased sessions (D4); bonus sessions are worth 0.
CREATE TABLE app.customer_package_item (
  id                   uuid PRIMARY KEY,
  tenant_id            uuid NOT NULL,
  customer_package_id  uuid NOT NULL,
  service_id           uuid NOT NULL,
  name_snapshot        text NOT NULL CHECK (length(btrim(name_snapshot)) BETWEEN 1 AND 200),
  sessions_purchased   integer NOT NULL CHECK (sessions_purchased BETWEEN 0 AND 500),
  sessions_bonus       integer NOT NULL DEFAULT 0 CHECK (sessions_bonus BETWEEN 0 AND 500),
  sessions_total       integer GENERATED ALWAYS AS (sessions_purchased + sessions_bonus) STORED,
  sessions_remaining   integer NOT NULL,
  per_session_value    bigint NOT NULL CHECK (per_session_value >= 0),
  version              bigint NOT NULL DEFAULT 1,
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, customer_package_id, service_id),
  FOREIGN KEY (tenant_id, customer_package_id) REFERENCES app.customer_package (tenant_id, id),
  FOREIGN KEY (tenant_id, service_id) REFERENCES app.service (tenant_id, id),
  CHECK (sessions_purchased + sessions_bonus > 0),
  CHECK (sessions_remaining BETWEEN 0 AND sessions_purchased + sessions_bonus)
);

-- Lines are immutable once written (a correction is a void + new sale).
CREATE TABLE app.sale_line (
  id                    uuid PRIMARY KEY,
  tenant_id             uuid NOT NULL,
  sale_id               uuid NOT NULL,
  line_type             text NOT NULL CHECK (line_type IN ('package', 'service')),
  service_id            uuid,
  package_plan_id       uuid,
  customer_package_id   uuid,
  description_snapshot  text NOT NULL CHECK (length(btrim(description_snapshot)) BETWEEN 1 AND 200),
  qty                   integer NOT NULL CHECK (qty > 0),
  unit_price            bigint NOT NULL CHECK (unit_price >= 0),
  line_total            bigint NOT NULL,
  position              integer NOT NULL CHECK (position >= 0),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, sale_id, position),
  FOREIGN KEY (tenant_id, sale_id) REFERENCES app.sale (tenant_id, id),
  FOREIGN KEY (tenant_id, service_id) REFERENCES app.service (tenant_id, id),
  FOREIGN KEY (tenant_id, package_plan_id) REFERENCES app.package_plan (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_package_id) REFERENCES app.customer_package (tenant_id, id),
  CHECK (line_total = qty * unit_price),
  CHECK (line_type <> 'package' OR (package_plan_id IS NOT NULL AND customer_package_id IS NOT NULL AND service_id IS NULL)),
  CHECK (line_type <> 'service' OR (service_id IS NOT NULL AND package_plan_id IS NULL AND customer_package_id IS NULL))
);
CREATE INDEX sale_line_sale_idx ON app.sale_line (tenant_id, sale_id);

-- Append-only money movements. A mistake is corrected by a 'reversal' row (S8), never by UPDATE/DELETE.
CREATE TABLE app.payment (
  id             uuid PRIMARY KEY,
  tenant_id      uuid NOT NULL REFERENCES app.tenant (id),
  branch_id      uuid NOT NULL,
  sale_id        uuid NOT NULL,
  business_date  date NOT NULL,
  kind           text NOT NULL DEFAULT 'payment' CHECK (kind IN ('payment', 'reversal')),
  method         text NOT NULL CHECK (method IN ('cash', 'bank_transfer')),
  amount         bigint NOT NULL CHECK (amount > 0),
  reversal_of    uuid,
  reference      text CHECK (length(reference) <= 200),
  paid_at        timestamptz NOT NULL DEFAULT now(),
  created_by     uuid NOT NULL,
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, reversal_of),
  FOREIGN KEY (tenant_id, branch_id) REFERENCES app.branch (tenant_id, id),
  FOREIGN KEY (tenant_id, sale_id) REFERENCES app.sale (tenant_id, id),
  FOREIGN KEY (tenant_id, reversal_of) REFERENCES app.payment (tenant_id, id),
  CHECK ((kind = 'reversal') = (reversal_of IS NOT NULL))
);
CREATE INDEX payment_sale_idx ON app.payment (tenant_id, sale_id);
CREATE INDEX payment_day_idx ON app.payment (tenant_id, branch_id, business_date);

-- Second wall behind the missing grants: even the schema owner cannot rewrite money history.
CREATE FUNCTION app.reject_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION '% is append-only', TG_TABLE_NAME USING ERRCODE = 'insufficient_privilege';
END
$$;
CREATE TRIGGER payment_append_only BEFORE UPDATE OR DELETE ON app.payment
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();
CREATE TRIGGER sale_line_append_only BEFORE UPDATE OR DELETE ON app.sale_line
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();

-- ---------------------------------------------------------------- privileges & RLS
GRANT SELECT, INSERT, UPDATE ON app.staff TO app_rw;                  -- no DELETE: staff leave, never vanish
GRANT SELECT, INSERT, UPDATE ON app.sale TO app_rw;                   -- UPDATE only for void (S8)
GRANT SELECT, INSERT ON app.sale_line TO app_rw;                      -- immutable
GRANT SELECT, INSERT ON app.payment TO app_rw;                        -- append-only
GRANT SELECT, INSERT, UPDATE ON app.customer_package TO app_rw;       -- extend / cancel (S8)
GRANT SELECT, INSERT, UPDATE ON app.customer_package_item TO app_rw;  -- session counter

ALTER TABLE app.staff ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.staff
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.sale ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.sale
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.sale_line ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.sale_line
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.payment ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.payment
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.customer_package ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.customer_package
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.customer_package_item ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.customer_package_item
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
