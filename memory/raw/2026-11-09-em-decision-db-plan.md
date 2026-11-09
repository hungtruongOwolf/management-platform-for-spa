# EM decision — 2026-11-09: apply the database review plan

After the review (memory/wiki/architecture/db-review-2026-11.md, PRs #105/#106), the lead summarized the plan (A1–A4, B1, C1, C2) and the questions Q-DB1–4. Verbatim:

> ok bạn cứ làm hết đi

## Lead's reading
- **Q-DB1, yes:** A1 (no ping on every pool checkout; ping only after idle) and A2 (UNION rewrite of the customer-history sale branch).
- **Q-DB2, yes:** A3, one SECURITY DEFINER statement for session + permissions + former-staff flag. Go through `secure` and write an ADR. A4: carry the tenant profile in `Actor`.
- **Q-DB3, yes:** B1, drop the 8 redundant or unusable indexes, with the rollback DDL kept.
- **Q-DB4:**
  - Expired or revoked sessions are purged after **30 days**.
  - `audit_log` gets **no retention for now** and is kept. It is the record of who changed money, attendance and HR data. Deleting it is a legal choice, and Vietnamese accounting records are normally kept for years, so the conservative default is to keep it. The lead can revisit with the EM.
- **C2:** run the invariant checks daily and surface failures.
- **C3/C4:** do them where they are low-risk:
  - `idle_in_transaction_session_timeout` for app_rw;
  - FKs on "who did it" columns via NOT VALID + VALIDATE, if sound;
  - the membership(user_id) index can wait until there are several spas.
- Everything on the review's "must not change" list stays.
