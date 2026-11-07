-- A bonus / penalty recorded by mistake can be removed (EM live test 2026-11-07, spec
-- staff-management AC4.2 as amended). Removing never deletes the row: it is voided (who, when),
-- leaves the log and payroll, and stays for the audit trail. Editing = void + a new entry, in one
-- transaction. A row is voided at most once and nothing else on it ever changes; a closed month's
-- entries can no longer be voided (its payslips are frozen).

ALTER TABLE app.staff_adjustment
  ADD COLUMN voided_at timestamptz,
  ADD COLUMN voided_by uuid REFERENCES app.user_account (id),
  ADD CONSTRAINT staff_adjustment_void_complete CHECK ((voided_at IS NULL) = (voided_by IS NULL));

CREATE FUNCTION app.staff_adjustment_void_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE'
     OR OLD.voided_at IS NOT NULL
     OR NEW.voided_at IS NULL
     OR (NEW.id, NEW.tenant_id, NEW.staff_id, NEW.work_date, NEW.kind, NEW.amount, NEW.points,
         NEW.reason, NEW.corrects_period, NEW.created_by, NEW.created_at)
        IS DISTINCT FROM
        (OLD.id, OLD.tenant_id, OLD.staff_id, OLD.work_date, OLD.kind, OLD.amount, OLD.points,
         OLD.reason, OLD.corrects_period, OLD.created_by, OLD.created_at) THEN
    RAISE EXCEPTION 'staff_adjustment can only be voided, once' USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN NEW;
END
$$;

DROP TRIGGER staff_adjustment_append_only ON app.staff_adjustment;
CREATE TRIGGER staff_adjustment_void_only BEFORE UPDATE OR DELETE ON app.staff_adjustment
  FOR EACH ROW EXECUTE FUNCTION app.staff_adjustment_void_only();
CREATE TRIGGER staff_adjustment_void_closed_period BEFORE UPDATE ON app.staff_adjustment
  FOR EACH ROW EXECUTE FUNCTION app.reject_closed_period();

GRANT UPDATE (voided_at, voided_by) ON app.staff_adjustment TO app_rw;
