-- 0021: round 6 — upsell in a visit, convert a one-off visit to a package (spec R1-1 §10, EM
-- answers 2026-11-06, memory/raw/2026-11-06-em-answers-round6-upsell.md).
-- Depends on 0005 (sale, customer_package), 0007 (visit, visit_line), 0013 (revenue_book),
-- 0016/0019 (appointment), 0020 (visit corrections).
--
-- - Every sale made during a visit (its one-off sale, a package sold in the cart, an addition)
--   points at the visit: `sale.visit_id`. `visit.sale_id` stays the visit's first one-off sale.
-- - Each one-off visit line knows the sale that charged it (`visit_line.sale_id`) and who
--   referred it, if anyone (`visit_line.referrer_staff_id`, the upsell credit; the technician
--   `staff_id` keeps the "làm" credit).
-- - A package sale may credit nobody ("khách tự chọn"): `closer_staff_id` becomes optional.
-- - A package may belong to someone other than the payer (`customer_package.customer_id` =
--   user, `sale.customer_id` = payer). No column: only the old assumption goes.
-- - Companions known only by a note ("chồng của chị Lan"), on bookings and visits.

-- ------------------------------------------------------------------------------ sale.visit_id
ALTER TABLE app.sale ADD COLUMN visit_id uuid;
CREATE INDEX sale_visit_idx ON app.sale (tenant_id, visit_id) WHERE visit_id IS NOT NULL;

-- Backfill: the one-off sale of every existing visit. Voided sales are final (0009), so lift that
-- guard for this one statement only, as 0011 did for cancelled packages.
ALTER TABLE app.sale DISABLE TRIGGER sale_voided_is_final;
UPDATE app.sale s SET visit_id = v.id
FROM app.visit v
WHERE v.tenant_id = s.tenant_id AND v.sale_id = s.id;
ALTER TABLE app.sale ENABLE TRIGGER sale_voided_is_final;

-- The foreign key is added only after the backfill: with a DEFERRED constraint already in place,
-- the UPDATE above queues deferred checks, and ALTER TABLE app.sale then fails with "pending
-- trigger events" on any database that has visits (CI's empty database never showed it).
-- Deferred: a visit and the sales it makes are inserted in one transaction, and the visit row
-- itself points at its first sale (`visit.sale_id`).
ALTER TABLE app.sale
  ADD CONSTRAINT sale_visit_fk FOREIGN KEY (tenant_id, visit_id)
    REFERENCES app.visit (tenant_id, id) DEFERRABLE INITIALLY DEFERRED;

-- The link is set once, at insert, to a visit of the same customer (the payer), and never moves.
CREATE FUNCTION app.check_sale_visit_link() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF NEW.visit_id IS DISTINCT FROM OLD.visit_id THEN
      RAISE EXCEPTION 'sale % visit link is immutable', OLD.id USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
  END IF;
  IF NEW.visit_id IS NOT NULL AND EXISTS (
    SELECT 1 FROM app.visit v
    WHERE v.tenant_id = NEW.tenant_id AND v.id = NEW.visit_id AND v.customer_id <> NEW.customer_id
  ) THEN
    RAISE EXCEPTION 'sale % must belong to the customer of visit %', NEW.id, NEW.visit_id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER sale_visit_link BEFORE INSERT OR UPDATE OF visit_id ON app.sale
  FOR EACH ROW EXECUTE FUNCTION app.check_sale_visit_link();

-- ------------------------------------------------------- visit_line: sale + upsell referrer
ALTER TABLE app.visit_line
  ADD COLUMN sale_id uuid,
  ADD COLUMN referrer_staff_id uuid;
ALTER TABLE app.visit_line
  ADD CONSTRAINT visit_line_sale_fk FOREIGN KEY (tenant_id, sale_id) REFERENCES app.sale (tenant_id, id),
  ADD CONSTRAINT visit_line_referrer_fk FOREIGN KEY (tenant_id, referrer_staff_id)
    REFERENCES app.staff (tenant_id, id),
  -- Only a paid (one-off) line is charged by a sale or sold by a referrer; a package line was
  -- sold with its package (that sale's closer has the credit).
  ADD CONSTRAINT visit_line_one_off_links
    CHECK (source = 'one_off' OR (sale_id IS NULL AND referrer_staff_id IS NULL));
UPDATE app.visit_line l SET sale_id = v.sale_id
FROM app.visit v
WHERE v.tenant_id = l.tenant_id AND v.id = l.visit_id AND l.source = 'one_off';
CREATE INDEX visit_line_sale_idx ON app.visit_line (tenant_id, sale_id) WHERE sale_id IS NOT NULL;
-- Payroll: upsell credits per person and month.
CREATE INDEX visit_line_referrer_idx ON app.visit_line (tenant_id, referrer_staff_id)
  WHERE referrer_staff_id IS NOT NULL;

-- --------------------------------------------- "khách tự chọn": a package sale credits nobody
DO $$
DECLARE c text;
BEGIN
  SELECT conname INTO c FROM pg_constraint
  WHERE conrelid = 'app.customer_package'::regclass AND contype = 'c'
    AND pg_get_constraintdef(oid) LIKE '%sale_id IS NOT NULL%closer_staff_id IS NOT NULL%';
  EXECUTE format('ALTER TABLE app.customer_package DROP CONSTRAINT %I', c);
END
$$;
ALTER TABLE app.customer_package
  ADD CONSTRAINT customer_package_sale_origin CHECK (origin <> 'sale' OR sale_id IS NOT NULL);

-- ----------------------------------------------------------------- companions (note only)
-- A person who comes along but is not served ("chồng của chị Lan", "gia đình của …"): a note,
-- never a customer, never charged. 0–5 notes of 1–100 characters.
CREATE FUNCTION app.valid_companions(notes text[]) RETURNS boolean
LANGUAGE sql IMMUTABLE AS $$
  SELECT coalesce(cardinality(notes), 0) <= 5
     AND NOT EXISTS (SELECT 1 FROM unnest(notes) n WHERE n IS NULL OR length(btrim(n)) NOT BETWEEN 1 AND 100)
$$;
ALTER TABLE app.visit
  ADD COLUMN companions text[] NOT NULL DEFAULT '{}' CHECK (app.valid_companions(companions));
ALTER TABLE app.appointment
  ADD COLUMN companions text[] NOT NULL DEFAULT '{}' CHECK (app.valid_companions(companions));
GRANT EXECUTE ON FUNCTION app.valid_companions(text[]) TO app_rw;

-- ------------------------------------------------------------ revenue book: upsell + visit
-- Same rows as 0013, plus who sold it: the closer of a package sale, the referrers of the
-- one-off lines of a service sale ("khách tự chọn" = NULL), and the visit the sale belongs to.
CREATE OR REPLACE VIEW app.revenue_book WITH (security_invoker = true) AS
SELECT s.tenant_id, s.id, s.id AS seq, 'sale'::text AS entry, s.business_date,
       s.created_at AS at, s.customer_id, s.id AS sale_id, NULL::uuid AS customer_package_id,
       d.sale_kind, d.one_off, d.package,
       pay.bank_transfer, pay.cash, (s.total - pay.received)::bigint AS unpaid,
       s.discount_reason AS note, s.status = 'voided' AS voided,
       s.visit_id, u.upsell_by
FROM app.sale s
CROSS JOIN LATERAL (
  SELECT coalesce(sum(p.amount) FILTER (WHERE p.method = 'bank_transfer'), 0)::bigint AS bank_transfer,
         coalesce(sum(p.amount) FILTER (WHERE p.method = 'cash'), 0)::bigint AS cash,
         coalesce(sum(p.amount), 0)::bigint AS received
  FROM app.payment p
  WHERE p.tenant_id = s.tenant_id AND p.sale_id = s.id AND p.kind = 'payment'
) pay
CROSS JOIN LATERAL (
  SELECT CASE WHEN bool_or(sl.line_type = 'package') THEN 'package' ELSE 'one_off' END AS sale_kind,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'service') AS one_off,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'package') AS package
  FROM app.sale_line sl
  WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
) d
CROSS JOIN LATERAL (
  SELECT coalesce(
           (SELECT st.full_name FROM app.staff st
            WHERE st.tenant_id = s.tenant_id AND st.id = s.closer_staff_id),
           (SELECT string_agg(DISTINCT st.full_name, ', ')
            FROM app.visit_line l
            JOIN app.staff st ON st.tenant_id = l.tenant_id AND st.id = l.referrer_staff_id
            WHERE l.tenant_id = s.tenant_id AND l.sale_id = s.id)) AS upsell_by
) u
UNION ALL
SELECT v.tenant_id, v.sale_id, v.seq, 'void', v.business_date, v.at, s.customer_id, s.id, NULL,
       d.sale_kind, d.one_off, d.package,
       -v.bank_transfer, -v.cash, 0::bigint, s.void_reason, true,
       s.visit_id, NULL::text
FROM (
  SELECT p.tenant_id, p.sale_id, p.business_date, max(p.paid_at) AS at,
         (array_agg(p.id ORDER BY p.id DESC))[1] AS seq,
         coalesce(sum(p.amount) FILTER (WHERE p.method = 'bank_transfer'), 0)::bigint AS bank_transfer,
         coalesce(sum(p.amount) FILTER (WHERE p.method = 'cash'), 0)::bigint AS cash
  FROM app.payment p
  WHERE p.kind = 'reversal'
  GROUP BY p.tenant_id, p.business_date, p.sale_id
) v
JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.id = v.sale_id
CROSS JOIN LATERAL (
  SELECT CASE WHEN bool_or(sl.line_type = 'package') THEN 'package' ELSE 'one_off' END AS sale_kind,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'service') AS one_off,
         string_agg(sl.description_snapshot, ', ' ORDER BY sl.position)
           FILTER (WHERE sl.line_type = 'package') AS package
  FROM app.sale_line sl
  WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
) d
UNION ALL
SELECT r.tenant_id, r.id, r.id, 'refund', r.business_date, r.created_at, r.customer_id, NULL,
       r.customer_package_id, 'package', NULL, cp.name_snapshot,
       CASE WHEN r.method = 'bank_transfer' THEN -r.amount ELSE 0 END,
       CASE WHEN r.method = 'cash' THEN -r.amount ELSE 0 END,
       0::bigint, r.reason, false,
       NULL::uuid, NULL::text
FROM app.refund r
JOIN app.customer_package cp ON cp.tenant_id = r.tenant_id AND cp.id = r.customer_package_id;
