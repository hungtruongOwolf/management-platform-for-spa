-- 0037: validates the FKs of 0036 on the existing rows. VALIDATE CONSTRAINT takes a SHARE UPDATE
-- EXCLUSIVE lock: reads and writes go on while it scans. Separate from 0036 so the scan does not
-- run under the stronger lock the ADD took (each migration is one transaction).

ALTER TABLE app.visit VALIDATE CONSTRAINT visit_created_by_fkey;
ALTER TABLE app.visit VALIDATE CONSTRAINT visit_voided_by_fkey;
ALTER TABLE app.sale VALIDATE CONSTRAINT sale_created_by_fkey;
ALTER TABLE app.sale VALIDATE CONSTRAINT sale_voided_by_fkey;
ALTER TABLE app.payment VALIDATE CONSTRAINT payment_created_by_fkey;
ALTER TABLE app.refund VALIDATE CONSTRAINT refund_created_by_fkey;
ALTER TABLE app.customer_package VALIDATE CONSTRAINT customer_package_created_by_fkey;
ALTER TABLE app.customer_package VALIDATE CONSTRAINT customer_package_cancelled_by_fkey;
ALTER TABLE app.appointment VALIDATE CONSTRAINT appointment_created_by_fkey;
ALTER TABLE app.appointment VALIDATE CONSTRAINT appointment_closed_by_fkey;
