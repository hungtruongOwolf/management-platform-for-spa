---
title: ADR-0018 Platform admin console — danh tính operator riêng, /admin, truy cập dữ liệu qua hàm hẹp có kiểm session
status: accepted (lead's design 2026-11-09, đã làm ở #111–#113 / migration 0038–0040; EM to review)
date: 2026-11-09
deciders: EM (what), agent (how)
---

# ADR-0018: Console admin nền tảng

## Context
- EM 2026-11-09 ([raw](../../raw/2026-11-09-em-request-docs-and-platform-admin.md)): "page cho admin để quản lý feature, role, functions và account", chọn **admin nền tảng** (operator bán cho nhiều spa). Spec: [platform-admin](../product/specs/platform-admin.md).
- Hiện tại: việc vận hành chạy bằng CLI với kết nối **schema owner** (`DATABASE_URL`) — ngoài RLS, không audit. ADR-0004 đã dự kiến "Platform admin MFA bắt buộc" và permission `platform.tenant.manage` (chưa role nào giữ). ADR-0003: operator **mặc định không đọc** dữ liệu khách / sức khỏe / tài chính / lương; metadata vận hành không cần grant.
- Runtime chỉ có một login DB: `app_rw` (RLS theo `app.tenant_id`, không `BYPASSRLS`). `app.session_actor` (ADR-0017, #109) là mẫu đã có cho hàm `SECURITY DEFINER` hẹp.
- Hosting: một deployment Vercel (web tĩnh + API container, ADR-0012 update), cùng origin `/api`.

## Options considered

### 1. Danh tính operator
| Option | Pros | Cons |
|---|---|---|
| A. Cờ / role trên `user_account` (operator là một tài khoản spa đặc biệt) | Dùng lại toàn bộ luồng login | Operator lẫn với spa: ADR-0016 (chủ spa đặt lại mật khẩu, đổi vai trò), `tenants_of_user`, switch-tenant đều phải nhớ loại trừ; một bug ở đó = chiếm quyền nền tảng |
| B. Role `platform.tenant.manage` trong một spa "nội bộ" | Đúng mô hình role-as-data cũ | Quyền nền tảng phụ thuộc membership của một spa; owner spa đó cấp được |
| **C. Principal riêng: `platform_operator` + `platform_session` + cookie riêng** | Không route spa nào đọc được bảng / cookie này; lỗi ở phía spa không leo thang thành nền tảng; chính sách session riêng (ngắn hơn, MFA luôn) | Thêm ~400 dòng auth (giảm bằng dùng lại `password`, `mfa`, `token`, `lockout` của `identity`) |

### 2. URL
| Option | Pros | Cons |
|---|---|---|
| Subdomain riêng `admin.<domain>` | Cách ly origin: XSS ở app spa không chạm cookie / DOM của console | Staging dùng `*.vercel.app` mặc định, chưa có tên miền; thêm domain + CORS/Origin cho API |
| Deployment Vercel riêng | Cách ly mạnh nhất | Hai pipeline, hai bộ env, chi phí build |
| **`/admin` cùng deployment + cookie `__Host-op-session` + API `/api/v1/admin/*` + `APP_ADMIN_ORIGIN` tùy chọn** | Không thêm hạ tầng; chunk lazy riêng; cookie `__Host-` host-only nên khi chuyển sang subdomain (đặt `APP_ADMIN_ORIGIN`) cookie tự tách | Cùng origin: XSS ở app spa trên **cùng trình duyệt** của operator gửi được request admin (cookie `SameSite` không chặn cùng origin) |

### 3. Truy cập dữ liệu xuyên spa
| Option | Pros | Cons |
|---|---|---|
| Kết nối schema owner / role `BYPASSRLS` ở runtime | Dễ | Một bug = đọc / ghi mọi bảng mọi spa; chính là "silent bypass" EM cấm |
| Policy RLS `OR current_setting('app.platform') = 'on'` | Ít code | Bất kỳ code nào cũng bật được GUC; bypass ngầm trên mọi bảng |
| Role DB riêng `app_platform` + `SET ROLE` + grant tường minh | Rõ ràng trong migration | `app_rw` phải là member mới `SET ROLE` được → không cách ly thêm so với app_rw; vẫn cần policy RLS mở cho role đó trên từng bảng |
| **Hàm `SECURITY DEFINER` hẹp, mỗi hàm tự kiểm session operator (hash token) + việc trong một spa chạy trong RLS của spa đó** | Không bảng nào mở rộng RLS; mỗi khả năng xuyên spa là một hàm có tên, có test, trả đúng cột cần; quên kiểm quyền ở Rust vẫn bị DB từ chối | Thêm vài hàm plpgsql; mỗi hàm phải tự giới hạn đầu ra |

## Decision

### D1 — Principal riêng (option C)
- Bảng `platform_operator` (username `^[a-z0-9][a-z0-9._-]{2,39}$`, tên, hash Argon2id, `status`, đếm sai / khóa, secret TOTP mã hóa AES-256-GCM với AAD `platform_operator.mfa:<id>` — khác AAD của `user_account` nên ciphertext không chuyển chéo được), `platform_recovery_code`, `platform_session` (chỉ lưu SHA-256 token, `recent_auth_at` không default).
- Tạo operator **chỉ bằng CLI** `app add-operator --username U --name N` (mật khẩu từ env `APP_NEW_OPERATOR_PASSWORD`, không argv). `app reset-operator-mfa --username U`, `app disable-operator --username U` / `app enable-operator --username U`. Không route đăng ký, không route tạo operator.
- **MFA luôn bắt buộc**: mật khẩu đúng → session pending (10 phút, chỉ `/admin/auth/mfa/*` + logout) → TOTP / mã khôi phục → session đầy đủ (token mới). Lần đầu: enroll ngay (TOFU giống owner; giảm thiểu: operator enroll ngay sau khi CLI tạo, CLI in cảnh báo).
- Session: **idle 30 phút, tối đa 8 giờ**, cookie `__Host-op-session; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=28800`. API spa không đọc cookie này; API admin không đọc `__Host-session`.
- **Step-up**: mọi request không phải `GET/HEAD` dưới `/admin` (trừ `/admin/auth/*`) cần `recent_auth_at` ≤ 5 phút → `403 platform.step_up_required`; `POST /admin/auth/reauth {password, code}` luôn cần cả hai. Kiểm ở extractor `Operator` theo method (không handler nào quên được) **và** ở hàm DB ghi (D3).
- Rate limit: lớp riêng cho `/admin/auth/*` (1 / 6 giây, burst 5 theo IP / IPv6 /64) + khóa theo operator (`identity::lockout`). Lớp chung 5/giây vẫn áp dụng.
- `platform.tenant.manage` (permission spa cũ) **không dùng** cho console; giữ trong catalogue như không cấp được (`role.not_grantable`).

### D2 — `/admin` cùng deployment, có đường lên subdomain
- Web: `/admin/*` là một **ứng dụng con lazy** (`src/admin/`), nạp từ `main.tsx` theo pathname; router, query client, khung riêng; không vào bundle khởi động của spa.
- `APP_ADMIN_ORIGIN` (tùy chọn): khi đặt, request ghi tới `/api/v1/admin/*` phải có `Origin` = giá trị đó (và request ghi tới API spa không được mang origin admin). Khi có tên miền (production, Q-PA3): thêm `admin.<domain>` vào cùng project Vercel + đặt biến → cookie `__Host-` host-only tách theo host, XSS của app spa không còn chạm console.
- Rủi ro còn lại khi cùng origin (chấp nhận cho staging / giai đoạn đầu): XSS ở app spa chạy trong trình duyệt operator đang đăng nhập console. Giảm: CSP `script-src 'self'` (không inline script, `vercel.json`), không `dangerouslySetInnerHTML`; step-up 5 phút cho mọi thao tác ghi (XSS không biết mật khẩu + mã); session ngắn; runbook: dùng profile trình duyệt riêng cho console, không đăng nhập spa trong cùng profile.

### D3 — Mô hình truy cập dữ liệu
1. **Không bypass ngầm:** không role `BYPASSRLS`, không kết nối owner ở runtime, không policy mở rộng cho bảng nghiệp vụ.
2. **Việc xuyên spa = hàm `SECURITY DEFINER` hẹp** (migration 0038+), `SET search_path = app, pg_temp`, SQL tĩnh, `REVOKE ALL FROM PUBLIC`, `GRANT EXECUTE TO app_rw`. **Tham số đầu luôn là `p_token_hash`** (SHA-256 cookie operator); hàm gọi `app.platform_gate(p_token_hash, p_write)` (không grant cho ai) → id operator, hoặc `RAISE insufficient_privilege` nếu không có session operator đầy đủ còn sống (hoặc step-up quá 5 phút khi ghi). Danh sách hàm và đúng những gì mỗi hàm đọc / trả: [architecture/platform-admin § Hàm DB](../architecture/platform-admin.md#hàm-db).
   - `platform_tenant_overview` — chỉ số đếm (`count(*)`) và mốc thời gian, không một dòng khách / lượt nào ra khỏi hàm.
   - `platform_insert_tenant`, `platform_set_tenant_status` (thu hồi session thành viên), `platform_set_feature`, `platform_apply_role_template`, `platform_account_memberships`. (Bản thiết kế có thêm `platform_account_search`; khi làm PR C, tìm tài khoản đọc thẳng bảng global `user_account` như CLI, không cần hàm definer, xem [architecture/platform-admin](../architecture/platform-admin.md).)
3. **Việc trong một spa** (seed role / chi nhánh / chủ spa khi tạo spa, sửa role của spa, đọc metadata audit spa, ghi audit spa) chạy bằng `TenantTx` của **đúng spa đó**: cùng RLS như request của thành viên spa; quyền quyết định ở Rust bởi extractor `Operator`. Mỗi transaction một spa.
4. **Bảng global của operator** (`platform_operator`, `platform_session`, `platform_recovery_code`): `app_rw` truy cập trực tiếp như `user_account` / `session` (crate `platform` là nơi duy nhất có SQL chạm `app.platform_*`, có test kiến trúc). RLS bật + policy `TO app_rw` để role khác (Supabase `anon` / `authenticated`) không đọc được dù grant lỡ tay. `platform_audit_log`: chỉ `SELECT, INSERT` (append-only).
5. **Cờ tính năng** `tenant_feature`: `app_rw` chỉ `SELECT` (RLS theo spa); ghi chỉ qua `platform_set_feature`. Spa không tự bật / tắt được, kể cả có bug.
6. **Mẫu vai trò** `role_template_permission`: `app_rw` chỉ `SELECT` (seed spa mới); ghi chỉ qua `platform_apply_role_template`.
- **Phạm vi tin cậy:** nếu `app_rw` bị chiếm (SQL tùy ý), kẻ tấn công đọc được hash token session operator và gọi được các hàm — cùng lớp với rủi ro đã ghi ở ADR-0017 (app_rw đã đặt `app.tenant_id` cho bất kỳ spa nào được). Cổng token trong hàm chống **lỗi ứng dụng** (gọi hàm nền tảng từ route spa, quên kiểm operator), không chống chiếm DB.

### D4 — Cờ tính năng: registry trong code, rút permission + chặn route
- `kernel::feature::Feature` (6 key, spec AC5.1): mỗi tính năng = tập permission bị rút + tập tiền tố route. Không dòng trong `tenant_feature` = bật (migration không đổi hành vi spa hiện có).
- `app.session_actor` (ADR-0017) trả thêm `disabled_features text[]` (migration thay hàm, giữ nguyên mọi điều kiện khác); `member_access` (đường RLS cho tooling) đọc cùng thông tin.
- `Actor` mang tập tính năng tắt: `effective_permissions` rút permission của chúng (sau quy tắc người đã nghỉ); `Actor::require(p)` với `p` bị rút vì tính năng → `403 feature.disabled` (không phải `auth.forbidden`); `AuthUser` kiểm `MatchedPath` với bảng tiền tố route → `403 feature.disabled` trước handler. Một nơi khai báo, một nơi chặn, test duyệt mọi route của bảng.
- `/me.features` = các key đang bật; web ẩn nav / nút theo permission (đã có) + `features`.

### D5 — Vai trò: mẫu trong DB + role của spa có cờ "theo mẫu"
- `role_template_permission (role_key, permission)` seed = `SystemRole::default_permissions()` hiện tại; `role.follows_template` (mặc định true; spa hiện có: true nếu tập permission trùng mẫu, false nếu đã khác).
- Spa mới (console và CLI `create-tenant`) seed từ **bảng mẫu**, không còn từ code. Migration thêm permission mới sau này phải thêm vào mẫu + role đang theo mẫu (quy ước ở [data](../architecture/data.md)).
- Sửa mẫu → cập nhật role các spa đang theo mẫu, cùng transaction. Sửa role của một spa → `follows_template = false`; "Về mẫu" → copy + true.
- Rào chắn (Rust thuần, test unit + tích hợp): permission phải biết; không cấp `platform.*` / `staff.read_own_past`; owner giữ `staff.manage`, `staff.read`, `staff.read_basic`, `audit.read`; `staff.manage` chỉ ở owner. Chỉ 3 role hệ thống; không role tùy biến ở v1.
- Có hiệu lực ở request kế tiếp: permission đã đọc mỗi request (`session_actor`).
- Ít nhất một chủ spa: khóa tài khoản là chủ spa đang hoạt động cuối cùng của spa đang hoạt động → `409 platform.last_owner`.

### D6 — Audit
- `platform_audit_log` (operator, action `platform.*`, tenant, target, detail jsonb không bí mật, lý do, IP, lúc): mọi thao tác operator, cùng transaction với thao tác. Đọc spa audit cũng ghi (`platform.tenant_audit.read`).
- Thao tác chạm một spa ghi thêm `audit_log` của spa đó với `actor_kind = 'platform_admin'` (đã có trong CHECK từ 0001) để chủ spa thấy (ADR-0003 "chủ spa xem được").
  - *Sửa bởi [ADR-0022](ADR-0022-inline-record-history.md) (2026-11-11):* chủ spa xem các dòng này qua màn "Nhật ký thao tác", nay sau cờ `activity_log` **mặc định tắt**. Khi cờ tắt, chủ spa không thấy việc của operator trên UI; dữ liệu vẫn ghi đủ (cả hai phía), operator bật cờ cho spa khi cần. Không có màn riêng cho việc này.
- Không bao giờ ghi: mật khẩu (tạm / mới), hash, secret, SĐT đầy đủ (chỉ 3 số cuối).

### Rollout (3 PR, chồng lên nhau, sau khi #109 + #110 vào main)
1. **PR A — danh tính operator + spa:** migration 0038, crate `platform`, CLI, `/admin/auth/*`, `/admin/me`, spa (danh sách / tạo / ngưng), audit nền tảng (ghi + xem), đăng nhập bị chặn khi spa ngưng, web `/admin` (đăng nhập, MFA, spa).
2. **PR B — cờ tính năng:** migration 0039 (`tenant_feature`, thay `session_actor`), `kernel::feature`, chặn route, `/me.features`, ẩn UI spa, màn tính năng, e2e.
3. **PR C — vai trò + tài khoản + nhật ký spa:** migration 0040 (mẫu vai trò, `follows_template`, hàm tài khoản), màn vai trò / tài khoản / nhật ký spa.

## Consequences
- Positive: operator không cần chuỗi kết nối DB cho việc hằng ngày; mọi thao tác có audit hai phía; lỗi ở app spa không leo thang thành nền tảng; CLI owner chỉ còn cho migrate / provisioning khẩn cấp.
- Negative / accepted: cùng origin ở giai đoạn đầu (D2); thêm 7 hàm plpgsql (`platform_gate` + 6 hàm ở D3) phải giữ đầu ra hẹp (test từng hàm: không có session → lỗi, đầu ra không có cột nhạy cảm); mẫu vai trò trong DB làm "nguồn sự thật" của quyền mặc định chuyển từ code sang DB (code giữ làm giá trị seed + test).
- Follow-ups: support grant (ADR-0003) cho xem chi tiết audit / dữ liệu spa; subdomain khi có domain; tính năng `packages` (Q-PA2); WebAuthn cho operator.

## Update 2026-11-09: Q-PA1…4 đã có câu trả lời ([ADR-0019](ADR-0019-audit-decisions-2026-11-09.md) D17, D19, D20)
- **Q-PA1:** audit spa chỉ metadata, chưa làm support grant. Console hiện nhãn tiếng Việt thay mã action thô.
- **Q-PA2:** chưa có cờ `packages`.
- **Q-PA3:** dùng subdomain `admin.<domain>` khi có domain chính thức; D2 (cùng origin) giữ tới lúc đó.
- **Q-PA4:** giữ quy tắc "không khóa chủ spa cuối".
- **Thêm (D17):** lượt tìm tài khoản xuyên spa ghi `platform.account.search` vào `platform_audit_log`, chuỗi tìm đã che; `%`, `_`, `\` được escape trong `LIKE`.
- **Thêm (0049):** `platform_tenant_overview(p_token_hash, p_tenant uuid DEFAULT NULL)` lọc một spa ngay trong hàm. `GET /admin/tenants/{id}` và bước kiểm spa tồn tại của mọi route theo spa không còn dựng overview của tất cả spa.

## Revisit when
Có hơn một operator với vai trò khác nhau, có tên miền production, có hợp đồng yêu cầu hỗ trợ đọc dữ liệu spa, hoặc số spa > 200 (danh sách cần phân trang / số đếm cần bảng tổng hợp).
