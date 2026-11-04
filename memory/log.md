# Log

Append-only. Newest at the bottom. Format: `## [YYYY-MM-DD] <op> | <title>` then 1–5 bullet lines.
Ops: `ingest`, `query`, `lint`, `record`, `decision`, `skip`.

## [2026-11-04] record | Project bootstrap
- Created memory/ wiki (raw/wiki/index/log), CLAUDE.md, and team skills in `.claude/skills/`.
- Ingested EM kickoff request → `raw/2026-11-04-kickoff-request.md`.
- Wrote tech-stack research and ADR-0001 (status: proposed, pending EM discussion).
- Next: `/idea` discussion about the product itself.

## [2026-11-04] record | Data integrity added to backend design
- EM asked whether backend design covered data conflicts/corruption: it was only partial (constraints, idempotency key, backups, migration safety).
- Added `.claude/skills/backend-design/data-integrity.md`: optimistic/pessimistic locking, DB-enforced invariants, idempotency, webhook dedupe, transactional outbox, state machines, cache consistency, NFC normalization, migration/backup/integrity checks, concurrency tests.
- `backend-design` now requires a conflict strategy + DB invariant per write path.

## [2026-11-04] ingest | Hosting & storage research
- EM has Supabase YC student deal ($300 credits over 12 months, redeemed 2026-11-04); Google Drive ruled out as backend storage.
- Wrote `wiki/research/hosting-storage.md`: Supabase Pro (DB) + Cloudflare R2 (files/backup) + Bunny Stream (video) + small VPS; Supabase Data API must be disabled/locked.
- Corrected VN data law refs: Law 91/2025/QH15 + Decree 356/2025 replace Decree 13/2023; foreign cloud = cross-border transfer (TIA within 60 days).
- EM preference: search in English/Chinese, reason in English.

## [2026-11-04] decision | Storage scope + automatic skill routing
- EM: no video for now, only customer data → current storage need = Supabase Postgres + R2 backups; video deferred.
- All 15 skills are now model-invoked with trigger-style descriptions; CLAUDE.md has an Auto-routing section.
- Added `.claude/settings.json` hooks: SessionStart loads memory index + recent log; UserPromptSubmit injects a routing reminder.
- Research/reasoning language rule (English + Chinese queries, Vietnamese replies) made permanent in CLAUDE.md.

## [2026-11-04] decision | Database hosting = Supabase (provisional)
- EM chose Supabase for now. ADR-0002 written: plain managed Postgres, Singapore, Rust API only client, Data API disabled, RLS on, R2 nightly backups.
- Blocker before loading real customer data: cross-border transfer impact assessment (legal check).
- Next: EM redeems credits + creates org/project; then `/idea` product discussion.

## [2026-11-04] ingest | Idea: internal spa platform + reference docs
- Ingested EM idea + 4 paper books, Excel timesheet (formulas), 2 compensation docs → raw/2026-11-04-idea-internal-platform.md.
- Wrote product/paper-books-analysis, product/compensation-rules (8 open contradictions), product/glossary (draft), architecture/principles.
- Added impact-analysis.md to build skill; build/pr-review/CLAUDE.md now require blast-radius check. Wrote research/code-intelligence.
- Next: continue `/idea` interview (first question: single spa vs multi-spa SaaS).

## [2026-11-04] decision | Multi-tenant (SaaS-ready) data model
- EM chose B: platform may later serve other spas. Data model is multi-tenant from day 1 (tenant_id + branch_id on business tables); launch with a single tenant (Viên Phúc Y Đường).
- Security consequence: tenant isolation enforced in the repository layer (every query tenant-scoped) + Postgres RLS by tenant as defence in depth; cross-tenant tests in the authz matrix. To be formalised as an ADR at `/architect`.

## [2026-11-04] record | Operating scale confirmed
- 1 branch now (more later), 3–8 technicians, 1 manager who is also cashier and keeps all books, 15–40 visits/day.
- Primary MVP user = manager-cashier → fast front-desk entry flow. Wrote product/personas.md.

## [2026-11-04] decision | Customer health info = free-text notes; consent handled offline
- EM: health info (contraindications, conditions, post-session reactions) stored as notes on customer/visit. Consent is obtained offline; platform has no consent module. Legal obligations (sensitive data, cross-border TIA) are owned by EM — accepted risk, not to be re-raised.
- Engineering baseline still applies: notes visible per permission (role-configurable), included in tenant isolation, encrypted at rest by the DB provider.

## [2026-11-04] record | Devices & connectivity
- All device types for manager; login required for everyone; technicians use own smartphones to view schedule, customers served, payslip; tech-comfortable users; always-online assumption (no offline mode).

## [2026-11-04] decision | MVP release scope accepted
- R1 replace paper books (now incl. counter product sales) · R2 HR & payroll · R3 internal appointments · later: homepage, online booking, shop, Zalo, multi-branch UI, native app.
- EM asked that a future native app can link with the web app → principle #10 API-first/multi-channel + stable deep-linkable URLs; PWA meanwhile. Wrote product/vision.md.

## [2026-11-04] decision | Product sales deferred
- EM: product selling (and inventory) is a later feature, not current priority → removed from R1. Sale line items stay type-extensible so products plug in later; product commission deferred with it.

## [2026-11-04] record | Success metrics, migration, timeline
- R1 success = no paper books, daily cash close matches, zero missed package renewals (after 1 month live). Opening-balance screen for in-progress packages. No fixed deadline.

## [2026-11-04] record | /idea complete
- Journeys, risks (6), assumptions (1 KTV/visit, fixed price + manual discount with reason, cash/transfer only) confirmed and written to product/vision.md; overview stage → spec.
- Next: `/spec` R1, starting with the front-desk visit + customer package flow; clickable prototype of the front-desk screen before coding.

## [2026-11-04] record | Spec R1-1 drafted
- Wrote product/specs/front-desk-and-packages.md: 8 user stories, global ACs (401/403/404 tenant, idempotency, audit, money rules), concurrency on last session, opening-balance import. 10 defaults [D1–D10] awaiting EM.

## [2026-11-04] ingest | Service menu + spec R1-1 approved
- Transcribed 2 menu images → product/service-catalog.md (36 services, 11 categories, ★ recommended, female/male price variants, "theo tư vấn" durations).
- Spec R1-1 approved (D1–D10 accepted) and extended: ServiceCategory, ServicePrice variants (all/female/male, variant may be N/A), gender-based default price at the counter.
- Open: price of "Ôn ấm tử cung"; list of packages currently sold (not on menu).
- Next: `/architect`.

## [2026-11-04] decision | /architect R1 complete
- Wrote architecture/system.md (quality ranking, C4 L1/L2, trust boundaries, data flows J1–J3, failure modes, cost ~$10–15/mo) and security/threat-model.md (STRIDE TB1–TB4 + insider).
- ADR-0001 accepted & amended: internal app = Vite React SPA same-origin with API (Next.js/Astro reserved for public site later); Redis deferred.
- ADR-0003 multi-tenancy, ADR-0004 auth, ADR-0005 compute hosting, ADR-0006 repo/module layout — accepted.
- Next: `/backend-design` + `/frontend-design` (with clickable front-desk prototype) for spec R1-1.

## [2026-11-04] decision | Platform admin support access
- Platform admin has no default read access to tenant customer/health/finance/payroll data; owner grants time-boxed (24h, revocable) support access; all access audited and visible to owner. Recorded in ADR-0003 + threat model.

## [2026-11-04] record | /backend-design R1-1 complete
- Wrote architecture/backend.md (API contract, error codes, idempotency/ETag, integrity strategy per write path, crate layering, jobs, AC→endpoint trace), architecture/data.md (schema, ER, RLS, composite tenant FKs, CHECKs, indexes, migrations), security/authz-matrix.md.
- Design choices: global user_account + per-tenant membership (one person, many spas); package status derived (no status job); bonus sessions per plan item; server-side pricing only; app-level AES-GCM for health/visit notes; amount_paid guarded by CHECK + atomic update.
- /frontend-design running in parallel (fork).

## [2026-11-04] record | /frontend-design R1-1 complete + prototype published
- Fork wrote design/design-system.md ("Trà thất yên tĩnh": warm paper surfaces, burgundy primary, gold accent, dark mode), design/front-desk-flows.md, architecture/frontend.md, prototypes/front-desk/index.html (vanilla JS, mock data; jsdom smoke-tested, not yet eyeballed in a real browser).
- Prototype published as artifact for EM/manager to try.
- UX gaps raised: combine old-debt collection with today's visit? "low sessions" threshold (proto uses ≤2, align with spec #3); who sets check-out time and is it needed for R2? package switch UI on a line; void of a bank-transfer visit — actual refund handling (affects spec #2).
- Next: EM tries prototype → `/breakdown` spec R1-1.

## [2026-11-04] decision | No receivables, no check-out time, reminder rules
- EM: customers pay in full after service → R1 has no debt/partial payment/receivables (US5 deferred, D7 dropped; payments must equal total; schema keeps amount_paid for later). Opening balance has no debt.
- Check-out time not tracked (removed from spec/data).
- Reminders (spec #3): package low (≤N sessions, default 2) and customer with sessions left not visiting ≥X days (e.g. 7/14) → notify manager for follow-up; N, X configurable by owner.
- Updated spec R1-1, backend.md, data.md, authz-matrix, paper-books-analysis, front-desk-flows banner; prototype updated (no "Ghi nợ").

## [2026-11-04] record | /breakdown R1-1
- Wrote plans/r1-1-front-desk.md: S0 repo/CI/LSP → S1 kernel (tenant/RLS/idempotency/audit) → S2 auth → S3 tracer bullet catalog read + menu seed → S4 catalog manage → S5 customers → S6 sell package → S7 record visit → S8 void/lifecycle → S9 opening balance → S10 hardening/UAT → S11 staging (needs EM: Supabase project, VPS, domain).
- All spec ACs covered (US5 deferred); no L tickets.
- Next: `/build S0`.

## [2026-11-04] record | S0 foundation built
- git repo (branch feat/s0-foundation), monorepo: apps/api (Cargo workspace, crate app: config fail-fast + /api/v1/health + tracing), apps/web (Vite React TS strict, Tailwind v4 + tokens, TanStack Query, self-hosted fonts, ApiStatus), packages/ui (tokens.css, formatVnd), infra (dev compose Postgres 16, prod Caddyfile with strict CSP), CI (fmt/clippy/test/cargo-deny, lint/typecheck/test/build/audit, gitleaks), README.
- Checks green: 5 Rust tests, 6 TS tests, clippy -D warnings, build; E2E smoke SPA→proxy→API 200.
- exFAT drive: git safe.directory, pnpm hoisted linker, workspace packages via path aliases.
- PostToolUse compile-check hook added + pipe-tested. EM to install LSP plugins; Docker Desktop must be running for dev:db.
- Next: S1 kernel.

## [2026-11-04] record | S0 pushed, S1 kernel built
- Repo pushed: github.com/hungtruongOwolf/management-platform-for-spa (private); first CI run green (API, Web, Secret scan). EM authorized autonomous commit/push.
- S1: migration 0001_kernel (schema app, current_tenant(), tenant/branch/audit_log/idempotency_record, RLS + grants, SECURITY DEFINER purge); kernel crate (ids, Money, BusinessDate, AppError→problem+json, Permission catalog + Actor, TenantTx, audit, idempotency, IdempotencyKey extractor, testing helpers); app: config (APP_DATABASE_URL, APP_PUBLIC_ORIGIN), middleware stack, `app migrate`, purge job.
- 36 Rust tests green; RLS mutation check passes; live smoke OK (migrate as owner, serve as app_rw, foreign Origin → 403).
- Docs updated: backend.md (idempotency semantics), data.md (roles, 0001), coding-standards (sqlx prepare, TenantTx, test conventions), README, plan.
