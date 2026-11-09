---
title: Platform admin console — backend + frontend design
updated: 2026-11-09
sources: [product/specs/platform-admin.md, decisions/ADR-0018-platform-admin-console.md, decisions/ADR-0017-one-statement-request-auth.md]
---

# Admin nền tảng: thiết kế backend + frontend

Spec: [platform-admin](../product/specs/platform-admin.md). Quyết định: [ADR-0018](../decisions/ADR-0018-platform-admin-console.md). Trang này là hợp đồng để build; đổi API thì sửa ở đây trước.

## Module (backend)
- Crate mới **`platform`** (bounded context theo ADR-0006), phụ thuộc `kernel` + `identity` (dùng lại `password`, `mfa::{TotpSecret, SecondFactor, generate_recovery_codes}`, `token`, `lockout`, `account::TemporaryPassword`, `roles::SystemRole`). Là crate **duy nhất** có SQL chạm `app.platform_*`, `app.tenant_feature` (ghi), `app.role_template_permission` (ghi) — test kiến trúc `platform_sql_stays_in_platform` (`app/tests/platform.rs`) quét `crates/*/src`.
- Module (`crates/platform/src`): `operator` (tài khoản, CLI), `auth` (login / MFA / session / step-up, extractor `Operator`), `tenants`, `features`, `roles` (rào chắn thuần + áp dụng), `accounts`, `audit`, `http` (router `/admin`). Không có module `repo` riêng: SQL nằm trong từng module.
- `kernel::feature` (registry, dùng chung spa + platform), `kernel::audit::record_platform` (ghi `audit_log` của spa với `actor_kind = 'platform_admin'`).
- `app` mount `platform::http::routes()` dưới `/api/v1`, lớp rate limit riêng cho `/admin/auth/*`.

## Dữ liệu (migration 0038–0040)
```sql
-- 0038 (PR A)
platform_operator (id uuid PK, username text UNIQUE CHECK (~ '^[a-z0-9][a-z0-9._-]{2,39}$'),
  full_name text CHECK (1..200), password_hash text CHECK (LIKE '$argon2id$%'),
  status text CHECK IN ('active','disabled'), failed_logins int ≥ 0, locked_until timestamptz,
  mfa_secret_ct bytea, mfa_enabled_at timestamptz, mfa_last_step bigint, created_at, updated_at,
  CHECK (mfa_enabled_at IS NULL OR mfa_secret_ct IS NOT NULL))
platform_recovery_code (id, operator_id FK, code_hash bytea UNIQUE (32), created_at, used_at)
platform_session (id, token_hash bytea UNIQUE (32), operator_id FK, created_at, last_seen_at,
  idle_expires_at, absolute_expires_at, recent_auth_at (NO DEFAULT), mfa_pending bool, ip inet,
  user_agent text, revoked_at, CHECK (idle_expires_at <= absolute_expires_at))
platform_audit_log (id, operator_id FK, action CHECK (~ '^platform(\.[a-z_]+)+$'), tenant_id FK NULL,
  target_type text NULL, target_id uuid NULL, detail jsonb NULL, reason text NULL, ip inet, at)
  + index platform_audit_log_at_idx (at DESC, id DESC)
audit_log_tenant_at_idx ON audit_log (tenant_id, at DESC)   -- nhật ký spa + "hoạt động gần nhất"
-- 0039 (PR B)
tenant_feature (tenant_id FK, feature text CHECK (~ '^[a-z_]{2,40}$'), enabled bool,
  updated_at, updated_by uuid FK platform_operator, PK (tenant_id, feature))   -- RLS theo spa
-- 0040 (PR C)
role_template_permission (role_key text CHECK IN ('owner','manager','technician'), permission text,
  PK (role_key, permission))                                       -- seed = SystemRole::default_permissions()
role.follows_template boolean NOT NULL DEFAULT true                -- spa hiện có: = (tập quyền trùng mẫu)
```
Grant cho `app_rw`: `platform_operator` / `platform_session` S/I/U; `platform_recovery_code` S/I/U/D; `platform_audit_log` S/I (append-only); `tenant_feature` **chỉ S**; `role_template_permission` **chỉ S**. Bảng `platform_*`: RLS bật + policy `TO app_rw USING (true)` (chặn role khác như Supabase `anon`).

## Hàm DB
Tất cả: `SECURITY DEFINER`, `SET search_path = app, pg_temp`, SQL tĩnh, `REVOKE ALL FROM PUBLIC`, `GRANT EXECUTE TO app_rw` (trừ `platform_gate`). Tham số đầu `p_token_hash bytea`.

| Hàm | Đọc / ghi | Trả về (chỉ thế này) |
|---|---|---|
| `platform_gate(p_token_hash, p_write bool) → uuid` (không grant) | `platform_session` khớp hash, chưa thu hồi, chưa hết idle / tuyệt đối, không pending; operator `active`; `p_write` ⇒ `recent_auth_at > now() - 5 min` | id operator, hoặc `RAISE insufficient_privilege` (42501) |
| `platform_tenant_overview(t)` | `tenant`; `count(*)` của `staff` (`status = 'active'`), `customer` (chưa ẩn danh), `visit` (30 ngày, `completed`), owner đang hoạt động (membership + tài khoản active, role có `staff.manage`); `max(audit_log.at)` | `id, name, slug, status, created_at, staff_count, customer_count, visits_30d, owner_count, last_activity_at` |
| `platform_insert_tenant(t, id, name, slug)` (ghi) | INSERT `tenant` | `void` (lỗi 23505 / 23514 như CLI) |
| `platform_set_tenant_status(t, tenant, status)` (ghi) | UPDATE `tenant.status`; ngưng ⇒ thu hồi mọi session còn mở có `active_tenant_id` = spa **hoặc** của bất kỳ user nào có membership ở spa đó (kể cả khi user còn làm ở spa khác) | `previous text, revoked int` (không dòng = không có spa) |
| `platform_set_feature(t, tenant, feature, enabled)` (ghi, PR B) | UPSERT `tenant_feature` | `previous bool` (true khi chưa có dòng) |
| `platform_apply_role_template(t, key, perms text[])` (ghi, PR C) | thay `role_template_permission` của key; thay `role_permission` của mọi role `is_system AND key AND follows_template`; INSERT `audit_log` mỗi spa bị đổi (`platform_admin`) | `updated_spas int` |
| `platform_account_memberships(t, users uuid[])` (PR C) | `membership` + `tenant` + role key + có `staff.manage` của những user đó; số chủ spa đang hoạt động khác trong mỗi spa | `user_id, tenant_id, tenant_name, tenant_status, membership_status, role_keys text[], is_owner, other_active_owners` |
| `user_spa_suspended(user) → bool` (không token; như `tenants_of_user`) | membership active của user ở spa `suspended` và không spa active nào | bool — chỉ dùng sau khi mật khẩu đã đúng |

Tìm tài khoản (PR C) **không** có hàm definer riêng (bản thiết kế ban đầu có `platform_account_search`, khi build đã bỏ): `user_account` là bảng toàn cục không RLS, nên `platform::accounts` (sau extractor `Operator`) truy vấn thẳng nó (SĐT tiền tố E.164 hoặc `search_fold(full_name) LIKE`, ≤ 50), rồi gọi `platform_account_memberships` cho các id tìm được (sửa theo code 2026-11-09).

Test các hàm nằm trong `app/tests/platform.rs` (`platform_functions_check_the_operator_session_themselves`, `app_rw_cannot_write_flags`, …): hash lạ / session spa / session pending / step-up cũ → 42501, `app_rw` không ghi thẳng được bảng cờ.

## Auth operator
- `POST /admin/auth/login {username, password}` → `200 {next: "mfa_enroll" | "mfa_verify"}` + cookie pending. Lỗi: `401 auth.invalid_credentials` (đồng nhất, dummy Argon2), `422`, `429`.
- `POST /admin/auth/mfa/enroll/start` (pending) → `{otpauth_uri, manual_key}` (`no-store`); `…/confirm {code}` → `{operator, recovery_codes}` + cookie đầy đủ; `…/mfa/verify {code}` → `{operator}` + cookie đầy đủ. Lỗi như `/auth/mfa/*` của spa.
- `POST /admin/auth/reauth {password, code}` (đầy đủ) → `204`; `POST /admin/auth/logout` → `204` (pending hoặc đầy đủ).
- `GET /admin/me` → `{operator: {id, username, full_name}, step_up_until}`.
- Extractor `Operator`: cookie `__Host-op-session` → `platform_session` (hash) → sống + đủ + operator active, trượt idle (≤ 1 ghi/phút, không vượt tuyệt đối); method ghi ⇒ step-up ≤ 5 phút (`403 platform.step_up_required`). Mang `token_hash` để gọi hàm DB. Không có cookie / không hợp lệ ⇒ `401 auth.unauthenticated`; pending ⇒ `401 auth.mfa_required` / `auth.mfa_enrollment_required`.
- Audit nền tảng: `platform.auth.login`, `platform.auth.mfa_enrolled`, `platform.auth.reauth`, `platform.auth.logout`.

## API console (`/api/v1/admin`, mọi route cần `Operator`)
| Route | Body / query | Trả về | Lỗi riêng |
|---|---|---|---|
| `GET /tenants` | — | `TenantRow[]` (overview) | |
| `POST /tenants` 🔐 | `{name, slug, branch_name?, owner: {phone, full_name}}` | `201 {tenant: TenantRow, owner: {user_id, phone, temporary_password}}` `no-store` | `409 tenant.slug_taken`, `409 account.phone_taken`, `422` |
| `GET /tenants/{id}` | — | `TenantRow` (tính năng và vai trò đọc bằng hai route riêng bên dưới) | `404` |
| `POST /tenants/{id}/suspend` 🔐 | `{reason}` | `{tenant, revoked_sessions}` | `409 tenant.status_unchanged` |
| `POST /tenants/{id}/unsuspend` 🔐 | `{reason}` | `{tenant, revoked_sessions}` (luôn 0) | `409 tenant.status_unchanged` |
| `GET /features` | — | `[{key, name, permissions[]}]` (registry `kernel::feature`) | |
| `GET /tenants/{id}/features` | — | `[{key, name, enabled, updated_at?}]` (không dòng = bật) | `404` |
| `PUT /tenants/{id}/features/{key}` 🔐 | `{enabled, reason}` | `{key, name, enabled, updated_at}` | `404` key lạ |
| `GET /permissions` | — | `[{key, group, grantable, owner_only, owner_floor, feature?}]` | |
| `GET /role-templates` | — | `[{key, name, permissions[]}]` | |
| `PUT /role-templates/{key}` 🔐 | `{permissions[], reason}` | `{key, permissions, updated_spas}` | `422 role.*`, `404` |
| `GET /tenants/{id}/roles` | — | `[{key, name, follows_template, permissions[]}]` | |
| `PUT /tenants/{id}/roles/{key}` 🔐 | `{permissions[], reason}` | role | `422 role.*` |
| `POST /tenants/{id}/roles/{key}/reset` 🔐 | `{reason}` | role | |
| `GET /accounts?q=` | q ≥ 3 ký tự | `AccountRow[]` ≤ 50 | `422 q too_short` |
| `POST /accounts/{id}/lock` / `unlock` 🔐 | `{reason}` | `AccountRow` | `409 platform.last_owner`, `409 account.status_unchanged` |
| `POST /accounts/{id}/reset-password` 🔐 | `{reason}` | `{temporary_password}` `no-store` | `409 account.locked` |
| `POST /accounts/{id}/reset-mfa` 🔐 | `{reason}` | `AccountRow` | `409 account.mfa_not_enabled` |
| `POST /accounts/{id}/revoke-sessions` 🔐 | `{reason}` | `{revoked}` | |
| `GET /audit?before=&limit=` | limit 1–100 (mặc định 50) | `{items[], next_before?}` | |
| `GET /tenants/{id}/audit?before=&limit=` | như trên | `{items: [{at, action, entity_type, actor_kind, actor_name}], next_before?}` | |

🔐 = step-up ≤ 5 phút. `reason`: 3–500 ký tự sau trim. Mọi body `deny_unknown_fields`. Thứ tự kiểm: `401` (operator) → `403 step_up` → `404` (tài nguyên) → `422` (body) → `409` (trạng thái).

## Spa: thay đổi ở đường xác thực
- **Đăng nhập khi spa ngưng** (`identity::service::login`): mật khẩu đúng + `user_spa_suspended(user)` ⇒ `403 auth.spa_suspended` (trước MFA; chỉ người biết mật khẩu thấy). Không đổi đếm sai.
- **Cờ tính năng** (PR B): `app.session_actor` thêm cột `disabled_features text[]` (DROP + CREATE, giữ nguyên điều kiện của 0034), `repo::member_access` đọc cùng; `MemberAccess.disabled_features`; `Actor::with_disabled_features` rút permission (`kernel::feature::strip`), `Actor::require` → `feature.disabled`; `AuthUser` kiểm `MatchedPath` (`kernel::feature::for_route`). `/me.features: string[]` (key đang bật). `hr::me` bỏ `evaluation` / `payslip` khi tính năng tắt.
- **Seed role** (PR C): `identity::provisioning::create_tenant` và console seed từ `role_template_permission`; `SystemRole::default_permissions()` còn là giá trị seed của migration + test "mẫu mới = code".

## Frontend
- **Ứng dụng con riêng** `src/admin/` nạp từ `main.tsx` khi `pathname` là `/admin` hoặc `/admin/…` (`import("./admin/boot")`); spa giữ nguyên đường khởi động. điều hướng tối giản bằng History API (`admin/nav.tsx`: `AdminNavProvider`, `matchAdminRoute`; **không** phải router TanStack thứ hai, vì kiểu router của spa là kiểu đã đăng ký và console chỉ cần path), `QueryClient` riêng, khung `AdminShell` (thanh trên + menu trái: Spa · Mẫu vai trò · Tài khoản · Nhật ký), không `AppShell`. Dùng chung `lib/api.ts`, component classes, tokens. Ngân sách: entry của spa không đổi (+~100 B cho nhánh pathname); chunk admin không preload từ spa.
- **Route**: `/admin/dang-nhap` (username + mật khẩu → MFA enroll / verify, mã khôi phục hiện một lần) · `/admin` (danh sách spa: bảng tên / trạng thái chip / ngày tạo / NV / khách / lượt 30 ngày / hoạt động gần nhất; "Tạo spa") · `/admin/spa/$id` (một trang, các panel xếp dọc, không tab: Thông tin + Tạm ngưng spa / Mở lại spa · Tính năng (công tắc + lý do) · Vai trò của spa (danh sách permission theo nhóm, nhãn "Theo mẫu" / "Riêng spa này", "Về mẫu") · Nhật ký của spa) · `/admin/vai-tro` (3 mẫu, chỉnh bằng checkbox theo nhóm, khóa các ô rào chắn) · `/admin/tai-khoan` (tìm SĐT / tên, mỗi dòng ⋯: Khóa / Mở khóa / Đặt lại mật khẩu / Reset MFA / Đăng xuất mọi thiết bị) · `/admin/nhat-ky`.
- **Phiên**: query `GET /admin/me` (`adminKeys.me` trong `admin/api.ts`) quyết định phiên của console; `401` ⇒ `/admin/dang-nhap`. Không đụng `authQuery` của spa (khóa query khác: `["admin", …]`, `QueryClient` riêng).
- **Step-up** (`admin/StepUp.tsx`): hook `useStepUp()` (trong `StepUpProvider`) bọc mỗi lệnh ghi, bắt `403 platform.step_up_required` → mở `StepUpDialog` (mật khẩu + mã 6 số, `autocomplete="one-time-code"`) → `POST /admin/auth/reauth` → chạy lại đúng request một lần.
- **Thao tác phá hủy**: `btn-danger-outline` → `ReasonSheet` có ô lý do bắt buộc (≥ 3 ký tự) → `btn-danger`. Mật khẩu tạm: sheet hiện một lần ("Sao chép", "Tôi đã lưu mật khẩu") trong `TenantsPage` (tạo spa) và `AccountsPage` (đặt lại mật khẩu).
- **Trạng thái**: mỗi màn skeleton khi tải, trống ("Chưa có spa nào"), lỗi (problem `code` → thông báo tiếng Việt trong `admin/messages.ts`, không vào `lib/errors.ts`).
- **Spa (PR B)**: `Me.features`; `hasFeature(me, key)`; nav ẩn "Nhật ký ghi chú" khi `notes_journal` tắt, "Báo cáo kinh doanh" / sổ doanh thu khi `reports` tắt; lịch hẹn / lương / đánh giá / gọi lại tự ẩn vì permission bị rút; màn mở bằng URL khi tính năng tắt → "Không có quyền" (route guard dùng cùng `mayOpen`).

## Vận hành
- Tạo operator đầu tiên (máy có `DATABASE_URL` schema owner): `APP_NEW_OPERATOR_PASSWORD=… app add-operator --username ops --name "…"` (mật khẩu ≥ 10 ký tự, từ env — không argv), rồi **ngay** mở `/admin/dang-nhap` để enroll TOTP (TOFU) và cất 10 mã khôi phục.
- Mất authenticator: `app reset-operator-mfa --username U` (xác minh người thật trước). Tạm khóa: `app disable-operator --username U` / `enable-operator`.
- Dùng một profile trình duyệt riêng cho console (không đăng nhập spa trong cùng profile) đến khi có `APP_ADMIN_ORIGIN` + subdomain.
- Lệnh CLI cũ `app unlock` / `reset-mfa` vẫn còn cho trường hợp không vào được console; PR C thay việc hằng ngày bằng màn Tài khoản.

## Test (bắt buộc theo spec)
- Rust: toàn bộ ở `app/tests/platform.rs` (đăng nhập + MFA, khóa operator, session spa / operator không lẫn nhau, step-up, tạo spa, spa ngưng, audit, hàm DB tự kiểm session, mã khôi phục dùng một lần, `platform_sql_stays_in_platform`, cờ tính năng, mẫu vai trò, sàn quyền chủ spa, tài khoản nhiều spa, nhật ký spa chỉ metadata; bản thiết kế dự kiến tách 6 file); `authz_matrix.rs` thêm lớp **operator** (mọi route `/admin/*`: không cookie → 401; cookie owner / manager / technician → 401; operator pending MFA → 401; operator đầy đủ → 2xx; ghi không step-up → 403).
- Vitest: `admin/admin.test.tsx`, `admin/rolesAccounts.test.tsx` (đăng nhập + MFA, step-up retry, danh sách spa, tạo spa + mật khẩu một lần, công tắc tính năng, rào chắn vai trò, tài khoản); spa: `app/featureNav.test.ts` (ẩn nav).
- E2E `admin.spec.ts`: operator (CLI) đăng nhập + MFA → tạo spa + chủ spa → chủ spa đăng nhập, enroll MFA, bị bắt đổi mật khẩu → operator tắt `appointments` → chủ spa không thấy "Lịch hẹn", `GET /appointments` → `403 feature.disabled`.

## Impact analysis (build)
- `app.session_actor` (0034, #109) thay bằng bản có `disabled_features`: `identity::repo::session_actor` + `.sqlx`, `member_access`, `MemberAccess`, `session_actor.rs` test (≤ 3 statement giữ nguyên).
- `Actor::require` có nhánh mới: mọi test khẳng định `auth.forbidden` vẫn đúng khi không tính năng nào tắt.
- `identity::provisioning::create_tenant` (CLI, e2e setup, test harness) seed từ bảng mẫu.
- `login`: thêm `403 auth.spa_suspended` — test `platform.rs::a_suspended_spa_cannot_sign_in`; web `LoginPage` thêm thông báo.
- `Me` (OpenAPI + `session.ts` type) thêm `features`; `openapi.json` sinh lại.
- `kernel::audit` thêm `record_platform`; CHECK `actor_kind` đã có `platform_admin`.
