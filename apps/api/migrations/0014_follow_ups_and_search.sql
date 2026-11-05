-- 0014: re-engagement follow-ups and accent-insensitive search over snapshots
-- (memory/wiki/product/specs/management-views.md US5b, US4 search; raw/2026-11-05-em-ui-review-round3.md).
-- Depends on 0002 (role_permission, user_account), 0004 (customer), 0005 (reject_mutation).

-- New permission for every existing system owner and manager role (new spas get it from
-- identity::roles::SystemRole::default_permissions).
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'customer.follow_up'
FROM app.role r
WHERE r.is_system AND r.key IN ('owner', 'manager')
ON CONFLICT DO NOTHING;

-- ------------------------------------------------------------------------------ follow-ups
-- One row per contact attempt with a customer who has not come back (the paper "gọi lại" list).
-- Append-only like the books: a mistaken entry is corrected by recording another one.
CREATE TABLE app.customer_follow_up (
  id                 uuid PRIMARY KEY,
  tenant_id          uuid NOT NULL REFERENCES app.tenant (id),
  customer_id        uuid NOT NULL,
  -- Business day of the contact (spa time zone); never in the future (checked by the service).
  contacted_on       date NOT NULL,
  channel            text NOT NULL CHECK (channel IN ('phone', 'zalo', 'in_person', 'other')),
  outcome            text NOT NULL
                     CHECK (outcome IN ('will_come', 'not_interested', 'no_answer', 'other')),
  note               text CHECK (note IS NULL OR length(btrim(note)) BETWEEN 1 AND 500),
  next_follow_up_on  date CHECK (next_follow_up_on IS NULL OR next_follow_up_on >= contacted_on),
  -- The signed-in user who recorded it (from the session, never from the body).
  created_by         uuid NOT NULL REFERENCES app.user_account (id),
  created_at         timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  FOREIGN KEY (tenant_id, customer_id) REFERENCES app.customer (tenant_id, id)
);
-- A customer's follow-ups newest first, and "the latest follow-up" per customer.
CREATE INDEX customer_follow_up_customer_idx
  ON app.customer_follow_up (tenant_id, customer_id, contacted_on DESC, created_at DESC);

CREATE TRIGGER customer_follow_up_append_only BEFORE UPDATE OR DELETE ON app.customer_follow_up
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();

GRANT SELECT, INSERT ON app.customer_follow_up TO app_rw;              -- append-only
ALTER TABLE app.customer_follow_up ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.customer_follow_up
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());

-- ------------------------------------------------------------------------- search folding
-- SQL twin of crm::text::search_key for text that only exists as a snapshot (service, package,
-- staff and category names): NFC, Vietnamese diacritics removed, đ → d, lowercase, single spaces.
-- `customer.search_text` keeps being computed by the app; tests check the two agree.
CREATE FUNCTION app.search_fold(input text) RETURNS text
LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT AS $$
  SELECT regexp_replace(btrim(lower(translate(normalize(input, NFC),
    'àÀáÁảẢãÃạẠăĂằẰắẮẳẲẵẴặẶâÂầẦấẤẩẨẫẪậẬèÈéÉẻẺẽẼẹẸêÊềỀếẾểỂễỄệỆìÌíÍỉỈĩĨịỊòÒóÓỏỎõÕọỌôÔồỒốỐổỔỗỖộỘơƠờỜớỚởỞỡỠợỢùÙúÚủỦũŨụỤưƯừỪứỨửỬữỮựỰỳỲýÝỷỶỹỸỵỴđĐ',
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaeeeeeeeeeeeeeeeeeeeeeeiiiiiiiiiioooooooooooooooooooooooooooooooooouuuuuuuuuuuuuuuuuuuuuuyyyyyyyyyydd'))),
    '\s+', ' ', 'g')
$$;
GRANT EXECUTE ON FUNCTION app.search_fold(text) TO app_rw;
