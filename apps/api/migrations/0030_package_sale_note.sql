-- 0030: a note per package sale (F4, EM 2026-11-08: "thường khách chỉ mua 1 gói 1 lần thì ghi chú
-- theo gói"). Each package sale (`POST /sales`, each package of `POST /sales/checkout`, each package
-- sold in a visit) may carry one optional note written at the desk. Free text there often holds
-- health information ("đau vai gáy"), so it is stored like a visit's session note: AES-256-GCM
-- ciphertext only (AAD = "sale.note" + tenant + sale), never plaintext, written with
-- `customer.health_notes.write` and read back with `customer.health_notes.read`.
--
-- Only the application decides which sales may carry one (package sales); a voided sale stays
-- frozen by `sale_voided_is_final` (0009), its note included. The partial index keeps the new
-- "Ghi chú gói" branch of `GET /notes` a short range scan over the sales that carry a note.

ALTER TABLE app.sale ADD COLUMN note_ct bytea;

CREATE INDEX sale_note_day_idx ON app.sale (tenant_id, business_date)
  WHERE note_ct IS NOT NULL;
