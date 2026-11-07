-- 0025: staff roster (EM 2026-11-07 item 9, spec staff-management §13, ADR-0012).
-- Employment is a list of periods: hiring opens one, ending employment (nghỉ việc) closes it with a
-- date and a reason, rehiring opens a new one. Nothing is ever deleted: payroll, visits,
-- commissions and payslips keep pointing at the same staff row. `app.staff.status / joined_on /
-- left_on` stay as the mirror of the latest period, so every picker filter (`status = 'active'`)
-- keeps working unchanged. Depends on 0005 (staff), 0016 (btree_gist), 0017 (staff profile).

-- Roster read access (names, positions, periods, tenure; no phone, note, login or end reason):
-- owner and manager. Writes stay `staff.manage` (owner).
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'staff.read'
FROM app.role r
WHERE r.is_system AND r.key IN ('owner', 'manager')
ON CONFLICT DO NOTHING;

-- Free-text HR note on the profile (owner only; never in the audit log, only `has_note`).
ALTER TABLE app.staff
  ADD COLUMN note text CHECK (note IS NULL OR length(btrim(note)) BETWEEN 1 AND 1000);

CREATE TABLE app.staff_employment (
  id           uuid PRIMARY KEY,
  tenant_id    uuid NOT NULL REFERENCES app.tenant (id),
  staff_id     uuid NOT NULL,
  -- NULL only for a profile created before start dates were recorded ("since before the books").
  started_on   date,
  ended_on     date,
  end_reason   text CHECK (end_reason IS NULL OR length(btrim(end_reason)) BETWEEN 1 AND 500),
  ended_by     uuid REFERENCES app.user_account (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id),
  CHECK (ended_on IS NULL OR started_on IS NULL OR ended_on >= started_on),
  -- Periods backfilled from 0005 have no reason; new ones always do (service).
  CHECK (end_reason IS NULL OR ended_on IS NOT NULL),
  CONSTRAINT staff_employment_no_overlap EXCLUDE USING gist (
    tenant_id WITH =, staff_id WITH =,
    daterange(COALESCE(started_on, '-infinity'::date), ended_on, '[]') WITH &&
  )
);
-- At most one open period per person.
CREATE UNIQUE INDEX staff_employment_open_idx ON app.staff_employment (tenant_id, staff_id)
  WHERE ended_on IS NULL;
CREATE INDEX staff_employment_staff_idx ON app.staff_employment (tenant_id, staff_id, started_on);

-- An ended period is history: it never changes again. An open one may get its start corrected
-- or be ended, once.
CREATE FUNCTION app.staff_employment_forward_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' OR OLD.ended_on IS NOT NULL
     OR (NEW.id, NEW.tenant_id, NEW.staff_id, NEW.created_at)
        IS DISTINCT FROM (OLD.id, OLD.tenant_id, OLD.staff_id, OLD.created_at) THEN
    RAISE EXCEPTION 'an ended employment period is final' USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER staff_employment_forward_only BEFORE UPDATE OR DELETE ON app.staff_employment
  FOR EACH ROW EXECUTE FUNCTION app.staff_employment_forward_only();

-- Every new staff row starts with its first period, whoever inserts it (API, CLI `add-staff`,
-- demo seed, tests), so no profile is ever without one.
CREATE FUNCTION app.staff_first_period() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO app.staff_employment (id, tenant_id, staff_id, started_on, ended_on)
  VALUES (gen_random_uuid(), NEW.tenant_id, NEW.id, NEW.joined_on, NEW.left_on);
  RETURN NEW;
END
$$;
CREATE TRIGGER staff_first_period AFTER INSERT ON app.staff
  FOR EACH ROW EXECUTE FUNCTION app.staff_first_period();

-- Backfill: today's single joined_on / left_on becomes the first period.
INSERT INTO app.staff_employment (id, tenant_id, staff_id, started_on, ended_on)
SELECT gen_random_uuid(), s.tenant_id, s.id, s.joined_on, s.left_on
FROM app.staff s;

-- Pickers (closer, KTV làm, booking, daily book) list active staff by name.
CREATE INDEX staff_active_name_idx ON app.staff (tenant_id, full_name) WHERE status = 'active';

GRANT SELECT, INSERT, UPDATE ON app.staff_employment TO app_rw;   -- no DELETE: history stays
ALTER TABLE app.staff_employment ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.staff_employment
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
-- Ending employment locks the person's login in this spa (offboarding), rehiring unlocks it:
-- app_rw already has UPDATE on app.membership (0002).
