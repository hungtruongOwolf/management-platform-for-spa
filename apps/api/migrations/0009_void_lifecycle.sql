-- 0009: voids and the package lifecycle (spec R1-1 US6, US7). 0008 was reserved for S9 and is unused.
--
-- Money history stays append-only: a void never edits a payment, it adds a `reversal` row that
-- points at the payment it cancels (`payment.reversal_of`, UNIQUE since 0005: one reversal per
-- payment). A refund of a cancelled package is its own append-only row. Statuses only move forward:
-- a voided visit or sale and a cancelled package can never be changed again, even by the schema owner.

-- ---------------------------------------------------------------- payment reversals
-- A reversal must mirror the payment it cancels: same sale, same method, same amount, and the
-- original must be a real payment (a reversal cannot itself be reversed). The UNIQUE (tenant_id,
-- reversal_of) from 0005 already makes a second reversal of the same payment impossible.
CREATE FUNCTION app.check_payment_reversal() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  original app.payment%ROWTYPE;
BEGIN
  IF NEW.kind <> 'reversal' THEN
    RETURN NEW;
  END IF;
  SELECT * INTO original FROM app.payment
   WHERE tenant_id = NEW.tenant_id AND id = NEW.reversal_of;
  IF NOT FOUND OR original.kind <> 'payment' THEN
    RAISE EXCEPTION 'reversal_of must reference a payment, not a reversal'
      USING ERRCODE = 'check_violation';
  END IF;
  IF original.sale_id <> NEW.sale_id OR original.method <> NEW.method
     OR original.amount <> NEW.amount THEN
    RAISE EXCEPTION 'a reversal must mirror its payment (sale, method, amount)'
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER payment_reversal_mirrors_original BEFORE INSERT ON app.payment
  FOR EACH ROW EXECUTE FUNCTION app.check_payment_reversal();

-- ---------------------------------------------------------------- voided / cancelled are final
CREATE FUNCTION app.reject_change_after_void() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status = 'voided' THEN
    RAISE EXCEPTION '% % is voided and can no longer change', TG_TABLE_NAME, OLD.id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER visit_voided_is_final BEFORE UPDATE ON app.visit
  FOR EACH ROW EXECUTE FUNCTION app.reject_change_after_void();
CREATE TRIGGER sale_voided_is_final BEFORE UPDATE ON app.sale
  FOR EACH ROW EXECUTE FUNCTION app.reject_change_after_void();

CREATE FUNCTION app.reject_change_after_cancel() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.cancelled_at IS NOT NULL THEN
    RAISE EXCEPTION 'customer_package % is cancelled and can no longer change', OLD.id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER customer_package_cancelled_is_final BEFORE UPDATE ON app.customer_package
  FOR EACH ROW EXECUTE FUNCTION app.reject_change_after_cancel();

-- Sessions never come back to a cancelled package (e.g. voiding an old visit after a refund).
CREATE FUNCTION app.reject_restore_on_cancelled() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.sessions_remaining > OLD.sessions_remaining AND EXISTS (
       SELECT 1 FROM app.customer_package p
        WHERE p.tenant_id = NEW.tenant_id AND p.id = NEW.customer_package_id
          AND p.cancelled_at IS NOT NULL) THEN
    RAISE EXCEPTION 'customer_package % is cancelled: sessions cannot be restored', NEW.customer_package_id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER customer_package_item_no_restore_when_cancelled BEFORE UPDATE ON app.customer_package_item
  FOR EACH ROW EXECUTE FUNCTION app.reject_restore_on_cancelled();

-- Who cancelled a package (the reason is in cancel_reason since 0005).
ALTER TABLE app.customer_package ADD COLUMN cancelled_by uuid;
ALTER TABLE app.customer_package
  ADD CONSTRAINT customer_package_cancel_complete
  CHECK (cancelled_at IS NULL OR (cancelled_by IS NOT NULL AND length(btrim(cancel_reason)) BETWEEN 1 AND 500));
ALTER TABLE app.sale
  ADD CONSTRAINT sale_void_complete
  CHECK (status <> 'voided' OR (voided_by IS NOT NULL AND amount_paid = 0
                                AND length(btrim(void_reason)) BETWEEN 1 AND 500));

-- ---------------------------------------------------------------- refunds (AC7.2)
-- Money paid back to the customer when a package is cancelled: a cash-book outflow of the day it
-- happens. At most one refund per package (a package is cancelled once), never more than was paid.
CREATE TABLE app.refund (
  id                   uuid PRIMARY KEY,
  tenant_id            uuid NOT NULL REFERENCES app.tenant (id),
  branch_id            uuid NOT NULL,
  customer_id          uuid NOT NULL,
  customer_package_id  uuid NOT NULL,
  business_date        date NOT NULL,
  method               text NOT NULL CHECK (method IN ('cash', 'bank_transfer')),
  amount               bigint NOT NULL CHECK (amount > 0),
  reason               text NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 500),
  created_by           uuid NOT NULL,
  created_at           timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, customer_package_id),
  FOREIGN KEY (tenant_id, branch_id) REFERENCES app.branch (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_id) REFERENCES app.customer (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_package_id) REFERENCES app.customer_package (tenant_id, id)
);
CREATE INDEX refund_day_idx ON app.refund (tenant_id, branch_id, business_date);

-- Backstop for the service check: a refund never exceeds what the customer paid for the package.
CREATE FUNCTION app.check_refund_within_paid() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.amount > (SELECT sold_price FROM app.customer_package
                    WHERE tenant_id = NEW.tenant_id AND id = NEW.customer_package_id) THEN
    RAISE EXCEPTION 'refund exceeds the amount paid for package %', NEW.customer_package_id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER refund_within_paid BEFORE INSERT ON app.refund
  FOR EACH ROW EXECUTE FUNCTION app.check_refund_within_paid();
CREATE TRIGGER refund_append_only BEFORE UPDATE OR DELETE ON app.refund
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();

GRANT SELECT, INSERT ON app.refund TO app_rw;                          -- append-only
ALTER TABLE app.refund ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.refund
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
