-- 0046: validates the FK of 0045 on the existing rows. VALIDATE CONSTRAINT takes a SHARE UPDATE
-- EXCLUSIVE lock: reads and writes go on while it scans. Separate from 0045 so the scan does not
-- run under the stronger lock the ADD took (each migration is one transaction).

ALTER TABLE app.staff VALIDATE CONSTRAINT staff_membership_fkey;
