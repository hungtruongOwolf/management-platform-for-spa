-- 0045: a staff profile's login is a member of the same spa (audit 2026-11-09, remaining ticket
-- "FK staff.user_id -> membership"). staff.user_id referenced user_account only, so a bug could
-- link a profile to an account of another spa, and every "who supervises / who is the owner" join
-- (membership ON tenant_id, user_id) would then miss it silently. The service already refuses it
-- (org::profile::check_link: active member); this is the database backstop.
--
-- app.membership has UNIQUE (tenant_id, user_id) and its rows are never deleted (app_rw has no
-- DELETE on it; leaving or locking disables a membership), so NO ACTION never fires on app paths.
-- A profile without a login (user_id NULL) is not checked (MATCH SIMPLE). The existing
-- staff_user_idx (tenant_id, user_id) serves the FK's reverse lookups.
--
-- NOT VALID: only a short lock, new and updated rows are checked; 0046 validates existing rows
-- without blocking writes. Before running it on staging, this must return no row:
--   SELECT s.tenant_id, s.id FROM app.staff s
--   WHERE s.user_id IS NOT NULL AND NOT EXISTS (
--     SELECT 1 FROM app.membership m WHERE m.tenant_id = s.tenant_id AND m.user_id = s.user_id);

ALTER TABLE app.staff
  ADD CONSTRAINT staff_membership_fkey FOREIGN KEY (tenant_id, user_id)
    REFERENCES app.membership (tenant_id, user_id) NOT VALID;
