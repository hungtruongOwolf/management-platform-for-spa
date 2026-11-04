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

## [2026-11-04] record | S1 PR open, CI green
- PR #1 (feat/s1-kernel) CI green after fixes: gitleaks needs `pull-requests: read`; cargo-deny `allow-wildcard-paths` for internal path deps; allow CDLA-Permissive-2.0 (webpki-roots).
- Auto-merge was blocked by the session's safety classifier ("merge without review") → waiting for EM to merge PR #1 or allow merges.
- Next: S2 auth & sessions (branch from main after merge).

## [2026-11-04] record | S1 merged; S2 auth built
- PR #1 merged by agent on EM's "làm đi".
- S2: identity crate + migration 0002, auth routes (/auth/login|logout|logout-all|reauth|switch-tenant, /me), AuthUser→Actor extractor, CLI create-tenant/add-user, web login + guard + quick lock (TanStack Router, zod 4, RHF).
- 62 Rust + 13 web tests; lockout mutation-checked; E2E through Vite proxy OK. MFA split to S2b.
- Dev data: tenant vien-phuc-y-duong, test owner 0905000001.

## [2026-11-04] record | S2 merged; S3 tracer bullet built
- PR #2 merged by agent on EM's "tự chạy đi".
- S3: migration 0003_catalog, catalog crate (price variants, seed of the printed menu), OpenAPI contract (utoipa 6, 9 endpoints) → generated TS client, web AppShell + /bang-gia. CI now fails if openapi.json or the TS client drift from code.
- Correction: the menu has 35 services / 10 categories (earlier entries said 36/11 — miscount). Equal female/male menu prices stored as one flat price; Ôn ấm tử cung seeded inactive (no price).
- Verified in real Chrome: guard → login → back to /bang-gia, dark theme renders, no console errors. Dev tenant catalog seeded.

## [2026-11-04] record | S3 merged; S4 catalog management built
- PR #3 merged on EM's "tự làm tiếp đi".
- S4: catalog write API (categories/services/plans, PUT + If-Match, audited, no delete), kernel IfMatch/etag, web edit forms + /bang-gia/goi. Owner-only (catalog.manage); manager/foreign tenant refused (tests).
- Found in real Chrome: signed-in user on /dang-nhap?next=… was sent to "/" — fixed (test red→green).
- Dev DB now has a test plan "Gội thảo dược 10 buổi + tặng 2 (thử)".

## [2026-11-04] record | S4 merged; S5 customers built
- S5: migration 0004_crm, crate crm (NFC names, accent-folded search, E.164 phones, encrypted health notes with AAD), routes GET/POST /customers, GET/PUT /customers/{id}, PUT /customers/{id}/health-note; config APP_DATA_KEY.
- Web: /khach-hang search + create, /khach-hang/:id profile with health warning.
- 7 Rust E2E (10k-customer search perf, health-note permission mutation-checked) + 33 web tests. Verified in real Chrome.
- Contract drift from design fixed in backend.md: PUT instead of PATCH, `existing_id`.
- Dev DB: test customer "Khách Thử Nghiệm".

## [2026-11-04] ingest | Supabase credits redeemed
- Source: raw/2026-11-04-em-supabase-credits.md. Org "For my future" (GitHub login). Unblocks part of S11; EM to create the staging project (Singapore, Data API off) and send back the project ref only.
- Process change: work now runs in parallel sub-agents (S6, S2b, S11 prep, S10 infra), each in its own worktree + dev DB; merges still need EM go-ahead (classifier).

## [2026-11-04] ingest | Supabase staging project created
- Ref `zyyiukcysjntjgjabiqe`. Region/Data-API setting to be confirmed by EM. DB password was pasted in chat → EM advised to rotate; not recorded anywhere.

## [2026-11-04] record | S11 prep — deploy artifacts ready, waiting on EM accounts
- Branch `chore/s11-deploy-prep`. Added:
  - `apps/api/Dockerfile` (cargo-chef, SQLX_OFFLINE, distroless nonroot, busybox-wget HEALTHCHECK) and `infra/Caddy.Dockerfile`. Caddyfile now uses `{$DOMAIN}`, adds COOP/CORP/XFO, `/assets` immutable, SPA fallback, strips client X-Real-IP/Forwarded.
  - `infra/docker-compose.prod.yml`: migrate one-shot with the owner URL, api with only the app_rw URL, caddy, ops/drill profiles, limits, read-only rootfs.
  - Supporting files: `.env.production.example`, `infra/backup/` (pg_dump → age → R2, systemd timer, restore.sh), `infra/deploy.sh`, CI `images` job, `deploy.yml` (workflow_dispatch → GHCR → SSH), `infra/prodtest/run.sh`.
- Runbook `wiki/runbooks/staging-setup.md`. EM decisions recorded in ADR-0002/0005 updates: staging Supabase `zyyiukcysjntjgjabiqe` is in a US region with Data API on, so the VPS goes in the same region and access is locked down with REVOKE; production will be a new Singapore project. SQLx connects through the Supavisor session pooler as `app_rw.<ref>`.
- Verified: `cargo build --release --locked` with SQLX_OFFLINE OK; `pnpm --filter web build` OK (no inline script/style, so the CSP holds); `docker compose config` OK, and the api env contains no owner URL; `sh -n` OK on all scripts.
- **Not verified:** the image builds and the local prod-like stack. Docker Desktop went read-only and then stopped responding because drive C: was 100% full, and the dev Postgres container must not be restarted. Rerun `sh infra/prodtest/run.sh` once there is space; CI job `images` also builds all three images.
- Recommended VPS: DigitalOcean Basic 2 GiB, $12, in the datacenter matching the Supabase region (NYC3 or SFO3). Production: SGP1.

## [2026-11-04] record | Disk-full incident; S11 prep merged and smoke-tested
- C: filled up (48 leftover Remotion temp bundles, 27.9 GB) → docker Postgres went read-only. EM approved deleting them; then Docker failed with `wsl-bootstrap … running mkfs: exit status 1` (data vhdx still attached in the shared WSL VM) → fixed by `wsl --shutdown` + restart. Dev data intact.
- PR #7 (S11 deploy prep) merged. Local prod smoke test now passes end to end after fixing the prodtest init-volume path.
- EM allowed `gh pr merge` in project settings; merges no longer need asking.

## [2026-11-04] record | S10 infra — authz matrix gate, Playwright E2E + axe, p95, Lighthouse CI
- Branch `test/s10-infra`. S10 is not ticked yet: it still needs the S6–S9 rows, `/quay`, and the trial session with the manager. Guide: wiki/conventions/testing.md.
- `app/tests/authz_matrix.rs` holds one `ROUTES` table that mirrors authz-matrix. The CI gate fails when a route that is mounted in a router or documented in OpenAPI has no row. Tests: anonymous or forged session → 401; role × route → 403 or allowed; foreign-tenant id → 404, with a control call on the caller's own record; rows must match roles.rs. Mutation-checked twice: dropping a row turns the gate red, and removing the `CustomerRead` guard turns the matrix red.
- Playwright (apps/web/e2e) runs the API on :8091 and Vite on :5181 (`VPYD_API_ORIGIN` proxy hook) against DB `vpyd_e2e`. The setup project provisions a fresh spa per run through the CLI with generated credentials. 10 tests green locally, ~2 min warm.
- a11y bug found and fixed: inactive services and plans used `opacity-60`, which made axe report a serious color-contrast violation on /bang-gia. They now use `text-text-muted` (plans also get a dashed border).
- p95 (debug build, 300 customers, 100 samples): every hot read endpoint is ~21–26 ms against a 300 ms budget. Lighthouse on /dang-nhap (mobile, slow 4G): LCP ≈ 2.1 s against a 2.5 s budget, so the margin is small.
- CI: new `e2e` job (Postgres service, app_rw, Chromium; runs e2e + axe + p95 + lhci; uploads artifacts).

## [2026-11-04] record | S6 sell packages built
- Branch `feat/s6-sell-packages` (rebased on main after #5, #6, #7). Migration `0005_sales_packages`: staff, sale, sale_line, payment, customer_package, customer_package_item; payment/sale_line append-only by grants + trigger.
- New crates `org` (GET /staff, CLI add-staff), `packages` (GET /customer-packages/{id}, GET /customers/{id}/packages, `create`, `consume_session`), `sales` (POST /sales with Idempotency-Key, GET /sales/{id}). Kernel `tenant::profile` + Money helpers; catalog `plan_snapshot`, crm `exists_in` for cross-context calls inside one TenantTx.
- per_session_value follows spec D4/AC3.1: price after discount ÷ purchased sessions (10 + 2 at 1.5M → 150.000đ), bonus = 0, purchased sessions consumed first.
- Tests: 11 sales E2E + 4 packages (incl. last-session race) + 6 web; manager-10% and amount-mismatch guards mutation-checked (red, then restored).
- Dev note: in agent worktrees the sqlx macros resolved DATABASE_URL to the main `vpyd` DB instead of the worktree `.env`; exporting DATABASE_URL explicitly fixed it.
- Skipped stage: no separate backend-design pass for S6; contract drift recorded in backend.md/data.md.
## [2026-11-04] record | CI reliability review
- EM felt CI fails often. Data: 24 runs → 21 success, 1 cancelled (superseded push), 2 failures — both in S1 while CI was being set up (cargo-deny path/license config, gitleaks permission). Every PR since S2 passed first time. Recent pain was local infra (disk full, Docker mkfs, reboot), plus PR #8 merged before CI started.
- Fixes: `scripts/merge-when-green.sh` is the only merge path; `pnpm check` now also runs contract-drift + cargo-deny (`scripts/check-extra.mjs`). Rule added to coding-standards Git section.
## [2026-11-04] record | S2b MFA TOTP for owners
- Branch `feat/s2b-mfa`, migration `0006_mfa` (depends on 0001–0002 only). Two-step login: password → pending-MFA session (10 min, refused by `CurrentSession`/`AuthUser`) → `/auth/mfa/verify`, or forced `/auth/mfa/enroll/start|confirm` → fresh full session (token rotated).
- MFA requirement is role data (`role.requires_mfa`; a CHECK keeps it on the system owner role; `platform.tenant.manage` also implies it). TOTP via `totp-rs` 6 (RFC 6238, ±1 step, one use per step). The secret is encrypted with AES-256-GCM via the new `kernel::crypto::FieldCipher`, and crm now re-exports it (single source). 10 hashed single-use recovery codes. MFA failures share the account lockout counter, and a correct password no longer resets it for MFA accounts. Reauth requires a code for MFA accounts. CLI `app reset-mfa`.
- Web `/xac-thuc` (code / recovery code), enrollment with a client-side QR (`uqr`), recovery codes shown once. Old auth tests moved to a manager account; 11 new HTTP MFA tests, mutation-checked (replay guard, pending guard, counter reset).
- Dev: owner `0905000001` enrolls on first login; needs `APP_DATA_KEY` in `.env`.
- Open: voluntary MFA for other roles (enroll from a full session), regenerating recovery codes, revoking sessions when a role gains `requires_mfa` (role editing is spec #4), audit entries for MFA events (audit_log is tenant-scoped).
