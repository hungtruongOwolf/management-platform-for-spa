-- 0048: validates the phone CHECKs 0047 added NOT VALID (ADR-0019 D12), then drops the wider
-- CHECKs they replace. Run on a database only after checking it holds no number outside the rule:
--
--   SELECT 'customer', count(*) FROM app.customer WHERE phone_e164 !~ '^\+84[35789][0-9]{8}$'
--   UNION ALL SELECT 'staff', count(*) FROM app.staff WHERE phone_e164 !~ '^\+84[35789][0-9]{8}$'
--   UNION ALL SELECT 'user_account', count(*) FROM app.user_account WHERE phone_e164 !~ '^\+84[35789][0-9]{8}$'
--   UNION ALL SELECT 'appointment', count(*) FROM app.appointment WHERE guest_phone !~ '^\+84[35789][0-9]{8}$';
--
-- (all four must be 0; NULL phones pass). VALIDATE takes a SHARE UPDATE EXCLUSIVE lock: reads and
-- writes go on while it scans. Separate from 0047 so the scan does not run under the stronger lock
-- the ADD took (each migration is one transaction).

ALTER TABLE app.customer VALIDATE CONSTRAINT customer_phone_mobile;
ALTER TABLE app.staff VALIDATE CONSTRAINT staff_phone_mobile;
ALTER TABLE app.user_account VALIDATE CONSTRAINT user_account_phone_mobile;
ALTER TABLE app.appointment VALIDATE CONSTRAINT appointment_guest_phone_mobile;

ALTER TABLE app.customer DROP CONSTRAINT customer_phone_e164_check;
ALTER TABLE app.staff DROP CONSTRAINT staff_phone_e164_check;
ALTER TABLE app.user_account DROP CONSTRAINT user_account_phone_e164_check;
ALTER TABLE app.appointment DROP CONSTRAINT appointment_guest_phone_check;
