---
title: Spec — Admin nền tảng (platform admin console)
status: draft (lead's design, EM to review)
updated: 2026-11-09
sources: [raw/2026-11-09-em-request-docs-and-platform-admin.md, decisions/ADR-0003-multi-tenancy.md, decisions/ADR-0004-authentication.md, decisions/ADR-0014-former-staff-login.md, decisions/ADR-0016-accounts-and-passwords.md, decisions/ADR-0018-platform-admin-console.md]
---

# Spec: Admin nền tảng

Raw: [2026-11-09-em-request-docs-and-platform-admin](../../../raw/2026-11-09-em-request-docs-and-platform-admin.md) (mục 2). EM chọn **"Admin nền tảng"**: màn cho người vận hành sản phẩm (operator) khi bán cho nhiều spa, tách khỏi app của từng spa. Thiết kế: [ADR-0018](../../decisions/ADR-0018-platform-admin-console.md), [architecture/platform-admin](../../architecture/platform-admin.md).

## 1. Goal
Hôm nay mọi việc vận hành (tạo spa, tạo chủ spa, mở khóa, reset MFA, khôi phục mật khẩu chủ spa) đi qua CLI với kết nối schema owner — chỉ người có máy dev + chuỗi kết nối DB làm được, không có nhật ký thao tác. Khi bán cho nhiều spa, operator cần một console **an toàn hơn CLI**: danh tính riêng có MFA bắt buộc, mọi thao tác có audit, không đọc dữ liệu khách của spa. Thước đo: mở một spa mới (spa + chủ spa đăng nhập được) < 3 phút, không cần DB; 100 % thao tác operator có một dòng audit.

## 2. Personas & roles
- **Operator (platform admin)**: người vận hành sản phẩm (hiện là EM). Không phải thành viên spa nào. Tạo bằng CLI `app add-operator`; không đăng ký công khai.
- **Chủ spa**: nhận tài khoản từ operator (mật khẩu tạm), thấy trong nhật ký của spa mình các thao tác operator đã làm trên spa.
- **Quản lý / KTV**: chịu ảnh hưởng của cờ tính năng và vai trò; không thấy console.

## 3. Quyết định chính (chi tiết ở ADR-0018)
- **URL:** `/admin` trên cùng deployment Vercel, cookie riêng `__Host-op-session`, API riêng `/api/v1/admin/*`. Có `APP_ADMIN_ORIGIN` để chuyển sang subdomain riêng (`admin.<domain>`) khi có tên miền, không đổi code.
- **Danh tính:** bảng `platform_operator` + `platform_session` riêng (không phải `user_account` / `membership`). TOTP bắt buộc, session ngắn (idle 30 phút, tối đa 8 giờ), **step-up** (mật khẩu + TOTP trong 5 phút gần nhất) cho mọi thao tác ghi.
- **Truy cập dữ liệu:** không `BYPASSRLS`, không policy "mở cho admin". Việc xuyên spa đi qua vài hàm `SECURITY DEFINER` hẹp (search_path ghim, chỉ trả số đếm / metadata), mỗi hàm tự kiểm session operator từ hash token. Việc trong một spa chạy trong đúng RLS của spa đó (`TenantTx`).
- **Không bao giờ trả:** danh sách khách, ghi chú buổi / sức khỏe, lương, hash mật khẩu, secret MFA, `before/after` của audit spa.

## 4. User stories + acceptance criteria

### US1 — Operator đăng nhập (MFA bắt buộc)
As an operator, I want to sign in with my own credentials and a TOTP code, so that only I can run the console even if my password leaks.
- **AC1.1** Given operator do CLI tạo, When `POST /admin/auth/login` đúng username + mật khẩu, Then `200 {next: "mfa_enroll" | "mfa_verify"}` + cookie `__Host-op-session` **pending** (10 phút); `GET /admin/me` với session đó → `401 auth.mfa_required` (chưa enroll: `401 auth.mfa_enrollment_required`). Không bao giờ có `next: done` khi chưa qua TOTP.
- **AC1.2** Lần đầu: `enroll/start` → QR + khóa nhập tay; `enroll/confirm` với mã đúng → session đầy đủ (token mới) + 10 mã khôi phục (một lần). Lần sau: `mfa/verify` với TOTP hoặc mã khôi phục.
- **AC1.3** Sai mật khẩu / username không có / tài khoản bị khóa → cùng `401 auth.invalid_credentials` sau cùng lượng việc Argon2. 5 lần sai (mật khẩu hoặc mã) → khóa tăng dần (`identity::lockout`).
- **AC1.4** Rate limit riêng cho `/admin/auth/*` theo IP (1 request / 6 giây, burst 5) → `429`.
- **AC1.5** Cookie / session spa (`__Host-session`, kể cả của chủ spa) gọi `/admin/*` → `401` (không phải 403: đó không phải danh tính operator). Cookie operator gọi API spa (`/customers`) → `401` (API spa không đọc cookie operator).
- **AC1.6** Session operator: idle 30 phút, tối đa 8 giờ; logout thu hồi ngay. Hết hạn → `401`.
- **AC1.7** Step-up: mọi `POST/PUT` dưới `/admin` (trừ `/admin/auth/*`) khi lần xác thực gần nhất > 5 phút → `403 platform.step_up_required`; `POST /admin/auth/reauth {password, code}` mở lại 5 phút. Mã sai tính vào khóa.

### US2 — Danh sách spa
As an operator, I want to see every spa with status and a few health numbers, so that I know who is active without reading their data.
- **AC2.1** `GET /admin/tenants` → mỗi spa: `id, name, slug, status, created_at, staff_count` (hồ sơ chưa nghỉ), `customer_count`, `visits_30d` (lượt không hủy, 30 ngày), `last_activity_at` (audit gần nhất), `owner_count`. Không tên khách, không doanh thu.
- **AC2.2** Không có session operator → `401`; chủ spa / quản lý / KTV (cookie spa) → `401`.

### US3 — Tạo spa + chủ spa đầu tiên
As an operator, I want to create a spa and its first owner in one step, so that a new customer can start the same day.
- **AC3.1** `POST /admin/tenants {name, slug, branch_name?, owner: {phone, full_name}}` (step-up) → `201` một transaction: `tenant`, chi nhánh mặc định, 3 role hệ thống theo **mẫu vai trò hiện tại** (US7), tài khoản chủ spa với **mật khẩu tạm** (ADR-0016: 12 ký tự CSPRNG, `must_change_password`), membership + role owner. Response có `temporary_password` **một lần** (`Cache-Control: no-store`), không log, không audit mật khẩu.
- **AC3.2** Chủ spa đăng nhập: mật khẩu tạm → enroll MFA (role owner bắt buộc) → `403 auth.password_change_required` ở mọi route → `/doi-mat-khau` → dùng được app.
- **AC3.3** Slug trùng → `409 tenant.slug_taken`; slug sai dạng → `422 slug invalid_format`; SĐT đã có tài khoản → `409 account.phone_taken` (một người một spa, ADR-0016); SĐT sai → `422 phone invalid_format`. Lỗi bất kỳ → không còn gì (không spa mồ côi).
- **AC3.4** Audit nền tảng `platform.tenant.create` (tenant, slug, owner user id; không SĐT đầy đủ, không mật khẩu) + audit spa `tenant.created` (actor `platform_admin`).

### US4 — Tạm ngưng / mở lại spa
As an operator, I want to suspend a spa that stopped paying or is under investigation, so that nobody can use it while its data stays intact.
- **AC4.1** `POST /admin/tenants/{id}/suspend {reason}` (step-up, lý do bắt buộc 3–500 ký tự) → `status = suspended`, thu hồi mọi session đang mở của thành viên spa đó; dữ liệu giữ nguyên.
- **AC4.2** Thành viên spa bị ngưng đăng nhập: mật khẩu đúng → `403 auth.spa_suspended` (chỉ người có mật khẩu đúng thấy); request bằng session cũ → `401` (đã thu hồi); session nào sót → `403 auth.not_a_member` (đã có từ M3).
- **AC4.3** `POST /admin/tenants/{id}/unsuspend {reason}` → `active`; đăng nhập lại bình thường (chủ spa vẫn phải MFA).
- **AC4.4** Ngưng spa đã ngưng / mở spa đang mở → `409 tenant.status_unchanged`. Spa không tồn tại → `404`.

### US5 — Cờ tính năng theo spa
As an operator, I want to turn a feature off for a spa, so that a spa on a smaller plan (or with a broken flow) does not see it.
- **AC5.1** Registry trong code (`kernel::feature`), 6 tính năng thật, mặc định **bật** cho mọi spa (không dòng = bật; migration không ghi gì → không đổi hành vi):

  | key | Tên | Khi tắt: permission bị rút | Khi tắt: route trả `403 feature.disabled` |
  |---|---|---|---|
  | `appointments` | Lịch hẹn | `appointment.manage`, `appointment.settings` | `/appointments*`, `/appointment-settings`, `/me/appointments` |
  | `payroll` | Bảng lương | `payroll.read`, `payroll.close` | `/payroll*` (+ phiếu lương biến khỏi `/me/staff`, lương khỏi hồ sơ) |
  | `evaluations` | Đánh giá tháng | — (không rút `evaluation.record`, xem ghi chú build ở Status) | `/evaluations*`, `/me/evaluations*` (+ đánh giá biến khỏi `/me/staff`) |
  | `follow_ups` | Gọi lại khách vắng | `customer.follow_up` | (qua permission) |
  | `notes_journal` | Nhật ký ghi chú | — | `/notes` |
  | `reports` | Báo cáo kinh doanh (khoảng ngày + sổ doanh thu) | `report.long_range` | `/reports/range`, `/reports/ledger` |

- **AC5.2** Tắt → request kế tiếp của mọi thành viên spa đó: route của tính năng → `403 {code: "feature.disabled"}`; endpoint khác tự bỏ phần của tính năng (vì permission bị rút); `/me.features` không còn key đó và `/me.permissions` không còn các permission bị rút → web ẩn nav / nút.
- **AC5.3** `PUT /admin/tenants/{id}/features/{key} {enabled, reason}` (step-up). Key lạ → `404`. Bật lại → hành vi cũ ngay request kế tiếp.
- **AC5.4** Cờ chỉ ghi được qua hàm DB có kiểm session operator; `app_rw` không có quyền ghi bảng `tenant_feature` (test).
- **AC5.5** Audit nền tảng `platform.feature.set` + audit spa `feature.changed` (before/after).

### US6 — Xem permission và vai trò
As an operator, I want to see the permission catalogue and what each role grants, so that I can answer "why can't my manager do X?".
- **AC6.1** `GET /admin/permissions` → mỗi permission: `key`, nhóm, mô tả tiếng Việt, `grantable` (`platform.tenant.manage`, `staff.read_own_past` = không), tính năng chứa nó (nếu có).
- **AC6.2** `GET /admin/role-templates` → 3 mẫu (owner / manager / technician) + permission. `GET /admin/tenants/{id}/roles` → role của spa + permission + `follows_template`.

### US7 — Sửa mẫu vai trò và vai trò riêng của một spa
As an operator, I want to change what a manager may do for all spas (or for one spa), so that a product decision does not need a migration.
- **AC7.1** `PUT /admin/role-templates/{key} {permissions, reason}` (step-up) → mẫu đổi; mọi role hệ thống cùng key của các spa **đang theo mẫu** đổi theo trong cùng transaction; response `{updated_spas}`. Spa mới tạo sau đó dùng mẫu mới.
- **AC7.2** `PUT /admin/tenants/{id}/roles/{key} {permissions, reason}` (step-up) → role của spa đó đổi, `follows_template = false` (mẫu đổi sau không ghi đè). `POST /admin/tenants/{id}/roles/{key}/reset {reason}` → quay về mẫu, `follows_template = true`.
- **AC7.3** Có hiệu lực ở **request kế tiếp** (quyền đọc mỗi request): ví dụ bỏ `customer.write` khỏi manager → manager `POST /customers` → `403`.
- **AC7.4** Rào chắn (cả mẫu và spa), lỗi `422`:
  - permission lạ → `422` lỗi field `permissions unknown_permission`; `platform.*`, `staff.read_own_past` → `role.not_grantable`;
  - owner phải giữ `staff.manage`, `staff.read`, `staff.read_basic`, `audit.read` → `role.owner_floor` (`details.missing`);
  - `staff.manage` chỉ ở owner → `role.owner_only_permission` (giữ quyết định C, ADR-0015/0016 "owner_protected" dựa trên `staff.manage`);
  - chỉ role hệ thống (`owner`, `manager`, `technician`) → key khác `404`.
- **AC7.5** Bất biến cứng không đổi: người đã nghỉ chỉ `staff.read_own_past` (ADR-0014), `staff.link_required`, quyết định C, owner luôn MFA (CHECK DB), tính năng tắt rút permission bất kể role.
- **AC7.6** Audit nền tảng `platform.role_template.set` / `platform.role.set` / `platform.role.reset` + audit spa `role.permissions_changed` (before/after list).

### US8 — Tài khoản xuyên spa
As an operator, I want to find a person's login and fix it (lock, reset password, reset MFA, end sessions), so that an owner who forgot their password is back in minutes — replacing the CLI path.
- **AC8.1** `GET /admin/accounts?q=` (≥ 3 ký tự; SĐT hoặc tên, không dấu) → tối đa 50: `id, full_name, phone, status, locked_until, mfa_enabled, must_change_password, created_at, memberships[{tenant_id, tenant_name, tenant_status, role_keys, status}]`. Không hash, không secret, không dữ liệu khách.
- **AC8.2** `POST /admin/accounts/{id}/lock {reason}` → `user_account.status = disabled` + thu hồi mọi session; đăng nhập → `401 auth.invalid_credentials`. Từ chối nếu là **chủ spa đang hoạt động cuối cùng** của một spa đang hoạt động → `409 platform.last_owner` (ngưng spa trước). `unlock` → `active` + xóa khóa tạm (thay `app unlock`).
- **AC8.3** `POST /admin/accounts/{id}/reset-password {reason}` → mật khẩu tạm mới (một lần, `no-store`), `must_change_password`, thu hồi mọi session, xóa đếm sai / khóa tạm. Tài khoản bị operator khóa → `409 account.locked`. Áp dụng cả cho chủ spa (thay đường CLI-only mà EM chấp nhận ở ADR-0016).
- **AC8.4** `POST /admin/accounts/{id}/reset-mfa {reason}` → xóa secret + mã khôi phục, thu hồi session; lần đăng nhập sau phải enroll lại (owner). Chưa có MFA → `409 account.mfa_not_enabled`.
- **AC8.5** `POST /admin/accounts/{id}/revoke-sessions {reason}` → `{revoked: n}`.
- **AC8.6** Mỗi thao tác: audit nền tảng `platform.account.*` + audit spa `account.*` ở **mọi spa** của tài khoản (actor `platform_admin`), không mật khẩu. Tài khoản không tồn tại → `404`.

### US9 — Nhật ký
As an operator, I want an audit of everything operators did, and read-only access to a spa's audit trail when supporting them, so that every support action is accountable.
- **AC9.1** `GET /admin/audit?before=&limit=` → nhật ký nền tảng (mới nhất trước, `limit ≤ 100`, phân trang theo `before`): `at, operator, action, tenant, target, reason, ip, detail`.
- **AC9.2** `GET /admin/tenants/{id}/audit?before=&limit=` → nhật ký spa **chỉ metadata**: `at, action, entity_type, actor_kind, actor_name`. Không `before/after`, không `reason`, không `entity_id` (có thể là id khách), không IP. Mỗi lần gọi ghi `platform.tenant_audit.read` (tenant, số dòng).
- **AC9.3** Nhật ký nền tảng append-only: `app_rw` không có `UPDATE/DELETE` (test).

## 5. UX notes
- Console ở `/admin/*`, chunk lazy riêng, giao diện cùng design system (palette "Trà thất Đông y", component classes), tiếng Việt. Không dùng `AppShell` của spa.
- Màn: Đăng nhập (+ MFA enroll / verify), Spa (danh sách + "Tạo spa"), Chi tiết spa (thông tin + Tạm ngưng/Mở lại, Tính năng, Vai trò, Nhật ký spa), Mẫu vai trò, Tài khoản (tìm + thao tác), Nhật ký nền tảng.
- Hộp thoại step-up (mật khẩu + mã) tự mở khi API trả `platform.step_up_required`, rồi gửi lại request.
- Mật khẩu tạm: hộp thoại một lần + "Sao chép", cảnh báo "chỉ hiện một lần".
- Mỗi màn có trạng thái tải / trống / lỗi; thao tác phá hủy dùng `btn-danger-outline` → xác nhận `btn-danger` + ô lý do bắt buộc.
- Chi tiết ở [architecture/platform-admin § Frontend](../../architecture/platform-admin.md#frontend).

## 6. Data
Bảng mới: `platform_operator`, `platform_session`, `platform_recovery_code`, `platform_audit_log`, `tenant_feature`, `role_template_permission`; cột `role.follows_template`. Dữ liệu nhạy cảm: hash mật khẩu operator (Argon2id), secret TOTP (AES-GCM, AAD gắn operator), mã khôi phục (SHA-256). Audit nền tảng giữ vĩnh viễn (như `audit_log`, EM Q-DB4). SĐT trong audit nền tảng: chỉ 3 số cuối.

## 7. Non-functional
- Bảo mật: như §3; `secure` review bắt buộc (auth, session, dữ liệu xuyên spa).
- Perf: JS khởi động của spa **≤ 125 KiB** (console là chunk lazy, không vào bundle của spa); `GET /admin/tenants` ≤ 300 ms p95 với 50 spa.
- A11y WCAG 2.2 AA (axe trong e2e), tiếng Việt, desktop trước (operator dùng máy tính), dùng được trên điện thoại.

## 8. Out of scope (v1)
- Support grant có thời hạn do chủ spa cấp (ADR-0003) để operator xem dữ liệu nghiệp vụ / `before-after` của audit — để sau.
- Role tùy biến (ngoài 3 role hệ thống), tính năng `packages` (xem Q-PA2), billing / gói cước, nhiều operator với phân quyền khác nhau (mọi operator ngang quyền), operator tự reset MFA lẫn nhau (CLI `reset-operator-mfa`), đổi mật khẩu operator trong console (CLI).
- Gửi mật khẩu tạm qua SMS / Zalo.

## 9. Open questions (EM)
- **Q-PA1** Nhật ký spa cho hỗ trợ: v1 chỉ metadata (không before/after, không lý do). Đủ cho hỗ trợ không, hay cần support grant (chủ spa cấp 24 giờ) ngay? Mặc định: metadata.
- **Q-PA2** Tính năng `packages` (bán gói) chưa đưa vào registry: Quầy xoay quanh gói, tắt nó cần thiết kế lại giỏ. Có spa nào không bán gói không? Mặc định: để sau.
- **Q-PA3** Subdomain `admin.<domain>` khi có tên miền thật (production châu Á, ADR-0012)? Mặc định: có — chỉ cần đặt `APP_ADMIN_ORIGIN` + domain trên Vercel.
- **Q-PA4** Khóa chủ spa cuối cùng: v1 từ chối (`platform.last_owner`), phải ngưng spa trước. Đồng ý?

## Status
draft (lead's design 2026-11-09, EM to review). Đã build theo 3 PR chồng nhau (ADR-0018 §Rollout): **#111** danh tính operator + spa + nhật ký nền tảng (0038), **#112** cờ tính năng (0039) + e2e, **#113** vai trò + tài khoản + nhật ký spa (0040). Cả ba đã merge vào `main` và chạy trên staging (2026-11-09, `memory/log.md`); spec vẫn chờ EM review.
- Ghi chú build: tính năng `evaluations` chỉ chặn đánh giá tháng (route), không rút `evaluation.record` vì quyền này còn giữ đánh giá ngày + thưởng/phạt; `staff.read_own_past` không còn lưu trong role owner (0040, vốn luôn bị rút lúc request).
