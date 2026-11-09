-- 0036: the "who did it" columns of the money and booking tables reference user_account (db review
-- 2026-11 C4), like the HR and follow-up tables already do (recorded_by, created_by, closed_by …).
-- A bug can no longer store an id that is no one. NOT VALID here: adding the constraint only takes
-- a short lock and checks new rows; 0037 validates the existing rows without blocking writes.
-- user_account rows are never deleted (accounts are disabled), so NO ACTION never fires; the
-- append-only tables (payment, refund) keep their triggers and grants unchanged.
-- Staging had no orphan on 2026-11-09 (infra/perf/db/actor_column_orphans.sql).

ALTER TABLE app.visit
  ADD CONSTRAINT visit_created_by_fkey FOREIGN KEY (created_by) REFERENCES app.user_account (id) NOT VALID,
  ADD CONSTRAINT visit_voided_by_fkey FOREIGN KEY (voided_by) REFERENCES app.user_account (id) NOT VALID;
ALTER TABLE app.sale
  ADD CONSTRAINT sale_created_by_fkey FOREIGN KEY (created_by) REFERENCES app.user_account (id) NOT VALID,
  ADD CONSTRAINT sale_voided_by_fkey FOREIGN KEY (voided_by) REFERENCES app.user_account (id) NOT VALID;
ALTER TABLE app.payment
  ADD CONSTRAINT payment_created_by_fkey FOREIGN KEY (created_by) REFERENCES app.user_account (id) NOT VALID;
ALTER TABLE app.refund
  ADD CONSTRAINT refund_created_by_fkey FOREIGN KEY (created_by) REFERENCES app.user_account (id) NOT VALID;
ALTER TABLE app.customer_package
  ADD CONSTRAINT customer_package_created_by_fkey FOREIGN KEY (created_by) REFERENCES app.user_account (id) NOT VALID,
  ADD CONSTRAINT customer_package_cancelled_by_fkey FOREIGN KEY (cancelled_by) REFERENCES app.user_account (id) NOT VALID;
ALTER TABLE app.appointment
  ADD CONSTRAINT appointment_created_by_fkey FOREIGN KEY (created_by) REFERENCES app.user_account (id) NOT VALID,
  ADD CONSTRAINT appointment_closed_by_fkey FOREIGN KEY (closed_by) REFERENCES app.user_account (id) NOT VALID;
