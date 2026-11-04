---
title: Plan R1-1 — Màn hình quầy & liệu trình
updated: 2026-11-04
sources: [product/specs/front-desk-and-packages.md, architecture/backend.md, architecture/data.md, architecture/frontend.md, security/authz-matrix.md]
---

# Plan R1-1: Màn hình quầy & liệu trình

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

- [ ] **S2b MFA TOTP cho chủ spa** (size: S)
  - **Goal:** chủ spa (và platform admin) bắt buộc TOTP khi đăng nhập (ADR-0004).
  - **Touches:** migration thêm `mfa_secret_ct`, `mfa_enabled`; enroll (QR) + verify; login bước 2; mã khôi phục.
  - **Tests:** sai mã → 401; replay mã cũ → 401; owner chưa bật MFA → bắt enroll.
  - **Depends on:** S2. Làm trước S11 (staging).

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

- [ ] **S5 Hồ sơ khách hàng & tìm kiếm** (size: M)
  - **Goal:** tìm khách theo SĐT/tên (không dấu) < 300 ms, tạo khách, xem/sửa hồ sơ; ghi chú sức khỏe mã hóa và chỉ trả về khi có quyền.
  - **Acceptance:** AC2.1–AC2.7.
  - **Touches:** crate `crm` (`customer`, chuẩn hóa SĐT → E.164, `search_text`, GIN trgm, AES-256-GCM cho `health_note`); `GET/POST/PATCH /customers`, `GET /customers/{id}`. Web: ô tìm khách và thẻ khách như prototype, form khách mới, trang `/khach-hang/:id`.
  - **Tests:** "hà" tìm được "Hà"; 3 dạng SĐT là cùng một số; trùng SĐT → 409 kèm `existing_customer_id`; NFC; thiếu `customer.health_notes.read` → response không có key `health_note`; ciphertext trong DB không chứa plaintext; p95 tìm kiếm < 300 ms với 10.000 khách seed; Technician → 403.
  - **Security:** IDOR (ID của tenant khác → 404); key mã hóa lấy từ env (`secrecy`), có phiên bản; audit không chứa plaintext sức khỏe.
  - **Depends on:** S2 (song song với S3/S4)

- [ ] **S6 Bán liệu trình** (size: M)
  - **Goal:** bán gói cho khách, ghi người chốt, giảm giá có lý do, thu đủ (TM/CK/kết hợp); liệu trình được tạo với snapshot.
  - **Acceptance:** AC3.1–AC3.7; AC1.2 (snapshot).
  - **Touches:** crates `sales` (`sale`, `sale_line`, `payment` append-only), `packages` (`customer_package`, `customer_package_item`, trạng thái suy ra); `POST /sales`, `GET /sales/{id}`, `GET /customer-packages/{id}`; `org` tối thiểu (`staff` + `GET /staff`). Web: hộp thoại "Bán liệu trình mới" trên thẻ khách.
  - **Tests:** gói 10 + tặng 2 giá 1.500.000đ → 12/12 buổi, `per_session_value` = 150.000đ; tổng thanh toán ≠ total → `422 payment.amount_mismatch`; giảm giá thiếu lý do → 422, Manager giảm > 10% → 403; người chốt đã nghỉ việc → 422; bấm Lưu 2 lần (cùng key) → 1 hóa đơn; client gửi giá → bị bỏ qua (server tính); `payment` không UPDATE/DELETE được.
  - **Security:** tính giá phía server; không mass assignment; idempotency; audit.
  - **Depends on:** S4, S5

- [ ] **S7 Ghi lượt khách (màn hình quầy)** (size: M)
  - **Goal:** luồng chính: trừ buổi và/hoặc dịch vụ lẻ theo giá giới tính, chọn KTV, thu đủ, lưu ≤ 4 thao tác; danh sách lượt hôm nay.
  - **Acceptance:** AC4.1–AC4.8, AC4.10, AC4.2b, AC4.2c; AC4.4 (dữ liệu số khách cho R2).
  - **Touches:** crate `operations` (`visit`, `visit_line`, gọi `packages` + `sales` service trong cùng `TenantTx`); `POST /visits`, `GET /visits?date=`, `GET/PATCH /visits/{id}`. Web: trang `/quay` theo prototype (giỏ lượt, chọn KTV, thanh toán, toast, phím tắt).
  - **Tests:** trừ buổi 5/12 → 4/12; chọn liệu trình mặc định là gói hết hạn sớm nhất; hết buổi → 409; gói hết hạn → 409, kèm lý do + quyền → cho qua và ghi audit; **2 request song song trừ buổi cuối → đúng 1 thành công** (test đồng thời); khách Nam + Đại Thông Lạc → 399.000đ; dịch vụ chỉ dành cho Nữ với khách Nam → 422; khách chưa có giới tính và không gửi `price_variant` → 422; KTV nghỉ việc hoặc liệu trình của khách khác → 422; `check_in_at` ở tương lai → 422; e2e Playwright cho luồng nhanh.
  - **Security:** update nguyên tử có `CHECK`; không nhận đơn giá từ client; idempotency; ghi chú buổi mã hóa.
  - **Depends on:** S6

- [ ] **S8 Hủy / sửa sai & vòng đời liệu trình** (size: M)
  - **Goal:** hủy lượt/hóa đơn có lý do (hoàn buổi, bút toán đảo); chủ spa gia hạn, hủy gói kèm hoàn tiền.
  - **Acceptance:** AC6.1–AC6.4, AC7.1–AC7.4.
  - **Touches:** `POST /visits/{id}/void`, `POST /sales/{id}/void`, `POST /customer-packages/{id}/extend|cancel`; `refund`; quy tắc "ngày đã chốt" tạm thời = trước hôm nay (cho tới spec #2). Web: nút hủy trong danh sách lượt, hành động trên liệu trình, hộp thoại nhập lại mật khẩu.
  - **Tests:** hủy lượt → buổi được cộng lại, lượt còn trong lịch sử với trạng thái đã hủy; hủy 2 lần → lần 2 bị chặn; hủy không lý do → 422; Manager hủy ngày cũ → 403 `day.locked`, Owner được nhưng phải reauth; hủy hóa đơn gói đã dùng buổi → 409; hoàn tiền > đã thu → 422; Manager gia hạn → 403; reversal không đảo được 2 lần.
  - **Security:** thao tác 🔒 cần reauth; append-only; audit có lý do.
  - **Depends on:** S7

- [ ] **S9 Nhập liệu trình dở dang từ sổ cũ** (size: S)
  - **Goal:** nhập nhanh khoảng 1–2 phút mỗi khách; không phát sinh doanh thu.
  - **Acceptance:** AC8.1, AC8.3–AC8.5.
  - **Touches:** `POST /opening-balances` (`origin=opening_balance`, không tạo sale hay payment). Web: `/nhap-so-cu` (tạo khách + gói trong một form), nhãn "Nhập từ sổ cũ".
  - **Tests:** còn 4/10 → tạo đúng; còn > tổng → 422; không có payment nào trong ngày nhập; Technician → 403; idempotency.
  - **Security:** permission `opening_balance.import`; audit.
  - **Depends on:** S6

- [ ] **S10 Hardening & nghiệm thu với quản lý** (size: S)
  - **Goal:** chứng minh spec R1-1 xong.
  - **Acceptance:** mọi AC; non-functional (§6 spec).
  - **Touches:** test suite tổng hợp authz matrix (mọi route × mọi role) và cách ly tenant cho mọi route; CI kiểm tra mọi route đều khai báo permission; Lighthouse CI (LCP < 2,5 s ở `/quay`); kiểm tra a11y (bàn phím, axe); đo p95 các endpoint chính; buổi dùng thử với quản lý (mục tiêu < 30 giây/lượt).
  - **Tests:** như trên + `/qa` tìm khoảng trống (Unicode, múi giờ, gửi đôi).
  - **Security:** chạy `secure` trên toàn bộ diff R1-1; `/pr-review` cuối cùng.
  - **Depends on:** S7, S8, S9

- [ ] **S11 Staging trên VPS** (size: S)
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
    - EM đã quyết: staging Supabase `zyyiukcysjntjgjabiqe` ở **Mỹ**, Data API bật → VPS staging phải cùng vùng, giảm thiểu bằng REVOKE. Lead tự bootstrap DB staging. Production sau này là project Singapore mới.
    - Còn chờ EM (runbook §9):
      - Mua VPS (DigitalOcean 2 GiB $12, datacenter khớp vùng Supabase) + domain.
      - R2 bucket + token.
      - Khóa `age`.
      - GitHub environment `staging`.
      - Monitor Better Stack + Healthchecks.io.
    - Sau đó: deploy, smoke, backup thử, restore drill, rồi tick S11.

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
