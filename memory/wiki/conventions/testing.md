---
title: Testing conventions
updated: 2026-11-04
sources: [security/authz-matrix.md, product/specs/front-desk-and-packages.md, plans/r1-1-front-desk.md]
---

# Testing conventions

Hạ tầng test hardening dựng ở S10. Mọi ticket S6–S9 trở đi cắm vào đây, không dựng lại.

| Lớp | Ở đâu | Chạy |
|---|---|---|
| Unit + integration (Rust) | `apps/api/crates/*/src`, `apps/api/crates/app/tests/*.rs` (`#[sqlx::test]`, mỗi test một DB riêng, chạy dưới `app_rw` có RLS) | `pnpm check:api` |
| **Authz matrix + tenant isolation** | `apps/api/crates/app/tests/authz_matrix.rs` | `cargo test -p app --test authz_matrix` |
| Unit (web) | Vitest, `apps/web/src/**/*.test.ts(x)` | `pnpm test` |
| E2E + a11y (axe) | Playwright, `apps/web/e2e/specs/` | `pnpm e2e` |
| p95 API | `apps/web/e2e/perf/api-latency.spec.ts` | `pnpm e2e:perf` |
| LCP (Lighthouse CI) | `apps/web/lighthouserc.json` | `pnpm lhci` |

## 1. Thêm route mới vào authz matrix (bắt buộc)

`authz_matrix.rs` có **một bảng `ROUTES`** phản chiếu [authz-matrix](../security/authz-matrix.md). Test `every_mounted_route_has_a_matrix_row` so sánh ba nguồn: OpenAPI (`ApiDoc`), các lời gọi `.route(...)` trong source router, và bảng `ROUTES`. Route có trong router mà thiếu trong OpenAPI, hoặc có trong OpenAPI mà thiếu dòng trong bảng, thì CI fail kèm hướng dẫn. Đây là cổng CI "mọi route đều khai báo permission".

Thêm endpoint thì làm 3 bước:
1. Thêm `#[utoipa::path]` vào `openapi.rs` (đằng nào cũng phải làm cho contract TS).
2. Thêm một dòng vào `ROUTES`, ví dụ S6 bán gói:
   ```rust
   route("POST", "/api/v1/customers/{id}/packages", Access::Perm(P::PackageSell, OWNER_MANAGER))
       .id(Target::Customer)
       .body(sell_package_body)
       .idempotent(),            // chỉ khi handler đọc Idempotency-Key (helper thêm ở S6)
   ```
   - `Access::Public` (không cần session), `Access::Session` (đăng nhập là đủ), `Access::Perm(permission, roles)`. `roles` là cột ✓ của authz-matrix: `ALL`, `OWNER_MANAGER`, `OWNER_ONLY`. `Access::PendingMfa` (S2b): chỉ session vừa qua bước mật khẩu và còn nợ MFA (`/auth/mfa/*`); session đầy đủ gọi vào thì `409 auth.mfa_not_pending`.
   - Helper `session()` trong matrix tự hoàn tất MFA cho owner (enroll lần đầu, sau đó nhập mã hiện tại).
   - `.id(Target::X)` khi path có `{id}`. Loại record mới (ví dụ `Target::CustomerPackage`) thì thêm variant, thêm field vào `Fixture`, rồi tạo record đó trong `fixture()` cho **cả hai** spa.
   - `.body(fn)`: body **hợp lệ** cho spa của người gọi. Body sai thì serde trả 4xx trước khi chạy permission check, và test báo nhầm.
   - `.if_match()` cho PUT có optimistic lock. `.consumes_session()` cho route thu hồi session (logout, switch-tenant).
3. Chạy `cargo test -p app --test authz_matrix`. Ngoài cổng coverage, file còn chạy:
   - `matrix_rows_agree_with_seeded_roles`: cột `roles` phải khớp `SystemRole::default_permissions()`. Lệch nghĩa là roles.rs hoặc tài liệu sai.
   - `every_non_public_route_rejects_anonymous_and_forged_sessions`: không có cookie, hoặc cookie giả, thì `401`.
   - `each_role_gets_exactly_what_the_matrix_grants`: role không có quyền nhận `403 auth.forbidden`. Role có quyền thì không bị 401/403/5xx.
   - `ids_from_another_spa_are_not_found`: chủ spa B gọi ID của spa A thì `404`. Có control call trên record của chính B để chứng minh 404 đến từ cách ly tenant, không phải do request hỏng.
   - `pending_mfa_sessions_reach_only_the_mfa_routes`: session pending MFA gọi mọi route khác (trừ logout) thì `401 auth.mfa_*`.

Route 🔒 (re-auth) và rule giảm giá có test riêng trong file test của slice (authz-matrix mục 5–6). Matrix chỉ kiểm cổng permission.

Chưa phủ: role `PlatformAdmin`/support grant (chưa có trong `identity::roles`). Khi S-ticket thêm role này, thêm nó vào `SystemRole::ALL` và các dòng tự động được kiểm thêm.

## 2. Viết E2E spec

- Stack: `playwright.config.ts` khởi động API (`cargo run -p app -- serve`, port **8091**) trên DB riêng `vpyd_e2e`, và Vite dev (port **5181**, proxy `/api` → 8091 qua `VPYD_API_ORIGIN`). Hai port này tránh đụng `pnpm dev:*`.
- Project `setup` (`e2e/setup/provision.setup.ts`) chạy `migrate`, rồi `create-tenant` (slug `e2e-<thời gian>`), `add-user` (owner, SĐT ngẫu nhiên, mật khẩu ngẫu nhiên truyền qua env `APP_NEW_USER_PASSWORD`, không qua argv), `add-user` thêm một quản lý (không MFA, cho Lighthouse), rồi `seed-catalog`. Mỗi lần chạy có một spa mới, nên không cần reset DB. Credentials nằm ở `e2e/.auth/run.json`, cookie owner ở `e2e/.auth/owner.json` (đều gitignored). **Không bao giờ hard-code credentials.**
- Spec mới đặt trong `e2e/specs/*.spec.ts`:
  - Cần đăng nhập sẵn: `test.use({ storageState: ownerState })`. Test luồng đăng nhập: `signIn(page, path)`, hoặc `submitLogin(page)` khi form đăng nhập đã ở trên màn hình.
  - Owner bắt buộc TOTP (S2b). `signIn`/`submitLogin` tự làm bước `/xac-thuc`: lần đầu (trong setup) enroll và lưu `totpKey` vào `run.json`; các lần sau sinh mã RFC 6238 cho bước thời gian chưa dùng (`lastTotpStep`). Server chặn replay, và mã cũ bị tính vào khóa tài khoản, nên helper không bao giờ gửi lại mã cũ (có thể phải chờ tới 30 giây). Gọi API trực tiếp thì dùng `apiSignIn`.
  - Locator theo role/label tiếng Việt (`getByRole("button", { name: "Lưu khách" })`). Không dùng CSS selector: đây cũng là kiểm tra a11y.
  - Dữ liệu unique trong run (`uniquePhone()`), để retry không đụng ràng buộc SĐT duy nhất.
  - Mỗi màn hình mới gọi `await expectAccessible(page, "<tên>")` (axe, WCAG 2.2 AA). Lỗi serious/critical thì fail. Lỗi moderate/minor được đính kèm vào report để triage.
- Chạy 1 worker. Rate limit `/auth/*` là 10 request burst mỗi IP, nên hạn chế đăng nhập qua UI, ưu tiên `storageState`.
- **Màn quầy `/quay` (S7 ✅)**: `e2e/specs/front-desk.spec.ts` chạy luồng nhanh đúng 4 thao tác bằng bàn phím (gõ SĐT + Enter → Enter trên "Trừ 1 buổi" đang focus → chọn KTV → Ctrl+Enter), kiểm tra focus nhảy đúng chỗ ở mỗi bước, axe ở 3 trạng thái (trống, có giỏ, đã lưu).

## 3. Chạy local

Một lần đầu:
```bash
docker exec vpyd-dev-postgres-1 psql -U vpyd -d vpyd -c "CREATE DATABASE vpyd_e2e"
pnpm --filter web exec playwright install chromium
```
Sau đó: `pnpm e2e` (cả setup + desktop + perf), `pnpm e2e:perf`, `pnpm --filter web exec playwright test --ui`, `pnpm --filter web exec playwright show-report`.

Connection string: `E2E_DATABASE_URL` / `E2E_APP_DATABASE_URL`. Nếu không đặt, chúng được suy ra từ `.env` ở gốc repo bằng cách đổi tên DB thành `vpyd_e2e`. Lần đầu chạy, API được compile cold (vài phút; timeout webServer 15 phút). Nếu API/Vite của E2E đang chạy sẵn thì Playwright dùng lại (chỉ local).

## 4. Hiệu năng

- `api-latency.spec.ts` seed 300 khách (`PERF_CUSTOMERS`), đo 100 mẫu (`PERF_SAMPLES`) tuần tự cho `GET /customers?q=` (tên không dấu, tên, số), `/customers`, `/service-categories`, `/services`, `/package-plans`. p95 phải < **300 ms** (spec §6, tìm khách; catalog dùng tạm cùng ngân sách). Kết quả nằm ở `test-results/**/api-p95.json` và report. Đây là chỉ báo hồi quy cho query chậm (một client, build debug), **không phải load test**. k6 để S11 trên staging.
- Lighthouse CI (`apps/web/lighthouserc.json`) chạy trên bản build (`vite preview`), throttling mặc định (mobile, Slow 4G, CPU 4×). Kiểm LCP < 2,5 s, CLS < 0,1, điểm a11y ≥ 0,95 cho `/dang-nhap` và `/quay` (S7). `pnpm lhci` gọi `apps/web/lighthouse/run.mjs`: chạy API đã build trên DB E2E (port 8093, `LHCI_API_PORT`) + `vite preview` (port 4173, `LHCI_WEB_PORT`; agent chạy song song thì đổi cả hai) proxy tới nó, đăng nhập bằng tài khoản **quản lý** của lần chạy E2E (`e2e/.auth/run.json`; quản lý không có MFA), rồi chạy `lhci autorun` với cookie đó qua `settings.extraHeaders` trong một file config tạm (không qua argv, xóa sau khi chạy). Vì vậy phải chạy sau `pnpm e2e`. Không dùng `puppeteerScript` (cần cài puppeteer và không qua được TOTP của chủ spa).

## 5. CI

Job `e2e` trong `.github/workflows/ci.yml` dùng Postgres service `vpyd_e2e` và role `app_rw` từ `infra/postgres-init/01-roles.sql`. Các bước: build API, cài Chromium, chạy `pnpm e2e` (gồm axe + p95), rồi `pnpm lhci`. Khi fail thì upload `playwright-report`, `test-results` và `lighthouse-report`. Khi pass thì upload `perf-report`.

## Gotcha: DB dev cũ hơn `main`
Khi `main` có migration mới mà DB dev chưa chạy, `pnpm db:migrate` không biên dịch được: macro `query!` kiểm tra SQL với DB còn thiếu bảng. Chạy với `SQLX_OFFLINE=true` để build từ `.sqlx` đã commit:
`SQLX_OFFLINE=true pnpm db:migrate` (PowerShell: `$env:SQLX_OFFLINE='true'; pnpm db:migrate`).
Trong worktree của agent, luôn đặt `DATABASE_URL` rõ ràng: `.env` của repo chính nằm ở thư mục cha và có thể bị đọc nhầm.
