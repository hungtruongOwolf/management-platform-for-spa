-- 0047: ADR-0019 D12 (phone CHECK = the code's rule) and D13 (customer audit keeps no PII).

-- ---------------------------------------------------------------- D12: mobile numbers only
-- The code accepts only Vietnamese mobile numbers (kernel::PhoneE164: +84, then 3/5/7/8/9, then 8
-- digits); the CHECKs allowed any +84 and 9 digits (and any +country for an appointment guest).
-- NOT VALID: every row written from now on is checked; rows already stored are checked by 0048
-- (VALIDATE) once the lead has confirmed staging holds none that break the rule. The old, wider
-- CHECKs stay until then and are dropped by 0048.
ALTER TABLE app.customer
  ADD CONSTRAINT customer_phone_mobile CHECK (phone_e164 ~ '^\+84[35789][0-9]{8}$') NOT VALID;
ALTER TABLE app.staff
  ADD CONSTRAINT staff_phone_mobile CHECK (phone_e164 ~ '^\+84[35789][0-9]{8}$') NOT VALID;
ALTER TABLE app.user_account
  ADD CONSTRAINT user_account_phone_mobile CHECK (phone_e164 ~ '^\+84[35789][0-9]{8}$') NOT VALID;
ALTER TABLE app.appointment
  ADD CONSTRAINT appointment_guest_phone_mobile CHECK (guest_phone ~ '^\+84[35789][0-9]{8}$') NOT VALID;

-- ---------------------------------------------------------------- D13: scrub customer audit rows
-- `customer.create` / `customer.update` rows stored the full name, phone, birth year and birth
-- date in clear, forever (audit_log has no retention), so anonymising a profile left them in the
-- trail (Nghị định 13/2023). From now on the code writes presence flags plus the list of changed
-- fields (crm::service::audit_presence). This rewrites the old rows the same way and keeps who
-- (actor_user_id), when (at) and what field changed (`after.changed`). Health notes were already
-- only "[redacted]"; they become a `has_health_note` flag like the rest.
CREATE FUNCTION pg_temp.customer_audit_scrub(j jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
  SELECT (j - 'full_name' - 'phone_e164' - 'birth_year' - 'birth_date' - 'health_note')
    || jsonb_strip_nulls(jsonb_build_object(
         'has_name',        CASE WHEN j ? 'full_name'   THEN jsonb_typeof(j -> 'full_name')   <> 'null' END,
         'has_phone',       CASE WHEN j ? 'phone_e164'  THEN jsonb_typeof(j -> 'phone_e164')  <> 'null' END,
         'has_birth_year',  CASE WHEN j ? 'birth_year'  THEN jsonb_typeof(j -> 'birth_year')  <> 'null' END,
         'has_birth_date',  CASE WHEN j ? 'birth_date'  THEN jsonb_typeof(j -> 'birth_date')  <> 'null' END,
         'has_health_note', CASE WHEN j ? 'health_note' THEN jsonb_typeof(j -> 'health_note') <> 'null' END))
$$;

-- Fields whose value differs between the two snapshots, named like the API (`phone`, not
-- `phone_e164`). The health note never changes through `customer.update`.
CREATE FUNCTION pg_temp.customer_audit_changed(b jsonb, a jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
  SELECT coalesce(jsonb_agg(f ORDER BY f), '[]'::jsonb)
  FROM (
    SELECT CASE k WHEN 'phone_e164' THEN 'phone' ELSE k END AS f
    FROM (SELECT jsonb_object_keys(coalesce(b, '{}')) UNION SELECT jsonb_object_keys(coalesce(a, '{}'))) keys(k)
    WHERE k <> 'health_note' AND b -> k IS DISTINCT FROM a -> k
  ) changed
$$;

-- SET expressions read the row as it was, so `changed` compares the unscrubbed snapshots.
-- Idempotent: a scrubbed row has none of the removed keys and already has `changed`.
UPDATE app.audit_log
SET before = pg_temp.customer_audit_scrub(before),
    after = pg_temp.customer_audit_scrub(after)
      || CASE WHEN action = 'customer.update' AND NOT (after ? 'changed')
              THEN jsonb_build_object('changed', pg_temp.customer_audit_changed(before, after))
              ELSE '{}'::jsonb END
WHERE entity_type = 'customer' AND action IN ('customer.create', 'customer.update');
