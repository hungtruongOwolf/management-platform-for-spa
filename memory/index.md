# Wiki Index

Catalog of every page in `wiki/`. One line per page: link — what it answers. Keep in sync on every ingest/record.

## Overview
- [overview](wiki/overview.md) — what the project is, current stage, open questions

## Product
- [vision](wiki/product/vision.md) — vision, access channels (web/PWA, native-app-ready), release scope R1–R3
- [paper-books-analysis](wiki/product/paper-books-analysis.md) — 4 paper books today → enter-once event model, auto reminders
- [compensation-rules](wiki/product/compensation-rules.md) — attendance, evaluation, salary, commission, Tết/loyalty bonus rules + 8 open contradictions
- [glossary](wiki/product/glossary.md) — ubiquitous language VI ↔ code terms (draft); **tên màn hình/nav chốt vòng 4** (Mẫu gói liệu trình, Gói liệu trình của khách, Báo cáo kinh doanh…)
- [service-catalog](wiki/product/service-catalog.md) — real menu: 35 services, 10 categories, female/male pricing (seed data)
- [personas](wiki/product/personas.md) — operating context (1 branch, 3–8 KTV, manager-cashier) + 5 personas
- [spec R1-1 front-desk-and-packages](wiki/product/specs/front-desk-and-packages.md) — **approved**: catalog, customers, sell package, record visit, payments/receivables, void, package lifecycle, opening balance (AC8.5 dropped 2026-11-05); **US6b Sửa lượt** (2026-11-06, lead's decision — EM review: sửa trực tiếp KTV/giờ/ghi chú, đụng tiền/buổi = hủy + ghi lại `POST /visits/{id}/correct`, Q-E1/Q-E2); **§10 vòng 6 (approved 2026-11-06, §10.8 quyết định EM)**: bán gói trong giỏ + dùng buổi hôm nay, "Tính buổi hôm nay vào gói" (tùy case, không tự động), thêm dịch vụ vào lượt, một người giới thiệu / chốt mỗi gói và mỗi dòng lẻ (hoặc khách tự chọn), hoa hồng upsell = gói + dịch vụ lẻ, mua cho người khác, người đi cùng dạng ghi chú, giỏ nháp; Q-U7 công nợ để sau
- [spec staff-management](wiki/product/specs/staff-management.md) — **draft**: hồ sơ + lương, chấm công ✓/P/ĐX/K, đánh giá ngày + tháng 100 điểm, thưởng/phạt chỉ thêm, bảng lương + phiếu lương + chốt tháng khóa sổ, KTV xem tháng của mình; 8 mâu thuẫn → mặc định + quy tắc chỉnh được; Q-S1…S4, Q-C1…C8; **vòng 5 (§10)**: sổ hằng ngày một trang + tiến độ, bảng công tháng dạng lưới, đánh giá tháng nhắc khi đến hạn, KTV "Khách của tôi" (`GET /me/work`)
- [spec R1-2 management-views](wiki/product/specs/management-views.md) — **draft**: dashboard Hôm nay, range report, revenue book, customer history/summary/package page, packages overview, re-engagement; no "sổ cũ" in product (CLI import); thu chi later; open Q1–Q4; **vòng 3**: follow-up khách vắng (US5b, `customer.follow_up`), phạm vi khách lẻ + liệu trình, tìm lịch sử/gói, check trùng SĐT, preset báo cáo + theo năm, `category_name`

- [spec R1-3 appointments](wiki/product/specs/appointments.md) — **draft (vòng 4 + 5)**: lịch hẹn booked → arrived → done (= lượt khách qua đúng use case), no-show/hủy; chặn trùng KTV; giờ làm + nhập bù của chủ spa (`visit.backdate`); giảm giá dịch vụ lẻ trong lượt (D10); đồng hồ đếm ngược; **vòng 5 (§10)**: lịch = dòng thời gian (lượt vãng lai + phase Chưa đến/Đang làm/Xong), thời lượng theo dịch vụ, đặt nhóm/gia đình (`POST /appointments/group`, `group_id`) + khách đến cả nhóm, bỏ sức chứa theo số KTV → "số khách tối đa" tùy chọn của chủ spa (`appointment.settings`, chỉ cảnh báo), nguyên nhân "Đặt lịch không phản hồi"; câu hỏi EM Q1–Q12; **vòng 6 (§10.9)**: lưới ngày = lớp sự kiện (thẻ không thay ô), dịch vụ kèm (trống thời lượng) không cộng giờ, xem theo KTV (`ktv`), Đặt lịch lên thanh công cụ

## Design
- [design-system](wiki/design/design-system.md) — **§ Quy tắc (rulebook redesign 2026-11-06)**, đã áp cho khách hàng/gói/danh mục (§ Trước / sau, `packageState` một trạng thái + "Sắp hết"): thang chữ `--fs-*`, khoảng cách, màu theo vai trò, nút phẳng 4 kiểu × 7 trạng thái, ô 16px, không card lồng/sọc, một bộ chip, trống/tải/lỗi, motion 150–250 ms; "Trà thất Đông y" palette (nâu đỏ/vàng/trắng/kem): tokens + contrast table (auto-tested), logo thật (trace từ PNG), app shell (sidebar/top bar/tab bar/palette), component classes, motion budget, before/after; vòng 4: **quy tắc viết hoa/định dạng ngày**, **mobile patterns + API** (`ResponsiveTable`, `BottomSheet`, `StickyActions`, `ResponsiveFilters`, `DateField`), favicon tab
- [front-desk-flows](wiki/design/front-desk-flows.md) — **redesign pilot /quay 2026-11-06** (gói một trạng thái + disclosure, chọn-là-thêm, checkout dính đáy, "Bỏ liên kết lịch hẹn"); screen flows US2–US8, ≤4-action fast path, all states; vòng 4: tab Lịch hẹn (ngày/tuần/danh sách, sheet, màu trạng thái), giờ làm/nhập bù, giảm giá, lượt hôm nay mở inline; **redesign Lịch hẹn 2026-11-06** (chip `.badge` theo phase, không sọc/không card lồng, trống/tải/lỗi, "+ Người đi cùng")

## Architecture
- [system](wiki/architecture/system.md) — R1 architecture: quality ranking, C4 diagrams, trust boundaries, data flows, failure modes, cost
- [frontend](wiki/architecture/frontend.md) — SPA routes, data fetching, perf budget (single startup chunk), auth/permission-aware UI, PWA, folder layout; management screens: report periods + ledger, follow-ups, search/filter analysis per page, query keys + invalidation
- [backend](wiki/architecture/backend.md) — R1-1 API contract + §3b R1-2 report endpoints (shapes, money rules, p95 bench) (endpoints, errors, idempotency, ETag), use cases + integrity strategies, layering, jobs, AC trace
- [data](wiki/architecture/data.md) — R1-1 Postgres schema + R1-2 read models (0013: package_status, revenue_book, day indexes) + 0014 customer_follow_up, search_fold + ER, tenant FKs, RLS, constraints, indexes, migrations, seed
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
- [ADR-0008 payroll close snapshots](wiki/decisions/ADR-0008-payroll-close-snapshots.md) — **proposed**: chốt lương một lần → phiếu lương jsonb chỉ thêm, khóa tháng 2 lớp, không mở lại, sửa bằng điều chỉnh tháng sau

## Security
- [authz-matrix](wiki/security/authz-matrix.md) — R1-1 + R1-2 (report.read, report.revenue.read) + 0016 staff (staff.manage, attendance.record, evaluation.record, payroll.read/close, staff.read_own) permissions × roles, re-auth, support grants, mandatory authz tests
- [threat-model](wiki/security/threat-model.md) — STRIDE per trust boundary for R1, with tests

## Plans
- [r1-1-front-desk](wiki/plans/r1-1-front-desk.md) — 12 slices S0–S11 (foundation → kernel → auth → tracer bullet catalog → … → staging); S10 block holds the AC → test coverage table
- [r1-2-management-views](wiki/plans/r1-2-management-views.md) — M1–M10: backend contract/migration/CLI/tests done; UI screens, drop /nhap-so-cu, then delete POST /opening-balances
- [r1-6-upsell](wiki/plans/r1-6-upsell.md) — vòng 6: phần A (0021, `package_sales` trong giỏ, `additions`, `convert-to-package`, lương upsell, báo cáo, hooks web) đã build; phần B giao diện giỏ chờ PR #56 (redesign pilot)
- [r1-1-trial-session](wiki/plans/r1-1-trial-session.md) — S10 buổi dùng thử với quản lý: setup máy dev, 15 nhiệm vụ quầy, bấm giờ (< 30 s/lượt), phiếu góp ý

## Runbooks
- [staging-setup](wiki/runbooks/staging-setup.md) — S11: chọn VPS (giá + nguồn, ghép vùng với Supabase), hardening, Supabase checklist (REVOKE khi Data API bật, pooler session mode), R2 backup + restore drill, deploy/rollback, việc EM cần làm
- [vercel-staging](wiki/runbooks/vercel-staging.md) — staging hiện tại: Vercel Services (web tĩnh + API container, pdx1) + Supabase us-west-2; env vars, bootstrap DB, dữ liệu demo (`app seed-demo`), deploy, kiểm tra, việc trước khi dùng thật
- [local-merge-gate](wiki/runbooks/local-merge-gate.md) — merge khi GitHub Actions tắt: `merge-local-gate.sh` theo thay đổi (phân loại file → bước), `--dry-run`/`--full`/`--batch`, build Rust một lần, khi nào tin gate chọn lọc (`--full` trước deploy), contract rỗng

## Research
- [tech-stack](wiki/research/tech-stack.md) — backend & frontend language/framework comparison, recommendation
- [code-intelligence](wiki/research/code-intelligence.md) — LSP/Serena/code-graph tools for agent impact analysis
- [hosting-storage](wiki/research/hosting-storage.md) — where to store DB, files, video long-term; Supabase deal math; costs; VN cross-border data law

## Conventions
- [coding-standards](wiki/conventions/coding-standards.md) — clean-code rules for Rust & TypeScript, git conventions
- [testing](wiki/conventions/testing.md) — test layers; add a route to the authz matrix test; write an E2E/axe spec; p95 + Lighthouse; run locally/CI; CI job graph (nextest shards, e2e split by duration, scope skipping, `CI ok`)
