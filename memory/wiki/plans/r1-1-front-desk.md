---
title: Plan R1-1 — Màn hình quầy & liệu trình
updated: 2026-11-09
sources: [product/specs/front-desk-and-packages.md, architecture/backend.md, architecture/data.md, architecture/frontend.md, security/authz-matrix.md]
---

# Plan R1-1: Màn hình quầy & liệu trình

> **Trạng thái (2026-11-09):** S0–S9 xong. S10 xong phần hardening, chưa tick vì buổi dùng thử theo [kịch bản](r1-1-trial-session.md) chưa chạy (thay vào đó EM đã live test trên staging 2026-11-07 và 2026-11-08, xem `memory/raw/2026-11-07-em-live-test-desk.md`, `2026-11-08-live-test-owner.md`, `2026-11-08-live-test-manager-technician.md`). S11 chạy theo đường khác: staging trên **Vercel + Supabase** từ 2026-11-05 ([ADR-0005](../decisions/ADR-0005-compute-hosting.md) update, [vercel-staging](../runbooks/vercel-staging.md)), không phải VPS. Plan này là nhật ký build: các quy tắc về sau bị thay được đánh dấu tại chỗ; quy tắc hiện hành ở [authz-matrix](../security/authz-matrix.md) và [backend](../architecture/backend.md).

Thứ tự ưu tiên: phụ thuộc trước, sau đó rủi ro cao trước. **S3 là tracer bullet**: lát mỏng nhất chạy xuyên DB → API → UI có đăng nhập và tenant, dùng để chứng minh kiến trúc. Mỗi ticket chạy `/build` (kèm impact analysis), `/qa` và `/pr-review`.

Quy ước chung cho mọi ticket có endpoint:
- **Test authz bắt buộc**: role thiếu quyền → 403; ID của tenant khác → 404; chưa đăng nhập → 401.
- Mọi lệnh ghi có test audit.
- Mọi lệnh ghi tiền hoặc buổi có test idempotency.

---

- [x] **S0 Nền móng repo & môi trường dev** (size: M) ✅ 2026-11-04, branch `feat/s0-foundation`
  - **Goal:** `git init`, monorepo đúng ADR-0006, chạy được `/api/v1/health` qua Caddy từ SPA trống, CI xanh.
  - **Acceptance:** nền cho mọi AC. DoD của `/devops`: một lệnh dựng stack local; PR không merge được khi CI đỏ.
  - **Touches:** `apps/api` (workspace + crate `kernel`, `app`), `apps/web` (Vite + React + TS strict + Tailwind + shadcn, TanStack Router/Query), `packages/api-client`, `packages/ui`, `infra/docker-compose.dev.yml` (Postgres 16 + pg_trgm), `infra/Caddyfile`, `.github/workflows/ci.yml`, `.gitignore`, `.env.example`. Cài plugin LSP `rust-analyzer-lsp` + `typescript-lsp`. Hook PostToolUse chạy `cargo check` / `tsc` khi sửa file.
  - **Tests:** smoke test health; CI chạy fmt, clippy `-D warnings`, `cargo test`, `cargo audit`/`deny`, `pnpm lint`/`typecheck`/`test`, gitleaks.
  - **Security:** secret chỉ nằm trong `.env` (đã gitignore); gitleaks; Caddy đặt header bảo mật (CSP, HSTS, nosniff, frame-ancestors).
  - **Depends on:** —
  - **Kết quả / sai khác so với kế hoạch:**
    - Dev dùng **Vite proxy** `/api` → API (Caddy chỉ dùng cho production, S11).
    - **TanStack Router và shadcn/ui** dời sang S2/S3, khi có route hoặc component đầu tiên.
    - Crate `kernel` tạo ở S1.
    - Ổ exFAT không có symlink nên pnpm chạy `node-linker=hoisted`, `@vpyd/ui` dùng qua path alias.
    - Font **self-host** (fontsource), CSP không còn cho phép Google Fonts.
    - Hook PostToolUse chạy `cargo check` / `tsc` sau mỗi lần Edit (đã pipe-test cả trường hợp lỗi).
    - Plugin LSP cần EM cài bằng `/plugin install` (binary `rust-analyzer` đã cài).

- [x] **S1 Kernel: tenant, lỗi, audit, idempotency** (size: M) ✅ 2026-11-04
  - **Goal:** hạ tầng xuyên suốt mà mọi use case dùng chung.
  - **Acceptance:** G3, G4, G5, G6 (spec §3).
  - **Touches:** `kernel`: `TenantId`/`BranchId`/`Money`/`BusinessDate` newtypes, `TenantTx` (`SET LOCAL app.tenant_id`, `statement_timeout`), `AppError → problem+json`, `audit::record`, `idempotency` (bảng + extractor + middleware), `can()` + danh mục permission. `app`: config fail-fast, tracing request-id, timeout, giới hạn body, rate limit, kiểm tra `Origin`. Migrations `0001` (extensions, roles `app_rw`/`app_migrator`, schema `app`), `tenant`, `branch`, `audit_log`, `idempotency_record`, mẫu RLS.
  - **Tests:** unit cho Money/BusinessDate; integration: role `app_rw` **không** đọc được dòng của tenant khác khi `app.tenant_id` khác (RLS hoạt động); idempotency: gửi lặp → cùng response, khác body → 422, song song → 409; lỗi 500 không lộ stack.
  - **Security:** RLS + newtype bắt buộc (ADR-0003); `app_rw` không có `UPDATE/DELETE` trên `audit_log`.
  - **Depends on:** S0
  - **Kết quả:**
    - 36 test Rust: 26 trong kernel (9 integration chạy với app_rw/RLS), 10 trong app.
    - Mutation check: tắt `set_config` → 6/9 test integration đỏ.
    - Idempotency dựa vào khóa PK của Postgres nên request trùng song song tự chờ rồi replay; không cần trạng thái `in_progress`.
    - Middleware: request-id, problem+json cho mọi lỗi, timeout 10 giây, body tối đa 64 KB, rate limit theo IP (5/giây, burst 60), kiểm tra Origin cho request ghi.
    - `app migrate` và job purge chạy mỗi giờ. CI có Postgres service + `SQLX_OFFLINE`.

- [x] **S2 Đăng nhập & session (tối thiểu cho R1)** (size: M) ✅ 2026-11-04
  - **Goal:** đăng nhập bằng SĐT + mật khẩu; seed tenant, chủ spa, quản lý bằng CLI. Giao diện quản lý tài khoản thuộc spec #4.
  - **Acceptance:** G1, G2; ADR-0004.
  - **Touches:** crate `identity` (`user_account`, `membership`, `role`, `role_permission`, `membership_role`, `session`, `auth_token`); endpoints `/auth/login|logout|logout-all|reauth`, `/me`, `/auth/switch-tenant`; CLI `app seed-tenant` (tạo role hệ thống theo [authz-matrix](../security/authz-matrix.md)); web: `/dang-nhap`, `AuthContext`, route guard, xử lý 401/403, nút Khóa nhanh.
  - **Tests:** đăng nhập đúng → cookie `__Host-session` đủ cờ; sai mật khẩu và sai SĐT trả cùng một lỗi; khóa tạm sau N lần sai; session xoay khi đăng nhập; logout-all hủy mọi session; idle/absolute timeout; request ghi với `Origin` lạ → 403; `/me` trả đúng permission theo role; reauth hết hạn → `auth.reauth_required`.
  - **Security:** Argon2id trong `spawn_blocking`; token chỉ lưu dạng hash; rate limit lớp `auth`; MFA TOTP cho owner (có thể tách S2b nếu vượt size).
  - **Depends on:** S1
  - **Kết quả:**
    - Migration `0002_identity`: account/session toàn cục, membership/role/permission theo tenant + RLS, hàm `tenants_of_user`.
    - Crate `identity`: Argon2id, token 256-bit (lưu hash), khóa tăng dần 15 phút → 24 giờ, session idle 12 giờ / absolute 30 ngày, xoay token khi login và khi đổi spa, reauth trong 5 phút, extractor `CurrentSession` / `AuthUser` (→ `Actor`).
    - CLI `create-tenant`, `add-user` (mật khẩu đọc từ env, không qua argv).
    - Web: TanStack Router, `/dang-nhap` (zod + react-hook-form), guard (chỉ là UX), `next` chống open redirect, Khóa nhanh.
    - Test: 62 Rust (11 HTTP E2E auth; mutation lockout ✓) + 13 web. E2E qua Vite proxy ✓.
    - Dữ liệu dev: spa `vien-phuc-y-duong` + tài khoản owner thử `0905000001`.

- [x] **S2b MFA TOTP cho chủ spa** (size: S) ✅ 2026-11-04
  - **Goal:** chủ spa (và platform admin) bắt buộc TOTP khi đăng nhập (ADR-0004).
  - **Touches:** migration thêm `mfa_secret_ct`, `mfa_enabled`; enroll (QR) + verify; login bước 2; mã khôi phục.
  - **Tests:** sai mã → 401; replay mã cũ → 401; owner chưa bật MFA → bắt enroll.
  - **Depends on:** S2. Làm trước S11 (staging).
  - **Kết quả:**
    - Migration `0006_mfa`: `role.requires_mfa` (role là dữ liệu; CHECK không cho bỏ MFA ở role hệ thống `owner`), `user_account.mfa_secret_ct` (AES-256-GCM, AAD gắn với user id), `mfa_enabled_at`, `mfa_last_step` (chống replay), bảng `mfa_recovery_code` (SHA-256(user id ‖ mã), dùng 1 lần), `session.mfa_pending`, hàm `app.user_requires_mfa` (SECURITY DEFINER, giống `tenants_of_user`). Migration thu hồi session cũ của tài khoản bắt buộc MFA.
    - MFA áp dụng khi user có role `requires_mfa` hoặc role giữ `platform.tenant.manage` ở bất kỳ spa nào, hoặc user đã tự bật MFA. (Từ 0040 `platform.tenant.manage` không cấp được cho role nào; quản trị nền tảng là operator riêng, [ADR-0018](../decisions/ADR-0018-platform-admin-console.md); 0041 bỏ hẳn nhánh này khỏi `app.user_requires_mfa`.)
    - Luồng: `POST /auth/login` trả `{next: done | mfa_verify | mfa_enroll, me}`. Khi chưa xong MFA, session ở trạng thái **pending** (hạn 10 phút, không có spa, `me = null`): `CurrentSession`/`AuthUser` từ chối với `401 auth.mfa_required | auth.mfa_enrollment_required`; chỉ `/auth/mfa/*` và `/auth/logout` nhận nó. `POST /auth/mfa/verify` (TOTP hoặc mã khôi phục) và `POST /auth/mfa/enroll/confirm` thu hồi session pending và cấp session đầy đủ mới (xoay token). `enroll/start` trả `otpauth_uri` + `manual_key`; đã enroll thì `409`.
    - TOTP: crate `totp-rs` 6 (MIT), RFC 6238 SHA-1, 6 số, 30 giây, lệch ±1 bước. Mỗi bước thời gian chỉ được chấp nhận 1 lần (`UPDATE … WHERE mfa_last_step < $step`, nguyên tử).
    - Khóa tài khoản: mã sai tính chung bộ đếm `failed_logins` (tăng bằng SQL, không mất lượt khi gửi song song). Nhập lại đúng mật khẩu **không** reset bộ đếm với tài khoản MFA; chỉ hoàn tất MFA mới reset. Rate limit IP của `/auth/*` áp dụng cho `/auth/mfa/*`.
    - Reauth: tài khoản đã bật MFA phải gửi thêm `code` (thiếu → `422`, sai → `401 auth.invalid_mfa_code`); lần sai mật khẩu/mã khi reauth cũng tính vào khóa.
    - `kernel::crypto::FieldCipher` là nguồn duy nhất cho mã hóa trường; `crm::cipher` chỉ còn re-export + `customer_aad` (layout AAD giữ nguyên). Config `APP_DATA_KEY` dùng chung.
    - CLI `app reset-mfa --phone P` (mất điện thoại; xác minh danh tính trước): xóa secret + mã khôi phục, thu hồi mọi session.
    - Web: `/xac-thuc` (nhập mã 6 số hoặc mã khôi phục), enroll (QR vẽ SVG tại trình duyệt bằng `uqr` 0.1.3, không gọi host ngoài, CSP giữ nguyên; khóa nhập tay chia nhóm 4), màn hình mã khôi phục hiện 1 lần + nút chép + checkbox "đã lưu". Guard: session pending bị đưa về `/xac-thuc` (chỉ UX).
    - Test: 11 HTTP E2E MFA + 6 unit `mfa` + 4 unit `kernel::crypto` + 2 config; test auth cũ chuyển sang tài khoản manager (không MFA). Web: 5 test luồng MFA. Mutation check: bỏ điều kiện replay → 2 test đỏ; bỏ chặn session pending → test pending đỏ; reset bộ đếm khi đúng mật khẩu → test lockout đỏ.
    - **Dev:** owner thử `0905000001` lần đăng nhập đầu sẽ bị bắt enroll: quét QR bằng Google/Microsoft Authenticator (hoặc nhập khóa tay), nhập mã, lưu mã khôi phục. Cần `APP_DATA_KEY` trong `.env` (`openssl rand -base64 32`). Mất app xác thực ở dev: `cargo run --manifest-path apps/api/Cargo.toml -p app -- reset-mfa --phone 0905000001` rồi đăng nhập lại.

- [x] **S3 Tracer bullet: bảng giá (đọc) + seed menu** (size: S) ✅ 2026-11-04
  - **Goal:** quản lý đăng nhập → thấy menu thật, gồm 10 nhóm, 35 dịch vụ và giá Nữ/Nam, trên `/bang-gia`.
  - **Acceptance:** AC1.7, AC1.9.
  - **Touches:** crate `catalog` (`service_category`, `service`, `service_price`, `package_plan`, `package_plan_item`); `GET /service-categories|services|package-plans`; seed từ [service-catalog](../product/service-catalog.md). "Ôn ấm tử cung" được seed **ngừng bán** cho tới khi có giá. Web: trang `/bang-gia` chỉ đọc; client sinh từ OpenAPI.
  - **Tests:** seed đủ 35 dịch vụ; dịch vụ giá chung trả 1 giá, giá theo giới tính trả 2 (hoặc 1 nếu chỉ áp dụng một giới); authz (Technician đọc được, chưa đăng nhập → 401); tenant B không thấy menu của tenant A.
  - **Security:** chỉ đọc; tenant isolation.
  - **Depends on:** S2
  - **Kết quả:**
    - Migration `0003_catalog` + crate `catalog`: `ServicePrices` (chung hoặc Nữ/Nam, thiếu một giới = không áp dụng), `price_for(gender)` sẵn cho S7, seed menu bằng `app seed-catalog`.
    - Dịch vụ menu in cùng giá Nữ = Nam → giá chung. Ôn ấm tử cung seed **ngừng bán, chưa có giá**.
    - Đính chính số liệu: menu có **35 dịch vụ / 10 nhóm** (wiki cũ ghi 36/11).
    - **Hợp đồng OpenAPI** (`utoipa` 6 → `apps/api/openapi.json`, 9 endpoint) → `packages/api-client` (openapi-typescript). CI fail nếu spec hoặc client lệch với code.
    - Web: AppShell + nav theo quyền, trang `/bang-gia`.
    - Test: 27 test app (5 catalog E2E) + 18 web. Đã kiểm tra trên **Chrome thật**: guard → đăng nhập → quay lại `/bang-gia` theo `next`; giao diện tối hiển thị đúng; console không lỗi.

- [x] **S4 Quản lý bảng giá & gói liệu trình** (size: M) ✅ 2026-11-04
  - **Goal:** chủ spa tạo/sửa/ngừng bán dịch vụ, phương án giá và gói (combo, buổi tặng theo dòng, hạn dùng, quà hiện vật).
  - **Acceptance:** AC1.1–AC1.6, AC1.8.
  - **Touches:** `catalog` service + repo; `POST/PATCH /services`, `/deactivate`, `/package-plans`, `/service-categories`; ETag/If-Match. Web: form dịch vụ và form gói (react-hook-form + zod).
  - **Tests:** quy tắc phương án giá (chỉ `all`, hoặc ≥ 1 giới tính) → 422 nếu sai; trùng tên gói đang bán → 422; dịch vụ đã dùng thì không xóa được, chỉ ngừng bán; sửa giá gói không đổi liệu trình đã bán (kiểm tra ở S6); Manager sửa → 403; If-Match lệch → 412.
  - **Security:** chỉ `catalog.manage`; DTO không nhận `tenant_id`/`id`.
  - **Depends on:** S3
  - **Kết quả:**
    - API ghi cho nhóm, dịch vụ và gói (`PUT` thay toàn bộ + `If-Match`, audit before/after, không có endpoint xóa).
    - Quy tắc: dịch vụ đang bán phải có giá; giá chung và giá giới tính không được trộn; gói cần ≥ 1 dịch vụ, không trùng dịch vụ, dịch vụ phải đang bán, buổi tặng theo từng dòng.
    - Web: form sửa / thêm dịch vụ trên `/bang-gia`, trang `/bang-gia/goi` (tạo / sửa gói combo), thêm nhóm.
    - **Lỗi tìm ra khi thử trên Chrome:** người đã đăng nhập mở link `/dang-nhap?next=…` bị đưa về `/` thay vì `next`. Đã sửa, có test xác nhận đỏ → xanh.
    - Test: 9 E2E catalog_manage (mutation khóa lạc quan ✓) + 26 web. Đã tạo gói thử trên Chrome thật.

- [x] **S5 Hồ sơ khách hàng & tìm kiếm** (size: M)
  - **Goal:** tìm khách theo SĐT/tên (không dấu) < 300 ms, tạo khách, xem/sửa hồ sơ; ghi chú sức khỏe mã hóa và chỉ trả về khi có quyền.
  - **Acceptance:** AC2.1–AC2.7.
  - **Touches:** crate `crm` (`customer`, chuẩn hóa SĐT → E.164, `search_text`, GIN trgm, AES-256-GCM cho `health_note`); `GET/POST/PATCH /customers`, `GET /customers/{id}`. Web: ô tìm khách và thẻ khách như prototype, form khách mới, trang `/khach-hang/:id`.
  - **Tests:** "hà" tìm được "Hà"; 3 dạng SĐT là cùng một số; trùng SĐT → 409 kèm `existing_customer_id`; NFC; thiếu `customer.health_notes.read` → response không có key `health_note`; ciphertext trong DB không chứa plaintext; p95 tìm kiếm < 300 ms với 10.000 khách seed; Technician → 403.
  - **Security:** IDOR (ID của tenant khác → 404); key mã hóa lấy từ env (`secrecy`), có phiên bản; audit không chứa plaintext sức khỏe.
  - **Depends on:** S2 (song song với S3/S4)
  - **Kết quả:**
    - Migration `0004_crm`, crate `crm`: chuẩn hóa tên NFC + `search_text` bỏ dấu (đ→d), SĐT → E.164, trùng SĐT → `409 customer.phone_taken` kèm `existing_id` (SAVEPOINT bắt `23505`).
    - Tìm: chữ số → theo SĐT; chữ → theo tên không dấu; rỗng → khách gần đây; quá ngắn → 422.
    - Ghi chú sức khỏe: AES-256-GCM (AAD = tenant + khách), endpoint riêng `PUT /customers/{id}/health-note`; thiếu quyền đọc → response **không có key** `health_note`; audit không chứa nội dung.
    - Hợp đồng đổi so với thiết kế: `PUT` thay `PATCH`; field lỗi là `existing_id`.
    - Web: `/khach-hang` (ô tìm có ↑↓ Enter, khách gần đây, form khách mới điền sẵn từ ô tìm, nút "Mở hồ sơ đó" khi trùng SĐT), `/khach-hang/:id` (cảnh báo sức khỏe, sửa hồ sơ). `CustomerSearch` dùng lại cho S7.
    - Test: 7 E2E customers (gồm 10.000 khách, mutation lọc quyền ghi chú ✓) + 33 web. Chrome thật: tạo khách, thấy cảnh báo, tìm "khach thu" ra "Khách Thử Nghiệm".

- [x] **S6 Bán liệu trình** (size: M)
  - **Goal:** bán gói cho khách, ghi người chốt, giảm giá có lý do, thu đủ (TM/CK/kết hợp); liệu trình được tạo với snapshot.
  - **Acceptance:** AC3.1–AC3.7; AC1.2 (snapshot).
  - **Touches:** crates `sales` (`sale`, `sale_line`, `payment` append-only), `packages` (`customer_package`, `customer_package_item`, trạng thái suy ra); `POST /sales`, `GET /sales/{id}`, `GET /customer-packages/{id}`; `org` tối thiểu (`staff` + `GET /staff`). Web: hộp thoại "Bán liệu trình mới" trên thẻ khách.
  - **Tests:** gói 10 + tặng 2 giá 1.500.000đ → 12/12 buổi, `per_session_value` = 150.000đ; tổng thanh toán ≠ total → `422 payment.amount_mismatch`; giảm giá thiếu lý do → 422, Manager giảm > 10% → 403; người chốt đã nghỉ việc → 422; bấm Lưu 2 lần (cùng key) → 1 hóa đơn; client gửi giá → bị bỏ qua (server tính); `payment` không UPDATE/DELETE được.
  - **Security:** tính giá phía server; không mass assignment; idempotency; audit.
  - **Depends on:** S4, S5
  - **Kết quả:**
    - Migration `0005_sales_packages`: `staff`, `sale`, `sale_line`, `payment`, `customer_package`, `customer_package_item` (FK kép theo tenant, RLS). `payment` và `sale_line` append-only hai lớp: app_rw chỉ có `SELECT, INSERT`, thêm trigger `app.reject_mutation` chặn cả schema owner.
    - Crate mới: `org` (`GET /staff?status=active|left|all`, CLI `app add-staff`), `packages` (`GET /customer-packages/{id}`, `GET /customers/{id}/packages`, trạng thái suy ra), `sales` (`POST /sales` bắt buộc `Idempotency-Key`, `GET /sales/{id}`). Kernel thêm `tenant::profile` (ngày nghiệp vụ, chi nhánh chính, `manager_discount_max_pct` mặc định 10) và `Money::per_unit / within_percent_of / sum`.
    - Quy tắc: giá lấy từ catalog trong transaction (khóa `FOR SHARE` gói), field giá client gửi bị bỏ qua; giảm giá cần lý do (`422 discount.reason_required`), Manager > 10% → `403 discount.limit_exceeded`, Owner không giới hạn; tổng payment ≠ total → `422 payment.amount_mismatch`; tối đa 2 payment, mỗi hình thức 1 lần; người chốt nghỉ việc hoặc của spa khác → `422 staff.inactive`. Một transaction tạo sale + lines + package + items + payments + audit + bản ghi idempotency.
    - `per_session_value` = giá sau giảm ÷ tổng buổi **mua** (D4, làm tròn xuống); buổi tặng = 0. Hóa đơn nhiều gói: giảm giá chia theo tỷ lệ giá, phần dư cho dòng cuối.
    - API cho ticket sau: `packages::service::consume_session(tx, Consume{..})` cho S7 (update nguyên tử, buổi mua dùng trước buổi tặng, có test 2 thiết bị tranh buổi cuối) và `packages::service::create(tx, actor, NewPackage{origin: OpeningBalance, ..})` cho S9 (không tạo sale/payment, có test).
    - Web: hồ sơ khách có mục "Liệu trình" (còn X/Y buổi, HSD, trạng thái, nhãn "Nhập từ sổ cũ") và hộp thoại "Bán liệu trình mới" (gói, quà, người chốt, giảm giá + lý do + cảnh báo vượt 10%, TM / CK / kết hợp, một Idempotency-Key cho cả hộp thoại).
    - Test: 11 E2E sales + 4 packages (mutation guard 10% và amount-mismatch ✓) + 6 web mới (tổng 39); 5 dòng mới trong `authz_matrix.rs` (Target `Sale`, `CustomerPackage`); Playwright `e2e/specs/sales.spec.ts` (bán gói TM + CK, axe sạch), setup E2E thêm `add-staff`.
    - Lệch so với thiết kế: `staff` chưa liên kết `membership` (chưa cần tới R2); `GET /customers/{id}/packages` là endpoint mới; thêm code `package.item_not_found` (422).

- [x] **S7 Ghi lượt khách (màn hình quầy)** (size: M) ✅ 2026-11-04
  - **Goal:** luồng chính: trừ buổi và/hoặc dịch vụ lẻ theo giá giới tính, chọn KTV, thu đủ, lưu ≤ 4 thao tác; danh sách lượt hôm nay.
  - **Acceptance:** AC4.1–AC4.8, AC4.10, AC4.2b, AC4.2c; AC4.4 (dữ liệu số khách cho R2).
  - **Touches:** crate `operations` (`visit`, `visit_line`, gọi `packages` + `sales` service trong cùng `TenantTx`); `POST /visits`, `GET /visits?date=`, `GET/PATCH /visits/{id}`. Web: trang `/quay` theo prototype (giỏ lượt, chọn KTV, thanh toán, toast, phím tắt).
  - **Tests:** trừ buổi 5/12 → 4/12; chọn liệu trình mặc định là gói hết hạn sớm nhất; hết buổi → 409; gói hết hạn → 409, kèm lý do + quyền → cho qua và ghi audit; **2 request song song trừ buổi cuối → đúng 1 thành công** (test đồng thời); khách Nam + Đại Thông Lạc → 399.000đ; dịch vụ chỉ dành cho Nữ với khách Nam → 422; khách chưa có giới tính và không gửi `price_variant` → 422; KTV nghỉ việc hoặc liệu trình của khách khác → 422; `check_in_at` ở tương lai → 422; e2e Playwright cho luồng nhanh.
  - **Security:** update nguyên tử có `CHECK`; không nhận đơn giá từ client; idempotency; ghi chú buổi mã hóa.
  - **Depends on:** S6
  - **Kết quả:**
    - Migration `0007_visits`: `visit` (không có giờ ra, `UNIQUE(tenant_id, sale_id)`) và `visit_line` (`unit_price`, `line_value`, `sessions_left_after`, một `CHECK` theo `source`). app_rw không DELETE; `visit_line` chỉ được UPDATE cột `staff_id` (grant theo cột).
    - Crate mới `operations`: `POST /visits` (bắt buộc `Idempotency-Key`), `GET /visits?date=`, `GET /visits/{id}` (có `ETag`), `PATCH /visits/{id}` (If-Match; sửa ghi chú và KTV, không đụng buổi hay tiền). Một `TenantTx`: claim idempotency → kiểm tra giờ vào → khách → KTV (`org::require_active`) → giá dịch vụ lẻ (`catalog::service_snapshot`, khóa `FOR SHARE`) → trừ buổi (`packages::consume_session`, **theo thứ tự item_id** để không deadlock) → `sales::record_service_sale` (sale `line_type='service'`, payments) → visit + lines → audit → lưu response.
    - Quy tắc: giá lẻ theo giới tính của khách; khách chưa có giới tính + dịch vụ giá Nữ/Nam + không gửi `price_variant` → `422 price.gender_required`; Nữ-only cho khách Nam → `422 price.variant_not_available`; `price_variant` trái giới tính đã ghi → 422. Không gửi item → gói còn dùng được hết hạn sớm nhất, rồi mua sớm nhất (AC4.3). Gói hết hạn + lý do + `package.use_expired_override` → cho qua, lưu lý do trên dòng, audit `customer_package.use_expired_override`. Giờ vào > now + 5 phút → `422 visit.check_in_in_future`; khác ngày nghiệp vụ → `422 visit.check_in_not_today`.
    - Ghi chú buổi: AES-256-GCM qua `kernel::crypto::FieldCipher` (AAD `visit.note` + tenant + visit). Không bao giờ có trong response của `POST` (vì response được lưu ở bảng idempotency, không mã hóa), trong danh sách ngày, hay trong audit. Chỉ `GET /visits/{id}` trả về, và chỉ khi có `customer.health_notes.read`. Ghi cần `customer.health_notes.write`.
    - Thêm API cho context khác: `crm::service::brief_in`, `catalog::service::service_snapshot`, `packages::service::default_item`, `Consumed { per_session_value, package_name, used_expired }`, `sales::service::{parse_payments, record_service_sale, payments_of}`, `kernel::TenantProfile.timezone`. `sale.closer_staff_id` của sale dịch vụ lẻ là NULL.
    - Web `/quay` (mục nav "Quầy", cần `visit.record`): ô tìm (dùng lại `CustomerSearch`, thêm `inputRef`/`autoFocus`), tạo khách inline, thẻ khách (cảnh báo sức khỏe, liệu trình theo thứ tự dùng, "Trừ 1 buổi" cho từng dòng gói, số buổi trừ tạm, hộp lý do khi gói hết hạn), giỏ lượt (KTV bắt buộc, KTV làm nhiều nhất hôm nay xếp đầu), dịch vụ lẻ (theo nhóm, ★ trước, nút Nữ/Nam khi chưa rõ giới tính), thanh toán TM / CK / kết hợp, ghi chú buổi, toast, danh sách hôm nay. Phím tắt `/`, `↑↓ Enter`, `Ctrl+Enter`, `Esc`. 409 → toast + tải lại số buổi, giữ giỏ; lỗi mạng → giữ giỏ, gửi lại cùng key.
    - **Luồng nhanh đúng 4 thao tác** (kiểm bằng Playwright, chỉ dùng bàn phím, assert focus ở mỗi bước): (1) gõ SĐT + Enter → focus nhảy tới "Trừ 1 buổi"; (2) Enter → thêm dòng, focus nhảy tới ô KTV; (3) chọn KTV; (4) Ctrl+Enter → toast "… còn 11/12", form reset, focus về ô tìm.
    - Test: 16 test HTTP `visits.rs` (mọi case của dòng Tests, gồm **2 request song song tranh buổi cuối → đúng 1 thành công**, double submit cùng key → 1 lượt kể cả khi gửi đồng thời, Nam + Đại Thông Lạc → 399.000đ) + 4 unit `operations::domain`; 4 dòng mới trong `authz_matrix.rs` (`Target::Visit`); 8 web test; Playwright `e2e/specs/front-desk.spec.ts` + axe ở 3 trạng thái. Mutation check: bỏ quy tắc giới tính và bỏ chặn gói hết hạn đều làm test đỏ.
    - Lighthouse: thêm `/quay`. `pnpm lhci` giờ chạy `apps/web/lighthouse/run.mjs`: lượt 1 đo `/dang-nhap` khi chưa đăng nhập (như S10), lượt 2 đo `/quay` bằng tài khoản quản lý mà setup E2E tạo (chủ spa có TOTP nên không dùng được). Đã sửa CLS 0,109 → 0,009 (header xuống dòng khi font web tải xong; giờ header luôn một hàng, nav cuộn ngang). Route được tách chunk (trừ `/quay`). Đo trên máy local, LCP khoảng 3,0 s cho **cả** `/dang-nhap` (CI của main vẫn đạt) lẫn `/quay`. Nút thắt chung là zod (~51 KB gz) trong `session.ts` trên đường render đầu tiên, cộng 5 file font. Nếu CI cũng vượt 2,5 s thì cần một ticket giảm bundle.
    - **Lỗi tìm ra khi chạy E2E:** rate limit theo IP hiểu nhầm `tower_governor::per_second(n)` (đó là chu kỳ n giây, không phải n request/giây). Thực tế chỉ cho burst 60 rồi 12 request/phút cho mỗi IP, trong khi mọi máy của một spa dùng chung một IP NAT, nên quầy (~10 request/lượt) sẽ bị 429. Đã sửa thành 5 request/giây, có test đỏ → xanh.
    - Lệch so với thiết kế: **không có giảm giá trong lượt** (backend.md có `discount`; ticket không yêu cầu; để sau); không có giờ ra (EM); ghi lượt cho ngày cũ chưa hỗ trợ; `customer_package_item_id` không bắt buộc (server chọn mặc định); thêm code `price.gender_required`, `visit.check_in_in_future`, `visit.check_in_not_today`, `visit.voided`; `GET /customers/{id}/visits` chưa làm.
    - Cho S8: hủy lượt cần `visit_line.customer_package_item_id` + `qty` để cộng lại buổi (cần thêm `packages::service::restore_sessions`), và `visit.sale_id` để hủy sale dịch vụ lẻ (reversal payment). `visit.status/void_*` đã có sẵn.

- [x] **S8 Hủy / sửa sai & vòng đời liệu trình** (size: M) ✅ 2026-11-04
  - **Goal:** hủy lượt/hóa đơn có lý do (hoàn buổi, bút toán đảo); chủ spa gia hạn, hủy gói kèm hoàn tiền.
  - **Acceptance:** AC6.1–AC6.4, AC7.1–AC7.4.
  - **Touches:** `POST /visits/{id}/void`, `POST /sales/{id}/void`, `POST /customer-packages/{id}/extend|cancel`; `refund`; quy tắc "ngày đã chốt" tạm thời = trước hôm nay (cho tới spec #2). Web: nút hủy trong danh sách lượt, hành động trên liệu trình, hộp thoại nhập lại mật khẩu.
  - **Tests:** hủy lượt → buổi được cộng lại, lượt còn trong lịch sử với trạng thái đã hủy; hủy 2 lần → lần 2 bị chặn; hủy không lý do → 422; Manager hủy ngày cũ → 403 `day.locked`, Owner được nhưng phải reauth (bỏ reauth 2026-11-05, [ADR-0004](../decisions/ADR-0004-authentication.md) update); hủy hóa đơn gói đã dùng buổi → 409; hoàn tiền > đã thu → 422; Manager gia hạn → 403; reversal không đảo được 2 lần.
  - **Security:** thao tác 🔒 cần reauth (bỏ cho thao tác nghiệp vụ 2026-11-05, ADR-0004 update); append-only; audit có lý do.
  - **Depends on:** S7
  - **Kết quả:**
    - Migration `0009_void_lifecycle` (0008 để trống, từng giữ cho S9). Bảng `refund` (append-only, một gói hoàn tối đa 1 lần, trigger hoàn ≤ `sold_price`), cột `customer_package.cancelled_by`. Trigger: reversal phải "soi gương" payment gốc (cùng sale, method, amount) và không đảo được reversal; `voided`/`cancelled` là trạng thái cuối (visit, sale, customer_package không UPDATE được nữa); không cộng buổi vào gói đã hủy. CHECK: sale `voided` phải có `voided_by`, `amount_paid = 0`, lý do; gói `cancelled` phải có `cancelled_by` + lý do.
    - **Thiết kế bút toán đảo:** dùng lại `payment.kind='reversal'` + `reversal_of` đã có từ 0005 (không tạo bảng `sale_reversal`). Lý do: sổ quỹ một ngày = tổng `payment` theo `business_date`, payment − reversal; reversal mang `business_date` của ngày hủy nên sổ ngày cũ không đổi; `UNIQUE (tenant_id, reversal_of)` chặn đảo 2 lần ngay trong DB; một bảng duy nhất cho mọi dòng tiền vào nên báo cáo spec #2 không phải `UNION`.
    - Endpoint (đều bắt buộc `Idempotency-Key`): `POST /visits/{id}/void`, `POST /sales/{id}/void` (chỉ hóa đơn bán gói; hóa đơn dịch vụ lẻ → `409 sale.void_via_visit`), `POST /customer-packages/{id}/extend` (If-Match), `POST /customer-packages/{id}/cancel` 🔒 (không còn 🔒 từ 2026-11-05). Body hủy `{reason}`; gia hạn `{expires_on, reason}`; hủy gói `{reason, refund_amount, refund_method?}`.
    - Quy tắc ngày đã chốt ở một chỗ: `kernel::corrections::authorize_void` (trước hôm nay theo múi giờ spa: Manager → `403 day.locked`, Owner cần reauth). Reauth: `Actor::require_recent_auth()`, cờ đặt trong extractor `AuthUser` từ `session.recent_auth_at` (mặc định tắt = deny). **Thay 2026-11-05:** Owner không cần reauth; `Actor` bỏ cờ `recent_auth`, guard còn lại là `identity::AuthUser::require_recent_auth` chỉ cho thao tác bảo mật tài khoản ([authz-matrix](../security/authz-matrix.md)).
    - API in-transaction mới: `packages::lifecycle::{restore_sessions, cancel_for_sale}`, `sales::voids::void_in` (dùng cho cả hủy lượt). Thứ tự khóa: visit/sale → gói (theo id) → item (theo id). Hủy gói đưa buổi về 0, nên lượt đang trừ buổi song song bị `409`.
    - DTO thêm (additive): `VisitDto.void_reason/voided_at`, `SaleDto.void_reason/voided_at`, `PaymentDto.reversal_of`, `CustomerPackageDto.cancelled_at/cancel_reason/refund`.
    - Web: nút "Hủy" trên từng lượt ở danh sách `/quay` (+ ô "Xem ngày" để chủ spa hủy lượt ngày cũ; Manager thấy "Ngày đã chốt"), lượt đã hủy hiện nhãn + lý do; trên hồ sơ khách mỗi liệu trình có "Gia hạn" (+1/2/3/6 tháng), "Hủy liệu trình + hoàn tiền" (chặn hoàn > đã trả ngay trên form), "Hủy hóa đơn bán gói" (khi chưa dùng buổi). `ReauthDialog` (mật khẩu, hiện ô mã TOTP khi server đòi) dùng chung qua `useGuardedAction`: gặp `401 auth.reauth_required` → hỏi lại → gửi lại cùng key (đã bỏ 2026-11-05: thao tác nghiệp vụ không hỏi lại mật khẩu, form dùng `useFormAction`).
    - Perf (frontend.md "Perf budget"): form hủy lượt (`VoidVisitForm`) và `PackageActions` là chunk lazy (0,4 KB + 1,6 KB + 2,5 KB gzip), bundle chính không đổi. **Lỗi tìm ra khi chạy `pnpm lhci`:** nút "Hủy" trong hàng `flex-wrap` làm hàng xuống dòng, cột phải cao hơn nên CLS `/quay` lên 0,27; giữ mỗi hàng một dòng flex → CLS 0,045 (S7 gốc 0,043), LCP 2,06 s (local).
    - Test: 14 HTTP `voids.rs` (mọi case của dòng Tests + 2 lượt hủy song song → đúng 1 thành công, retry cùng key, DB chặn đảo 2 lần / đảo reversal / sửa payment / mở lại sale) + 4 dòng mới `authz_matrix.rs` + unit (`kernel::corrections` 3, `Actor` 1, `lifecycle` 1); 11 web (`Corrections.test.tsx`); Playwright `e2e/specs/voids.spec.ts` (axe 5 màn hình). Mutation check: bỏ chặn hủy 2 lần, bỏ chặn hoàn > đã trả, bỏ khóa ngày → test đỏ, rồi khôi phục.
    - Lệch / chưa làm: không có trang chi tiết lượt riêng (hủy ngay trên danh sách); `PATCH /visits/{id}` ngày cũ (S7) cho Owner sửa mà **chưa** đòi reauth → để S10; hoàn tiền gói nhập sổ cũ tối đa bằng giá ghi trong sổ; chưa có báo cáo sổ quỹ (spec #2) đọc `refund`/reversal.

- [x] **S9 Nhập liệu trình dở dang từ sổ cũ** (size: S)
  - **Goal:** nhập nhanh khoảng 1–2 phút mỗi khách; không phát sinh doanh thu.
  - **Acceptance:** AC8.1, AC8.3–AC8.5.
  - **Touches:** `POST /opening-balances` (`origin=opening_balance`, không tạo sale hay payment). Web: `/nhap-so-cu` (tạo khách + gói trong một form), nhãn "Nhập từ sổ cũ".
  - **Tests:** còn 4/10 → tạo đúng; còn > tổng → 422; không có payment nào trong ngày nhập; Technician → 403; idempotency.
  - **Security:** permission `opening_balance.import`; audit.
  - **Depends on:** S6
  - **Kết quả:**
    - Không cần migration: permission `opening_balance.import` đã có trong `kernel::perm` và seed role (Owner, Manager) từ S2, nên tenant đã tạo cũng có quyền. (Quản lý mất quyền này ở migration 0013; nhập sổ giấy nay là việc của team qua CLI `app import-paper-book`, route chỉ còn tạm cho chủ spa.)
    - `POST /opening-balances` (bắt buộc `Idempotency-Key`), module `packages::opening_balance`. Body: `customer_id` **hoặc** `customer` (hồ sơ khách mới, giống `POST /customers`) + `packages[1..10]` gồm `package_plan_id?`, `name?` (mặc định tên gói), `sold_price`, `starts_on` (không ở tương lai), `expires_on?`, `gift_note?`, `items[{service_id, sessions_purchased, sessions_bonus=0, sessions_remaining}]`. Trả `201 {customer_id, customer_created, customer_packages[]}`.
    - Một transaction: claim idempotency → tìm hoặc tạo khách → kiểm tra tất cả gói (báo mọi lỗi một lần, field có tiền tố `customer.` / `packages[j].`) → `packages::service::create(origin=OpeningBalance)` → audit `opening_balance.import` + `customer_package.create`. Không tạo `sale`, `sale_line`, `payment`, nên doanh thu và sổ quỹ không bị ảnh hưởng (AC8.1, AC8.3).
    - Số buổi và giá lấy theo sổ giấy (client gửi), vì đây là dữ liệu lịch sử chứ không phải bán mới; gói trong bảng giá chỉ để tham chiếu và lấy tên mặc định. Dịch vụ phải thuộc spa (`422 …service_id not_found`); khách hoặc gói của spa khác → `404`; trùng SĐT → `409 customer.phone_taken` + `existing_id`.
    - Hàm public mới (thêm, không phá vỡ): `crm::service::create_in(tx, cipher, actor, input, health_note)` (`create` giờ gọi nó), `catalog::service::service_name(tx, id)`, `packages::service::validate(&NewPackage)`. `packages::http::routes` cần thêm `Arc<FieldCipher>` trong state.
    - Web `/nhap-so-cu` (menu "Nhập sổ cũ" theo quyền; **đã gỡ khỏi web** sau 2026-11-05, cùng nhãn "Nhập từ sổ cũ" trên UI): tìm khách (↑↓ Enter) hoặc "+ Khách mới" ngay trong form; mỗi gói chọn từ bảng giá (kể cả gói ngừng bán) hoặc "Gói khác (nhập tay)" nhiều dịch vụ; kiểm tra còn lại ≤ tổng ngay trên form; "Lưu và nhập tiếp" xóa form, tạo key mới, đưa con trỏ về ô tìm khách; đếm số liệu trình đã nhập; trùng SĐT → nút "Dùng hồ sơ đó". Nhãn "Nhập từ sổ cũ" trên hồ sơ đã có từ S6, nay có test.
    - Test: 8 E2E API `opening_balances.rs` + 1 dòng `authz_matrix.rs` + 1 unit; 10 web; Playwright `e2e/specs/opening-balance.spec.ts` (axe sạch; đã xóa cùng màn). Mutation check: bỏ guard còn lại ≤ tổng và cho import ghi một payment → test đỏ, rồi khôi phục.
    - Chưa làm: "còn nợ" của US8/front-desk-flows (công nợ hoãn ở R1, US5); bộ đếm là theo phiên trình duyệt chứ không phải "hôm nay" từ server.

- [ ] **S10 Hardening & nghiệm thu với quản lý** (size: S)
  - **Goal:** chứng minh spec R1-1 xong.
  - **Acceptance:** mọi AC; non-functional (§6 spec).
  - **Touches:** test suite tổng hợp authz matrix (mọi route × mọi role) và cách ly tenant cho mọi route; CI kiểm tra mọi route đều khai báo permission; Lighthouse CI (LCP < 2,5 s ở `/quay`); kiểm tra a11y (bàn phím, axe); đo p95 các endpoint chính; buổi dùng thử với quản lý (mục tiêu < 30 giây/lượt).
  - **Tests:** như trên + `/qa` tìm khoảng trống (Unicode, múi giờ, gửi đôi).
  - **Security:** chạy `secure` trên toàn bộ diff R1-1; `/pr-review` cuối cùng.
  - **Depends on:** S7, S8, S9
  - **Hạ tầng test (xong, 2026-11-04, branch `test/s10-infra`):** cách dùng xem [testing](../conventions/testing.md).
    - `apps/api/crates/app/tests/authz_matrix.rs` có một bảng `ROUTES`. CI fail khi route có trong router hoặc OpenAPI mà thiếu dòng trong bảng. Test chạy: 401 khi ẩn danh hoặc cookie giả; role × route → 403/allowed; ID của spa khác → 404; cột role phải khớp `roles.rs`.
    - Playwright `apps/web/e2e/`: stack riêng (API :8091 + Vite :5181, DB `vpyd_e2e`), setup project provision qua CLI. Spec phủ login → bảng giá, guard `next`, `next` ngoài site, tạo khách → tìm không dấu → cảnh báo sức khỏe, và axe trên login/bảng giá/khách hàng/hồ sơ.
    - Perf: p95 `/customers?q=` và catalog < 300 ms. `lighthouserc.json` (LCP < 2,5 s) hiện chỉ đo `/dang-nhap`.
    - CI: job `e2e`.
    - **Còn lại cho S10:** (matrix đã có dòng cho mọi route S6–S9) `PATCH /visits/{id}` ngày cũ cần reauth như hủy (về sau bỏ reauth, 2026-11-05); `/quay` đã có trong Lighthouse (S7); test role PlatformAdmin/support grant khi có role (thay bằng lớp operator của ADR-0018, test `every_admin_route_needs_a_full_operator_session`; support grant chưa xây); `/qa` tìm khoảng trống; buổi dùng thử với quản lý.
  - **Kết quả hardening (2026-11-04, branch `test/s10-hardening`; ticket chưa tick, chờ buổi dùng thử [r1-1-trial-session](r1-1-trial-session.md)):**
    - **Lỗi tìm ra và đã sửa** (mỗi lỗi có test hoặc số đo đỏ trước khi sửa):
      1. **[high] Deadlock → 500** khi ghi lượt trừ buổi **song song** với hủy hóa đơn bán gói, hủy lượt, hoặc chủ spa hủy liệu trình trên cùng gói. Ghi lượt khóa `customer_package_item` trước, rồi FK `visit_line → customer_package` lấy `FOR KEY SHARE` trên gói; hủy thì khóa gói `FOR UPDATE` rồi mới tới item → 40P01. Sửa: `packages::service::lock_for_consumption` khóa `FOR SHARE` các gói (theo id) trước khi trừ buổi, nên thứ tự khóa luôn là gói → item. Test `hardening.rs::voiding_a_package_sale_while_a_visit_uses_it_leaves_one_consistent_outcome` (5 vòng hủy hóa đơn + 5 vòng hủy liệu trình, đúng một bên thắng, không 5xx).
      2. **[medium] `PATCH /visits/{id}` ngày đã chốt** không đòi reauth (Owner sửa KTV/ghi chú ngày cũ không cần nhập lại mật khẩu). Sửa: `kernel::corrections::authorize_closed_day` dùng chung với `authorize_void`. Test `voids.rs::correcting_a_closed_day_visit_follows_the_void_rule`. (Từ 2026-11-05 quy tắc ngày đã chốt giữ nguyên nhưng Owner không còn phải reauth.)
      3. **[medium] Ký tự NUL (`\u0000`) trong JSON → 500** ở mọi field text (tên khách, tên dịch vụ, lý do, ô tìm). Postgres `text` không chứa được NUL. Sửa ở một chỗ: `AppError::Database` với SQLSTATE `22021`/`22P05` → `422 validation_failed` (`body: invalid_character`), log mức warn. Test `nul_characters_are_rejected_as_422_not_500`.
      4. **[low] Giá trong bảng giá không có trần** (G6 "vượt giới hạn → 422"): giá dịch vụ / giá gói `i64::MAX` được lưu. Sửa: `Money::MAX_INPUT` = 10 tỷ đồng (đã dùng cho giá sổ cũ ở S9), `prices`/`price` vượt → `422 out_of_range`. Test `catalog_amounts_above_the_limit_are_422`.
      5. **[low] Ô tìm khách không giới hạn độ dài** → giới hạn 100 ký tự, `422 q:too_long`. Test `search_queries_are_bounded`.
      6. **[low] AC8.5** nhãn "Nhập từ sổ cũ" chỉ có ở hồ sơ, thiếu trên thẻ khách ở `/quay`. Đã thêm + test web (mutation ✓).
      7. **[perf] CLS `/quay` 0,156 (> 0,1) vào ngày đông khách** (104 lượt, do perf spec mới tạo): lần tải đầu, web font swap làm chữ xuống dòng khác và đẩy cả trang. Sửa: `@font-face` fallback khớp số đo (`size-adjust`…) trong `packages/ui/src/tokens/tokens.css` → CLS 0,013 với cùng dữ liệu. Quy tắc ở [frontend](../architecture/frontend.md) "Perf budget".
      8. **[test] E2E reauth không tất định**: `voids.spec.ts` giờ làm cũ `recent_auth_at` của chủ spa qua `e2e/support/db.ts` (client `postgres`, chỉ DB E2E) nên hộp thoại xác nhận lại mật khẩu luôn hiện. (Từ 2026-11-05 test này kiểm ngược lại: `recent_auth_at` cũ mà **không** có hộp thoại.)
    - **Test mới** `apps/api/crates/app/tests/hardening.rs`: Unicode (NFD → NFC, tìm "Đặng"/"đặng"/"ĐẶNG"/"dang"/chuỗi NFD, emoji, giới hạn 200 *ký tự* với chữ 3 byte, tên toàn khoảng trắng Unicode), ranh giới ngày nghiệp vụ (00:00:00 giờ VN = 17:00 UTC hôm trước vẫn là hôm nay; 23:59:59 hôm trước → `422 visit.check_in_not_today`; payment ghi đúng ngày VN), tiền (trần 10 tỷ, overflow 2 payment `i64::MAX` → 422, `per_session_value` 1.000.000 ÷ 3 = 333.333 làm tròn xuống, gói 0đ không thu tiền, chủ spa giảm 100%, giảm > giá → 422), đồng thời (hủy hóa đơn/hủy gói vs ghi lượt; 2 PATCH cùng version → 200 + 412), gửi đôi ở route không có Idempotency-Key (khách: khóa tự nhiên SĐT → 201 + 409 kèm `existing_id`; dịch vụ: tên duy nhất → đúng 1 bản ghi), body hỏng (`{`, `[]`, sai kiểu) trên 15 route ghi → 4xx `application/problem+json` có `code`, G5 audit cho 9 action chưa có test audit (+ ghi chú sức khỏe không lọt vào audit).
    - **Authz matrix:** đủ 37 route của `ApiDoc` (lúc đó; 2026-11-09 là 133 dòng) có dòng trong `ROUTES` (cổng CI `every_mounted_route_has_a_matrix_row`); mọi ô role × route (Owner/Manager/Technician) được chạy trong `each_role_gets_exactly_what_the_matrix_grants`. **PlatformAdmin / support grant: ngoài phạm vi R1-1** (không có role, bảng hay route trong code), ghi ở [authz-matrix](../security/authz-matrix.md). (Từ 2026-11-09: console operator riêng, ADR-0018; support grant vẫn chưa có.)
    - **A11y:** axe giờ chạy trên mọi màn hình: thêm `e2e/specs/a11y.spec.ts` (trang chủ, `/bang-gia/goi` + form gói, form dịch vụ) và axe ở bước MFA (`/xac-thuc` enroll/verify) trong `signIn`. Luồng nhanh bàn phím `/quay` đã có (S7, `front-desk.spec.ts`).
    - **Perf:** `e2e/perf/api-latency.spec.ts` thêm test thứ hai: `POST /sales`, `POST /visits` (trừ buổi + dịch vụ lẻ), `POST /opening-balances` (ngân sách 500 ms p95, spec §6 "lưu lượt") và `GET /visits` (ngày > 100 lượt), `GET /customers/{id}`, `/customers/{id}/packages` (> 200 gói), `GET /sales/{id}`, `GET /staff` (300 ms). p95 (local, API debug, 1 client): tìm khách 14 ms, catalog 13–16 ms, `POST /sales` 41, `POST /visits` 37, `POST /opening-balances` 24, `GET /visits` (104 lượt) 38, hồ sơ 13, liệu trình của khách (> 200 gói) 34. Lighthouse: `/dang-nhap` LCP ~1,9–2,1 s, `/quay` ~2,5 s (máy đang tải nặng vì nhiều agent chạy song song), CLS ≤ 0,013; `pnpm lhci` xanh. Chi tiết: log 2026-11-04 "S10 hardening".
    - **Độ phủ AC** (Rust = `apps/api/crates/app/tests/<file>::<test>`, web = Vitest, E2E = Playwright `e2e/specs`):

      | AC | Test |
      |---|---|
      | G1 401 | `authz_matrix::every_non_public_route_rejects_anonymous_and_forged_sessions` |
      | G2 403 | `authz_matrix::each_role_gets_exactly_what_the_matrix_grants` |
      | G3 404 tenant khác | `authz_matrix::ids_from_another_spa_are_not_found` |
      | G4 idempotency | `sales::saving_twice_with_one_key_creates_one_sale`, `visits::double_submit_with_the_same_key_records_one_visit`, `voids::a_visit_is_voided_once` (retry), `opening_balances::double_submit_with_one_key_imports_once`, kernel idempotency tests; route không có key: `hardening::double_submits_of_natural_key_writes_create_one_record` |
      | G5 audit | test audit theo slice (sale/visit/void/extend/cancel/customer.create/plan_create/opening) + `hardening::every_catalog_and_profile_write_is_audited` |
      | G6 tiền | `kernel::money` unit, `sales::payments_must_add_up_to_the_total_exactly`, `hardening::catalog_amounts_above_the_limit_are_422`, `hardening::per_session_value_rounds_down_and_free_packages_take_no_payment` |
      | AC1.1 | `catalog_manage::owner_creates_a_package_plan_visible_at_the_counter` |
      | AC1.2 | `sales::later_plan_edits_do_not_change_sold_packages` |
      | AC1.3 | `catalog_manage::deactivated_services_leave_the_menu_but_are_never_deleted`, `sales::inactive_plans_and_unknown_customers_cannot_be_sold` |
      | AC1.4 | `catalog_manage::managers_cannot_change_the_price_list` |
      | AC1.5 | `catalog_manage::invalid_plans_are_rejected_with_field_errors` |
      | AC1.6 | `catalog_manage::deactivated_services_leave_the_menu_but_are_never_deleted` (không có route xóa) |
      | AC1.7 | `catalog::seeded_menu_is_served_with_categories_and_price_variants`, web `PriceListPage.test` |
      | AC1.8 | `catalog_manage::price_variant_rules_are_enforced`, `activating_the_unpriced_service_requires_its_price` |
      | AC1.9 | `catalog::seeded_menu_is_served_with_categories_and_price_variants` |
      | AC2.1 | `customers::creates_a_customer_and_finds_her_by_phone_or_accentless_name`, `customers::search_stays_fast_with_ten_thousand_customers`, E2E perf p95 |
      | AC2.2 | web `Customers.test`, `FrontDesk.test` (tạo khách inline), E2E `customers.spec` |
      | AC2.3 | `customers::equivalent_phone_spellings_are_one_customer`, `kernel::phone` unit |
      | AC2.4 | `customers::equivalent_phone_spellings_are_one_customer`, `hardening::double_submits_of_natural_key_writes_create_one_record` |
      | AC2.5 | `customers::equivalent_phone_spellings_are_one_customer` (sai định dạng → 422) |
      | AC2.6 | `customers::health_notes_are_encrypted_and_hidden_without_permission`, `visits::session_note_is_encrypted_and_shown_only_to_health_readers` |
      | AC2.7 | `customers::decomposed_unicode_names_are_stored_composed_and_found`, `hardening::names_from_any_keyboard_are_stored_nfc_and_found_with_or_without_accents` |
      | AC3.1 | `sales::ten_plus_two_at_1_5m_gives_12_sessions_worth_150k_each` |
      | AC3.2 | `sales::mixed_cash_and_transfer_is_two_payments` |
      | AC3.3, AC3.4 | `sales::payments_must_add_up_to_the_total_exactly` |
      | AC3.5 | `sales::discounts_need_a_reason_and_managers_stop_at_ten_percent` |
      | AC3.6 | `sales::the_closer_must_be_working_here` |
      | AC3.7 | `sales::saving_twice_with_one_key_creates_one_sale`, E2E `sales.spec` |
      | AC4.1 | `visits::deducting_one_session_takes_5_of_12_to_4_of_12`, E2E `front-desk.spec` |
      | AC4.2 | `visits::one_off_service_creates_a_sale_and_payment_tied_to_the_visit` |
      | AC4.2b | `visits::one_off_price_follows_the_customers_gender`, `customer_without_gender_needs_a_price_variant_for_gendered_services`, web `FrontDesk.test` |
      | AC4.2c | `visits::female_only_service_for_a_male_customer_is_422` |
      | AC4.3 | `visits::default_package_is_the_one_expiring_soonest`, web `FrontDesk.test` |
      | AC4.4 | `visits::one_off_service_creates_a_sale_and_payment_tied_to_the_visit` (2 dòng cùng KTV = 1 khách) |
      | AC4.5, AC4.10 | `visits::exhausted_and_cancelled_packages_are_409` |
      | AC4.6 | `visits::expired_package_needs_a_reason_and_the_override_permission`, `packages::expired_packages_need_an_override` |
      | AC4.7 | `visits::two_devices_racing_for_the_last_session_exactly_one_wins`, `packages::two_devices_racing_…`, `hardening::voiding_a_package_sale_while_a_visit_uses_it_…` |
      | AC4.8 | `visits::technician_must_be_working_and_package_must_be_the_customers` |
      | AC6.1 | `voids::voiding_a_visit_gives_sessions_back_and_reverses_its_payment`, E2E `voids.spec` |
      | AC6.2 | `voids::a_void_needs_a_reason` |
      | AC6.3 | `voids::closed_days_are_locked_for_managers_and_open_to_owners_without_reauth`, `voids::correcting_a_closed_day_visit_follows_the_void_rule`, `kernel::corrections` unit |
      | AC6.4 | `voids::a_package_sale_with_used_sessions_cannot_be_voided` |
      | AC7.1 | `voids::owner_extends_an_expired_package_with_a_reason` |
      | AC7.2 | `voids::owner_cancels_a_package_with_a_refund`, E2E `voids.spec` (không hỏi lại mật khẩu, EM 2026-11-05) |
      | AC7.3 | `authz_matrix` (extend/cancel OWNER_ONLY), `voids::owner_extends_…` |
      | AC7.4 | `voids::refund_is_capped_by_what_was_paid_after_discount` |
      | AC8.1 | `opening_balances::four_of_ten_left_is_imported_without_any_sale_or_payment`, `packages::opening_balance_creates_a_package_without_money` |
      | AC8.3 | như AC8.1 (không có `sale`/`payment` nên báo cáo/sổ quỹ không thấy; báo cáo thuộc spec #2/#3) |
      | AC8.4 | `opening_balances::more_left_than_total_is_422_and_nothing_is_written` |
      | AC8.5 | web `Sales.test` (hồ sơ), `FrontDesk.test` "labels a package imported from the paper books" (quầy, S10), E2E `opening-balance.spec` — nhãn và E2E đã gỡ cùng màn sổ cũ (EM 2026-11-05 bỏ phân biệt sổ cũ trên UI) |
      | AC8.6–8.7 | `opening_balances::only_the_owner_imports_today_without_a_window_or_reauth`, AC6.3 tests |
      | AC8.8 | `opening_balances::an_opening_balance_refund_is_capped_by_its_book_price` |
      | §6 perf | `e2e/perf/api-latency.spec.ts` (2 test), `customers::search_stays_fast_with_ten_thousand_customers`, `pnpm lhci` (`/dang-nhap`, `/quay`) |
      | §6 a11y | axe trong mọi spec E2E + `a11y.spec.ts`; bàn phím `front-desk.spec.ts`; Lighthouse a11y ≥ 0,95 |

    - **Còn mở (không phải lỗi, để EM quyết):** lịch sử buổi làm trên hồ sơ khách (US2 "Màn hình hồ sơ", `GET /customers/{id}/visits` hoãn từ S7) chưa có (**đã có ở R1-2**: `GET /customers/{id}/history`); báo cáo "số khách của KTV trong ngày" (AC4.4) mới chỉ là dữ liệu (**đã có ở R1-2**: bảng KTV trong `GET /reports/daily`); `GET /visits` không phân trang (một ngày của một spa nhỏ, chấp nhận được); NUL bị chặn ở tầng DB nên lỗi trả về field `body` chứ không chỉ đúng field.

- [ ] **S11 Staging trên VPS** (size: S) — **thay bằng staging Vercel + Supabase (2026-11-05)**: đã chạy, có `app seed-demo`; phần VPS / backup R2 / uptime alert dưới đây chưa làm, để cho production ([ADR-0012](../decisions/ADR-0012-colocate-api-db-singapore.md)). Runbook hiện hành: [vercel-staging](../runbooks/vercel-staging.md).
  - **Goal:** chạy thật trên VPS Singapore + Supabase để quản lý dùng thử bằng dữ liệu giả.
  - **Acceptance:** ADR-0002/0005: Data API tắt, RLS bật, backup đêm ra R2, uptime alert.
  - **Touches:** `/devops`: chọn nhà cung cấp VPS, domain, Caddy production, deploy, backup, restore drill.
  - **Tests:** smoke production; thử khôi phục backup; kiểm tra Supabase Data API trả 404/không truy cập được.
  - **Security:** SSH key-only, firewall, secret trong env.
  - **Depends on:** S10 + **EM:** tạo project Supabase (Singapore, Pro), mua VPS + domain.
  - **Chuẩn bị (xong, 2026-11-04, branch `chore/s11-deploy-prep`):**
    - Đã sẵn sàng:
      - `apps/api/Dockerfile`: cargo-chef, `SQLX_OFFLINE`, distroless nonroot, HEALTHCHECK `/api/v1/health`.
      - `infra/Caddy.Dockerfile` + Caddyfile `{$DOMAIN}`: TLS tự động, CSP/HSTS/COOP/CORP, SPA fallback, `/assets` immutable.
      - `infra/docker-compose.prod.yml`: `migrate` one-shot bằng owner; `api` chỉ có URL `app_rw`; `caddy`; profile `ops` (backup) và `drill` (restore). Có giới hạn tài nguyên, read-only rootfs, `cap_drop`, log rotation.
      - `infra/.env.production.example`.
      - `infra/backup/`: pg_dump → age → R2, systemd timer, `restore.sh`.
      - `infra/deploy.sh` (deploy/rollback).
      - CI job `images` (build 3 image + validate compose).
      - `deploy.yml` (`workflow_dispatch` → GHCR → SSH).
      - `infra/prodtest/run.sh` (smoke test local).
      - Runbook [staging-setup](../runbooks/staging-setup.md).
    - EM đã quyết: staging Supabase ở **Mỹ**, Data API bật → VPS staging phải cùng vùng, giảm thiểu bằng REVOKE. Lead tự bootstrap DB staging. Production sau này là project Singapore mới.
    - Còn chờ EM (runbook §9):
      - Mua VPS (DigitalOcean 2 GiB $12, datacenter khớp vùng Supabase) + domain.
      - R2 bucket + token.
      - Khóa `age`.
      - GitHub environment `staging`.
      - Monitor Better Stack + Healthchecks.io.
    - Sau đó: deploy, smoke, backup thử, restore drill, rồi tick S11.
    - **Smoke test local đã chạy (2026-11-04):** `sh infra/prodtest/run.sh` qua hết. Image: api 40 MB, web 59 MB, backup 117 MB. migrate bằng owner OK, API healthy, Caddy phục vụ SPA + proxy `/api`, CSP/HSTS/cache đúng, API không mở cổng ra host, backup → restore drill khôi phục đủ 4 migration. 2 bảng không có RLS là `user_account`, `session` (toàn cục theo ADR-0003/0004, đúng thiết kế).
    - Lỗi tìm ra khi chạy: file prodtest mount `../postgres-init` nhưng compose tính đường dẫn từ thư mục của file `-f` đầu tiên (`infra/`) → role `app_rw` không được tạo. Đã sửa thành `./postgres-init`.

## Kiểm tra độ phủ
| Spec | Ticket |
|---|---|
| G1–G6 | S1, S2 (+ test trong mọi ticket) |
| US1 | S3, S4 |
| US2 | S5 |
| US3 | S6 |
| US4 | S7 |
| US5 | **hoãn** (EM: R1 thu đủ) |
| US6, US7 | S8 |
| US8 | S9 |
| §6 Non-functional | S10 |

Không ticket nào size L. **Chờ EM** (không chặn S0–S2): giá Ôn ấm tử cung, danh sách gói liệu trình (seed ở S4/S6 hoặc chủ spa tự tạo).
