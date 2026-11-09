-- 0043: validates the FKs 0042 added NOT VALID. VALIDATE CONSTRAINT takes a SHARE UPDATE
-- EXCLUSIVE lock: reads and writes go on while it scans. Separate from 0042 so the scan does not
-- run under the stronger lock the ADD took (each migration is one transaction).

ALTER TABLE app.visit_line VALIDATE CONSTRAINT visit_line_item_of_package_fkey;
ALTER TABLE app.appointment_setting VALIDATE CONSTRAINT appointment_setting_updated_by_fkey;
