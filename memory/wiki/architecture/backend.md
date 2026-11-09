---
title: Backend design (R1-1)
updated: 2026-11-09
sources: [product/specs/front-desk-and-packages.md, product/specs/management-views.md, architecture/system.md, architecture/data.md, security/authz-matrix.md]
---

# Backend design R1-1

Contract-first: các bảng dưới đây là hợp đồng API, hiện thực bằng `utoipa` → `apps/api/openapi.json` (lệnh `app openapi`, commit vào repo) → `packages/api-client/src/schema.ts` (openapi-typescript). `pnpm api:gen` sinh lại cả hai; CI fail nếu lệch (S3 ✅). Khi trang này và `openapi.json` lệch nhau thì `openapi.json` (sinh từ code) là đúng.

Route nằm ở `apps/api/crates/*/src/http.rs` (lịch hẹn: `operations/src/appointments/http.rs`), ghép trong `app::router` (`crates/app/src/lib.rs`) dưới tiền tố `/api/v1`. Crate: `kernel`, `identity`, `org`, `hr`, `catalog`, `crm`, `packages`, `sales`, `operations`, `reporting`, `platform`, `app` (binary). API của console admin nền tảng (`/api/v1/admin/*`): [platform-admin](platform-admin.md).

## 1. Quy ước API
- REST + JSON, `snake_case`, prefix `/api/v1`. Tiền là số nguyên VND. Thời gian là ISO 8601 có offset. Ngày nghiệp vụ dạng `YYYY-MM-DD`.
- **Lỗi:** RFC 9457 `application/problem+json`: `{type, title, status, code, detail, errors?: [{field, code}]}`. `code` ổn định, frontend dịch sang tiếng Việt.
- **Danh sách:** cursor (`?cursor=&limit=`), `limit` mặc định 20, tối đa 100 (server ép). Sort và filter chỉ theo allowlist.
- **Idempotency:** mọi `POST` có tạo tiền hoặc trừ buổi bắt buộc header `Idempotency-Key` (UUID). Thiếu key → `400 idempotency.key_required`.
  - Cùng key, cùng body → trả lại response cũ.
  - Cùng key, khác body → `422 idempotency.key_reuse_mismatch`. Với lệnh trên một bản ghi (`/visits/{id}/void`, `/sales/{id}/void`, `/customer-packages/{id}/extend|cancel`), hash gồm cả `{id}` (`kernel::idempotency::hash_request_on`), nên dùng lại key cho bản ghi khác → `422`, không bao giờ replay nhầm (S8).
  - Request đầu vẫn đang chạy → request trùng **chờ** (khóa PK của Postgres) cho tới khi request đầu commit, rồi replay; nếu request đầu rollback thì request trùng chạy mới. (`409 idempotency.in_progress` chỉ là nhánh phòng thủ.) Hiện thực: `kernel::idempotency` (S1).
- **Optimistic lock:** DTO có trường `version`; ghi xong trả về `ETag: "<version>"`. Lệnh sửa (`PUT`, thay toàn bộ bản ghi) bắt buộc `If-Match`: thiếu → `400 precondition.if_match_required`, lệch phiên bản → `412 conflict.stale_version` (S4 ✅).
- **Cache:** mọi response `/api` mang `Cache-Control`. Handler không tự đặt → mặc định `private, no-store` (layer ngoài cùng, `app::middleware`, 2026-11-07). Endpoint nào muốn cache phải tự đặt `private, no-cache` + `ETag` (chưa có endpoint nào; `ETag` hiện chỉ cho `If-Match`). Không `public`/`s-maxage`: CDN sẽ chia dữ liệu giữa người dùng.
- **Rate limit (theo IP, in-memory mỗi instance, `app::middleware`):** mọi `/api` 5 request/giây, burst 60 (`HttpPolicy::standard`; chỉ `APP_ENV=development` mới nới được bằng `APP_RATE_PER_SECOND` + `APP_RATE_BURST`); `/auth/*` 1 request / 3 giây, burst 10 (≈ 20/phút); `/admin/auth/*` 1 / 6 giây, burst 5. Lớp thứ hai cho đăng nhập: khóa theo tài khoản sau 5 lần sai (15 phút, gấp đôi mỗi lần sai tiếp, tối đa 24 giờ; `identity::lockout`). Vượt → `429 rate_limited`. Cột "Rate" trong các bảng endpoint dưới đây là phân loại của bản thiết kế đầu (`auth` / `search` / `write` / `read` theo user), **chưa hiện thực**: hiện chỉ có các giới hạn theo IP vừa kể.
- **Middleware khác** (`app::middleware::apply`): timeout 10 giây, body tối đa 64 KB, request ghi phải có `Origin` của spa (`APP_PUBLIC_ORIGIN`; console admin: `APP_ADMIN_ORIGIN` nếu đặt) → thiếu/lạ `403 auth.bad_origin`.

## 2. Mã lỗi nghiệp vụ
| code | HTTP | Khi nào |
|---|---|---|
| `auth.unauthenticated` / `auth.reauth_required` | 401 | Chưa đăng nhập / thao tác 🔒 bảo mật tài khoản cần nhập lại mật khẩu (không dùng cho thao tác nghiệp vụ, EM 2026-11-05) |
| `auth.invalid_credentials` | 401 | Sai SĐT / mật khẩu, tài khoản khóa hoặc disabled (một mã cho mọi trường hợp) |
| `auth.mfa_required` / `auth.mfa_enrollment_required` / `auth.invalid_mfa_code` | 401 | Session mới qua bước mật khẩu, còn nợ mã TOTP / chưa đăng ký MFA / mã sai, hết hạn hoặc dùng lại |
| `auth.password_change_required` | 403 | Đăng nhập bằng mật khẩu tạm (ADR-0016): mọi route trừ `POST /auth/password` và logout |
| `auth.spa_suspended` | 403 | Mật khẩu đúng nhưng mọi spa của tài khoản đang bị tạm ngưng (ADR-0018) |
| `auth.no_active_tenant` / `auth.not_a_member` | 403 | Session chưa chọn spa / không còn là thành viên spa đang chọn |
| `auth.bad_origin` | 403 | Request ghi thiếu hoặc sai `Origin` |
| `auth.forbidden` | 403 | Thiếu permission |
| `feature.disabled` | 403 | Tính năng của route bị admin nền tảng tắt cho spa này (`kernel::feature`, ADR-0018 D4) |
| `not_found` | 404 | Không tồn tại **hoặc thuộc tenant khác** |
| `validation_failed` | 422 | Kèm `errors[]` |
| `customer.phone_taken` | 409 | Kèm `existing_id` (ID hồ sơ đã có) |
| `package.no_sessions_left` / `package.cancelled` | 409 | |
| `package.expired` | 409 | Gửi kèm `expiry_override_reason` + có permission → cho qua |
| `payment.amount_mismatch` | 422 | Tổng thanh toán khác số phải trả (R1 luôn thu đủ) |
| `discount.reason_required` | 422 | |
| `discount.limit_exceeded` | 403 | |
| `staff.inactive` | 422 | Người chốt / KTV không tồn tại, đã nghỉ, hoặc thuộc spa khác (S6 ✅) |
| `package.item_not_found` | 422 | Dòng liệu trình không có hoặc của khách khác (AC4.8) |
| `idempotency.key_reuse_mismatch` | 422 | Cùng `Idempotency-Key`, khác body |
| `price.variant_not_available` | 422 | Ví dụ dịch vụ chỉ dành cho Nữ chọn cho khách Nam |
| `price.gender_required` | 422 | Khách chưa có giới tính, dịch vụ có giá Nữ/Nam, không gửi `price_variant` (S7) |
| `visit.check_in_in_future` / `visit.check_in_not_today` | 422 | Giờ vào > now + 5 phút / khác ngày nghiệp vụ hôm nay (S7) |
| `visit.voided` | 409 | Sửa hoặc hủy lại lượt đã hủy (S7, S8) |
| `visit.corrected` | 409 | Sửa / hủy lượt đã được "Sửa lượt" thay bằng lượt mới (US6b): mở lượt mới nhất. Field `check_in_at not_visit_day` (422): giờ làm khác ngày nghiệp vụ của lượt |
| `sale.voided` | 409 | Hủy lại hóa đơn đã hủy (S8) |
| `sale.has_used_sessions` | 409 | Hủy hóa đơn gói đã dùng buổi (AC6.4) |
| `sale.void_via_visit` | 409 | Hủy trực tiếp hóa đơn dịch vụ lẻ của một lượt: phải hủy lượt (S8) |
| `package.restore_exceeds_total` | 409 | Cộng lại buổi vượt tổng (phòng thủ, S8) |
| `refund.exceeds_paid` | 422 | Hoàn nhiều hơn `sold_price` của gói (AC7.4) |
| `day.locked` | 403 | Quản lý sửa/hủy bản ghi của ngày đã chốt (tạm thời: mọi ngày trước hôm nay) |
| `conflict.stale_version` | 412 | |

## 3. Endpoints
| Method & path | Mục đích (story) | Permission | Idem | Rate |
|---|---|---|---|---|
| `POST /auth/login` | Đăng nhập (SĐT, mật khẩu) → `200 LoginResponse {next: done \| mfa_verify \| mfa_enroll \| password_change, me?}` + Set-Cookie (`me` chỉ có khi `done`); sai SĐT/mật khẩu/khóa/disabled đều → `401 auth.invalid_credentials`; spa ngưng → `403 auth.spa_suspended` | công khai | — | auth (~20/phút/IP + khóa theo tài khoản) |
| `POST /auth/mfa/verify` · `/auth/mfa/enroll/start` · `/auth/mfa/enroll/confirm` | Bước 2 của đăng nhập (TOTP hoặc mã khôi phục, ADR-0004): chỉ nhận session đang `mfa_pending`; `verify`/`confirm` thay bằng session đầy đủ (cookie mới), `confirm` trả mã khôi phục một lần; `409 auth.mfa_not_pending / mfa_already_enrolled / mfa_enrollment_not_started / mfa_enrollment_changed` | session pending MFA | — | auth |
| `POST /auth/password` | Đổi mật khẩu `{current_password, new_password}` (10–128 ký tự, khác mật khẩu cũ và SĐT): kết thúc mọi session của tài khoản, cấp cookie mới, trả `Me`. Route duy nhất nhận session mật khẩu tạm (ADR-0016) | đã đăng nhập | — | auth |
| `POST /auth/password-reset-requests` | "Quên mật khẩu?" `{phone}` → luôn `202 {status: "received"}` dù SĐT có tài khoản hay không; chủ spa thấy yêu cầu thật (`password_reset_request`, ADR-0016) | công khai | — | auth |
| `POST /auth/logout`, `/auth/logout-all` | Đăng xuất | đã đăng nhập | — | write |
| `POST /auth/reauth` | Nhập lại mật khẩu `{password, code?}` (`code` bắt buộc khi tài khoản có MFA) → `recent_auth_at` | đã đăng nhập | — | auth |
| `GET /me` | `Me {user, active_tenant?, tenants[], permissions[], features[]}` (`features` = tính năng đang bật của spa, ADR-0018) | đã đăng nhập | — | read |
| `POST /auth/switch-tenant` | Đổi spa đang làm việc; **xoay token** (đổi quyền); spa không phải thành viên → `404` | membership | — | write |
| `GET /service-categories` · `GET /services?active=` · `GET /package-plans?active=` | Bảng giá (US1) | `catalog.read` | — | read |
| `POST /service-categories` · `PUT /service-categories/{id}` | Thêm / đổi tên + thứ tự nhóm (S4 ✅) | `catalog.manage` | — | write |
| `POST /services` · `PUT /services/{id}` (If-Match) | Dịch vụ + `prices` (all \| female/male). **Ngừng bán = `active:false` trong PUT**, không có endpoint xóa (S4 ✅) | `catalog.manage` | — | write |
| `POST /package-plans` · `PUT /package-plans/{id}` (If-Match) | Gói: items `service × sessions + bonus` (S4 ✅) | `catalog.manage` | — | write |
| `GET /staff?status=active` | Picker KTV, người chốt; `status` = `active` (mặc định), `left` hoặc `all`; chỉ trả tên, chức vụ, trạng thái (S6 ✅) | `staff.read_basic` | — | read |
| `GET /customers?q=` | Tìm theo SĐT (≥ 3 số) / tên (≥ 2 ký tự, không dấu) (US2); không `q` = khách gần đây (bỏ hồ sơ không tên) | `customer.read` | — | search |
| `POST /customers` | Tạo khách (US2) | `customer.write` (+ `customer.health_notes.write` nếu gửi ghi chú) | — | write |
| `GET /customers/{id}` | Hồ sơ `CustomerDto` (thông tin khách, `named`; liệu trình đọc riêng ở `/customers/{id}/packages`, không có công nợ vì R1 không có nợ); `health_note` chỉ khi có `customer.health_notes.read` (US2) | `customer.read` | — | read |
| `PUT /customers/{id}` | Sửa hồ sơ, thay toàn bộ (If-Match); không gồm ghi chú sức khỏe | `customer.write` | — | write |
| `PUT /customers/{id}/health-note` | Đặt/xóa ghi chú sức khỏe (If-Match) | `customer.health_notes.write` | — | write |
| `GET /customer-packages/{id}` | Chi tiết + items, trạng thái suy ra (S6 ✅; lịch sử sử dụng thêm ở S7) | `customer.read` | — | read |
| `GET /customers/{id}/packages` | Liệu trình của khách, mới nhất trước, còn X/Y buổi (S6 ✅) | `customer.read` | — | read |
| `POST /customer-packages/{id}/extend` | Gia hạn (US7, S8 ✅): `{expires_on, reason}` + If-Match. Hạn mới phải sau hạn hiện tại và không ở quá khứ (`422 expires_on not_later / in_past / package_has_no_expiry`); gói đã hủy → `409 package.cancelled`. `200 CustomerPackageDto` + `ETag`; audit before/after + lý do | `package.extend_expiry` | ✓ | write |
| `POST /customer-packages/{id}/cancel` | Hủy + hoàn tiền (US7, S8 ✅): `{reason, refund_amount (0 = không hoàn), refund_method?}`. Buổi còn lại → 0, `cancelled`; hoàn > `sold_price` → `422 refund.exceeds_paid` (gói sổ cũ cũng vậy, AC8.8); tạo một dòng `refund` (sổ quỹ hôm nay). `200 CustomerPackageDto` (có `cancel_reason`, `refund`). Không reauth (EM 2026-11-05) | `package.cancel_refund` | ✓ | write |
| `POST /opening-balances` | **Tạm thời, chỉ chủ spa** (R1-2 §7: team migrate bằng `app import-paper-book`; xóa route khi web bỏ màn `/nhap-so-cu`; 2026-11-09: web không còn màn này, route API vẫn còn, chưa xóa). Nhập liệu trình dở dang (US8, S9 ✅): `customer_id` **hoặc** `customer{…như POST /customers}` + `packages[{package_plan_id?, name?, sold_price, starts_on, expires_on?, gift_note?, items[{service_id, sessions_purchased, sessions_bonus?, sessions_remaining}]}]` (1–10). `201 {customer_id, customer_created, customer_packages}`; không sale/payment; lỗi field có tiền tố `customer.` / `packages[j].`; bắt buộc `Idempotency-Key` | `opening_balance.import` (chỉ Owner từ 0013; + `customer.write` khi tạo khách) | ✓ | write |
| `POST /sales` | Bán gói: `lines[{package_plan_id, gift_note?}]` (1–10), `closer_staff_id`, `discount{amount, reason}?`, `payments[{method, amount, reference?}]` (0–2, mỗi hình thức 1 lần). `201` trả `SaleDto` kèm `customer_packages` (US3, S6 ✅) | `package.sell` (+ `sale.discount*`) | ✓ | write |
| `POST /sales/checkout` | Giỏ bán gói không kèm lượt: `{customer_id, package_sales[1–3]{package_plan_id, closer_staff_id?, gift_note?, discount?, beneficiary_customer_id?, note?}, payments[0–2]}` → `201 SaleCheckoutDto {sales[], total}`: mỗi gói một hóa đơn, tất cả hoặc không gì; tiền chia như checkout của lượt (TM trả theo thứ tự trước, còn lại CK) | `package.sell` (+ `payment.collect`, `sale.discount*`, `customer.health_notes.write` khi có `note`) | ✓ | write |
| `GET /sales?date=` | Bán gói không kèm lượt của một ngày (F6, 2026-11-08): `[{sale, customer_name, customer_gender, beneficiary_name}]`, mới nhất trước, ≤ 200; hóa đơn có dòng gói, `visit_id IS NULL`, `source = 'app'` | `sale.read` + `customer.read` | — | read |
| `GET /sales/{id}` | Chi tiết hóa đơn: lines, payments, customer_packages (S6 ✅); `note` ("Ghi chú gói", F4) chỉ cho `customer.health_notes.read` | `sale.read` | — | read |
| `PATCH /sales/{id}` | "Sửa ghi chú gói" (F4, 2026-11-08): `{note}` (≤ 2000, `null`/trống = xóa), mã hóa AAD `sale.note`; ngày trước cần `record.void_closed_day` (`403 day.locked`, trước lỗi thân); `409 sale.voided` / `sale.not_package`; audit `sale.note_update` không chữ. Ghi `note` lúc bán: `POST /sales`, `package_sales[k].note` của `/sales/checkout`, `/visits`, `/additions` (cần `customer.health_notes.write`) | `package.sell` + `customer.health_notes.write` | — | update |
| `POST /sales/{id}/void` | Hủy hóa đơn bán gói + `{reason}` (US6, S8 ✅): mỗi payment một `reversal`, `amount_paid` → 0, các gói của hóa đơn → `cancelled` (buổi → 0). Gói đã dùng buổi → `409 sale.has_used_sessions`; gói đã hủy/hoàn tiền → `409 package.cancelled`; hóa đơn dịch vụ lẻ của lượt → `409 sale.void_via_visit`; hủy lần 2 → `409 sale.voided`. `200 SaleDto` (`void_reason`, `voided_at`) | `record.void_same_day`; ngày trước hôm nay: `record.void_closed_day` (thiếu → `403 day.locked`) | ✓ | write |
| `POST /visits` | Ghi lượt: `lines` (1–20), `check_in_at?`, `note?`, `payments[]` (US4). `201` trả `VisitDto` (không bao giờ có `note`) (S7 ✅) | `visit.record` (+ `payment.collect` khi có tiền, `customer.health_notes.write` khi có ghi chú, `package.use_expired_override` khi có lý do) | ✓ | write |
| `GET /visits?date=&limit=` | Lượt của một ngày nghiệp vụ (mặc định hôm nay), mới nhất trước, kèm lines + payments; `limit` 1–500 (mặc định cả ngày); `note` chỉ có khi người gọi có `customer.health_notes.read` (từ 2026-11-07; S7 ban đầu không trả ghi chú) | `visit.read` | — | read |
| `POST /visits/{id}/correct` | **Sửa lượt có đụng tiền/buổi (US6b, 0020):** If-Match + `Idempotency-Key` (hash gồm `{id}`); thân `VisitCorrection` = thân `POST /visits` + `reason` (1–490, lưu "Sửa lượt: …"), `note` vắng = giữ ghi chú cũ (server giải mã + mã hóa lại cho lượt mới), `check_in_at` vắng = giờ cũ, phải cùng ngày nghiệp vụ. Một transaction: `begin_correction` (khóa lượt → `ensure_current` → `authorize_void` → version → `void_locked`, đúng thân của `/void`) rồi `finish_correction` (`record_in` với `Order.correction` = lượt cũ, ngày của lượt cũ, không cần `visit.backdate`; chuyển `appointment.visit_id`; audit `visit.correct`). `201 VisitCorrected {voided, visit, difference}` (không có `note`); hai lần sửa song song → một `201`, một `409 visit.corrected`. Hai bước là `pub(crate)` để "chuyển sang gói" (`POST /visits/{id}/convert-to-package`, §3e) bán gói giữa hai bước | `visit.record` + `record.void_same_day` (+ quyền của thân như `POST /visits`); ngày trước: `record.void_closed_day` | ✓ | write |
| `GET /visits/{id}` · `PATCH /visits/{id}` | `VisitDto` thêm `corrects_visit_id?`, `corrected_by_visit_id?` (0020). Xem (có `note` nếu có `customer.health_notes.read`, `ETag`) / sửa ghi chú, KTV từng dòng và **giờ làm trong cùng ngày** (`check_in_at`, US6b: khác ngày → `422 check_in_at not_visit_day`, tương lai → `422 visit.check_in_in_future`) (If-Match; không đụng buổi hay tiền; lượt đã được sửa → `409 visit.corrected`; ngày đã chốt theo đúng quy tắc của hủy: Manager → `403 day.locked`, Owner (`record.void_closed_day`) → được, không reauth) (S7 ✅) | `visit.read` / `visit.record` (+ `customer.health_notes.write` khi sửa ghi chú) | — | read/write |
| `POST /visits/{id}/void` | Hủy lượt + `{reason}` (US6, S8 ✅): lượt → `voided` (giữ trong lịch sử, `void_reason`, `voided_at`), buổi đã trừ được cộng lại, sale dịch vụ lẻ của lượt bị hủy bằng reversal. Hủy lần 2 → `409 visit.voided`; gói đã bị hủy từ lúc đó → `409 package.cancelled`; thiếu lý do → `422`. `200 VisitDto` (không có `note`) | `record.void_same_day`; ngày trước hôm nay: `record.void_closed_day` (thiếu → `403 day.locked`) | ✓ | write |

**Có trong thiết kế đầu nhưng không có route:**
- `POST /auth/activate` (kích hoạt / đặt lại mật khẩu bằng token): thay bằng mật khẩu tạm do chủ spa / operator tạo + `POST /auth/password` + `POST /auth/password-reset-requests` ([ADR-0016](../decisions/ADR-0016-accounts-and-passwords.md)).
- `GET /customers/{id}/visits`: lịch sử lượt nằm trong `GET /customers/{id}/history` (§3b).
- `POST /customers/{id}/anonymize`: chưa làm (cột `customer.anonymized_at` và permission `customer.anonymize` đã có).
- `POST /support-grants` · `DELETE /support-grants/{id}`: chưa làm (permission `support_grant.manage` còn trong danh mục, không bảng, không route; [ADR-0003](../decisions/ADR-0003-multi-tenancy.md)).

### 3b. Xem lại & theo dõi (R1-2, crate `reporting`, [spec](../product/specs/management-views.md))
Chỉ đọc, `GET`, `Cache-Control: no-store`, rate class `read`. Query param nào cũng có mặc định, nên thiếu param không bao giờ thành 400 trước khi kiểm quyền. Danh sách: cursor mờ (base64url của khóa sắp xếp), `limit` 1–100 (mặc định 20); cursor sửa tay → `422 cursor invalid`. Trường tiền là `Option` và **vắng hẳn** khi thiếu `report.revenue.read`.

| Method & path | Trả về (schema) | Permission |
|---|---|---|
| `GET /reports/daily?date=` | `DailyReport {date, counts: DayCounts{customers, visits, voided_visits, sessions_used, one_off_services, packages_sold}, revenue?: Revenue{collected, collected_by_kind, voids, voids_by_kind, refunds, net, net_by_kind}, technicians[]{staff_id, name, visits, sessions_used, one_off_services}, compare{yesterday, same_weekday_last_week: DayHeadline{date, customers, visits, sessions_used, packages_sold, net_revenue?}}}`. `MethodSplit = {cash, bank_transfer, total}`, `KindSplit = {package, one_off}` | `report.read` (+ `report.revenue.read` cho tiền) |
| `GET /reports/range?from=&to=&granularity=` | `RangeReport {from, to, granularity: day/week/month, totals: DayCounts (customers = khác nhau trên cả khoảng), revenue?, series[]{start, end, counts, revenue?} (đủ mọi kỳ, kỳ rỗng = 0, tuần ISO bắt đầu thứ Hai), top_services[10]{service_id, name, sessions_used, one_off_services, one_off_revenue?}, top_packages[10]{plan_id, name, sold, revenue?}}`. Mặc định 7 ngày; tối đa 366 ngày (`422 to range_too_long`); `from > to` → `422 from after_to` | `report.read` |
| `GET /reports/ledger?from=&to=&cursor=&limit=` | `LedgerPage {from, to, rows[]{id, entry: sale/void/refund, business_date, at, customer_id, customer_name, kind: package/one_off, one_off?, package?, bank_transfer, cash, unpaid, note?, voided}, days[]{date, bank_transfer, cash, unpaid, total, entries} (mọi ngày của khoảng, không chỉ trang), next_cursor}`. Thứ tự sổ: ngày rồi thứ tự ghi (UUIDv7). Mặc định hôm nay; tối đa 92 ngày | `report.revenue.read` |
| `GET /customers/{id}/history?cursor=&limit=` | `HistoryPage {customer_id, events[]{kind: visit/purchase/visit_voided/purchase_voided/package_cancelled, at, business_date, visit?{id, voided, check_in_at, lines[]{service_name, source, customer_package_id?, package_name?, qty, sessions_left_after?, staff_id, staff_name, amount}, paid, void_reason?, voided_at?}, purchase?{sale_id, voided, lines[]{description, qty, line_total, customer_package_id?}, subtotal, discount_amount, discount_reason?, total, paid, closer_name?, void_reason?, voided_at?}, void?{id, reason, reversed, sessions_restored}, cancellation?{customer_package_id, package_name, reason, refund?{amount, method, business_date}}}, next_cursor}`. Ngày mới nhất trước | `customer.read` + `visit.read` + `sale.read` |
| `GET /customers/{id}/summary` | `CustomerStats {customer_id, first_visit_on?, last_visit_on?, days_since_last_visit?, visit_count, voided_visit_count, sessions_used, paid, voided, refunded, net_spent, packages{active, expired, exhausted, cancelled}, sessions_remaining}` | `customer.read` + `visit.read` + `sale.read` |
| `GET /customer-packages/{id}/usage` | `PackageUsage {package: CustomerPackageDto, uses[]{visit_id, visit_line_id, business_date, check_in_at, item_id, service_name, qty, sessions_left_after?, staff_id, staff_name, voided}}` (cũ → mới, như trang sổ liệu trình) | `customer.read` + `visit.read` |
| `GET /packages?status=&expiring_within_days=&q=&cursor=&limit=` | `PackageOverviewPage {items[]{id, customer_id, customer_name, customer_phone, name, status, sessions_total, sessions_remaining, starts_on, expires_on?, days_to_expiry?, last_used_on?, cancelled_at?}, counts{active, expiring_soon, expired, exhausted, cancelled}, expiring_within_days, next_cursor}`. Không có `expiring_within_days`: mới nhất trước; có: chỉ gói active hết hạn ≤ hôm nay + N, gần nhất trước | `customer.read` |
| `GET /customers/inactive?days=&scope=&cursor=&limit=` | `InactivePage {days, items[]{customer_id, full_name, phone, last_visit_on?, last_seen_on, days_absent, active_packages, sessions_remaining}, next_cursor}`; vắng lâu nhất trước; mặc định 30 ngày, `scope=with_sessions` (mặc định) hoặc `all` | `customer.read` + `visit.read` |

**Vòng 3 (EM 2026-11-05, spec "Bổ sung vòng 3", migration 0014):**

| Method & path | Thay đổi | Permission |
|---|---|---|
| `POST /customers/{id}/follow-ups` (mới, crate `crm::follow_up`) | body `FollowUpCreate {contacted_on?, channel, outcome, note?, next_follow_up_on?}` → `201 FollowUpDto {id, customer_id, contacted_on, channel, outcome, note?, next_follow_up_on?, by_user_id, by_name, created_at}`. Idempotency-Key (hash = route + id + JSON chuẩn hóa). Mặc định khi không có ngày: `no_answer` +3, `will_come` +7, `not_interested` / `other` không có (`not_interested` nghỉ 90 ngày khỏi `follow_up=due`). `GET` trả `suggested_next_on_by_outcome` để form điền sẵn. `422`: `contacted_on in_future/too_old`, `next_follow_up_on before_contact/too_far`, `note too_long` | `customer.follow_up` |
| `GET /customers/{id}/follow-ups` (mới) | `FollowUpList {items[≤100], has_more}`, mới nhất trước | `customer.read` |
| `GET /customers/phone-check?phone=` (mới) | `PhoneCheckDto {valid, taken, existing_id?, full_name?}`; chuẩn hóa `PhoneE164::parse` như tạo khách | `customer.read` |
| `GET /customers/inactive` | thêm `kind`, `follow_up`, `sort=due\|absent`, `q`; `scope` mặc định `all`; item thêm `gender`, `kind`, `last_follow_up {on, channel, outcome, by}`, `next_follow_up_on`, `due_on` (ngày hẹn, hoặc `not_interested` + 90); "lần gần nhất" = max(lượt, hóa đơn chưa hủy, ngày bắt đầu gói); cursor 3 phần (khóa hạn, last_seen, id) | như cũ |
| `GET /customers/{id}/history` | thêm `q`, `kind`, `from`, `to` (lọc trong SQL qua `app.search_fold`) | như cũ |
| `GET /notes?from=&to=&kind=&staff_id=&customer_id=&q=&cursor=&limit=` (2026-11-07, management-views §10) | `NotesPage {from, to, items[]{kind, at, business_date, text, customer_id?, customer_name?, staff_id?, staff_name?, author_name?, visit_id?, detail?}, kinds[], next_cursor, scan_capped}`; 16 loại (`reporting::notes_dto::NoteKind`, lúc đầu 9, thêm ở 0028–0030), mỗi loại quyền riêng; mặc định 30 ngày, ≤ 366; tìm ghi chú mã hóa quét ≤ 500/request | `report.read` + quyền từng loại |
| `GET /packages` | `q` khớp cả tên gói; row thêm `customer_gender` | như cũ |
| `GET /reports/range` | `preset` (`last_7_days`, `last_30_days`, `this_month`, `last_month`, `this_year`, `last_12_months`, `last_2_years`; chủ spa thêm `last_3_years`, `last_5_years`), `granularity=year`, giới hạn 366 ngày (day/week) / 732 ngày (month/year) / 1830 ngày với `report.long_range` (0015); bucket thêm `net_revenue`; report thêm `preset`, `technicians[]`; đọc sổ bằng `BETWEEN` (không gửi mảng ngày) | như cũ |
| `GET /reports/ledger` | row thêm `customer_gender` | như cũ |
| DTO có tên dịch vụ | `category_name` (danh mục hiện tại): `VisitLineDto`, `HistoryVisitLine`, `PackageUse`, `CustomerPackageItemDto`, `TopService`. `VisitDto.customer_gender` | — |

OpenAPI: struct query (`SearchQuery`, `ActiveFilter`, `VisitListQuery`, `StaffQuery`, `PhoneCheckQuery`) trước đây bị khai báo là **path** param (thiếu `#[into_params(parameter_in = Query)]`); đã sửa, test `openapi::non_path_parameters_are_documented_as_query` chặn tái phát. Search: mọi `q` ≤ 100 ký tự; `/visits?date=`, `/staff`, `/services`, `/package-plans` trả trọn danh sách nhỏ (một ngày / danh mục) nên lọc ở client; sổ doanh thu không có `q` (sổ theo ngày, tìm theo khách dùng lịch sử khách).

**Quy tắc tiền** (spec §3): thu = `payment.kind='payment'`; hủy = `reversal`, ghi vào ngày hủy; hoàn = `refund`; net = thu − hủy − hoàn; loại = hóa đơn có dòng `package` hay không. Số đếm theo trạng thái hiện tại (lượt/hóa đơn đã hủy không đếm). `CustomerPackageDto` **không còn `origin`** (R1-2 §7); gói từ sổ giấy chỉ khác ở `sale_id = null`.

**Hiệu năng** (`app/tests/reports_bench.rs`, `--ignored`; 3 năm ≈ 44.000 lượt, 30.800 hóa đơn; build debug, 1 client, p95 ms): daily 41 · range 7 ngày 41 · range 1 năm theo tháng 150 · ledger 1 ngày 19 · ledger 92 ngày 59 · history 31 · summary 16 · usage 16 · packages 40 · expiring 26 · `q` 16 · inactive 35. Ngân sách 300 ms. Vòng 3 (cùng bench): range 1 năm theo tháng **120** · `last_12_months` 133 · `last_2_years` 224 · 2 năm theo năm 237 · history `q` 23 · history `kind+from/to` 22 · inactive `follow_up=due` 60 · inactive `q+kind` 51 · phone-check 9. Mọi số đếm phía lượt (theo kỳ, theo dịch vụ, theo KTV, khách khác nhau) lấy từ **một** CTE `MATERIALIZED` trên visit ⋈ visit_line (đọc bảng gốc + RLS một lần; trước đó 5 lần quét, 1 năm 250 ms, 3 năm 690 ms). 3 năm vẫn ~380 ms nên tháng/năm giới hạn 2 năm; chủ spa (`report.long_range`) xem tới 5 năm với ngân sách riêng **1,5 s** (`LONG_RANGE_BUDGET_MS` trong bench; đo `last_3_years` 368, `last_5_years` 478, 3 năm theo tháng 454 ms). Lần đo đầu, range 1 năm là 396 ms vì tiền được tổng hợp qua view `revenue_book` (mỗi hóa đơn một string_agg mô tả rồi bỏ đi), nên đổi sang tổng hợp thẳng từ `payment` + tập hóa đơn có dòng gói.

**Thân `POST /visits` (rút gọn):**
```json
{ "customer_id": "…", "check_in_at": "2026-11-05T09:12:00+07:00", "note": "…",
  "lines": [
    { "service_id": "…", "source": "package", "customer_package_item_id": "…", "staff_id": "…", "qty": 1,
      "expiry_override_reason": null },
    { "service_id": "…", "source": "one_off", "staff_id": "…", "qty": 1, "price_variant": "male" }
  ],
  "payments": [ { "method": "bank_transfer", "amount": 100000, "reference": null } ] }
```
- **Không có giờ ra** (EM 2026-11-04). **Giảm giá dịch vụ lẻ có từ vòng 4** (xem §3c).
- **Server tự quyết định giá:** `one_off` lấy giá của dịch vụ (khóa `FOR SHARE`) theo giới tính của khách. Khách chưa có giới tính: dịch vụ giá chung thì không cần gì, dịch vụ có giá Nữ/Nam thì bắt buộc gửi `price_variant` (thiếu → `422 price.gender_required`). Khách đã có giới tính mà gửi `price_variant` khác → `422 validation_failed` (`lines[i].price_variant`). Dịch vụ không có giá cho giới tính đó → `422 price.variant_not_available`. Field lạ (`unit_price`, `total`) bị bỏ qua.
- `package`: `customer_package_item_id` bỏ trống → server chọn gói còn dùng được (chưa hủy, chưa hết hạn, còn buổi) chứa dịch vụ, **hết hạn sớm nhất trước, không thời hạn sau cùng, rồi mua sớm nhất** (AC4.3); không có → `422 package.item_not_found`. Item không chứa `service_id` của dòng → `422 validation_failed` (`not_in_package`). `unit_price` = `per_session_value`, `line_value` = giá trị buổi thực trừ (buổi tặng = 0).
- Các item được trừ theo thứ tự `item_id` để hai lượt nhiều dòng tranh cùng các item không deadlock. Dòng `one_off` tạo một `sale` (không người chốt, `line_type='service'`) qua `sales::service::record_service_sale` trong cùng transaction; `sum(payments)` phải bằng tổng dòng lẻ (lượt chỉ trừ buổi mà gửi payment → `422 payment.amount_mismatch`).
- Ghi chú buổi mã hóa AES-256-GCM (`kernel::crypto`, AAD `visit.note` + tenant + visit). Response lưu trong idempotency và audit **không bao giờ** chứa ghi chú. Đọc được (giải mã) ở `GET /visits/{id}`, `GET /visits` (danh sách ngày) và `GET /customers/{id}/history` (`reporting::customer`, cùng AAD), chỉ cho `customer.health_notes.read` (2026-11-07).
- `POST /sales` (S6): field lạ trong body (ví dụ `unit_price`, `total`) bị **bỏ qua**, không báo lỗi; mọi số tiền tính từ catalog. Request hash của idempotency = JSON đã parse rồi serialize lại (key đã sắp xếp), nên khác khoảng trắng hay thứ tự key vẫn là cùng một request.
- `check_in_at` = **giờ làm dịch vụ** (mặc định = lúc lưu; khác `created_at` = lúc ghi): quá 5 phút trong tương lai → `422 visit.check_in_in_future`; ngày trước hôm nay mà thiếu `visit.backdate` → `422 visit.check_in_not_today`; quá 366 ngày → `422 visit.check_in_too_old`. Ngày nghiệp vụ của lượt, hóa đơn và payment = ngày của giờ làm (`operations::domain::service_time`); hạn liệu trình xét tại ngày đó (vòng 4).

### 3c. Vòng 4: lịch hẹn, nhập bù, giảm giá lượt ([spec](../product/specs/appointments.md), migration 0016)
| Method & path | Mục đích | Permission | Idem |
|---|---|---|---|
| `GET /appointments?from=&to=` | `CalendarDto {from, to, today, opens_at, closes_at, slot_minutes, time_zone, technicians[]{id, full_name}, max_per_slot?, appointments[], visits[]}`; mặc định hôm nay, tối đa 14 ngày (`422 to range_too_long / before_from`). Vòng 5: `capacity` bỏ, thêm `max_per_slot` (cài đặt của chủ spa, null = không giới hạn) và `visits[]` = lượt vãng lai (`CalendarVisitDto`, rỗng nếu thiếu `visit.read`) | `appointment.manage` | — |
| `POST /appointments` | `{customer_id \| guest{full_name, phone}, service_ids[1–5], staff_id?, starts_at, duration_min?, note?}` → `201 AppointmentDto` | `appointment.manage` | ✓ |
| `POST /appointments/group` (vòng 5) | `{starts_at, note?, people[2–6]{customer_id \| guest, service_ids, staff_id?, starts_at?, duration_min?}}` → `201 {group_id, appointments[]}`; cả nhóm một transaction, lỗi field `people[i].…` | `appointment.manage` | ✓ |
| `PUT /appointment-settings` (vòng 5) | `{max_per_slot: 1–200 \| null}` (bắt buộc có field) → `200 {max_per_slot}`; audit `appointment.settings` | `appointment.settings` (Owner) | — |
| `GET /appointments/{id}` · `PATCH /appointments/{id}` (If-Match) | xem / đổi giờ, thời lượng, KTV, dịch vụ, ghi chú, nối khách mới với hồ sơ (`customer_id`) | `appointment.manage` | — |
| `POST /appointments/{id}/arrive` | `{staff_id}` booked → arrived (chỉ đúng ngày, KTV phải rảnh) | `appointment.manage` | — |
| `POST /appointments/{id}/undo-arrival` | "Hoàn tác đã đến": arrived → booked, chỉ lịch hôm nay chưa có lượt (đã hoàn tất → `409 appointment.closed`; lịch không ở trạng thái đến → `409 appointment.not_arrived`); audit `appointment.undo_arrival` | `appointment.manage` | — |
| `POST /appointments/{id}/move-group` (If-Match) | "Đổi cả nhóm" `{starts_at}`: lịch `{id}` dời tới giờ mới, mọi người khác trong nhóm còn `booked` dời cùng khoảng (giữ thời lượng, KTV), cả nhóm một transaction; người đã đến / xong đứng yên và trả về trong `not_moved`; một người bị từ chối → `422 validation_failed` với `errors[].field = appointments.<id người đó>`, không ai dời; lịch lẻ → `422 appointment.not_in_group`; `{id}` không còn booked → `409 appointment.already_arrived` | `appointment.manage` | — |
| `POST /appointments/{id}/complete` | thân như `POST /visits` trừ `customer_id` → `200 AppointmentCompleted {appointment, visit}` | `appointment.manage` + `visit.record` | ✓ |
| `POST /appointments/{id}/cancel` · `/no-show` | `{reason}` (no-show chỉ sau giờ hẹn) | `appointment.manage` | — |

- `AppointmentDto`: `status` (`booked`/`arrived`/`done`/`no_show`/`cancelled`), **`phase`** (vòng 5: `waiting` / `in_service` / `done` / `no_show` / `cancelled` / `voided`, `domain::appointment_phase`: arrived = in_service; done = in_service tới `visit.check_in_at + duration_min`, sau đó done; lượt bị void = voided), **`group_id?`**, `business_date`, `starts_at`, `ends_at`, `duration_min`, `customer_id?`, `customer_name`, `customer_phone` (E.164), `customer_gender?`, `is_guest`, `staff_id?`, `staff_name?`, `services[]{service_id, service_name, category_name, duration_min?}`, `note?`, `status_reason?`, `arrived_at?`, `closed_at?`, `visit_id?`, **`payment_state`** (`unpaid` / `paid` / `no_charge` / `voided` / `none`), `paid_amount`, `version`.
- `CalendarVisitDto` (vòng 5): `id, phase, business_date, starts_at (= check_in_at), ends_at, duration_min` (tổng `duration_min` dịch vụ của các dòng, trống = 60, làm tròn lên ô, kẹp 1 ô–8 giờ: `domain::visit_minutes`), `customer_id, customer_name, customer_gender?, staff_id, staff_name` (dòng đầu = cột), `services[], payment_state (paid / no_charge / voided), paid_amount`. Không có ghi chú buổi, không SĐT. Hai query (`repo::walk_ins` + `repo::visit_lines`).
- **Trùng KTV** (`operations::appointments`): khóa `pg_advisory_xact_lock(hashtextextended('appointment:<tenant>:<ngày>'))` rồi tìm lịch `booked`/`arrived` của cùng KTV chồng `[starts_at, ends_at)` → `409 appointment.staff_busy` + `existing_id`. **Trùng khách (2026-11-07, [spec §10.11](../product/specs/appointments.md)):** cùng khóa, `repo::customer_overlap` tìm lịch mở chồng giờ của cùng `customer_id`, hoặc cùng `guest_phone` ngoài nhóm của lịch → `409 appointment.customer_busy` + `existing_id` (đặt mới, nhóm, `group_with`; `PATCH` chỉ khi đổi giờ / gắn hồ sơ; `arrive` không kiểm). Không có ràng buộc DB lớp 2 cho trùng khách (khách mới chỉ có SĐT; khóa ngày đủ). **Thời lượng chính xác (quyết định A):** `duration_minutes` / `visit_minutes` = tổng phút dịch vụ (không `div_ceil` theo ô), 5–480; chỉ giờ bắt đầu phải đúng ô. **Ngày đã qua:** `check_day` → `422 appointment.in_the_past` cho mọi người khi đặt mới / dời ngày (lịch đã ở ngày cũ vẫn qua `lock_open` + `authorize_closed_day`). **Vòng 5 bỏ chặn sức chứa** (`409 appointment.slot_full` theo số KTV): số khách tối đa là `appointment_setting.max_per_slot` của chủ spa, chỉ hiển thị + cảnh báo ở web. Đặt nhóm: từng người đi qua đúng `insert_booking` của lịch lẻ trong cùng transaction, nên người sau thấy lịch của người trước (2 người cùng KTV → `staff_busy`). KTV (cột lịch) = nhân viên đang làm có chức vụ chứa "kỹ thuật"/"KTV" (không ai → mọi nhân viên), đọc qua `org::service::active_in` (cùng nguồn với `GET /staff`). Lớp 2: `EXCLUDE USING gist (tenant_id =, staff_id =, tstzrange &&) WHERE open`. Giờ mở cửa / ô giờ: `tenant.settings.opening_hours {open, close}`, `slot_minutes` (mặc định 09:00–21:00, 15 phút; giá trị hỏng → mặc định).
- **Hoàn tất = lượt khách:** `operations::service::record_in(tx, …)` là thân của `POST /visits` chạy trong transaction của người gọi (idempotency claim do route sở hữu). `complete` khóa lịch → kiểm ngày đã chốt → khách bắt buộc (`422 appointment.customer_required`) → `record_in` → `status = done, visit_id` → audit `appointment.complete` → lưu response idempotent (không có ghi chú buổi). Giờ làm mặc định: lúc khách đến, nếu không thì giờ hẹn (nếu đã qua), nếu không thì bây giờ.
- **Giảm giá lượt:** `VisitCreate.discount {percent? | amount?, reason}` (đúng một). `sales::service::record_service_sale` tính `percent → floor(subtotal × % / 100)`, kiểm D10 như bán gói, chia cho từng dòng lẻ bằng `allocate_discount` → `visit_line.discount_amount`, `line_value = qty × unit_price − phần giảm` (CHECK 0016). `VisitDto` thêm `subtotal`, `discount_amount`, `discount_reason`; `VisitLineDto.discount_amount`. Sổ doanh thu hiện lý do giảm ở cột ghi chú (view `revenue_book` đã dùng `sale.discount_reason`).
- Mã lỗi mới: `appointment.staff_busy` / `customer_busy` (2026-11-07) / ~~`slot_full`~~ (bỏ ở vòng 5) / `closed` / `already_arrived` (409), `appointment.in_the_past` (422, 2026-11-07), vòng 5: field `people count_out_of_range`, `people[i].customer_id duplicate`, `people[i].starts_at not_same_day`, `max_per_slot required / out_of_range`; `appointment.outside_opening_hours` / `not_today` / `not_started` / `customer_required` (422), `visit.check_in_too_old` (422); field: `starts_at not_on_slot / too_far`, `duration_min out_of_range`, `discount percent_or_amount`, `discount.percent out_of_range`.

### 3d. Quản lý nhân viên (crate `org` + crate `hr`, [spec](../product/specs/staff-management.md), migration 0017; đợt làm việc 0025, tài khoản 0032)
Mọi response `Cache-Control: no-store`; `?month=YYYY-MM` mặc định tháng hiện tại theo giờ spa.

| Method + path | Mô tả | Permission | Ghi chú |
|---|---|---|---|
| `POST /staff`, `PUT /staff/{id}` (`If-Match`) | Tuyển / sửa hồ sơ: tên, vị trí, SĐT, ngày vào (đợt hiện tại), `shift_start`, `note` (chủ spa); `POST` nhận thêm `user_id?` để gắn tài khoản ngay | `staff.manage` | `409 staff.account_taken`, `412` |
| `GET /staff/profiles`, `GET /staff/{id}` | Danh sách nhân sự / một hồ sơ (`StaffProfileDto`: đợt làm việc, thâm niên); `details` (SĐT, ghi chú, tài khoản) chỉ cho `staff.manage`, `pay` chỉ cho `payroll.read` | `staff.read` | |
| `PUT /staff/{id}/account` (`If-Match`) | Gắn / bỏ gắn tài khoản (`user_id`, `null` = bỏ) | `staff.manage` | `409 staff.account_taken` |
| `POST /staff/{id}/end` · `POST /staff/{id}/rehire` (`If-Match`) | Nghỉ việc (ngày cuối ≤ hôm nay, lý do, `lock_login?`) / nhận lại (đợt mới sau đợt cũ), ADR-0013, ADR-0014 | `staff.manage` | `409 staff.already_left / has_upcoming_bookings / already_active`, `422 staff.cannot_end_self / linked_to_owner` |
| `GET /staff/{id}/history` | Lịch sử thay đổi hồ sơ (từ audit: `staff.create`, `staff.update`, `staff.account_link`, `staff.end`, `staff.rehire`, `staff.pay_update` chỉ cho `payroll.read`) | `staff.manage` | |
| `PUT /staff/{id}/pay` | Lương cứng, `commission_rate_bp`, thưởng gắn bó, tiền ăn | `staff.manage` + `payroll.read` | audit `staff.pay_update` |
| `GET /staff/accounts` | Tài khoản của spa để gắn | `staff.manage` | |
| `POST /staff/{id}/login` · `POST /staff/{id}/login/reset-password` · `PUT /staff/{id}/login/role` | Tạo tài khoản cho hồ sơ (`{phone, role: manager \| technician}` → `201` kèm mật khẩu tạm hiện một lần, `no-store`) / đặt lại mật khẩu tạm (mọi session của tài khoản bị đóng) / đổi role ([ADR-0016](../decisions/ADR-0016-accounts-and-passwords.md)); cần nhập lại mật khẩu (`401 auth.reauth_required`) | `staff.manage` | `409 account.phone_taken / staff.account_exists / staff.no_login / account.locked`, `422 account.owner_protected / account.other_spa` |
| `GET /staff/password-reset-requests` · `POST /staff/password-reset-requests/{id}/dismiss` | Yêu cầu "Quên mật khẩu?" đang mở / "Bỏ qua" (`204`) | `staff.manage` | |
| `GET/PUT /attendance` | Lưới tháng × người; PUT 1–100 ô/transaction, `status: null` xóa | `attendance.record` | phút muộn do server tính |
| `GET/PUT /daily-reviews` | Khen/phàn nàn/ghi chú theo ngày | `evaluation.record` | |
| `GET/PUT /evaluations` | Phiếu 6 tiêu chí tháng (upsert) | `evaluation.record` | `hr::evaluation::score` |
| `GET /evaluations/history?staff_id=&from=&to=` · `GET /me/evaluations/history?from=&to=` | Kết quả đánh giá qua các tháng của một người / của chính mình (`?staff_id=` ở bản `/me` → 400) | `evaluation.record` / `staff.read_own` | |
| `GET /adjustments`, `POST /adjustments` (`Idempotency-Key`) | Sổ thưởng/phạt, `corrects_month` | `evaluation.record` | |
| `PUT /adjustments/{id}` (`Idempotency-Key`) · `DELETE /adjustments/{id}` | Sửa = hủy dòng cũ + ghi dòng mới (id mới) trong một transaction / xóa = hủy (`voided_at/by`, giữ cho audit; 0024). Tháng đã chốt → `409 payroll.period_closed`; đã hủy → `409 adjustment.voided` | `evaluation.record` | |
| `GET /payroll`, `GET /payroll/payslips/{staff_id}` | Bảng lương; tạm tính hoặc bản chụp | `payroll.read` | |
| `POST /payroll/close` (`Idempotency-Key`) | Chốt tháng đã kết thúc | `payroll.close` (+ `payroll.read`) | `422 payroll.month_not_ended / unrecorded_days`, `409 payroll.period_closed` |
| `GET /payroll/rules`, `PUT /payroll/rules` | Quy tắc cho 8 mâu thuẫn | `payroll.read` / `payroll.close` | `deny_unknown_fields`, có giới hạn |
| `GET /me/staff` | Tháng của chính mình | `staff.read_own` | `{linked:false}` nếu chưa gắn |
| `GET /me/staff/day?date=` (EM 2026-11-07) | `MyDay {linked, data?: {date, today, closed, attendance?, review?, adjustments[], visits[]{visit_id, check_in_at, customer_name, lines[]{service_name, source, qty, value, commission?}}, upsell[]{kind, description, customer_name, qty, value, commission?}, commission_rate_bp?}}`: một ngày của chính mình; `value` theo quy tắc buổi tặng, hoa hồng = tỷ lệ × giá trị (tháng chốt: tỷ lệ của phiếu đã chốt) | `staff.read_own` | `hr::day`, `repo::day_lines` + `repo::day_upsell`; `?staff_id=` 400, ngày sai 422 `invalid_date`; không SĐT/ghi chú lượt/id khách |
| `GET /me/appointments?from=&to=` (EM 2026-11-07, thay `GET /me/work`) | `MyCalendar {linked, data?: {from, to, today, opens_at, closes_at, slot_minutes, entries[]{id, kind appointment/visit, business_date, starts_at, ends_at, duration_min, phase, customer_name, services[]{service_name, category_name, from_package}, group_size?, walk_in}}}`: lịch của chính KTV — lịch hẹn gán cho mình (mọi trạng thái) + lượt mình làm không thuộc lịch hẹn của mình (chỉ dòng của mình) | `staff.read_own` | `operations::appointments::service::my_calendar`, 4 query (`my_appointments`, `services_of`, `my_visits`, `my_visit_lines`); ≤ 14 ngày; không SĐT/ghi chú/tiền/id |

Các route "của chính mình" của `hr` (`/me/staff`, `/me/staff/day`, `/me/evaluations/history`) nhận `staff.read_own`, hoặc `staff.read_own_past` (quyền suy ra cho người đã nghỉ, chỉ các tháng đã làm; [ADR-0014](../decisions/ADR-0014-former-staff-login.md)) qua `hr::me::own_scope`.

Tính lương: bộ tính thuần `hr::calc::compute` (unit test theo ô Excel E55/K54, Tết, cờ). Hoa hồng: 1 query gom `visit_line` (lượt `completed`, theo `business_date`) theo (KTV, dịch vụ, nguồn) + `sale.total` theo `closer_staff_id` (không hủy). Khóa tháng: `hr::period` (FOR SHARE khi ghi, FOR UPDATE khi chốt) + trigger. Mã lỗi mới: `payroll.period_closed` (409), `payroll.month_not_ended`, `payroll.unrecorded_days` (422), `staff.account_taken` (409).

### 3e. Vòng 6: upsell trong lượt, chuyển lượt lẻ sang gói ([spec §10](../product/specs/front-desk-and-packages.md), migration 0021)
| Method | Path | Permission | Ghi chú |
|---|---|---|---|
| POST | `/visits` (mở rộng) | `visit.record` (+ `package.sell` nếu có `package_sales`, `payment.collect`, `sale.discount*`) | `package_sales[]` 0–3 `{ref?, package_plan_id, closer_staff_id?, gift_note?, discount?, beneficiary_customer_id?}`; dòng `source: "package"` + `package_ref` dùng buổi hôm nay từ gói vừa bán; dòng lẻ có `referrer_staff_id?` (người giới thiệu); `companions[]` 0–5 ghi chú. `payments` = lẻ + mọi gói; server chia: TM trả hóa đơn lẻ trước, rồi gói theo thứ tự, còn lại CK (`sales::service::allocate_payments`, hàm thuần có unit test). Mỗi gói một hóa đơn, mọi hóa đơn có `sale.visit_id`. Cũng áp dụng cho `POST /appointments/{id}/complete` (cùng `record_in`) |
| POST | `/visits/{id}/additions` | `visit.record` (+ như trên); ngày trước: `record.void_closed_day` | Idempotency-Key. Thêm dòng + hóa đơn mới (lẻ và/hoặc gói) nối `visit_id`, không đụng dòng cũ, không lý do; `version` +1. Tháng đã chốt lương → `409 payroll.period_closed`. `201 VisitDto` |
| POST | `/visits/{id}/convert-to-package` | `visit.record` + `package.sell` + `record.void_same_day` (+ `payment.collect`); ngày trước: `record.void_closed_day` | If-Match + Idempotency-Key. `begin_correction` (hủy lượt cũ "Chuyển sang gói <tên>", đảo tiền lẻ hôm nay) → `finish_correction` với đơn hàng dựng từ lượt cũ: dòng lẻ gói bao phủ → dòng `package_ref` của gói bán kèm, dòng khác ghi lại y nguyên (KTV, người giới thiệu, phần giảm giá). Hóa đơn gói ghi vào **ngày chuyển** (Q-E1). `422 visit.nothing_to_convert`, `409 visit.corrected`, `409 payroll.period_closed`. `201 {voided, visit, difference}` (`difference` = khách đưa thêm) |
| POST | `/sales` (mở rộng) | `package.sell` | `closer_staff_id` nullable (= khách tự chọn), `beneficiary_customer_id?` (mua cho người khác: gói thuộc người đó). `SaleDto.visit_id` |
| PATCH | `/visits/{id}` (mở rộng) | `visit.record` | `companions?` (danh sách đầy đủ) |

- `VisitDto` thêm `package_sales[]` (`VisitPackageSaleDto`: tổng, người chốt, người dùng gói nếu khác, thanh toán), `companions[]`, `lines[].{sale_id, referrer_staff_id, referrer_name}`; `payments` = mọi hóa đơn lẻ của lượt (lần đầu + thêm). `AppointmentDto.companions`; `POST /appointments` và `/appointments/group` nhận `companions[]` (nhóm 1 khách + ghi chú hợp lệ).
- **Hủy lượt** đảo mọi hóa đơn lẻ còn mở của lượt (`repo::open_service_sales`); hóa đơn gói bán trong lượt giữ nguyên (gói là của khách, hủy riêng bằng `POST /sales/{id}/void`).
- **Thứ tự ghi trong một transaction** (`operations::service::checkout`): định giá mọi gói (`sales::service::price_package_sale`) + dòng lẻ (`price_services`) → chia thanh toán → ghi hóa đơn gói (`record_package_sale`, `visit_id` có FK `DEFERRABLE`) → khóa + trừ buổi theo thứ tự item (gồm item của gói mới) → hóa đơn lẻ → lượt (mới) + dòng → audit `visit.create` / `visit.addition`. Lỗi bất kỳ → rollback hết.
- **Lương:** `hr::repo::upsell_lines` = gói (Σ `customer_package.sold_price` của hóa đơn mở, theo `sale.closer_staff_id`) ∪ dịch vụ lẻ (Σ `line_value` của lượt hoàn tất, theo `visit_line.referrer_staff_id`); `calc::compute` → `closing_commission_base` = `upsell_package_base` + `upsell_service_base`, cùng tỷ lệ `commission_rate_bp`; `upsell_lines[]` trên phiếu (serde default cho phiếu đã chốt cũ). Hoa hồng làm không đổi (`visit_line.staff_id`).
- **Báo cáo:** `app.revenue_book` thêm `visit_id`, `upsell_by` → `LedgerRow.{visit_id, upsell_by}`; lịch sử khách: hóa đơn lẻ của lượt (kể cả thêm) thuộc lượt, `HistoryPurchase.{visit_id, beneficiary_name}`. Báo cáo ngày không đổi công thức (mỗi loại một hóa đơn).
- Test: `crates/app/tests/upsell.rs` (10), 2 dòng mới trong `authz_matrix.rs`, unit test `allocate_payments` (4) và `calc` upsell (2).

### 3f. Người đi cùng được làm ([spec §10.10](../product/specs/front-desk-and-packages.md), migration 0022, [ADR-0009](../decisions/ADR-0009-served-companions.md))
| Method | Path | Permission | Ghi chú |
|---|---|---|---|
| POST | `/appointments/group` (mở rộng) | `appointment.manage` | `people[1..]` là người đi cùng được làm: `guest.phone` tùy chọn (chỉ tên, ví dụ "Chồng của chị Hà"); `people[0]` vẫn bắt buộc SĐT (`422 people[0].guest.phone required`). Mỗi người `starts_at?` riêng (mặc định giờ của nhóm, cùng ngày). Người đi cùng không làm vẫn là `companions[]` |
| POST | `/appointments` (mở rộng) | `appointment.manage` | `group_with?`: lịch đang mở mà người này đi cùng → khóa lịch đó (`FOR UPDATE`, quy tắc ngày đã chốt), mở nhóm nếu chưa có (version +1), lịch mới cùng `group_id`; `guest.phone` tùy chọn. `404` (không có / spa khác), `409 appointment.closed`, `422 customer_id duplicate` (khách đã trong nhóm), `422 starts_at not_same_day`. Không `group_with` → SĐT bắt buộc như cũ |
| PATCH | `/appointments/{id}` (mở rộng) | `appointment.manage` | `companions?` = danh sách ghi chú đầy đủ (vắng = giữ nguyên, `[]` = xóa), cùng quy tắc `valid_companions`; audit ghi số mục |

- `GuestCreate.phone: string \| null`; `AppointmentDto.customer_phone: string \| null` (null = người đi cùng chỉ có tên). Ghi lượt cho họ vẫn `422 appointment.customer_required` cho tới khi `PATCH customer_id` (hồ sơ tạo ở quầy) — lượt, trừ buổi, hoa hồng "làm" (`visit_line.staff_id`) đi đúng đường cũ.
- Thứ tự khóa của `group_with`: dòng lịch chủ (`FOR UPDATE`) → khóa ngày (`pg_advisory_xact_lock` trong `check_fits`), giống `PATCH` → không đảo thứ tự, không deadlock.
- Test: `appointments.rs` `served_companions_get_their_own_technician_and_block`, `companions_are_added_and_edited_on_an_existing_booking`.

## 4. Use case & tính toàn vẹn dữ liệu (áp dụng [data-integrity](../../../.claude/skills/backend-design/data-integrity.md))
Mọi use case chạy trong **một transaction**, mở bởi `kernel::TenantTx` (`kernel/src/db.rs`). `TenantTx` mở bằng một simple query `BEGIN; SELECT set_config('app.tenant_id', …, true), set_config('statement_timeout', '5s', true), set_config('idle_in_transaction_session_timeout', '30s', true)` (đều transaction-local), giữ `actor` và `business_date`, và **không gọi mạng ra ngoài** trong lúc transaction mở.

| Write path | Bất biến | Chiến lược xung đột / trùng lặp |
|---|---|---|
| Trừ buổi (`POST /visits`) | `0 ≤ sessions_remaining ≤ total` | Update nguyên tử `… - qty WHERE sessions_remaining >= qty`; 0 dòng → `409`. Có `CHECK` constraint. Idempotency |
| Hoàn buổi khi hủy lượt (S8 ✅) | Không vượt `sessions_total`; không cộng vào gói đã hủy | Khóa visit `FOR UPDATE` → `status` phải `completed` → `UPDATE … WHERE status='completed'`; rồi với mỗi dòng gói (theo package, item): khóa gói `FOR UPDATE`, gói đã hủy → `409`, `+ qty WHERE sessions_remaining + qty <= sessions_total`. Trigger 0009: lượt `voided` không đổi được nữa; item của gói đã hủy không tăng buổi được. Hai lần hủy song song → đúng 1 thành công (test) |
| Sửa lượt (US6b, 0020) | Mỗi lượt sửa tối đa 1 lần; tiền/buổi đi đúng một lần; không chuyển ngày | Khóa visit `FOR UPDATE` (đọc `corrected_by` bằng statement riêng sau khóa để thấy lần sửa vừa commit) → hủy như `/void` → ghi như `POST /visits`. DB: `UNIQUE (tenant_id, corrects_visit_id)`, trigger `visit_correction_link` (lượt bị thay phải `voided`, cùng `business_date`; liên kết bất biến), `appointment_closed_is_final` chỉ cho lịch `done` đổi `visit_id` sang lượt sửa nó. Báo cáo: `DayCounts.voided_visits` không tính lượt đã sửa, thêm `corrected_visits`; `CustomerStats.voided_visit_count` cũng vậy; lịch không hiện lượt đã bị thay như lượt vãng lai. Đảo tiền ghi ngày sửa, lượt mới ghi ngày cũ (Q-E1) |
| Thu tiền (payments[] trong sale/visit) | R1: `sum(payments) = total`, `amount_paid = total` | Kiểm tra trong service trước khi insert; `CHECK (amount_paid BETWEEN 0 AND total)` vẫn giữ để bật công nợ sau này. Idempotency |
| Hủy hóa đơn (S8 ✅) | Payment không bị xóa; mỗi payment đảo tối đa 1 lần | Khóa sale → gói (theo id) → item (theo id). Gói phải chưa dùng buổi và chưa hủy. `UPDATE sale … WHERE status='open'`, `amount_paid` → 0, mỗi payment chưa đảo sinh một `reversal` cùng phương thức + số tiền, `business_date` = ngày hủy. DB: `UNIQUE (tenant_id, reversal_of)` (0005) + trigger 0009 (reversal phải trỏ tới một `payment` cùng sale, cùng method, cùng amount; reversal không đảo được) + trigger `voided` là cuối |
| Bán gói | Snapshot giá / tên / buổi | Transaction duy nhất tạo sale + lines + customer_package + items + payments |
| Nhập sổ cũ (S9) | `sessions_remaining ≤ purchased + bonus`; không sale, không payment | Transaction duy nhất: khách (tìm hoặc tạo qua `crm::service::create_in`) + các gói `origin=opening_balance` + audit. Idempotency |
| Hoàn tiền (S8 ✅) | Hoàn ≤ `sold_price` (R1 thu đủ nên đã thu = giá sau giảm; sổ cũ = giá ghi trong sổ); một gói hoàn tối đa 1 lần | Khóa `customer_package FOR UPDATE` → kiểm tra → khóa item → buổi về 0 + `cancelled_at/by/reason` → insert `refund`. DB: `UNIQUE (tenant_id, customer_package_id)`, trigger `refund ≤ sold_price`, append-only, gói đã hủy không đổi được nữa |
| Sửa khách / dịch vụ / gói / gia hạn | | Optimistic lock `version` + `If-Match` → `412`. Gia hạn còn có Idempotency-Key |
| Tạo khách | SĐT duy nhất trong tenant | Partial `UNIQUE` → `23505` → `409 customer.phone_taken` |
| Mọi mutation | Có dấu vết | `audit_log` insert trong cùng transaction |

- **Ngày đã chốt (S8):** `kernel::corrections::authorize_void(actor, record_day, today)`: cùng ngày cần `record.void_same_day`; ngày trước hôm nay (theo múi giờ của spa) cần thêm `record.void_closed_day` (Manager → `403 day.locked`). Không reauth cho thao tác nghiệp vụ (EM 2026-11-05; `Actor` không còn cờ `recent_auth`). Re-auth chỉ cho thao tác bảo mật tài khoản qua `identity::AuthUser::require_recent_auth`.
- **Thứ tự khóa (S8, sửa S10):** visit / sale → customer_package (theo id) → customer_package_item (theo id), **kể cả khi trừ buổi**: `POST /visits` gọi `packages::service::lock_for_consumption` (`FOR SHARE` các gói theo id) trước khi trừ item, rồi kiểm tra lại `sessions_remaining >= qty`. Trước S10 trừ buổi chỉ khóa item, rồi FK `visit_line → customer_package` lấy key-share trên gói, nên deadlock (500) với hủy hóa đơn / hủy lượt / hủy gói chạy song song. Vì hủy gói đưa buổi về 0 nên không thể trừ buổi của gói vừa bị hủy.
- Retry transaction **tối đa 3 lần** với lỗi `40001`/`40P01`.
- Bảng chỉ có `INSERT` cho `payment`, `refund`, `audit_log`.
- Không cần outbox ở R1-1 vì không có hệ thống ngoài. Sẽ thêm khi có Zalo/SMS (spec #3).

## 5. Layering (ADR-0006)
```
http (Axum route, DTO, extractor AuthUser/TenantCtx/IdempotencyKey/IfMatch)
  └─▶ service (use case, can(actor, perm), quy tắc nghiệp vụ, mở TenantTx)
        └─▶ repo (SQL qua sqlx::query!, nhận &mut TenantTx + TenantId)
domain: kiểu dữ liệu & bất biến thuần (Money, PhoneE164, SessionCount, PriceVariant, PackageStatus::derive)
```
- **Đồ thị phụ thuộc crate** (theo `Cargo.toml`, 2026-11-09): `operations → {catalog, crm, hr, org, packages, sales}` (chỉ qua `service` API, truyền chung `&mut TenantTx`) · `sales → {catalog, crm, org, packages}` · `reporting → {crm, org, packages}` (chỉ đọc; SQL thẳng trên các bảng sổ, như một read model) · `packages → {catalog, crm}` · `hr → {org}` · mọi crate còn lại → `identity` + `kernel` (`identity → kernel`) · `app` → tất cả. Không vòng. Crate `ledger` (sổ quỹ, spec #2) chưa có.
- **Cross-cutting ở `app`:** config (validate khi khởi động, fail-fast), middleware tower (tracing request-id, timeout 10 giây, giới hạn body 64 KB, rate limit, kiểm tra `Origin` cho request ghi, mặc định `Cache-Control: private, no-store`), error mapping `AppError → problem+json` tại một chỗ. Header bảo mật (HSTS, CSP, …) do `vercel.json` đặt trên Vercel, do Caddy (`infra/Caddyfile`) đặt trên đường VPS.
- **Chi phí mỗi request (perf giai đoạn 0, 2026-11-07):** `TenantTx::begin` gửi `BEGIN; SELECT set_config('app.tenant_id', …, true), set_config('statement_timeout','5s', true)` trong **một** simple query (`begin_with`; vẫn transaction-local, test `request_plumbing.rs`). Quyền của member = **một** câu (`array_agg` + `LEFT JOIN` role). Một request đăng nhập lúc đó: tra session (+ touch) → tx quyền (BEGIN+settings, quyền, COMMIT) → tx nghiệp vụ. Trước: 8–17 câu/request, giai đoạn 0 bớt 3 (1 ở quyền, 1 ở mỗi `TenantTx`). Tx quyền riêng đã bỏ ở A3 bên dưới.
- **Rà soát DB A1/A4 (2026-11-09, [db-review](db-review-2026-11.md)):**
  - **A1:** pool `app::db_pool::options` tắt `test_before_acquire`. Chỉ kết nối đã rảnh ≥ `PING_WHEN_IDLE_FOR` (30 s) mới bị ping trước khi giao ra. Ping hỏng thì sqlx đóng kết nối và đưa kết nối khác. `idle_timeout` 5 phút. Bớt một lượt đi về mỗi lần lấy kết nối, ~3 lượt mỗi request. sqlx 0.9 vẫn ping khi **trả** kết nối về pool, nhưng việc đó chạy ở task nền, không nằm trên đường đi của request.
  - **A4:** câu quyền (`identity::repo::member_access`) đọc luôn timezone, chi nhánh và trần giảm giá của spa (`kernel::tenant::TenantFacts`) rồi gắn vào `Actor`. `TenantTx::for_actor` mang chúng vào transaction, nên `kernel::tenant::profile` không còn query. Transaction mở bằng `TenantTx::begin` (tooling, test) vẫn đọc như cũ.
  - **A3 ([ADR-0017](../decisions/ADR-0017-one-statement-request-auth.md), 0034):** `app.session_actor(token_hash)` trả session + cờ MFA / mật khẩu tạm + quyền + người đã nghỉ + cài đặt spa trong **một** câu, không cần transaction.
    - `AuthUser` = `identity::service::authenticate`; `/me` = `resolve_with_access` + tài khoản + danh sách spa.
    - Một request đăng nhập giờ là: `session_actor` (+ touch mỗi phút) → tx nghiệp vụ.
    - `actor_for` / `tenant_permissions` (dưới RLS) vẫn dùng cho tooling, demo, đổi mật khẩu.
- **`Server-Timing`** (`app::server_timing`): mọi response trừ `/auth/*` có `db;dur=<ms>;desc="<n> statements", total;dur=<ms>`. `db` = tổng thời gian các câu sqlx của request (layer tracing riêng nghe event `sqlx::query` DEBUG, filter theo layer nên `RUST_LOG` không ảnh hưởng; task-local theo request). `/auth/*` không có: số câu lệnh phân biệt được SĐT lạ / bị khóa / đúng. Đọc bằng DevTools hoặc `infra/perf/endpoints.mjs`.
- **Tiện ích domain:** chuẩn hóa SĐT VN → E.164 (`phonenumber`); tạo `search_text` (NFC → lowercase → bỏ dấu → `đ→d`); mã hóa field AES-256-GCM (`aes-gcm`, key qua `secrecy`).

## 6. Background jobs (in-process, `tokio`)
| Job | Lịch | Retry |
|---|---|---|
| Dọn `idempotency_record` > 48 giờ (`app.purge_idempotency_records`, 0001) và session đã thu hồi / hết hạn quá **30 ngày** (`app.purge_sessions`, 0035, EM Q-DB4); `audit_log` giữ hết, không có hạn | Vòng lặp `app::jobs::spawn_maintenance` thức dậy lúc khởi động rồi mỗi giờ; job chỉ chạy khi `app.claim_maintenance('purge', '55 minutes')` trả `true` (0035), nên cả hệ thống chạy khoảng một lần mỗi giờ dù Vercel dựng bao nhiêu instance | Idempotent, chạy lại thoải mái (`app::jobs::run_maintenance`) |
| Kiểm bất biến dữ liệu: 11 phép kiểm bản sao / liên kết (`app.invariant_violations`, 0035 = `infra/perf/db/invariants.sql`) | Mỗi ngày (`claim_maintenance('invariants', '23 hours')`) | Phép kiểm có dòng lệch → log **ERROR** `data invariant violated` với `check` + `rows`; log của Vercel / VPS là chỗ báo |
| Backup `pg_dump` → mã hóa `age` → R2 (`infra/backup/backup.sh`) | Chỉ đường VPS: systemd `vpyd-backup.timer`, 02:30 giờ VN. Staging Vercel + Supabase không chạy job này (cần Supabase Pro hoặc job riêng, [vercel-staging](../runbooks/vercel-staging.md)) | Dead-man's switch `HEALTHCHECK_PING_URL` (healthchecks.io) báo khi không có lần chạy thành công |

Chưa có hàng đợi job: nhắc nhở (spec #3) sẽ dùng bảng job trên Postgres (`FOR UPDATE SKIP LOCKED`) có dead-letter.

## 6b. Lệnh vận hành (CLI `app`, `apps/api/crates/app/src/main.rs`)
Chạy trên máy vận hành, không có route HTTP nào tương ứng. Mật khẩu chỉ qua env, không bao giờ qua argv.

| Lệnh | Việc | Kết nối |
|---|---|---|
| `app` / `app serve` | Chạy API | `APP_DATABASE_URL` (app_rw) |
| `app migrate` | Chạy migration | `DATABASE_URL` (schema owner) |
| `app create-tenant --slug S --name N [--branch B]` | Tạo spa + chi nhánh + role hệ thống | owner |
| `app add-user --tenant S --phone P --name N --role owner\|manager\|technician` | Thêm người vào spa, mật khẩu từ `APP_NEW_USER_PASSWORD` | owner |
| `app add-staff --tenant S --name N --position P` | Thêm nhân viên cho picker KTV / người chốt | owner |
| `app seed-catalog --tenant S` | Nạp menu in (35 dịch vụ / 10 nhóm) | owner |
| `app import-paper-book --tenant S --file F.csv [--dry-run]` | Migrate sổ liệu trình giấy một lần (R1-2 §7, cột CSV ở spec, gồm `paid_amount`/`paid_on`/`method`: tiền sổ giấy ghi thành hóa đơn vào ngày trên sổ, 0018). Đi qua `sales::paper::import_opening_balance` với quyền của chủ spa; gói đã nhập mà chưa có tiền thì lần chạy sau ghi tiền một lần (`sales::paper::book`); gói đã có (cùng khách, tên, ngày bắt đầu, không hóa đơn) bị bỏ qua nên chạy lại an toàn; file lỗi ở bất kỳ dòng nào (cột, số, ngày, tên dịch vụ) → không ghi gì | owner + app_rw + `APP_DATA_KEY` |
| `app seed-demo --tenant S [--force-again]` | Dữ liệu demo cho staging/buổi dùng thử, xem [runbook](../runbooks/vercel-staging.md#dữ-liệu-demo) | owner + app_rw + `APP_DATA_KEY` + `APP_NEW_USER_PASSWORD` |
| `app reset-mfa --phone P` / `app unlock --phone P` | Mất authenticator / gỡ khóa đăng nhập (xác minh người trước) | owner |
| `app add-operator --username U --name N` | Tạo operator console admin nền tảng (ADR-0018), mật khẩu từ `APP_NEW_OPERATOR_PASSWORD`; MFA đăng ký ở lần đăng nhập đầu tại `/admin` | owner |
| `app reset-operator-mfa --username U` · `app disable-operator --username U` · `app enable-operator --username U` | Operator mất authenticator / tạm khóa / mở lại | owner |
| `app openapi` | In contract ra stdout | — |

`seed-demo` đi qua đúng các use case của quầy (`crm`, `sales`, `operations`, `packages`), nên validation, audit và quy tắc append-only vẫn giữ. Để ghi lịch sử 3 tuần, `sales::service::create_sale_as_of` và `operations::service::record_as_of` nhận đồng hồ làm tham số (`kernel::tenant::profile_as_of`). Handler HTTP chỉ gọi `create_sale` / `record`, hai hàm này truyền `Utc::now()`: client không có cách nào ghi lùi ngày.

## 7. Truy vết acceptance criteria → endpoint
| Story | AC chính | Endpoint |
|---|---|---|
| US1 | 1.1–1.9 | `/services`, `/service-categories`, `/package-plans` |
| US2 | 2.1–2.7 | `GET/POST /customers`, `GET/PUT /customers/{id}`, `PUT /customers/{id}/health-note` |
| US3 | 3.1–3.7 | `POST /sales` |
| US4 | 4.1–4.10, 4.2b/c | `POST/GET/PATCH /visits` |
| US5 | hoãn | Không có endpoint thu nợ / công nợ ở R1; không có endpoint sửa/xóa payment |
| US6 | 6.1–6.4 | `POST /visits/{id}/void`, `POST /sales/{id}/void` (S8 ✅) |
| US7 | 7.1–7.4 | `/customer-packages/{id}/extend`, `/cancel` (S8 ✅) |
| US8 | 8.1, 8.3–8.8 | `POST /opening-balances` (tạm, chỉ Owner; không sale, không payment) + `app import-paper-book` (tạo `customer_package` với `origin=opening_balance`; từ 0018 tiền đã trả trên sổ ghi thành hóa đơn `source = 'paper_book'` vào ngày trên sổ); DTO không lộ `origin` |
| R1-2 US1–US5 | [management-views](../product/specs/management-views.md) | §3b |
| G1–G6 | | Middleware + extractor + `kernel` |

## 8. Áp dụng `secure` (tóm tắt)
- AuthN, session, cookie, CSRF: ADR-0004.
- AuthZ: `can()` ở service layer, ma trận [authz-matrix](../security/authz-matrix.md).
- Tenant isolation: ADR-0003.
- Không nhận giá hay identity từ client (§3).
- Field nhạy cảm: mã hóa và bị loại khỏi response nếu thiếu quyền.
- Audit, append-only, rate limit, giới hạn body.
- Lỗi không lộ thông tin nội bộ.
- Mọi mục trong [threat-model](../security/threat-model.md) TB1–TB2 đều có ít nhất một biện pháp ở đây.
