-- 0012: EM decision 2026-11-05 (memory/raw/2026-11-05-em-simplify-owner-and-opening-balance.md).
-- Undoes the M2 parts of 0011; 0011 stays as written because it may already be applied.
-- Depends on 0002 (role_permission), 0009 (refund trigger), 0011 (window, refund_cap).

-- No "open the import today" window: an import always lands on today's business day, and editing
-- an earlier day follows the ordinary closed-day rule.
DROP TABLE app.opening_balance_window;
DELETE FROM app.role_permission WHERE permission = 'opening_balance.manage_window';

-- Refund cap for paper-book packages is deferred: a refund is again capped by sold_price, as 0009
-- defined it. The function goes back first, so no refund insert ever sees a missing column.
CREATE OR REPLACE FUNCTION app.check_refund_within_paid() RETURNS trigger
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

-- DROP COLUMN rewrites no rows and fires no UPDATE trigger, so customer_package_cancelled_is_final
-- (0009) does not need lifting here (unlike the 0011 backfill). Its two CHECK constraints go with it.
ALTER TABLE app.customer_package DROP COLUMN refund_cap;
