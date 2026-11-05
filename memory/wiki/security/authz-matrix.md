---
title: Authorization matrix (R1-1)
updated: 2026-11-05
sources: [product/specs/front-desk-and-packages.md, product/specs/management-views.md, decisions/ADR-0003-multi-tenancy.md, decisions/ADR-0004-authentication.md]
---

# Authorization matrix R1-1 + R1-2

Permission là hằng số trong code (`kernel::perm`). Role là dữ liệu, chủ spa chỉnh được ở spec #4. Bảng dưới là **seed mặc định**.

**Phạm vi mọi permission:** chỉ trong tenant của session (ADR-0003). Branch-scope áp dụng khi có nhiều chi nhánh: quản lý chỉ thấy chi nhánh được gán.

| Permission | Owner | Manager | Technician | PlatformAdmin |
|---|---|---|---|---|
| `catalog.read` | ✓ | ✓ | ✓ | grant |
| `catalog.manage` | ✓ | — | — | — |
| `staff.read_basic` (picker: tên, trạng thái) | ✓ | ✓ | ✓ | grant |
| `customer.read` / `customer.write` | ✓ | ✓ | — | grant (read) |
| `customer.health_notes.read` / `.write` | ✓ | ✓ | — | — |
| `customer.anonymize` 🔒 | ✓ | — | — | — |
| `package.sell` | ✓ | ✓ | — | — |
| `package.extend_expiry` | ✓ | — | — | — |
| `package.cancel_refund` | ✓ | — | — | — |
| `package.use_expired_override` | ✓ | ✓ | — | — |
| `opening_balance.import` (luôn ghi vào ngày kinh doanh hôm nay; **chỉ còn route tạm**, team dùng CLI `app import-paper-book`) | ✓ | — (bỏ ở 0013) | — | — |
| `visit.record` / `visit.read` | ✓ | ✓ | — | grant (read) |
| `payment.collect` | ✓ | ✓ | — | — |
| `sale.read` | ✓ | ✓ | — | grant (read) |
| `sale.discount` (≤ `manager_discount_max_pct`) | ✓ | ✓ | — | — |
| `sale.discount_unlimited` | ✓ | — | — | — |
| `record.void_same_day` | ✓ | ✓ | — | — |
| `record.void_closed_day` | ✓ | — | — | — |
| `report.read` (R1-2: dashboard ngày, báo cáo khoảng ngày, chỉ số đếm) | ✓ | ✓ | — | — |
| `report.revenue.read` (R1-2: mọi con số tiền trong báo cáo + sổ doanh thu; mặc định bật cho quản lý, **câu hỏi Q1 cho EM**) | ✓ | ✓ | — | — |
| `audit.read` | ✓ | — | — | — |
| `support_grant.manage` | ✓ | — | — | — |
| `platform.tenant.manage` | — | — | — | ✓ |

- 🔒 = yêu cầu **xác thực lại** (nhập mật khẩu trong 5 phút gần nhất, `session.recent_auth_at`; tài khoản có MFA nhập thêm mã TOTP/khôi phục), qua `POST /auth/reauth`. **Chỉ dành cho thao tác bảo mật tài khoản** (đổi mật khẩu, cài đặt MFA: chưa có route, sẽ dùng guard `identity::AuthUser::require_recent_auth` → `401 auth.reauth_required`). `customer.anonymize` 🔒 chưa có route. **Thao tác nghiệp vụ không bao giờ đòi nhập lại mật khẩu** (quyết định EM 2026-11-05, [raw](../../raw/2026-11-05-em-simplify-owner-and-opening-balance.md); [ADR-0004](../decisions/ADR-0004-authentication.md) update 2026-11-05): hủy/sửa ngày đã chốt, hủy gói + hoàn tiền, nhập sổ cũ. `Actor` không còn mang cờ `recent_auth`; web bỏ hộp thoại nhập lại mật khẩu.
- **Ngày đã chốt (S8, đơn giản hóa 2026-11-05):** hủy bản ghi của ngày trước hôm nay cần `record.void_closed_day` (chỉ Owner); chỉ có `record.void_same_day` → `403 day.locked`. Ngày khóa tự động lúc 0 giờ (Asia/Ho_Chi_Minh), không ai phải "mở" gì. **Sửa** lượt ngày đã chốt (`PATCH /visits/{id}`: đổi KTV, ghi chú) theo đúng quy tắc này (`kernel::corrections::authorize_closed_day`, dùng chung với `authorize_void`): Manager → `403 day.locked`, Owner → được, không nhập lại mật khẩu. Route: `POST /visits/{id}/void`, `POST /sales/{id}/void` (`record.void_same_day`), `POST /customer-packages/{id}/extend` (`package.extend_expiry`), `POST /customer-packages/{id}/cancel` (`package.cancel_refund`) đều có dòng trong `authz_matrix.rs`.
- **MFA (S2b):** role có `requires_mfa` (Owner mặc định, không tắt được) hoặc giữ `platform.tenant.manage` (PlatformAdmin) phải qua TOTP. Session **pending MFA** không có quyền nào: mọi route dùng `CurrentSession`/`AuthUser` trả `401 auth.mfa_required | auth.mfa_enrollment_required`; chỉ `/auth/mfa/verify`, `/auth/mfa/enroll/start|confirm`, `/auth/logout` nhận nó.
- **Thu tiền (security review r1, 2026-11-04):** `POST /sales` có `payments` khác rỗng cần **cả** `package.sell` **và** `payment.collect`, giống `POST /visits` (`visit.record` + `payment.collect` khi thu tiền). Seed mặc định Owner/Manager có cả hai nên không đổi hành vi; role tùy biến chỉ có `package.sell` thì bán được gói 0đ nhưng thu tiền → `403 auth.forbidden` (`app/tests/sales.rs::taking_money_needs_payment_collect`). Dòng `ROUTES` của `POST /sales` giữ `PackageSell` (cổng matrix kiểm permission chính; seed có đủ cả hai).
- **Nhập sổ cũ (spec AC8.6–8.8, quyết định EM 2026-11-05, [raw](../../raw/2026-11-05-em-simplify-owner-and-opening-balance.md)):** không còn "cửa sổ nhập" của PR #23 (đã bỏ `/opening-balances/window`, `opening_balance.manage_window`, `403 opening_balance.closed`; migration 0012). Ai có `opening_balance.import` đều nhập được; gói nhập luôn thuộc ngày kinh doanh hôm nay, sửa/hủy bản ghi của ngày cũ theo quy tắc ngày đã chốt. Hoàn tiền gói sổ cũ ≤ `sold_price` như gói bán (trần riêng `refund_cap` + `confirm_book_price` đã bỏ, **để sau**). Test: `app/tests/opening_balances.rs::anyone_who_may_import_imports_today_without_a_window_or_reauth`, `::an_opening_balance_refund_is_capped_by_its_book_price`.
- **Xem lại & theo dõi (R1-2, [spec](../product/specs/management-views.md), migration 0013):** mọi route chỉ đọc, `Cache-Control: no-store`, tenant lọc tường minh + RLS (2 view mới `security_invoker`).
  - `GET /reports/daily`, `/reports/range` cần `report.read`; các trường tiền (`revenue`, `net_revenue`, `one_off_revenue`, `top_packages[].revenue`) **chỉ có trong response** khi có `report.revenue.read` (không gửi rồi giấu ở UI). `GET /reports/ledger` cần `report.revenue.read`.
  - `GET /customers/{id}/history`, `/customers/{id}/summary` cần `customer.read` + `visit.read` + `sale.read`; `GET /customer-packages/{id}/usage`, `/customers/inactive` cần `customer.read` + `visit.read`; `GET /packages` cần `customer.read`. Matrix kiểm permission đầu tiên (các permission kèm theo có cùng người giữ mặc định).
  - Không bao giờ có ghi chú sức khỏe / ghi chú buổi trong các response này.
  - Test: `app/tests/reports.rs::money_is_only_sent_to_holders_of_report_revenue_read` (tắt quyền tiền của quản lý → không còn key tiền nào; KTV 403; ẩn danh 401; chủ spa khác chỉ thấy sổ trống của mình, ID khách → 404). Đã mutation-check: bỏ cổng quyền tiền → test đỏ.
- **Nhập sổ giấy (EM 2026-11-05, R1-2 §7):** quản lý mất `opening_balance.import` (migration 0013 xóa khỏi role hệ thống `manager`; `SystemRole::default_permissions` cũng vậy). `POST /opening-balances` còn tạm cho chủ spa tới khi web xóa màn `/nhap-so-cu`; team migrate bằng `app import-paper-book` (CLI, đi qua đúng use case với quyền của chủ spa). Test: `opening_balances.rs::only_the_owner_imports_today_without_a_window_or_reauth`, `paper_book.rs`.
- **Spa bị tạm ngưng (M3):** `tenant.status = 'suspended'` → không cấp permission nào, kể cả cho session đang mở: route nghiệp vụ trả `403 auth.not_a_member` (cùng mã với membership bị tắt), `/me` trả `active_tenant: null`, `permissions: []`. Yêu cầu MFA **không** mất theo spa bị ngưng (migration 0011). Test: `app/tests/auth.rs::suspending_a_spa_cuts_off_its_open_sessions`.
- **switch-tenant không phải re-auth (H1):** session mới sau `POST /auth/switch-tenant` giữ nguyên `recent_auth_at` và `absolute_expires_at` của session cũ (`app/tests/auth.rs::a_rotated_session_needs_a_real_reauth` kiểm trên session đã resolve, đúng cái guard 🔒 đọc; `auth.rs::switch_tenant_keeps_absolute_expiry_and_reauth_time`). Vẫn quan trọng: hạn tuyệt đối không được kéo dài, và guard 🔒 của thao tác bảo mật tài khoản dựa vào `recent_auth_at`.
- **PlatformAdmin / support grant: ngoài phạm vi R1-1** (kiểm ở S10). Code chưa có role này (`identity::SystemRole` = Owner, Manager, Technician), chưa có bảng `support_grant` hay route nào cấp/thu hồi grant; permission `platform.tenant.manage` và `support_grant.manage` chỉ là hằng số trong `kernel::perm`. Cột PlatformAdmin và test #4 bên dưới chờ ticket thêm role (thêm vào `SystemRole::ALL` thì matrix tự kiểm). Tương tự `customer.anonymize` và `audit.read` chưa có route.
- **grant** = chỉ khi có `support_grant` còn hiệu lực do chủ spa cấp; mọi truy cập đều audit. Platform admin **không bao giờ** đọc được ghi chú sức khỏe.
- Technician ở R1-1 không có quyền thao tác quầy. Quyền "xem dữ liệu của chính mình" thuộc R2.

## Ánh xạ endpoint → permission
Danh sách đầy đủ ở [backend](../architecture/backend.md) §3 (cột "Permission"). CI fail nếu có route chưa khai báo permission (trừ `/auth/*` công khai và `/health`).

## Test bắt buộc
Test 1–2 và cổng CI "route chưa khai báo permission" nằm ở `apps/api/crates/app/tests/authz_matrix.rs`: bảng `ROUTES` phải khớp bảng trên. Cách thêm dòng xem [testing](../conventions/testing.md).
1. Mỗi endpoint × mỗi role: role không có permission → `403`.
2. Mỗi endpoint đọc hoặc ghi theo ID: dùng ID của **tenant khác** → `404`.
3. `GET /customers/{id}` khi thiếu `customer.health_notes.read` → response **không có** key `health_note`.
4. Platform admin không có grant → `403` trên mọi endpoint dữ liệu tenant; grant hết hạn → `403`.
5. Thao tác nghiệp vụ của Owner **không** đòi re-auth: với `recent_auth_at` quá 5 phút, Owner hủy lượt/hóa đơn ngày cũ, sửa lượt ngày cũ, hủy gói + hoàn tiền, nhập sổ cũ vẫn thành công (`app/tests/voids.rs::closed_days_are_locked_for_managers_and_open_to_owners_without_reauth`, `::correcting_a_closed_day_visit_follows_the_void_rule`, `::owner_cancels_a_package_with_a_refund`, `opening_balances.rs`); Manager vẫn `403 day.locked`. E2E `voids.spec.ts` làm cũ `recent_auth_at` qua `e2e/support/db.ts` rồi kiểm không có hộp thoại mật khẩu. Thao tác 🔒 bảo mật tài khoản (khi có route) phải có test `401 auth.reauth_required`; đổi spa (`switch-tenant`) không làm mới cửa sổ này (`auth.rs::a_rotated_session_needs_a_real_reauth`).
6. Manager giảm giá vượt ngưỡng → `403` code `discount.limit_exceeded`.
7. Session pending MFA gọi `/me` hoặc route nghiệp vụ → `401`; owner chưa enroll → `next = mfa_enroll`; manager → `next = done` (`app/tests/mfa.rs`).
8. G5: mọi route ghi có permission, khi thành công, để lại ít nhất 1 dòng `audit_log` có `actor_user_id` (`authz_matrix.rs::every_successful_tenant_write_is_audited`, S10).
