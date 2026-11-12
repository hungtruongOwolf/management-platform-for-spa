# Wiki Index

Catalog of every page in `wiki/`. One line per page: link — what it answers. Keep in sync on every ingest/record. Lịch sử thay đổi của từng trang nằm trong chính trang đó và trong [log](log.md), không ở đây.

## Overview
- [overview](wiki/overview.md) — dự án là gì, đang ở giai đoạn nào, đã build gì, câu hỏi còn chờ EM
- [docs/ (ngoài wiki)](../docs/README.md) — bản đọc nhanh cho người mới, link vào wiki: [system-design](../docs/system-design.md) (thành phần, vòng đời request, bảo mật, triển khai, job nền, perf budget), [database-design](../docs/database-design.md) (nhóm bảng + ER, RLS/definer, toàn vẹn, mã hóa, index, migration theo dải), [features](../docs/features.md) (theo vai trò + khu vực, cờ tính năng, `/admin`); wiki vẫn là nguồn sự thật

## Product
- [vision](wiki/product/vision.md) — vision, kênh truy cập (web/PWA, sẵn sàng native), phạm vi R1–R3 gốc + ghi chú phần đã build / chưa build
- [paper-books-analysis](wiki/product/paper-books-analysis.md) — 4 sổ giấy hiện tại → mô hình "nhập một lần, mọi sổ tự sinh" + nhắc nhở tự động
- [compensation-rules](wiki/product/compensation-rules.md) — quy chế chấm công, đánh giá 100 điểm, lương, hoa hồng, thưởng Tết/gắn bó theo tài liệu spa + 8 mâu thuẫn chờ EM
- [glossary](wiki/product/glossary.md) — ngôn ngữ chung VI ↔ code (bảng/field thật, phần chưa build có đánh dấu) + tên màn hình / nav / route hiện hành
- [service-catalog](wiki/product/service-catalog.md) — menu thật: 35 dịch vụ, 10 nhóm, giá Nữ/Nam (seed data bảng giá)
- [personas](wiki/product/personas.md) — bối cảnh vận hành (1 cơ sở, 3–8 KTV, quản lý kiêm thu ngân) + 5 persona
- [spec R1-1 front-desk-and-packages](wiki/product/specs/front-desk-and-packages.md) — **approved**: Quầy + gói liệu trình: ghi lượt, bán / trừ gói, hủy / Sửa lượt, upsell trong lượt và giỏ bán gói (§10), khách vãng lai + danh tính khách (§10.11), ghi chú buổi / ghi chú gói; AC + permission
- [spec R1-2 management-views](wiki/product/specs/management-views.md) — **draft (đã build)**: Tổng quan hôm nay, báo cáo theo kỳ, sổ doanh thu, lịch sử / gói của khách, khách lâu chưa quay lại + gọi lại, Nhật ký ghi chú (16 loại, mỗi loại giữ quyền đọc của màn gốc), Nhật ký thao tác §11 (chỉ chủ spa, đọc `audit_log`, một thao tác một dòng §11.3b); sổ cũ chỉ qua CLI
- [spec R1-3 appointments](wiki/product/specs/appointments.md) — **draft (đã build)**: lịch hẹn booked → arrived → done, chặn trùng KTV / khách / giờ đã qua, lịch = dòng thời gian quầy, đặt nhóm, số khách tối đa (chỉ cảnh báo), nhập bù, giảm giá dịch vụ lẻ
- [spec staff-management](wiki/product/specs/staff-management.md) — **draft (đã build)**: hồ sơ + đợt làm việc, chấm công, khen/phàn nàn, đánh giá tháng, thưởng/phạt, bảng lương + chốt tháng, ai ghi cho ai (quyết định C), `/me/*` của KTV và người đã nghỉ, tài khoản + mật khẩu (§17)
- [spec record-history](wiki/product/specs/record-history.md) — **approved; đợt 1 (1a + 1b) đã build và lên staging 2026-11-12, đợt 2 (lịch hẹn, chấm công, khen / phàn nàn, thưởng / phạt) đã build và lên staging, đợt 3 (bảng giá, mẫu gói, nhóm dịch vụ, quy tắc lương, số khách tối đa) đã build; việc còn lại phía web (form "Sửa nhóm", sheet ô ngày chỉ xem, một request mỗi lần mở, chữ "6 → không giới hạn") đã làm**: "Ghi bởi Hà · 15:10 · đã sửa 1 lần" + lịch sử trên từng bản ghi, bảng 64 action (37 có / 27 không), "Đã sửa" hẹp + chip "Đã thay", công tắc spa-local chỉ ở `/admin`, ẩn Nhật ký thao tác từ đợt 1, AC, edge case, review log 4 lượt (gồm cổng cuối) + quyết định khi build K1–K11 (1a), K12–K20 (1b), K21–K36 (đợt 2), K37–K47 (đợt 3), K48–K55 (việc còn lại)
- [spec platform-admin](wiki/product/specs/platform-admin.md) — **draft (đã build #111–#113)**: console operator: đăng nhập riêng + MFA + step-up, spa (tạo / tạm ngưng), công tắc MFA chủ spa (US10, ADR-0020), 6 cờ tính năng, mẫu vai trò, tài khoản xuyên spa, nhật ký nền tảng + spa (chỉ metadata); Q-PA1…4

## Design
- [design-system](wiki/design/design-system.md) — design system "Trà thất Đông y": rulebook, token màu / chữ / control (`--fs-*`, `--size-control`, `--radius-control`), app shell + nav + tab bar, mobile patterns + API (`ResponsiveTable`, `BottomSheet`, `DateField`, `TimeField`, `TimeGrid`), class dùng chung, định dạng ngày/giờ
- [record-history (design)](wiki/design/record-history.md) — UI lịch sử trên từng bản ghi: chia module theo ngân sách JS, `RecordTrace` (phần tử cuối, `data-row-inert`), nút mở có sẵn ở màn nhiều bản ghi, chip Đã thay / Đã hủy / Đã sửa loại trừ nhau, wireframe desktop + 360 px từng màn, mọi chữ tiếng Việt, panel admin + cờ "Nhật ký thao tác"
- [front-desk-flows](wiki/design/front-desk-flows.md) — luồng màn Quầy `/quay` theo từng đợt EM review: đường nhanh ghi lượt, giỏ (gập món, bán gói, giỏ nháp), Sửa / Hủy lượt, tab Lịch hẹn + sheet; phần đã thay được đánh dấu tại chỗ

## Architecture
- [system](wiki/architecture/system.md) — kiến trúc gốc R1: thứ tự thuộc tính chất lượng, C4, trust boundary, data flow, failure mode, chi phí (phần hosting lỗi thời có banner, bản hiện hành: docs/system-design)
- [frontend](wiki/architecture/frontend.md) — kiến trúc SPA: route map (`router.tsx` + `mayOpen`), query key + invalidation, trạng thái trên URL (`useSearchState`), form, perf budget, auth UI, CSP, PWA, cấu trúc thư mục, `TimeGrid` dùng chung
- [backend](wiki/architecture/backend.md) — hợp đồng API (endpoint, lỗi, idempotency, ETag), báo cáo R1-2 (shape, quy tắc tiền, p95), use case + chiến lược toàn vẹn, layering, job nền
- [data](wiki/architecture/data.md) — schema Postgres theo migration: bảng, FK kèm tenant, RLS, ràng buộc, index, read model (`package_status`, `revenue_book`), quy ước migration + seed
- [record-history (backend)](wiki/architecture/record-history.md) — `record.history.read` spa-local + hàm `platform_set_record_history`, cờ mặc định tắt (`default_enabled`), 13 route `GET /record-history/*` (đã build cả 13; đổi neo rồi kiểm quyền loại neo, thành viên theo id dòng audit, gộp chỉ lượt / gói + dòng sửa thưởng / phạt, ô ngày theo `date`, lọc quyền), "tính là sửa" một định nghĩa, `edited`, index 0052, bench audit thật, thứ tự deploy
- [platform-admin](wiki/architecture/platform-admin.md) — thiết kế console nền tảng: crate `platform`, bảng `platform_*` / `tenant_feature` / `role_template_permission`, hàm definer có cổng token, API `/api/v1/admin/*`, thay đổi ở đường xác thực spa, ứng dụng con web `/admin`
- [performance-latency-2026-11](wiki/architecture/performance-latency-2026-11.md) — báo cáo đo staging 2026-11-07: mạng VN↔Oregon chiếm > 80 % mỗi request, cold start, phương án vùng/host có giá; việc giai đoạn 0 đã làm; staging ở Mỹ, production châu Á
- [db-review-2026-11](wiki/architecture/db-review-2026-11.md) — báo cáo rà soát DB 2026-11-08: dung lượng 3 năm, câu nóng, nút thắt lượt đi về API↔DB, đề xuất A1–A4 / B1 / C1–C4 (đã làm ở 0033–0037, ADR-0017), danh sách không được đổi
- [principles](wiki/architecture/principles.md) — quy tắc mở rộng: modular monolith, permission là dữ liệu, tenant/branch, chính sách có phiên bản, calculator thuần

## Decisions (ADR)
- [_template](wiki/decisions/_template.md) — ADR format
- [ADR-0001 tech stack](wiki/decisions/ADR-0001-tech-stack.md) — **accepted**: Rust/Axum + Postgres; app nội bộ = Vite React SPA; site công khai sau (Next/Astro); không Redis; hiện trạng: không shadcn/Motion/service worker
- [ADR-0002 database hosting](wiki/decisions/ADR-0002-database-hosting.md) — **accepted (provisional)**: Supabase Pro dùng như Postgres thuần, Singapore, khóa chặt bắt buộc; ngoại lệ staging: vùng Mỹ + Data API bật, giảm thiểu bằng REVOKE
- [ADR-0003 multi-tenancy](wiki/decisions/ADR-0003-multi-tenancy.md) — **accepted**: shared schema + `tenant_id` + RLS, `TenantId` newtype; support grant chưa làm
- [ADR-0004 authentication](wiki/decisions/ADR-0004-authentication.md) — **accepted**: Argon2id + session opaque trong Postgres, cookie `__Host-`, MFA cho chủ spa; re-auth chỉ cho thao tác bảo mật tài khoản; tài khoản/mật khẩu nay theo ADR-0016
- [ADR-0005 compute hosting](wiki/decisions/ADR-0005-compute-hosting.md) — **accepted cho production (VPS Singapore, chưa mua)**; staging thay bằng Vercel + Supabase (update 2026-11-05); production đang chờ ADR-0012
- [ADR-0006 repo & module layout](wiki/decisions/ADR-0006-repo-and-module-layout.md) — **accepted**: monorepo, Cargo workspace mỗi bounded context một crate; danh sách crate thật
- [ADR-0007 navigation & URL state](wiki/decisions/ADR-0007-navigation-and-url-state.md) — **accepted**: breadcrumb = cây, nút quay lại = lịch sử có tên đích (fallback về cha), trạng thái danh sách trên URL (`useSearchState`)
- [ADR-0008 payroll close snapshots](wiki/decisions/ADR-0008-payroll-close-snapshots.md) — **accepted (đã làm ở 0017, EM to review)**: chốt lương một lần → phiếu lương jsonb chỉ thêm, khóa tháng 2 lớp, không mở lại, sửa bằng điều chỉnh tháng sau
- [ADR-0009 served companions](wiki/decisions/ADR-0009-served-companions.md) — **accepted**: người đi cùng được làm = lịch hẹn trong nhóm; nhóm chỉ ghi "đi cùng nhau", mỗi người phục vụ riêng
- [ADR-0010 commission follows kept money](wiki/decisions/ADR-0010-commission-follows-kept-money.md) — **accepted (EM to review)**: gói hủy có hoàn tiền → trừ tiền hoàn khỏi cơ sở hoa hồng chốt ở tháng hoàn; hủy không hoàn giữ nguyên
- [ADR-0011 anonymous walk-in + optional phone](wiki/decisions/ADR-0011-anonymous-walk-in-and-optional-phone.md) — **accepted**: khách vãng lai không tên = hồ sơ không tên (`app.customer_label`), SĐT tùy chọn, ngày sinh, chủ gói phải có tên, sổ gia hạn (0023)
- [ADR-0012 colocate API + DB Singapore](wiki/decisions/ADR-0012-colocate-api-db-singapore.md) — **proposed (production)**: API + DB cùng Singapore, API luôn chạy; staging ở lại Mỹ (EM 2026-11-07)
- [ADR-0013 employment periods](wiki/decisions/ADR-0013-employment-periods.md) — **accepted (EM to review)**: quá trình làm việc = danh sách đợt (`staff_employment`, 0025), nghỉ việc là hủy mềm, nhận lại = đợt mới, thâm niên cộng các đợt
- [ADR-0014 former staff login](wiki/decisions/ADR-0014-former-staff-login.md) — **accepted (Q-S6)**: người đã nghỉ vẫn đăng nhập, quyền suy ra mỗi request → chỉ `staff.read_own_past`; `lock_login` khóa hẳn
- [ADR-0015 own profile out of the books](wiki/decisions/ADR-0015-own-profile-out-of-the-books.md) — **accepted**: hồ sơ của chính mình không có trong sổ quản lý (xem qua `/me/*`); vai trò theo tài khoản, quản lý chưa gắn hồ sơ → `staff.link_required`
- [ADR-0016 accounts and passwords](wiki/decisions/ADR-0016-accounts-and-passwords.md) — **accepted (EM 2026-11-08)**: tài khoản tạo từ hồ sơ, mật khẩu tạm một lần + bắt đổi, đổi / đặt lại mật khẩu, đổi vai trò, "Quên mật khẩu?" gửi yêu cầu cho chủ spa (0032)
- [ADR-0017 one-statement request auth](wiki/decisions/ADR-0017-one-statement-request-auth.md) — **accepted (EM 2026-11-09)**: `app.session_actor(token_hash)` SECURITY DEFINER trả session + quyền + người đã nghỉ + cài đặt spa trong một câu (0034); quyết định vẫn ở Rust
- [ADR-0018 platform admin console](wiki/decisions/ADR-0018-platform-admin-console.md) — **accepted (đã làm 0038–0040, EM to review)**: operator là principal riêng (`__Host-op-session`, TOTP, step-up 5 phút), `/admin` cùng deployment, hàm definer hẹp có cổng token, cờ tính năng, mẫu vai trò, audit hai phía
- [ADR-0019](wiki/decisions/ADR-0019-audit-decisions-2026-11-09.md) — lead quyết các câu hỏi mở sau rà soát 2026-11-09 (EM ủy quyền): chốt lương, sửa lượt, hoàn tiền, PII trong audit, cột lịch hẹn của quản lý, UX
- [ADR-0020 owner MFA per spa](wiki/decisions/ADR-0020-owner-mfa-per-spa.md) — **accepted (EM chọn 2026-11-09, sửa ADR-0004)**: công tắc "Bắt buộc xác thực 2 bước cho chủ spa" theo spa ở `/admin` (mặc định bật, 0050), đăng nhập hỏi mã khi và chỉ khi `user_requires_mfa`, nghiêm nhất thắng, bật thì thu hồi session, tắt giữ authenticator
- [ADR-0021 activity log for the owner](wiki/decisions/ADR-0021-activity-log-for-the-owner.md) — **accepted (EM 2026-11-11, sửa một phần ADR-0019 D13; D1 sửa bởi ADR-0022)**: "Nhật ký thao tác" chỉ chủ spa (`audit.read` owner-only), server trả mục có cấu trúc + web ghép câu, `kernel::audit::Action` có kiểu (thiếu nhóm / câu thì không compile / typecheck), index 0051, `customer.update` giữ tên + SĐT cũ → mới, giờ audit theo đồng hồ use case trong seed, một thao tác một dòng (gộp lượt + hóa đơn + gói khi đọc, D9)

- [ADR-0022 inline record history](wiki/decisions/ADR-0022-inline-record-history.md) — **accepted (EM 2026-11-11 chiều + LEAD sau review độc lập; đợt 1a + 1b + 2 + 3 đã build, 0052)**: `record.history.read` spa-local (chủ spa mặc định hiện, quản lý ẩn, KTV không bao giờ), chỉ operator đổi ở `/admin`; cờ `activity_log` mặc định tắt từ đợt 1 (sửa ADR-0018 D6 / ADR-0003: chủ spa không thấy việc của operator khi cờ tắt); `/record-history/*` + `edited`; SĐT cũ chỉ cho `audit.read`

## Security
- [authz-matrix](wiki/security/authz-matrix.md) — ai gọi được gì: permission × role (seed mẫu 0040), bảng endpoint → permission khớp `authz_matrix.rs`, quy tắc server từng vòng, cờ tính năng, console operator, test bắt buộc
- [threat-model](wiki/security/threat-model.md) — STRIDE theo trust boundary + bảng lạm dụng → kiểm soát → test của từng lần secure review (nhân sự, nhật ký ghi chú, ghi chú gói, admin nền tảng); rủi ro còn mở

## Plans
- [r1-1-front-desk](wiki/plans/r1-1-front-desk.md) — nhật ký build R1-1 (slice S0–S11): mục tiêu, kết quả, lệch thiết kế, bảng AC → test; S10 chờ nghiệm thu, S11 thành staging Vercel
- [r1-2-management-views](wiki/plans/r1-2-management-views.md) — ticket M1–M10 cho màn quản lý / báo cáo; M8 (xóa `POST /opening-balances`) còn mở
- [r1-6-upsell](wiki/plans/r1-6-upsell.md) — **done**: ticket A1–A11 (backend upsell trong lượt) + B1–B6 (giỏ); ghi rõ dòng nào bị spec §10.12–§10.13 thay
- [r1-1-trial-session](wiki/plans/r1-1-trial-session.md) — kịch bản buổi dùng thử với quản lý (chưa chạy): chuẩn bị dữ liệu, 15 nhiệm vụ bấm giờ, phiếu góp ý
- [record-history (plan)](wiki/plans/record-history.md) — đợt 1a (0052, công tắc, cờ, API lượt / hóa đơn / gói / khách) + 1b (các màn) lên staging cùng nhau; đợt 2 lịch hẹn, chấm công, khen / phàn nàn, thưởng / phạt (đã build, không migration); đợt 3 danh mục, quy tắc lương, cài đặt lịch hẹn (đã build, không migration); việc còn lại sau 3 đợt (web, đã làm); cửa sổ hồi quy; test từng đợt
- [audit-2026-11-09](wiki/plans/audit-2026-11-09.md) — rà soát toàn bộ 2026-11-09: ~35 PR (#117–#148), migration 0041–0044, việc đã sửa, câu hỏi chờ EM, ticket còn lại

## Runbooks
- [vercel-staging](wiki/runbooks/vercel-staging.md) — staging hiện tại: Vercel (web tĩnh + API container, pdx1) + Supabase us-west-2; biến môi trường, CLI `app`, bootstrap DB, dữ liệu demo (`reset-demo` + `seed-demo` kịch bản 12 tháng, bảng xóa / giữ), deploy, kiểm tra sau deploy, việc trước khi dùng thật
- [local-merge-gate](wiki/runbooks/local-merge-gate.md) — merge khi GitHub Actions không chạy được (dự phòng): `merge-local-gate.sh`, bảng nhãn file → bước, `--dry-run` / `--full` / `--batch`, khi nào bắt buộc `--full`
- [staging-setup](wiki/runbooks/staging-setup.md) — **superseded cho staging** (thay bằng vercel-staging): đường VPS Docker Compose + Caddy + backup R2, còn giữ vì code `infra/` + `deploy.yml` vẫn có và là phương án production; checklist Supabase §4.2 vẫn dùng

## Research
- [tech-stack](wiki/research/tech-stack.md) — so sánh ngôn ngữ / framework backend + frontend (tư liệu, quyết định ở ADR-0001)
- [code-intelligence](wiki/research/code-intelligence.md) — công cụ LSP / Serena / code graph cho impact analysis của agent
- [hosting-storage](wiki/research/hosting-storage.md) — nơi lưu DB, file, video dài hạn; Supabase credits; chi phí; luật dữ liệu ra nước ngoài (tư liệu, hiện trạng ở ADR-0005/0012)

## Conventions
- [coding-standards](wiki/conventions/coding-standards.md) — chuẩn code một nguồn: universal, Rust (layering, sqlx, test RLS, không unsafe), TypeScript, Git (merge-when-green)
- [testing](wiki/conventions/testing.md) — các lớp test và cách chạy, thêm route vào authz matrix, viết E2E/axe, ngân sách perf, đồ thị job CI
