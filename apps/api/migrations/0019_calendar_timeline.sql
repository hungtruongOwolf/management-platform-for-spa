-- 0019: the calendar as the desk's timeline (EM review round 5, items 3–6; spec appointments.md §10):
-- group / family bookings, and the owner's optional "max customers per slot" instead of a capacity
-- derived from the technician count.

-- ------------------------------------------------------------------------------ permissions
-- New spas get it from identity::roles::SystemRole::default_permissions (owner = every permission).
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'appointment.settings'
FROM app.role r
WHERE r.is_system AND r.key = 'owner'
ON CONFLICT DO NOTHING;

-- ------------------------------------------------------------------------------- group bookings
-- Appointments booked together (a family) share one id; each person is still its own appointment,
-- visit and payment. NULL = booked alone.
ALTER TABLE app.appointment ADD COLUMN group_id uuid;
CREATE INDEX appointment_group_idx ON app.appointment (tenant_id, group_id) WHERE group_id IS NOT NULL;

-- ----------------------------------------------------------------------------- calendar settings
-- One row per spa, written only by the owner (`appointment.settings`). A table of its own rather
-- than `tenant.settings`, so app_rw never gets UPDATE on app.tenant.
CREATE TABLE app.appointment_setting (
  tenant_id     uuid PRIMARY KEY REFERENCES app.tenant (id),
  -- Advisory: the calendar shows "n / max" and warns when full; bookings are never refused on it.
  -- NULL = no limit.
  max_per_slot  integer CHECK (max_per_slot BETWEEN 1 AND 200),
  updated_by    uuid NOT NULL,
  updated_at    timestamptz NOT NULL DEFAULT now()
);

GRANT SELECT, INSERT, UPDATE ON app.appointment_setting TO app_rw;  -- never DELETE
ALTER TABLE app.appointment_setting ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.appointment_setting
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
