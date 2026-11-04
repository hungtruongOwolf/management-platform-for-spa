-- 0004: customers. Health notes are encrypted by the application (AES-256-GCM); the database never sees plaintext.

CREATE TABLE app.customer (
  id              uuid PRIMARY KEY,
  tenant_id       uuid NOT NULL REFERENCES app.tenant (id),
  full_name       text NOT NULL CHECK (length(btrim(full_name)) BETWEEN 1 AND 200),
  -- lowercase, diacritics removed, đ→d (computed by the app; used for accent-insensitive search)
  search_text     text NOT NULL,
  phone_e164      text NOT NULL CHECK (phone_e164 ~ '^\+84[0-9]{9}$'),
  gender          text CHECK (gender IN ('female', 'male')),
  birth_year      smallint CHECK (birth_year BETWEEN 1900 AND 2100),
  address         text CHECK (length(address) <= 500),
  source          text CHECK (length(source) <= 200),
  note            text CHECK (length(note) <= 2000),
  health_note_ct  bytea,                       -- "v<key version>:" || nonce || ciphertext
  anonymized_at   timestamptz,
  version         bigint NOT NULL DEFAULT 1,
  created_at      timestamptz NOT NULL DEFAULT now(),
  updated_at      timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id)
);

-- AC2.4: one live profile per phone per spa (anonymised profiles release their number).
CREATE UNIQUE INDEX customer_phone_uq ON app.customer (tenant_id, phone_e164) WHERE anonymized_at IS NULL;
-- AC2.1: substring search on accent-folded name and on phone digits.
CREATE INDEX customer_search_trgm ON app.customer USING gin (search_text gin_trgm_ops);
CREATE INDEX customer_phone_trgm ON app.customer USING gin (phone_e164 gin_trgm_ops);

GRANT SELECT, INSERT, UPDATE ON app.customer TO app_rw;   -- no DELETE: anonymise instead

ALTER TABLE app.customer ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.customer
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
