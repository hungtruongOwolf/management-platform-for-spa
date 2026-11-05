-- 0016: appointments (spec R1-3, memory/wiki/product/specs/appointments.md), owner back-dating and
-- discounts on visits (EM review round 4, items 1, 5, 8).

-- ---------------------------------------------------------------------------- permissions
-- New spas get them from identity::roles::SystemRole::default_permissions.
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'appointment.manage'
FROM app.role r
WHERE r.is_system AND r.key IN ('owner', 'manager')
ON CONFLICT DO NOTHING;

INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'visit.backdate'
FROM app.role r
WHERE r.is_system AND r.key = 'owner'
ON CONFLICT DO NOTHING;

-- ------------------------------------------------------------------- discounts on visit lines
-- The visit's one-off discount, split across its one-off lines (sales::domain::allocate_discount).
-- `line_value` stays "what the line is worth": listed amount minus its share of the discount.
ALTER TABLE app.visit_line
  ADD COLUMN discount_amount bigint NOT NULL DEFAULT 0 CHECK (discount_amount >= 0);
ALTER TABLE app.visit_line DROP CONSTRAINT visit_line_check;
ALTER TABLE app.visit_line ADD CONSTRAINT visit_line_check CHECK (
  (source = 'package' AND customer_package_id IS NOT NULL AND customer_package_item_id IS NOT NULL
     AND sessions_left_after IS NOT NULL AND price_variant IS NULL AND discount_amount = 0)
  OR
  (source = 'one_off' AND customer_package_id IS NULL AND customer_package_item_id IS NULL
     AND sessions_left_after IS NULL AND price_variant IS NOT NULL AND expiry_override_reason IS NULL
     AND discount_amount <= qty * unit_price
     AND line_value = qty * unit_price - discount_amount)
);

-- ------------------------------------------------------------------------------ appointments
-- Trusted extension (PG13+): the schema owner may create it; on Supabase it is preinstalled.
CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE app.appointment (
  id               uuid PRIMARY KEY,
  tenant_id        uuid NOT NULL REFERENCES app.tenant (id),
  branch_id        uuid NOT NULL,
  -- An existing customer, or a newcomer known only by name + phone until they arrive.
  customer_id      uuid,
  guest_name       text CHECK (length(btrim(guest_name)) BETWEEN 1 AND 200),
  guest_phone      text CHECK (guest_phone ~ '^\+[0-9]{8,15}$'),
  business_date    date NOT NULL,
  starts_at        timestamptz NOT NULL,
  ends_at          timestamptz NOT NULL,
  -- Preferred technician while booked; the one serving once arrived.
  staff_id         uuid,
  status           text NOT NULL DEFAULT 'booked'
                     CHECK (status IN ('booked', 'arrived', 'done', 'no_show', 'cancelled')),
  -- Booking note (not health data: that belongs to the visit note / health note).
  note             text CHECK (length(note) <= 500),
  status_reason    text CHECK (length(btrim(status_reason)) BETWEEN 1 AND 500),
  arrived_at       timestamptz,
  closed_at        timestamptz,
  closed_by        uuid,
  visit_id         uuid,
  created_by       uuid NOT NULL,
  created_at       timestamptz NOT NULL DEFAULT now(),
  updated_at       timestamptz NOT NULL DEFAULT now(),
  version          bigint NOT NULL DEFAULT 1,
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, visit_id),
  FOREIGN KEY (tenant_id, branch_id) REFERENCES app.branch (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_id) REFERENCES app.customer (tenant_id, id),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id),
  FOREIGN KEY (tenant_id, visit_id) REFERENCES app.visit (tenant_id, id),
  CHECK (ends_at > starts_at AND ends_at <= starts_at + interval '8 hours'),
  CHECK ((customer_id IS NOT NULL AND guest_name IS NULL AND guest_phone IS NULL)
      OR (customer_id IS NULL AND guest_name IS NOT NULL AND guest_phone IS NOT NULL)),
  CHECK (status <> 'arrived' OR (staff_id IS NOT NULL AND arrived_at IS NOT NULL)),
  CHECK ((status = 'done') = (visit_id IS NOT NULL)),
  CHECK (status NOT IN ('no_show', 'cancelled') OR status_reason IS NOT NULL),
  CHECK ((status IN ('done', 'no_show', 'cancelled')) = (closed_at IS NOT NULL AND closed_by IS NOT NULL)),
  -- Second wall behind the service's advisory-locked check: one technician, one open booking at a time.
  CONSTRAINT appointment_staff_no_overlap EXCLUDE USING gist (
    tenant_id WITH =, staff_id WITH =, tstzrange(starts_at, ends_at) WITH &&
  ) WHERE (status IN ('booked', 'arrived') AND staff_id IS NOT NULL)
);
CREATE INDEX appointment_day_idx ON app.appointment (tenant_id, business_date, starts_at);
CREATE INDEX appointment_customer_idx ON app.appointment (tenant_id, customer_id, starts_at DESC)
  WHERE customer_id IS NOT NULL;

CREATE TABLE app.appointment_service (
  tenant_id             uuid NOT NULL,
  appointment_id        uuid NOT NULL,
  position              integer NOT NULL CHECK (position BETWEEN 0 AND 4),
  service_id            uuid NOT NULL,
  service_name_snapshot text NOT NULL CHECK (length(btrim(service_name_snapshot)) BETWEEN 1 AND 200),
  PRIMARY KEY (tenant_id, appointment_id, position),
  FOREIGN KEY (tenant_id, appointment_id) REFERENCES app.appointment (tenant_id, id),
  FOREIGN KEY (tenant_id, service_id) REFERENCES app.service (tenant_id, id)
);

-- A closed appointment (done / no-show / cancelled) never changes again.
CREATE FUNCTION app.appointment_closed_is_final() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status IN ('done', 'no_show', 'cancelled') THEN
    RAISE EXCEPTION 'appointment % is closed', OLD.id USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER appointment_closed_is_final BEFORE UPDATE ON app.appointment
  FOR EACH ROW EXECUTE FUNCTION app.appointment_closed_is_final();

-- ---------------------------------------------------------------- privileges & RLS
GRANT SELECT, INSERT, UPDATE ON app.appointment TO app_rw;          -- never DELETE
GRANT SELECT, INSERT, DELETE ON app.appointment_service TO app_rw;  -- services replaced on reschedule

ALTER TABLE app.appointment ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.appointment
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.appointment_service ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.appointment_service
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
