# Wiki Index

Catalog of every page in `wiki/`. One line per page: link — what it answers. Keep in sync on every ingest/record.

## Overview
- [overview](wiki/overview.md) — what the project is, current stage, open questions

## Product
- [vision](wiki/product/vision.md) — vision, access channels (web/PWA, native-app-ready), release scope R1–R3
- [paper-books-analysis](wiki/product/paper-books-analysis.md) — 4 paper books today → enter-once event model, auto reminders
- [compensation-rules](wiki/product/compensation-rules.md) — attendance, evaluation, salary, commission, Tết/loyalty bonus rules + 8 open contradictions
- [glossary](wiki/product/glossary.md) — ubiquitous language VI ↔ code terms (draft)
- [service-catalog](wiki/product/service-catalog.md) — real menu: 35 services, 10 categories, female/male pricing (seed data)
- [personas](wiki/product/personas.md) — operating context (1 branch, 3–8 KTV, manager-cashier) + 5 personas
- [spec R1-1 front-desk-and-packages](wiki/product/specs/front-desk-and-packages.md) — **approved**: catalog, customers, sell package, record visit, payments/receivables, void, package lifecycle, opening balance (AC8.5 dropped 2026-11-05)
- [spec R1-2 management-views](wiki/product/specs/management-views.md) — **draft**: dashboard Hôm nay, range report, revenue book, customer history/summary/package page, packages overview, re-engagement; no "sổ cũ" in product (CLI import); thu chi later; open Q1–Q4

## Design
- [design-system](wiki/design/design-system.md) — "Trà thất Đông y" palette (nâu đỏ/vàng/trắng/kem): tokens + contrast table (auto-tested), logo thật (trace từ PNG), app shell (sidebar/top bar/tab bar/palette), component classes, motion budget, before/after
- [front-desk-flows](wiki/design/front-desk-flows.md) — screen flows US2–US8, ≤4-action fast path, all states

## Architecture
- [system](wiki/architecture/system.md) — R1 architecture: quality ranking, C4 diagrams, trust boundaries, data flows, failure modes, cost
- [frontend](wiki/architecture/frontend.md) — SPA routes (incl. management views + data flag), data fetching, perf budget (single startup chunk), auth/permission-aware UI, PWA, folder layout
- [backend](wiki/architecture/backend.md) — R1-1 API contract + §3b R1-2 report endpoints (shapes, money rules, p95 bench) (endpoints, errors, idempotency, ETag), use cases + integrity strategies, layering, jobs, AC trace
- [data](wiki/architecture/data.md) — R1-1 Postgres schema + R1-2 read models (0013: package_status, revenue_book, day indexes) + ER, tenant FKs, RLS, constraints, indexes, migrations, seed
- [principles](wiki/architecture/principles.md) — extensibility rules: modular monolith, permissions-as-data, tenant/branch, versioned policies, pure calculators

## Decisions (ADR)
- [_template](wiki/decisions/_template.md) — ADR format
- [ADR-0001 tech stack](wiki/decisions/ADR-0001-tech-stack.md) — **accepted**: Rust/Axum + Postgres; internal app = Vite React SPA; public site later (Next/Astro); no Redis yet
- [ADR-0002 database hosting](wiki/decisions/ADR-0002-database-hosting.md) — **accepted (provisional)**: Supabase Pro as plain Postgres, Singapore, with mandatory lockdown; staging exception: US region + Data API on, mitigated by REVOKE
- [ADR-0003 multi-tenancy](wiki/decisions/ADR-0003-multi-tenancy.md) — **accepted**: shared schema + tenant_id + RLS, TenantId newtype
- [ADR-0004 authentication](wiki/decisions/ADR-0004-authentication.md) — **accepted**: Argon2id + opaque Postgres sessions, __Host- cookie, MFA owner/admin; re-auth only for account-security actions (2026-11-05)
- [ADR-0005 compute hosting](wiki/decisions/ADR-0005-compute-hosting.md) — **accepted**: 1 Singapore VPS, Docker Compose + Caddy (provider at /devops)
- [ADR-0006 repo & module layout](wiki/decisions/ADR-0006-repo-and-module-layout.md) — **accepted**: monorepo, Cargo workspace crate per bounded context
- [ADR-0007 navigation & URL state](wiki/decisions/ADR-0007-navigation-and-url-state.md) — **accepted**: breadcrumb = cây, nút quay lại = lịch sử có tên đích (fallback về cha), trạng thái danh sách trên URL (`useSearchState`)

## Security
- [authz-matrix](wiki/security/authz-matrix.md) — R1-1 + R1-2 (report.read, report.revenue.read) permissions × roles, re-auth, support grants, mandatory authz tests
- [threat-model](wiki/security/threat-model.md) — STRIDE per trust boundary for R1, with tests

## Plans
- [r1-1-front-desk](wiki/plans/r1-1-front-desk.md) — 12 slices S0–S11 (foundation → kernel → auth → tracer bullet catalog → … → staging); S10 block holds the AC → test coverage table
- [r1-2-management-views](wiki/plans/r1-2-management-views.md) — M1–M10: backend contract/migration/CLI/tests done; UI screens, drop /nhap-so-cu, then delete POST /opening-balances
- [r1-1-trial-session](wiki/plans/r1-1-trial-session.md) — S10 buổi dùng thử với quản lý: setup máy dev, 15 nhiệm vụ quầy, bấm giờ (< 30 s/lượt), phiếu góp ý

## Runbooks
- [staging-setup](wiki/runbooks/staging-setup.md) — S11: chọn VPS (giá + nguồn, ghép vùng với Supabase), hardening, Supabase checklist (REVOKE khi Data API bật, pooler session mode), R2 backup + restore drill, deploy/rollback, việc EM cần làm
- [vercel-staging](wiki/runbooks/vercel-staging.md) — staging hiện tại: Vercel Services (web tĩnh + API container, pdx1) + Supabase us-west-2; env vars, bootstrap DB, dữ liệu demo (`app seed-demo`), deploy, kiểm tra, việc trước khi dùng thật

## Research
- [tech-stack](wiki/research/tech-stack.md) — backend & frontend language/framework comparison, recommendation
- [code-intelligence](wiki/research/code-intelligence.md) — LSP/Serena/code-graph tools for agent impact analysis
- [hosting-storage](wiki/research/hosting-storage.md) — where to store DB, files, video long-term; Supabase deal math; costs; VN cross-border data law

## Conventions
- [coding-standards](wiki/conventions/coding-standards.md) — clean-code rules for Rust & TypeScript, git conventions
- [testing](wiki/conventions/testing.md) — test layers; add a route to the authz matrix test; write an E2E/axe spec; p95 + Lighthouse; run locally/CI
