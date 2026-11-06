-- 0020: "Sửa lượt" (spec R1-1 US6b, EM 2026-11-06). A correction that touches money or sessions
-- voids the visit (exactly as US6) and records the corrected one in the same transaction; the new
-- visit points back at the one it replaces. The old row is never updated after its void (0009), so
-- the link lives on the new row only; "corrected by" is the reverse lookup.

ALTER TABLE app.visit ADD COLUMN corrects_visit_id uuid;
ALTER TABLE app.visit
  ADD CONSTRAINT visit_corrects_fk FOREIGN KEY (tenant_id, corrects_visit_id)
    REFERENCES app.visit (tenant_id, id),
  -- A visit is corrected at most once: the newest version is always a single chain.
  ADD CONSTRAINT visit_corrects_once UNIQUE (tenant_id, corrects_visit_id),
  ADD CONSTRAINT visit_corrects_other CHECK (corrects_visit_id <> id);

-- Second wall behind the service: the replaced visit is already voided, on the same business day,
-- and the link never changes afterwards.
CREATE FUNCTION app.check_visit_correction() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF NEW.corrects_visit_id IS DISTINCT FROM OLD.corrects_visit_id THEN
      RAISE EXCEPTION 'visit % correction link is immutable', OLD.id USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
  END IF;
  IF NEW.corrects_visit_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM app.visit o
    WHERE o.tenant_id = NEW.tenant_id AND o.id = NEW.corrects_visit_id
      AND o.status = 'voided' AND o.business_date = NEW.business_date
  ) THEN
    RAISE EXCEPTION 'visit % must correct a voided visit of the same day', NEW.id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER visit_correction_link BEFORE INSERT OR UPDATE ON app.visit
  FOR EACH ROW EXECUTE FUNCTION app.check_visit_correction();

-- A done appointment stays final, except that its visit may move to the visit correcting it (the
-- calendar then shows the corrected visit, not the voided one). Nothing else may change.
CREATE OR REPLACE FUNCTION app.appointment_closed_is_final() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  moved app.appointment;
BEGIN
  IF OLD.status IN ('done', 'no_show', 'cancelled') THEN
    IF OLD.status = 'done' AND NEW.visit_id IS DISTINCT FROM OLD.visit_id AND EXISTS (
      SELECT 1 FROM app.visit v
      WHERE v.tenant_id = NEW.tenant_id AND v.id = NEW.visit_id AND v.corrects_visit_id = OLD.visit_id
    ) THEN
      moved := NEW;
      moved.visit_id := OLD.visit_id;
      moved.version := OLD.version;
      moved.updated_at := OLD.updated_at;
      IF moved IS NOT DISTINCT FROM OLD THEN
        RETURN NEW;
      END IF;
    END IF;
    RAISE EXCEPTION 'appointment % is closed', OLD.id USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END $$;
