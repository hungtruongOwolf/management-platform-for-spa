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

## [2026-11-04] record | S9 opening balance (nhập sổ cũ) built
- Branch `feat/s9-opening-balance` on main. No migration: `opening_balance.import` was already seeded for Owner/Manager since S2.
- `POST /opening-balances` (Idempotency-Key) in `packages::opening_balance`: existing `customer_id` or a new `customer` profile + 1–10 packages (plan reference or free-form items with sessions left); one TenantTx, packages `origin=opening_balance`, no sale/payment, audit `opening_balance.import`. Field errors prefixed `customer.` / `packages[j].`.
- New public fns: `crm::service::create_in` (tx variant; `create` delegates), `catalog::service::service_name`, `packages::service::validate`.
- Web `/nhap-so-cu` + nav "Nhập sổ cũ"; "Lưu và nhập tiếp" resets and refocuses the search. Tests: 8 API + 1 matrix row, 10 web, 2 Playwright (axe clean); remaining ≤ total guard and no-payment assertion mutation-checked.
- Skipped stage: no separate backend-design pass (contract recorded in backend.md). Deferred: "còn nợ" (receivables, US5 deferred in R1). Dev note: the agent scratchpad is shared between parallel agents — name helper scripts per ticket.
## [2026-11-04] record | S7 front desk — record visits (/quay)
- Branch `feat/s7-front-desk` (on main after #12 S6, #13, #14 S2b). Migration `0007_visits`: visit (no check-out, one sale per visit), visit_line (unit_price, line_value, sessions_left_after; one CHECK per source; column grant UPDATE(staff_id) only).
- New crate `operations`: POST /visits (Idempotency-Key), GET /visits?date=, GET/PATCH /visits/{id}. One TenantTx across crm/org/catalog/packages/sales; items consumed in item-id order (no deadlock); one-off lines → `sales::record_service_sale`. Default package = soonest expiry, then oldest. Gender pricing on the server. Visit note encrypted (kernel::crypto), never in the stored idempotent response, the day list or audit.
- Web /quay per prototype; fast path verified in Playwright as exactly 4 keyboard actions, focus asserted at each step.
- Tests: 16 HTTP visit tests (last-session race, concurrent double submit, male Đại Thông Lạc = 399.000đ, …) + 4 domain unit + 4 authz-matrix rows + 8 web + Playwright spec (axe ×3). Mutation-checked: gender rule, expired-package guard. Lighthouse now also audits /quay (`apps/web/lighthouse/run.mjs`, signs in an E2E manager because owners need TOTP).
- Deviations: no visit-level discount, no back-dated visits, GET /customers/{id}/visits not built. S8 needs `packages::service::restore_sessions` + sale void with reversal payments.
- Bug found by E2E: per-IP limit used `tower_governor::per_second(5)` as a rate; it is a period → 12 req/min per IP after the burst (one NAT IP per spa). Fixed to 5 req/s with a red→green test. Header CLS fixed (0.109 → 0.009). Local LCP ≈ 3.0 s on both /dang-nhap and /quay (CI passed /dang-nhap on main); open: drop zod from the session critical path if CI also fails.

## [2026-11-04] record | perf: LCP /dang-nhap 3.07 → 1.81 s, /quay 3.17 → 2.06 s (local lhci)
- PR #19 was blocked by Lighthouse CI (CI: /dang-nhap 2674 ms, /quay 2870 ms, budget 2500, unchanged). Lantern traces showed the cause: the 9 font files (≈ 136 KB, including 4 `latin-ext` files that Vietnamese letters pull in) and all the JS finished before the observed LCP paint, so all of it counted; `/me` started only after the JS.
- Changes (`apps/web`): `session.ts` parses `/me`, login and MFA replies with hand-written guards (`parseMe`), no zod; the login form uses plain React state (no RHF/zod); `/xac-thuc` is a lazy route (`MfaRoutePage`, takes `uqr` with it); `CustomerForm` is `React.lazy` inside `/quay`; `HealthNote` has its own module, so `/quay` no longer pulls CustomerProfile → CustomerPackages → SellPackageDialog; `src/fonts.css` declares only latin + vietnamese woff2; `app/fonts.ts` loads it 500 ms after the first rendered route on a cold load, and at startup once a `localStorage` flag says the files are cached; `index.html` preloads `GET /api/v1/me`. `lighthouse/run.mjs` reads `LHCI_WEB_PORT`.
- Critical-path JS (gzip): 164 KB (index 112 + api 52) → 120 KB (index 106 + errors 8 + session 6). zod + RHF now sit in a lazy `schemas-*.js` (37 KB gzip). Fonts fetched before LCP: 9 → 0.
- Local lhci medians (same machine, before → after): /dang-nhap 3071 → 1809 ms, /quay 3173 → 2058 ms; CLS 0.001 / 0.009 unchanged. CI (run 41933500171, green): /dang-nhap 2674 → 1869 ms, /quay 2870 → 2100 ms, CLS 0.001 / 0.004. /quay has the least headroom (~0.4 s): its visits/staff requests land before the paint and count too. Steps: fonts −0.7 s, `/me` preload −0.2 s, bundle split the rest. The rules are in [frontend § Perf budget](wiki/architecture/frontend.md).
- Tests: +18 web (parseMe/login guards, login focus/revalidate/trim, font policy); lint, typecheck, 80 unit and 14 Playwright tests (axe) pass.
## [2026-11-04] record | S8 voids & package lifecycle
- Branch `feat/s8-void-lifecycle` (built on S7 `feat/s7-front-desk`). Migration `0009_void_lifecycle` (0008 left unused): `refund` table (append-only, one per package, trigger amount ≤ sold_price), `customer_package.cancelled_by`, triggers: a reversal must mirror a real payment (no reversal of a reversal), voided/cancelled rows are final, no session restore on a cancelled package.
- Reversal design: reuse `payment.kind='reversal'` + `reversal_of` (UNIQUE since 0005), dated on the void day, instead of a separate table: one money table for the cash book, old days never change, a double reversal is impossible in the DB.
- Endpoints (all Idempotency-Key): POST /visits/{id}/void, POST /sales/{id}/void (package sales only, unused sessions only), POST /customer-packages/{id}/extend (If-Match, owner), POST /customer-packages/{id}/cancel 🔒 (refund ≤ paid). Closed-day rule in `kernel::corrections::authorize_void`; reauth flag on `Actor`, set by `AuthUser` from `session.recent_auth_at`.
- Web: void per visit in the /quay list (+ day picker for owners), package actions on the customer profile, shared `ReauthDialog` (password, then TOTP when asked) that retries on the same key. Tests: 14 HTTP (voids.rs, incl. parallel double void → exactly one) + 4 matrix rows + 5 unit + 11 web + Playwright voids.spec.ts (axe). Mutation-checked: double-void guard, refund cap, day lock.
- Perf: void and package-action forms are lazy chunks. `pnpm lhci` caught a CLS regression on /quay (0.27: the void button made rows wrap); fixed to 0.045 (S7 baseline 0.043), LCP 2.06 s local.
- Open for S10: `PATCH /visits/{id}` on a closed day does not ask for reauth (S7); no cash-book report reads refunds/reversals yet (spec #2). Skipped stage: no separate backend-design pass (contract recorded in backend.md).
## [2026-11-04] record | security review fixes r1 (H1, M2, M3, L1–L8, INFO)
- Branch `fix/security-review-r1` from `main` @ 0e0c102; migration `0011_security_review` (0010 reserved for S10). M1 (`PATCH /visits/{id}` reauth) done by S10 (#22); merged origin/main @ 2e8a223 before opening the PR.
- H1: `switch-tenant` no longer counts as re-auth nor extends the session: `identity::service::Issue::{MfaPending, Authenticated, Rotated}`; `session.recent_auth_at` has no DEFAULT. M3: suspended spa → `403 auth.not_a_member` for open sessions; `user_requires_mfa` ignores tenant status. L4: granting an MFA role revokes sessions. L2: per-account lockout kept + `app unlock --phone P` (reason in ADR-0004 update).
- L6: startup refuses unverified DB TLS outside development (prodtest-only staging exemption `APP_DB_INSECURE_TLS_FOR_LOCAL_TEST`) and superuser/BYPASSRLS/table-owning runtime roles. L1: rate-limit buckets IPv4 / IPv6 /64, swept every 60 s. L5: Caddy redacts query strings + drops Referer; API spans log the path only. L7: R2 bucket lock + lifecycle (R2 has no write-only token), backup.sh never prunes, uploads via `incoming/`. L8: actions pinned by SHA, deploy job `packages: read`.
- M2 (EM decision 2026-11-05, brought into scope by the lead): owner opens paper-book import per business day (`PUT /opening-balances/window`, new permission `opening_balance.manage_window`); closed day → manager `403 opening_balance.closed`, owner 🔒; `customer_package.refund_cap` (0 for a manager-typed book price until the owner sends `confirm_book_price`), refund trigger uses it. Web: toggle + closed-day explanation on `/nhap-so-cu`, "Đã đối chiếu giá trong sổ" on cancel. Spec US8 AC8.6–8.8.
- INFO: `safeNextPath` rejects `[\x00-\x20\\]`; `kernel::idempotency::hash_route` (method + route + record + body) in sales, packages and (after S10 merged) operations; `POST /sales` with payments needs `payment.collect`; `allocate_discount` never exceeds a line total. L3 (TOFU on first MFA enrollment) documented only.

## [2026-11-04] record | S10 hardening — AC coverage audit, 7 bugs fixed, trial-session script
- Branch `test/s10-hardening` (on main after S8 #20). S10 stays unticked until the manager trial ([r1-1-trial-session](wiki/plans/r1-1-trial-session.md)). AC → test table in the plan's S10 block: every spec AC maps to ≥ 1 automated test.
- Bugs (red test first, then fix): [high] a visit consuming a session deadlocked (500) with a concurrent sale void / visit void / package cancel: consume locked the item, then the visit_line FK key-shared the package, while voids lock package → item. Fixed with `packages::service::lock_for_consumption` (FOR SHARE on the packages in id order first). [medium] `PATCH /visits/{id}` on a closed day skipped reauth → `kernel::corrections::authorize_closed_day`. [medium] `\u0000` in any text field → 500; SQLSTATE 22021/22P05 now 422. [low] catalog prices unbounded → `Money::MAX_INPUT` 10 tỷ. [low] search `q` unbounded → 100 chars (+ `maxLength` on the box). [low] AC8.5 label missing on the /quay card. [perf] /quay CLS 0.156 on a busy day (104 visits): the cold-load font swap re-wraps text → metric-matched fallback @font-face in tokens.css (Arial/Georgia with size-adjust measured via canvas measureText) → CLS 0.013.
- Tests: `app/tests/hardening.rs` (12: Unicode, business-day edges 00:00/23:59:59 VN, money limits/rounding/0đ, races, natural-key double submits, problem+json on 15 write routes, G5 audit for 9 actions); `voids.rs` +1; web +1; E2E `a11y.spec.ts` (home, plans, service form) + axe on the MFA step; deterministic reauth in `voids.spec.ts` via `e2e/support/db.ts` (devDep `postgres` 3.4.7, E2E DB only); perf spec times POST /sales, /visits, /opening-balances and busy-day reads.
- Perf (local, debug API, 1 client, p95 ms): search 14–14.4, catalog 13–16, POST /sales 41, POST /visits 37, POST /opening-balances 24, GET /visits (104 visits) 38, /customers/{id} 13, /customers/{id}/packages (> 200 packages) 34, /sales/{id} 18, /staff 13. Lighthouse (local, machine loaded by parallel agents, 3 runs): /dang-nhap LCP 1911/2079/4233 ms, CLS 0; /quay LCP 2495/2572/4268 ms, CLS 0.013/0.013/0; `pnpm lhci` passes. A run without the busy day showed LCP up to 7 s, which is machine noise; earlier quiet runs measured 1.8–2.1 s.
- Authz matrix: all 37 ApiDoc routes have rows. PlatformAdmin / support grant is documented as out of R1-1 scope (no such role in code). Coordinated with the parallel security-fix agent: did not touch identity, middleware, config, infra, workflows, api.ts, idempotency hashing, allocate_discount.
- Open (EM decision): visit history on the customer profile (`GET /customers/{id}/visits`, deferred since S7); AC4.4 per-technician count is data only (the report belongs to spec #3); `GET /visits` has no pagination.

## [2026-11-05] ingest + record | EM simplifies owner re-auth and opening balance (undo M2 of #23)
- Source: [raw/2026-11-05-em-simplify-owner-and-opening-balance.md](raw/2026-11-05-em-simplify-owner-and-opening-balance.md). Branch `feat/simplify-owner-opening-balance` from `main` @ df11da5. The hosting part of the decision (Vercel + Supabase) shipped separately in #24.
- Opening balance: no import window. Removed `GET/PUT /opening-balances/window`, permission `opening_balance.manage_window`, `403 opening_balance.closed`, web toggle + closed-day explanation. Imports always land on today's business day; past days follow the closed-day rule.
- Owner re-auth: `kernel::corrections::authorize_closed_day` → closed day without `record.void_closed_day` = `403 day.locked`, with it = allowed (no reauth). Removed reauth from opening-balance import and package cancel/refund. `Actor` lost `recent_auth`/`require_recent_auth` (dead). Re-auth kept only for account-security actions (`identity::AuthUser::require_recent_auth`, `POST /auth/reauth`; no such route yet). Web: `useGuardedAction` → `useFormAction` (no dialog); `ReauthDialog` + `reauthenticate()` deleted (no caller left; rebuild with the password/MFA settings screens).
- Refund cap deferred: dropped `refund_cap`, `confirm_book_price`, `422 refund.book_price_unconfirmed`, "Đã đối chiếu giá trong sổ". Migration `0012_simplify_owner_and_opening_balance` (0011 untouched): drop `opening_balance_window`, delete the permission rows, restore `check_refund_within_paid` to `sold_price`, drop `refund_cap` (DROP COLUMN fires no UPDATE trigger, so `customer_package_cancelled_is_final` needs no lifting).
- H1 coverage moved from `voids.rs` to `auth.rs::a_rotated_session_needs_a_real_reauth`. Security trade-off accepted by the EM recorded in ADR-0004 (update 2026-11-05) and the threat model. Wiki: spec AC8.6–8.8 rewritten; authz-matrix, threat-model, backend, frontend, data, front-desk-flows, testing, plan AC table updated.
## [2026-11-05] record | Staging lên Vercel + Supabase
- EM chọn Vercel + Supabase cho đơn giản ([raw](raw/2026-11-05-em-simplify-owner-and-opening-balance.md)). ADR-0005 cập nhật; runbook [vercel-staging](wiki/runbooks/vercel-staging.md).
- Supabase staging đã dựng: app_rw (NOBYPASSRLS), migrations 0001–0011, REVOKE anon/authenticated, tenant + chủ spa + menu 35 dịch vụ.
- Vercel project `vien-phuc-y-duong`: Services (web Vite tĩnh + API container, pdx1), TLS verify-full với Supabase Root 2021 CA. Smoke test: `/`, `/api/v1/health` 200, `/api/v1/me` 401, CSP/HSTS có, đăng nhập chủ spa → `mfa_enroll`.

## [2026-11-05] record | Brand redesign "Trà thất Đông y" (nâu đỏ / vàng / trắng / kem)
- EM: UI "hơi xấu", giao việc phối 4 màu đặc trưng cho "sinh động và tinh tế" ([raw](raw/2026-11-05-em-brand-colors-and-demo-data.md)). Branch `feat/brand-redesign`. Skip stage: không spec riêng (thuần visual), đi thẳng frontend-design → build.
- Palette trong `tokens.css` (bảng + quy tắc ở [design-system](wiki/design/design-system.md)): primary nâu đỏ `#7A2620`, khung app `#5A1D18 → #74271F`, vàng `#B07D24` (UI) / `#8A6114` (chữ) / `#E3B455` (trên nâu đỏ), nền kem `#FAF4E8`, card trắng; dark nâu trà ấm. `packages/ui/src/tokens/contrast.test.ts` kiểm 34 cặp × 2 theme (WCAG AA) và 2 khối dark giống nhau.
- UI: header nâu đỏ + logo sen SVG inline (`BrandMark`) + wordmark vàng; `AuthFrame` cho đăng nhập/MFA; trang chủ có khung chào + lối tắt theo quyền; viền input `border-strong` 3.47:1 (cũ ~1.3:1); nút chính ánh sơn mài; badge trạng thái gói; thanh số buổi ở quầy; avatar chữ cái ở danh sách khách; toast nâu đỏ viền vàng.
- Bug: `/quay` tràn ngang 48px khi có dòng trong giỏ (select dịch vụ lẻ rộng theo option dài nhất) → `w-56`, E2E assert không scroll ngang. Toast lỗi dark: chữ trắng trên `#FF8A80` (~2:1) → chữ `surface` (7.85).
- Perf: critical path JS 121.7 → 123.1 KB gzip (+1.4 KB; ngân sách 125, còn ~2 KB); CSS 5.4 → 6.7 KB gzip. Không thêm dependency, không đổi font. Không sửa `opening-balance`, `corrections/*`, `ReauthDialog` (agent khác đang làm), các màn đó nhận style qua tokens + element polish.
## [2026-11-05] record | seed-demo: dữ liệu demo cho staging
- EM không thử được luồng vì staging trống ([raw](raw/2026-11-05-em-brand-colors-and-demo-data.md)). Branch `feat/seed-demo`. Lệnh `app seed-demo --tenant S [--force-again]`: 5 KTV + 1 quản lý (đăng nhập `0905000002`, mật khẩu từ `APP_NEW_USER_PASSWORD`), menu nếu trống + 5 gói, 40 khách (14 ghi chú sức khỏe), 3 nhập sổ cũ (1 sắp hết hạn, 1 đã hết hạn dùng override), 22 hóa đơn gói (1 hủy), ~120 lượt trong 21 ngày (12 hôm nay, 1 hủy), 1 hủy liệu trình + hoàn tiền. Chi tiết ở [vercel-staging § Dữ liệu demo](wiki/runbooks/vercel-staging.md#dữ-liệu-demo), CLI list ở [backend §6b](wiki/architecture/backend.md).
- Mọi bản ghi đi qua use case thật (actor = chủ spa / quản lý demo, quyền đọc từ role). Ghi lùi ngày: `kernel::tenant::profile_as_of`, `sales::service::create_sale_as_of`, `operations::service::record_as_of` nhận `now`; handler HTTP vẫn gọi `create_sale` / `record` với `Utc::now()`. `identity::service::tenant_permissions` thành `pub`. `BusinessDate::today` bỏ (không còn ai gọi). Không có migration.
- Phát hiện từ yêu cầu: "trả một phần / công nợ" và "khách không SĐT" không làm được vì spec (US5 hoãn, D6 SĐT bắt buộc), nên demo không có. Refuse khi `APP_ENV=production`; chạy lần hai từ chối (dải SĐT `0399 RR CCCC` làm dấu).
- Tests: `app/tests/demo_seed.rs` (3: spa mới đủ số liệu + `/visits` hôm nay + hồ sơ khách + từ chối lần hai; `--force-again`; spa không có chủ spa) + 3 unit. Skip stage: không spec/design riêng (công cụ vận hành, không có hành vi người dùng mới).

## [2026-11-05] ingest + record | UI overhaul: logo thật, app shell, khung màn quản lý, bỏ "sổ cũ" khỏi UI
- Source: [raw/2026-11-05-em-management-views-logo-ui.md](raw/2026-11-05-em-management-views-logo-ui.md) (EM: PR #27 "vẫn còn rất tệ"; dùng đúng logo; muốn xem/quản lý; bỏ phân biệt sổ cũ). Branch `feat/ui-overhaul` từ `main` @ 47a56e8 (#27 đã merge). Skip stage: không spec riêng cho shell (visual + điều hướng); màn quản lý dựng khung theo contract đang làm ở `feat/management-views`.
- Logo: kênh alpha của PNG chính là nét sạch (ánh vàng chỉ nằm ở RGB của pixel trong suốt) → `scripts/brand/build_logo_assets.py` trace bằng potrace, tách mark / wordmark / emblem theo connected component, sinh favicon + icon PWA (mark vàng trên nâu đỏ, PNG 48 màu) + `manifest.webmanifest`. Dùng làm CSS mask (`currentColor`). Logo sen tự vẽ của #27 đã thay. Chi tiết: [design-system § Thương hiệu](wiki/design/design-system.md).
- Shell: sidebar nâu đỏ nhóm Quầy / Khách hàng / Gói liệu trình / Báo cáo / Danh mục / Cài đặt (icon sprite, thanh vàng mục chọn, thu gọn nhớ theo thiết bị), top bar (ngày, tìm nhanh Ctrl/⌘+K = command palette lazy, user, Khóa nhanh), điện thoại: tab bar + drawer `<dialog>`. Chuyển trang, card nâng, đếm số, skeleton có vệt sáng, cột biểu đồ mọc: chỉ transform/opacity, tắt khi reduced-motion. Theme sáng/tối/theo máy ở `/cai-dat`. Fast path `/quay` và phím tắt giữ nguyên.
- Màn quản lý (lazy, sau cờ `VITE_MANAGEMENT_DATA`, mặc định mock ở dev/test, off ở production): `/bao-cao` (thẻ số + so với hôm qua, 7 ngày, cơ cấu tiền mặt/CK và gói/lẻ, bảng KTV, dịch vụ dùng nhiều), `/goi-lieu-trinh`, `/nhac-khach`, lịch sử + tóm tắt trên hồ sơ khách. Kiểu DTO chép tạm từ `reporting/src/dto.rs` của branch kia (TODO thay bằng `@vpyd/api-client`). Branch kia chưa push contract nên chưa nối thật.
- "Sổ cũ": bỏ mục nav, ô trang chủ, badge "Nhập từ sổ cũ" ở quầy và hồ sơ; `/nhap-so-cu` chỉ chủ spa mở bằng URL (`opening_balance.import` + `package.cancel_refund`). Test unit + e2e cập nhật.
- Perf: một chunk khởi động (`codeSplitting.groups` `$initial`): critical JS 121.6 → **120.3 KB** gzip -9 (3 file → 1); CSS 6.7 → ~10.8 KB gzip (shell + components). Trang chủ chuyển sang lazy. Emblem 11.8 KB chỉ ở đăng nhập. Lighthouse để CI.
- Tests: `Shell.test.tsx` (nav theo quyền, không có sổ cũ, cờ, prefs khi storage lỗi, palette Ctrl+K → hồ sơ, drawer, thu gọn sidebar, theme), `Reports.test.tsx` (helpers, mock khớp tổng contract, `/bao-cao`, lọc gói, khách vắng, timeline), `Entitlement.test.tsx` (không có `revenue` → không hiện tiền). `configure({ asyncUtilTimeout: 4000 })` vì màn lazy trên máy chậm.
- Merge `origin/feat/management-views` (PR #28) vào branch: contract thật thay mock (bỏ cờ `VITE_MANAGEMENT_DATA`, gate = `report.read`), `origin` bỏ khỏi `CustomerPackageDto` (PackageActions dùng `sale_id != null`), xóa màn `/nhap-so-cu` + spec E2E opening-balance (dữ liệu sổ giấy qua CLI `app import-paper-book`).
- EM feedback 2 (cùng ngày): màn **Khách lâu không quay lại** `/khach-lau-khong-quay-lai` (7/14/30 ngày, mặc định 7, phạm vi, phân trang); **audit layout** (quy tắc ở [design-system § Quy tắc layout](wiki/design/design-system.md)), spec `layout.spec.ts` đo dịch chuyển khi hover/nhấn + không cuộn ngang ở 360/768/1280/1440. Chạy local trên dữ liệu seed-demo: 0 dịch chuyển, 0 cuộn ngang (sau khi sửa `/bao-cao` tràn 51–67px do cột grid ngầm định).
- Câu hỏi EM: file logo gốc vector? Manager có được xem tiền (`report.revenue.read`, Q1 của #28)?
## [2026-11-05] ingest + record | R1-2 management views: spec + API (backend)
- Source: [raw/2026-11-05-em-management-views-logo-ui.md](raw/2026-11-05-em-management-views-logo-ui.md) (EM: app chỉ để nhập, thiếu phần xem lại; bỏ phân biệt sổ cũ/sổ mới). Branch `feat/management-views` from `main` @ 47a56e8. Stages: spec → backend-design → build (qa, pr-review, secure, perf). Logo/UI part of the raw note belongs to the UI agent (`feat/ui-overhaul`).
- Spec [management-views](wiki/product/specs/management-views.md) (draft): US1 dashboard ngày (+ so sánh hôm qua / cùng thứ tuần trước, theo KTV), US2 khoảng ngày (day/week/month, top), US3 sổ doanh thu, US4 lịch sử khách + trang liệu trình + tóm tắt, US5 tổng quan liệu trình + khách lâu không quay lại, US6 thu chi → R1-3. Open Q1 (quản lý xem tiền? default yes), Q2 (30 ngày), Q3 (cash basis), Q4.
- API: new crate `reporting` (read-only), 8 GETs (backend §3b). Contract committed early (openapi.json + schema.ts + api-client aliases). Money cash-basis from `payment`/`sale_line`/`refund`: reversal on the void day, refunds separate, net = collected − voids − refunds, split by method and kind. Fields behind `report.revenue.read` are absent, not hidden.
- Migration 0013: permissions `report.read` + `report.revenue.read` (owner, manager), manager loses `opening_balance.import`; `app.package_status()`, views `customer_package_state` + `revenue_book` (security_invoker), 6 indexes (all used in the 3-year bench; old branch-leading day indexes 0 scans).
- "Sổ cũ": `CustomerPackageDto.origin` removed (paper-book packages only differ by `sale_id = null`). Recommended + built CLI `app import-paper-book --tenant --file [--dry-run]` (CSV columns in spec §7, owner use case, skips already-imported packages, any bad row → nothing written). `POST /opening-balances` kept owner-only until the web drops `/nhap-so-cu`, then delete (plan M8).
- Impact: `app::demo` parsed `origin` from the DTO → `sale_id.is_some()`; opening_balances/hardening/demo_seed tests import as owner and assert no `origin`; authz matrix +8 rows, opening-balances OWNER_ONLY. Web still reads `origin` in `PackageActions.tsx`, `CustomerCard.tsx`, `CustomerPackages.tsx` (UI agent, plan M7).
- Tests: `reports.rs` (9), `paper_book.rs` (2) + unit tests (cursor, buckets, windows, CSV). Mutation check: forcing `with_money = true` turns `money_is_only_sent_to_holders_of_report_revenue_read` red. Flake found and fixed while writing tests: ordering by wall clock mixed API and DB clocks (and back-dated demo sales carry `created_at` = seed time) → ledger orders by UUIDv7 `seq`, timeline by business day first.
- Perf (bench, debug, p95): worst 150 ms (1-year range by month; first version 396 ms through the view → aggregate payments directly); everything else ≤ 59 ms.


## [2026-11-05] ingest + record | Vòng 3 backend: follow-up khách vắng, tìm kiếm, trùng SĐT, preset báo cáo, danh mục
- Source: [raw/2026-11-05-em-ui-review-round3.md](raw/2026-11-05-em-ui-review-round3.md) (mục 3, 4, 6, 13, 15, 16 + phần API của 2, 12). Branch `feat/followup-and-reports-api` từ `feat/ui-overhaul`, sau đó merge `origin/main` (#29 đã merge, nội dung giống hệt nên conflict giữ bản của branch). Stages: spec (bổ sung vòng 3 trong [management-views](wiki/product/specs/management-views.md)) → backend-design ([backend §3b](wiki/architecture/backend.md), [data](wiki/architecture/data.md)) → build → qa → secure ([authz-matrix](wiki/security/authz-matrix.md)) → pr-review. Contract push sớm 3 lần cho agent frontend (`feat/ui-features-r3`).
- Migration **0014**: bảng `customer_follow_up` (append-only, RLS), quyền `customer.follow_up` (chủ spa + quản lý), hàm `app.search_fold` (bản SQL của `search_key`).
- Endpoint mới: `POST/GET /customers/{id}/follow-ups`, `GET /customers/phone-check`. Mở rộng: `/customers/inactive` (khách lẻ + liệu trình, `kind`, `follow_up`, `sort`, `q`, `last_follow_up`, `next_follow_up_on`, `gender`; `scope` mặc định `all`), `/customers/{id}/history` (`q`, `kind`, `from`, `to`), `/packages?q=` tìm cả tên gói, `/reports/range` (`preset`, `year`, `net_revenue`, `technicians[]`), `category_name` ở mọi DTO có tên dịch vụ, `customer_gender` ở lượt / sổ doanh thu / tổng quan gói.
- Gói không thời hạn: đã đúng (NULL không bao giờ `expired` / `expiring_soon`); thêm test.
- Bug cũ phát hiện: OpenAPI khai báo các query param (`q`, `active`, `date`, `status`, `phone`) là **path** param → sửa + test chặn.
- Perf: range đọc visit lines một lần (CTE materialized): 1 năm 150 → 120 ms; 3 năm vẫn ~380 ms > 300 → tháng/năm giới hạn 2 năm, preset `last_2_years` thay cho `last_5_years` (Q5 cho EM).
- Tests: `app/tests/follow_ups.rs` (10: ghi follow-up + quyền + idempotency + audit; append-only + RLS; phạm vi khách vắng + biên 7 ngày; lọc/sắp xếp follow-up + phân trang; phone-check; tìm lịch sử; gói không hạn + tìm tên gói; preset; category/gender; search_fold = search_key), authz matrix +3 dòng, unit (quy tắc +3 ngày, validate, preset theo lịch, text filter), openapi query-param guard, bench +8 shape.

## [2026-11-05] record | Vòng 3 UI tính năng: báo cáo theo kỳ, tìm/lọc mọi danh sách, follow-up, trùng SĐT, giới tính, danh mục (PR #34)
- Source: [raw/2026-11-05-em-ui-review-round3.md](raw/2026-11-05-em-ui-review-round3.md) mục 2, 3, 4 (UI), 6, 7 (áp dụng), 12, 13 (UI), 15 (UI), 16 (UI). Branch `feat/ui-features-r3` từ `origin/feat/ui-overhaul` (PR #29 lúc đó chưa merge), sau đó merge (không rebase) `origin/main` (#29, #30), `origin/feat/followup-and-reports-api` (contract r3, r3b, bản cuối #33) và `origin/feat/ui-shell-r3` (#31: `PageHeader`, `useSearchState`, `CustomerName`, `BrandLogo`). Stages: frontend-design → build → qa → perf → pr-review (bỏ spec riêng: yêu cầu nằm trong raw + spec R1-2 vòng 3 do backend viết).
- **`/bao-cao`:** chọn kỳ (Hôm nay … 12 tháng, Tùy chọn ≤ 2 năm) trong URL; KPI so với kỳ trước cùng độ dài; **line chart SVG tự viết** (kỳ này vs kỳ trước, đổi chỉ số Thực thu/Khách/Lượt, crosshair + tooltip chuột và bàn phím); bảng chi tiết theo ngày/tuần/tháng (cũng là bảng thay thế của biểu đồ); KTV, top dịch vụ (có danh mục), top gói theo kỳ; tab **Sổ doanh thu** nối `/reports/ledger`. Bỏ `BarChart`. Palette theo skill dataviz (emphasis: nâu đỏ vs xám `border-strong`; validator: CVD ΔE 23, tương phản ≥ 3:1 cả 2 mode).
- **Tìm/lọc** (phân tích từng màn ở [frontend § Màn quản lý](wiki/architecture/frontend.md)): gói (tìm khách/gói/SĐT + trạng thái), khách vắng (tìm, ngưỡng 7–90, loại khách, phạm vi, tình trạng liên hệ), lịch sử hồ sơ (dịch vụ/gói/KTV, loại, khoảng ngày, phân trang). Bảng giá: chưa cần (35 dịch vụ, đã nhóm danh mục). Tất cả qua `useSearchState` của shell.
- **Follow-up:** "Đã liên hệ" (dialog) trên danh sách vắng, thẻ "Chăm sóc lại" + timeline trên hồ sơ; gợi ý ngày hẹn (không nghe máy +3 như server, hẹn quay lại/khác +7, không nhu cầu: không hẹn). Mặc định danh sách vắng = **Cần liên hệ** + mọi khách (lẻ + liệu trình).
- **Trùng SĐT khi gõ** (debounce 400 ms, chỉ số đủ 10 chữ số), 409 lúc lưu giữ nguyên. **Gói không hạn** = "Không hạn" mọi nơi. **Giới tính** (`CustomerName`) + **danh mục** (`serviceLabel`) ở bảng gói, khách vắng, sổ doanh thu, thẻ khách quầy, lượt hôm nay, giỏ, lịch sử, mục gói, top dịch vụ.
- **Cache:** `lib/invalidate.ts` `invalidateManagementViews` sau ghi/hủy lượt, bán/hủy/gia hạn gói (trước đây báo cáo không làm mới); follow-up invalidate danh sách vắng + hồ sơ.
- **Perf:** JS khởi động 122.9 KB gzip -9 (ngân sách 125; shell đo 119.2), phần thêm vào đường khởi động chỉ là helper nhỏ (phone-check, invalidate, serviceLabel/Không hạn, danh sách preset cho router), một file `index-*.js`; `period.ts` (Intl, tính kỳ) để trong chunk lazy, router chỉ import `search.ts`. Chunk: `ReportsPage` 7.0 KB, `InactiveCustomersPage` 2.5 KB, `FollowUpForm` 2.0 KB, `filters` 1.2 KB, `period` 1.2 KB (gzip). Không thêm việc vào lần vẽ đầu của `/quay` (invalidation chỉ đánh dấu stale).
- **Tests:** vitest 139/139 (mới: kỳ + nhãn ngày, URL state, sổ doanh thu, ẩn tiền thêm các chỗ mới, follow-up GET/POST + gợi ý + idempotency, lọc khách vắng, tìm gói, gói không hạn, lịch sử có lọc, trùng SĐT debounce, invalidation). E2E mới `e2e/specs/management.spec.ts` (+ `support/db.ts` `backdateCustomer`). Ảnh 360/768/1280/1440 light/dark: scratchpad `ui3-features/`.
- Câu hỏi EM: (1) gợi ý ngày hẹn +7 cho "Hẹn sẽ quay lại"/"Khác" (server chỉ mặc định +3 cho không nghe máy) có hợp không? (2) Sổ doanh thu mỗi lần tối đa 3 tháng (giới hạn API) có đủ? (3) Bảng giá có cần ô tìm không (hiện chưa)?
## [2026-11-05] ingest + record | UI vòng 3 — shell: logo nguyên vẹn, điều hướng, URL state, focus, kích thước, menu tài khoản
- PR #31. Source: [raw/2026-11-05-em-ui-review-round3.md](raw/2026-11-05-em-ui-review-round3.md). Agent shell nhận mục 1, 5, 7 (phía shell), 8, 9, 10 (hạ tầng), 11, 14. Branch `feat/ui-shell-r3` từ `feat/ui-overhaul`, merge `origin/main` (#29 squash 6a427a5) bằng merge commit. Skip stage: không spec riêng (visual + điều hướng); quyết định điều hướng ghi ở [ADR-0007](wiki/decisions/ADR-0007-navigation-and-url-state.md).
- Logo: dùng **toàn bộ** logo gốc ở mọi nơi (sidebar 104/52px, top bar điện thoại, drawer, khung chào, empty state, favicon + PWA); xóa `mark.svg`/`wordmark.svg`; `BrandLogo` + token `--brand-ink` (contrast test thêm 4 cặp). Đăng nhập bỏ dòng tagline chữ lặp. Script `build_logo_assets.py` chỉ còn emblem + icon (bỏ tách connected component).
- Điều hướng: `PageHeader` (breadcrumb ≥ 640px, nút quay lại theo lịch sử có tên màn trước, fallback về cha), `app/trail.ts` (sessionStorage theo `__TSR_index`), `scrollRestoration`. Áp vào khách hàng, hồ sơ, theo dõi gói, khách vắng, báo cáo, bảng giá, gói bán, cài đặt. `reports/ui` re-export `PageHeader`.
- URL state: `lib/searchState.ts` (`searchSchema`, `param.*`, `useSearchState`); `/goi-lieu-trinh?status`, `/khach-lau-khong-quay-lai?days&scope`, `/bao-cao?date`, `/khach-hang?q`. Quy ước state ghi ở [frontend § State management](wiki/architecture/frontend.md#state-management); sửa theme picker trùng state (store chung).
- `CustomerName` (tên + chip Nữ/Nam): tìm khách, khách gần đây, tiêu đề hồ sơ, ô khách bảng báo cáo (khi dòng có gender — dòng overview/inactive chưa có, cần backend).
- Menu tài khoản thay chip avatar: tên, vai trò (suy từ permission, chỉ hiển thị), spa, giao diện, Khóa nhanh (quay lại màn đang mở sau đăng nhập), Đăng xuất.
- Focus ring: ô nhập 2px sát viền, bỏ quầng, nhãn cách 6px; control xếp sát vẽ ring bên trong; link chữ 2px. Kích thước: StatCard bỏ min-h 152px (→ ~92px), khung chào gọn, lối tắt 72px, empty state 32px, bảng điện thoại không bẻ chữ. Size scale ghi ở design-system.
- Tests: vitest 125 (mới: `searchState.test.ts`, `trail.test.ts`, `Navigation.test.tsx` list → hồ sơ → quay lại giữ bộ lọc, deep link về cha, lọc replace không push, `q` từ URL, `CustomerName`; menu tài khoản, role label, theme picker dùng chung); contrast 81. E2E `layout.spec.ts` quét 320→1920 bước 80 + zoom 200 %, chồng lấn anh em, chữ bị cắt, focus ring đè chữ; mutation check: bản #29 bị bắt đúng lỗi ring ô tìm khách của EM.
- Perf: critical JS gzip -9 117.5 → 119.2 KB (ngân sách 125); CSS 10.5 → 11.2 KB; ảnh logo shell 6.1 → 11.3 KB (một file, dùng chung với đăng nhập).
- Câu hỏi: thêm `role` vào `/me`? thêm `customer_gender` vào dòng overview/inactive/history? "Khóa nhanh" giờ ở trong menu (2 chạm thay 1) — EM có muốn giữ nút 1 chạm riêng không? File logo vector gốc?
## [2026-11-05] ingest + record | EM trả lời Q5–Q7: báo cáo dài cho chủ spa, mặc định follow-up
- Source: [raw/2026-11-05-em-answers-q5-q7.md](raw/2026-11-05-em-answers-q5-q7.md). Branch `feat/followup-defaults-long-range` từ `main` sau #33.
- Q5: quyền mới `report.long_range` (chủ spa, migration **0015**): tháng/năm tới 1830 ngày, preset `last_3_years` / `last_5_years`; người khác vẫn 2 năm (`422 to range_too_long`). Tính tại chỗ, ngân sách bench riêng p95 ≤ 1,5 s (đo 368–478 ms).
- Q6 (lead chốt, EM sẽ review): `no_answer` +3, `will_come` +7, `not_interested` không ngày + nghỉ 90 ngày khỏi `follow_up=due` rồi hiện lại một lần (`due_on` mới trên mỗi dòng khách vắng), `other` không ngày; ngày nhập tay luôn thắng. `GET /customers/{id}/follow-ups` trả `today`, `suggested_next_on_by_outcome`, `not_interested_pause_days` để UI điền sẵn trước khi lưu.
- Q7: KTV không follow-up (đã đúng); thêm assert trong `roles.rs`.
- Tests: `follow_ups.rs` (mặc định +7, gợi ý, F hết 90 ngày nghỉ → due lại, E vừa nói không muốn → không due, `due_on`; chủ spa 3/5 năm OK, quản lý 422), unit (gợi ý theo kết quả, preset chủ spa bị từ chối khi thiếu quyền), bench +3 shape với ngân sách 1,5 s.
## [2026-11-05] ingest + spec + build + record | Vòng 4 mục 1, 5, 6, 8: lịch hẹn, giờ làm + nhập bù, lượt hôm nay, giảm giá lượt
- Source: [raw/2026-11-05-em-review-round4.md](raw/2026-11-05-em-review-round4.md) (chép từ branch `docs/em-round4-note`) + [raw/2026-11-05-em-simplify-owner-and-opening-balance.md](raw/2026-11-05-em-simplify-owner-and-opening-balance.md). Branch `feat/appointments-front-desk` từ `origin/main` (#35), merge `origin/feat/ui-polish-mobile` (d5ea367, component mobile) bằng merge commit. Stage: spec → backend/frontend design (ghi thẳng vào wiki) → build → qa → secure → perf → pr-review; không có breakdown riêng (một slice).
- Spec mới [appointments](wiki/product/specs/appointments.md) (draft, lead chốt mặc định, Q1–Q8 cho EM). Migration **0016** (`appointment`, `appointment_service`, `EXCLUDE` trùng KTV, trigger final, `visit_line.discount_amount` + CHECK mới, quyền `appointment.manage` owner+manager, `visit.backdate` owner). `kernel/build.rs` để migration mới luôn được nhúng.
- Backend: `operations::appointments` (8 route), `operations::service::record_in` = thân `POST /visits` trong transaction của người gọi (hoàn tất lịch hẹn dùng lại, không chép quy tắc); `domain::service_time` (giờ làm, nhập bù ≤ 366 ngày); `sales::service::record_service_sale` nhận giảm giá (D10) và trả phần chia từng dòng; `org::service::active_in`, `crm::service::phone_holder_in`.
- Web: `/quay` có tab Ghi lượt / Lịch hẹn (URL state), `DayCountdown`, thông tin khách inline dưới tên, giờ làm + nhập bù (chủ spa), giảm giá dịch vụ lẻ, lượt hôm nay toàn chiều rộng mở inline + link hồ sơ, `DateField` thay `type=date`. Lịch: lưới ngày theo KTV + chip chỗ trống, tuần, danh sách (điện thoại), `BottomSheet` chi tiết/đặt lịch, giỏ điền sẵn khi hoàn tất. Startup JS 123.0 → 124.7 KB gzip -9 (tách TodayVisits / CartOptions / prefill thành chunk lazy).
- Tests: `app/tests/appointments.rs` (7), matrix +8 dòng, roles, domain unit; vitest +13 (161); e2e `appointments.spec.ts` (+ ảnh chụp khi đặt `R4_SHOTS_DIR`), `layout.spec.ts` thêm `/quay?view=lich`.
