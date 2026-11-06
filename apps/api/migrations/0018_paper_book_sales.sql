-- 0018: money paid on paper is booked as a real sale (EM 2026-11-05,
-- memory/raw/2026-11-05-em-carried-over-is-revenue.md). Follows 0016 (appointments) and
-- 0017 (staff management).
-- Depends on 0005 (sale, sale_line, customer_package).
--
-- A paper-book package (origin = 'opening_balance') may be tied, once, to a sale that books what
-- the customer paid on paper, on the paper date. That sale is an ordinary sale for the money
-- rules, the books and the reports; `source` only marks it for audit and re-run idempotency
-- (the API never shows it).

ALTER TABLE app.sale
  ADD COLUMN source text NOT NULL DEFAULT 'app' CHECK (source IN ('app', 'paper_book'));

-- A paper-book package used to have no sale at all. The rule becomes the trigger below.
DO $$
DECLARE c text;
BEGIN
  SELECT conname INTO c FROM pg_constraint
  WHERE conrelid = 'app.customer_package'::regclass AND contype = 'c'
    AND pg_get_constraintdef(oid) LIKE '%opening_balance%sale_id IS NULL%';
  EXECUTE format('ALTER TABLE app.customer_package DROP CONSTRAINT %I', c);
  -- A paper package may have no plan (its name comes from the paper book), so the package line
  -- of its paper sale may have none either.
  SELECT conname INTO c FROM pg_constraint
  WHERE conrelid = 'app.sale_line'::regclass AND contype = 'c'
    AND pg_get_constraintdef(oid) LIKE '%package_plan_id IS NOT NULL%customer_package_id IS NOT NULL%';
  EXECUTE format('ALTER TABLE app.sale_line DROP CONSTRAINT %I', c);
END
$$;
ALTER TABLE app.sale_line ADD CONSTRAINT sale_line_package_shape
  CHECK (line_type <> 'package' OR (customer_package_id IS NOT NULL AND service_id IS NULL));

-- Who may point a package at a sale:
-- - an app sale creates its package with the sale (origin 'sale'), never relinked;
-- - a paper-book package is created without a sale and may be linked once, later, to a
--   `paper_book` sale of the same customer.
CREATE FUNCTION app.check_package_sale_link() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'UPDATE' AND OLD.sale_id IS NOT DISTINCT FROM NEW.sale_id THEN
    RETURN NEW;
  END IF;
  IF NEW.origin = 'opening_balance' AND NEW.sale_id IS NOT NULL THEN
    IF TG_OP = 'INSERT' OR OLD.sale_id IS NOT NULL OR NOT EXISTS (
         SELECT 1 FROM app.sale s
         WHERE s.tenant_id = NEW.tenant_id AND s.id = NEW.sale_id
           AND s.source = 'paper_book' AND s.customer_id = NEW.customer_id) THEN
      RAISE EXCEPTION 'paper-book package % can only be linked once, to a paper_book sale', NEW.id
        USING ERRCODE = 'check_violation';
    END IF;
  ELSIF TG_OP = 'UPDATE' THEN
    RAISE EXCEPTION 'customer_package % cannot change its sale', NEW.id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER customer_package_sale_link BEFORE INSERT OR UPDATE OF sale_id ON app.customer_package
  FOR EACH ROW EXECUTE FUNCTION app.check_package_sale_link();
