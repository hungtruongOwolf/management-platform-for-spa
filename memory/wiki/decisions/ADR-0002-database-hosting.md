---
title: ADR-0002 Database hosting — Supabase (managed Postgres)
status: accepted (provisional — revisit at /architect and before credits expire)
date: 2026-11-04
deciders: EM, agent
---

# ADR-0002: Database hosting — Supabase

## Context
Current data is customer records only (no video). EM holds a Supabase YC student deal: $300 credits over 12 months, which covers the Pro plan ($25/mo). Google Drive was ruled out. See [research/hosting-storage](../research/hosting-storage.md).

## Options considered
| Option | Pros | Cons |
|---|---|---|
| Supabase Pro | Free for ~12 months via credits; managed Postgres with daily backups; Singapore region | Data API exposed by default; data offshore |
| Neon | Scale-to-zero, cheap staging | No credits; pure DB only |
| Self-hosted Postgres on VPS | Cheapest long-term, can be in Vietnam | We own backups, HA, patching |
| Vietnamese cloud Postgres | Data stays in Vietnam | Higher ops effort / cost, unverified pricing |

## Decision
Use **Supabase Pro** as **plain managed PostgreSQL**, in region **Singapore (ap-southeast-1)**. The Rust/Axum API is the only client: it connects with SQLx through a dedicated least-privilege role. Supabase Auth, Storage, and the auto-generated Data API are **not** part of our architecture.

## Mandatory setup (security)
- Disable the Data API (PostgREST), or expose only an empty schema. App tables live in a dedicated schema (e.g. `app`), never `public`.
- RLS enabled on every table as defence in depth; no policies granting `anon`/`authenticated`.
- `service_role` and DB passwords stay server-side in a secret store; never in the client, never in git.
- App DB role: DML only on `app` schema; migrations run with a separate role.
- Enforce SSL connections; restrict network access where the plan allows.
- MFA on the Supabase account and org; at least two owners recorded offline.
- Independent nightly `pg_dump` → encrypted, versioned Cloudflare R2 bucket; monthly restore drill.

## Consequences
- Positive: zero DB cost for ~12 months; mature backups; standard Postgres keeps lock-in low (`pg_dump` to migrate).
- Negative: data stored outside Vietnam → cross-border transfer obligations (transfer impact assessment within 60 days under Law 91/2025/QH15 + Decree 356/2025). Must be handled before real customer data is loaded.
- Follow-ups: confirm whether health data is stored (`/idea`); legal check on cross-border filing; set a reminder ~2 months before credits expire.

## Revisit when
Credits are about to run out; DB exceeds ~8 GB or needs larger compute; legal advice requires data residency in Vietnam; health records become a core data type.

## Update 2026-11-04 (EM, S11)
- Project **staging** `vpyd-staging` nằm ở **vùng Mỹ** và **Data API vẫn bật**. Đây là ngoại lệ chỉ dành cho staging, giảm thiểu bằng: schema `app` không nằm trong exposed schemas; `REVOKE ALL` + `ALTER DEFAULT PRIVILEGES … REVOKE` cho `anon`/`authenticated`; RLS; chỉ dùng dữ liệu giả. Checklist ở [runbooks/staging-setup](../runbooks/staging-setup.md) §4.
- **Production** sẽ là một project mới ở Singapore, tắt Data API ngay khi tạo, đúng như phần Decision ở trên.
- SQLx kết nối qua Supavisor **session mode** (port 5432, user `app_rw.<ref>`), không dùng transaction mode (vì prepared statements).
