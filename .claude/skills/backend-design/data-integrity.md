# Data integrity: conflicts, consistency, corruption

PostgreSQL is the source of truth. Invariants live **in the database** (constraints) *and* in the service layer, so that an app bug, a race, or a manual SQL fix cannot leave data in an invalid state. Each item below must be satisfied, marked not applicable with a reason, or recorded as a finding.

## 1. Classify every write path
For each entity and each write in the spec, answer three questions: who can write it concurrently (one user on two devices, many users, staff + user, background jobs)? What invariant must hold (no double-booking, stock ≥ 0, balance = sum of ledger)? What does a conflict look like to the user? Record the answers in `architecture/data.md`.

## 2. Concurrent writes
- **Lost update (two editors of the same record):** use optimistic locking. Add a `version BIGINT` column and run `UPDATE … SET …, version = version + 1 WHERE id = $1 AND version = $2`. Zero rows affected → return `409 Conflict`. The API exposes it as `ETag` / `If-Match`. The frontend shows a conflict UI (reload, compare, re-apply) and never silently overwrites.
- **Counters and quantities:** do the update atomically in SQL (`SET qty = qty - 1 WHERE id = $1 AND qty > 0`). Read-modify-write in application code is not allowed.
- **Short critical sections** (allocating a slot, claiming an item): `SELECT … FOR UPDATE` inside the transaction, always locking rows in the same order to avoid deadlocks. Job queues use `FOR UPDATE SKIP LOCKED`.
- **Invariants as constraints:** `UNIQUE`, `CHECK`, `FOREIGN KEY`, and `EXCLUDE USING gist` for overlapping time ranges (e.g. no two bookings for the same practitioner overlap; needs `btree_gist`). A race that slips past app code still fails at the DB, and the resulting error maps to a `409`.
- **Isolation:** `READ COMMITTED` by default. Use `SERIALIZABLE` for multi-row invariants that no constraint can express, with a bounded retry on SQLSTATE `40001`/`40P01`.
- **Transaction scope:** one use case = one transaction, opened in the service layer. Network calls (payment API, email, HTTP) never happen inside an open transaction.

## 3. Duplicates and retries
- **Double-submit / client retry:** mutations with side effects take an `Idempotency-Key`. Store `(actor_id, key) UNIQUE` together with a request hash and the stored response. A replay returns the same response; a same key with a different body returns `422`.
- **Webhooks** (payment, video provider): verify the signature, dedupe on the provider's event ID (`UNIQUE`), and keep processing idempotent, since providers deliver at least once and out of order.
- **Background jobs:** every handler is idempotent, because jobs are retried. The retry policy uses exponential backoff and a dead-letter queue that alerts.

## 4. Consistency across systems
- **Transactional outbox:** a domain event (send email, grant course access, start transcode) is written as an `outbox` row **in the same transaction** as the state change; a worker publishes it later. A crash can never commit the state while losing the event, or send the event while losing the state.
- **State machines** for money and access (order: `pending → paid → refunded`, etc.): allowed transitions are enforced in the service layer and guarded in SQL (`WHERE status = 'pending'`), and every transition is logged.
- **Reconciliation job:** compare our payments and entitlements against the provider daily; mismatches alert the EM.
- **Multi-step flows:** use a saga with explicit compensation steps (refund, revoke access) rather than distributed transactions.

## 5. Caches
Use cache-aside with a TTL, and invalidate **after commit**. Authorization and entitlement decisions are cached for seconds at most, and are invalidated on revoke, refund, or role change. The cache is never the only copy of anything.

## 6. Offline and multi-device
If a spec allows offline or concurrent editing, choose the policy per entity in the spec:
- **version conflict → 409**: the default.
- **last-write-wins with server timestamps**: only for low-value fields such as UI preferences.
- **CRDT / merge**: only when collaborative editing is a real feature.

## 7. Preventing and detecting corruption
- **Types:** money is stored as integer minor units (`BIGINT` đồng) or `NUMERIC`, never float. Times are `timestamptz` stored in UTC and displayed in Asia/Ho_Chi_Minh. IDs are UUIDv7.
- **Vietnamese text:** normalize to Unicode **NFC** at the boundary. The same diacritics in composed and decomposed form otherwise create invisible duplicates and break search and unique constraints. Use `unaccent` or a dedicated search index for diacritic-insensitive search.
- **Two validation layers:** typed parsing at the boundary plus DB constraints. Nullable columns need a reason.
- **Migrations:** expand → backfill in batches → verify row counts and invariants → contract. Take a verified backup before anything destructive. Test every migration against a production-like snapshot.
- **History:** audit or history tables (or temporal versioning) for sensitive and business-critical entities, so a bad write can be traced and reverted without restoring the whole database.
- **Storage-level:** PostgreSQL data checksums enabled (confirm on the managed provider); object storage keeps a SHA-256 of each upload in the DB and has bucket versioning on.
- **Integrity checks:** a scheduled job runs invariant queries (orphans, ledger sums, impossible states) and alerts on any hit.
- **Backups:** automated backups + PITR, encrypted, one copy off-site, restore drill monthly with a measured RPO/RTO.

## 8. Tests that prove it
- Concurrency test: fire N parallel requests at the same resource and assert the invariant (exactly one booking wins, stock never negative).
- Stale-version update returns 409.
- Idempotency replay returns the identical response and has no second side effect.
- Duplicate webhook delivery is a no-op.
- Each migration runs up on seeded data and the invariant queries come back clean.
