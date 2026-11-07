-- Customer identity (EM 2026-11-07, ADR-0010; spec front-desk-and-packages §10.11).
--
-- 1. Decision E: the phone is optional on every profile. A number, when given, keeps its
--    format CHECK and stays unique among live profiles of a spa: `customer_phone_uq` is a plain
--    unique index, and NULLs are distinct in Postgres, so any number of phone-less profiles fit.
-- 2. Anonymous walk-in: a walk-in who gives neither name nor phone gets a profile without a name.
--    The visit, payroll, history and report paths stay the same (they all key on customer_id);
--    read models show such a profile as "Khách vãng lai" through `app.customer_label`.
--    `search_text` stays NOT NULL ('' for a nameless profile: no name search can match it).
-- 3. Optional date of birth (birthday promotions later). `birth_year` stays (paper-book imports
--    and old profiles only know the year) and must agree with the date when both are set.
-- 4. Package extensions get their own append-only book, so the customer timeline can show
--    "Gia hạn" (before, only the audit log knew).

ALTER TABLE app.customer ALTER COLUMN full_name DROP NOT NULL;
ALTER TABLE app.customer ALTER COLUMN phone_e164 DROP NOT NULL;
ALTER TABLE app.customer
  ADD COLUMN birth_date date,
  -- "Not in the future" moves with the clock, so the service checks it; the floor is fixed.
  ADD CONSTRAINT customer_birth_date_plausible CHECK (birth_date >= DATE '1900-01-01'),
  ADD CONSTRAINT customer_birth_year_matches_date CHECK (
    birth_date IS NULL OR birth_year IS NULL
    OR birth_year = extract(year FROM birth_date)::smallint);

-- The one place the walk-in label is spelled for SQL read models (crm::WALK_IN_LABEL is the
-- Rust twin; a test checks they agree).
CREATE FUNCTION app.customer_label(full_name text) RETURNS text
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT coalesce(full_name, 'Khách vãng lai')
$$;
GRANT EXECUTE ON FUNCTION app.customer_label(text) TO app_rw;

CREATE TABLE app.customer_package_extension (
  id                   uuid PRIMARY KEY,
  tenant_id            uuid NOT NULL REFERENCES app.tenant (id),
  customer_package_id  uuid NOT NULL,
  previous_expires_on  date NOT NULL,
  expires_on           date NOT NULL CHECK (expires_on > previous_expires_on),
  reason               text NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 500),
  -- Business day it was recorded on (spa time zone): where the timeline shows it.
  business_date        date NOT NULL,
  -- The signed-in user (from the session, never from the body).
  created_by           uuid NOT NULL REFERENCES app.user_account (id),
  created_at           timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_package_id) REFERENCES app.customer_package (tenant_id, id)
);
CREATE INDEX customer_package_extension_package_idx
  ON app.customer_package_extension (tenant_id, customer_package_id, created_at);

CREATE TRIGGER customer_package_extension_append_only
  BEFORE UPDATE OR DELETE ON app.customer_package_extension
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();

GRANT SELECT, INSERT ON app.customer_package_extension TO app_rw;     -- append-only
ALTER TABLE app.customer_package_extension ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.customer_package_extension
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());

-- Extensions made before this migration live only in the audit log; copy them so old packages
-- show their "Gia hạn" too.
INSERT INTO app.customer_package_extension
  (id, tenant_id, customer_package_id, previous_expires_on, expires_on, reason, business_date,
   created_by, created_at)
SELECT a.id, a.tenant_id, a.entity_id, (a.before ->> 'expires_on')::date,
       (a.after ->> 'expires_on')::date, coalesce(nullif(btrim(a.reason), ''), '—'),
       (a.at AT TIME ZONE t.timezone)::date, a.actor_user_id, a.at
FROM app.audit_log a
JOIN app.tenant t ON t.id = a.tenant_id
JOIN app.customer_package cp ON cp.tenant_id = a.tenant_id AND cp.id = a.entity_id
WHERE a.action = 'customer_package.extend' AND a.actor_user_id IS NOT NULL
  AND (a.after ->> 'expires_on')::date > (a.before ->> 'expires_on')::date;
