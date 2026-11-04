---
name: backend-design
description: Backend design — use before writing backend code for a feature: API endpoints/contract, database schema, migrations, authorization matrix, data integrity and concurrency.
---

# Backend design

Contract first: the API and schema are designed and agreed before handlers are written, so frontend and backend can build in parallel against one contract.

## Steps
1. Read the spec, `architecture/system.md`, `security/threat-model.md`, `conventions/coding-standards.md`.
2. **Resources & endpoints** — nouns from `glossary.md`. For each endpoint: method, path, request/response schema, status codes, required permission, rate-limit class, idempotency.
   - REST, JSON, `snake_case` fields, versioned under `/api/v1`.
   - Errors: RFC 9457 `application/problem+json` with a stable `code`.
   - Lists: cursor pagination, server-enforced max page size, allowlisted sort/filter keys.
   - Mutations that may be retried (payments, bookings) take an `Idempotency-Key`.
   - Write it as OpenAPI 3.1 (`api/openapi.yaml` once the repo exists; generated from code with `utoipa` later is fine as long as it stays the contract).
3. **Authorization matrix** — role × action × resource in `wiki/security/authz-matrix.md`, ownership rules spelled out. Every endpoint maps to a cell.
4. **Data model** — PostgreSQL. Tables, columns with types, PK (UUIDv7), FKs, `NOT NULL`/`CHECK`/`UNIQUE` constraints that encode business rules, indexes justified by queries, `created_at`/`updated_at`, soft-delete only where a spec needs it, audit table for sensitive changes, column encryption for sensitive personal/health fields. ER diagram in Mermaid → `architecture/data.md`.
5. **Migrations** — forward-only `sqlx migrate`, each one safe on a live database (expand → migrate → contract).
6. **Module layout** — Cargo workspace; per bounded context: `domain` (types, invariants), `service` (use cases + `authorize`), `repo` (SQL), `http` (Axum routes, extractors, DTOs). Cross-cutting: config, error mapping, auth extractor, tracing, rate limiting.
7. **Async work** — what runs in background jobs (emails, transcoding, webhooks), retry and dead-letter policy.
8. **Data integrity** — apply [data-integrity.md](data-integrity.md) to every entity and write path: concurrency control, idempotency, outbox, state machines, corruption prevention, recovery.
9. Run `secure` over the design.
10. Done when every acceptance criterion maps to an endpoint, every endpoint maps to an authz-matrix cell, every query an endpoint needs is served by an index or justified as small, and every write path has a named conflict strategy and an invariant enforced in the database.
11. Write `architecture/backend.md` and `architecture/data.md`; index + log. Next: `/breakdown`.
