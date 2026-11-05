---
title: Authorization matrix (R1-1)
updated: 2026-11-04
sources: [product/specs/front-desk-and-packages.md, decisions/ADR-0003-multi-tenancy.md, decisions/ADR-0004-authentication.md]
---

# Authorization matrix R1-1

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
| `package.cancel_refund` 🔒 | ✓ | — | — | — |
| `package.use_expired_override` | ✓ | ✓ | — | — |
| `opening_balance.import` (chỉ trong ngày chủ spa đã mở; ngày chưa mở: Owner + 🔒, Manager → `403 opening_balance.closed`) | ✓ | ✓ | — | — |
| `opening_balance.manage_window` (mở/đóng nhập sổ cũ cho hôm nay) | ✓ | — | — | — |
| `visit.record` / `visit.read` | ✓ | ✓ | — | grant (read) |
| `payment.collect` | ✓ | ✓ | — | — |
| `sale.read` | ✓ | ✓ | — | grant (read) |
| `sale.discount` (≤ `manager_discount_max_pct`) | ✓ | ✓ | — | — |
| `sale.discount_unlimited` | ✓ | — | — | — |
| `record.void_same_day` | ✓ | ✓ | — | — |
| `record.void_closed_day` 🔒 | ✓ | — | — | — |
| `audit.read` | ✓ | — | — | — |
| `support_grant.manage` | ✓ | — | — | — |
| `platform.tenant.manage` | — | — | — | ✓ |

- 🔒 = yêu cầu **xác thực lại** (nhập mật khẩu trong 5 phút gần nhất, `session.recent_auth_at`; tài khoản có MFA nhập thêm mã TOTP/khôi phục). Hiện thực (S8): extractor `AuthUser` đặt `Actor::with_recent_auth(session.recently_authenticated(now))`, use case gọi `actor.require_recent_auth()` → `401 auth.reauth_required`; web hiện hộp thoại nhập lại mật khẩu (+ mã khi server trả `422 code required`) rồi gửi lại với cùng Idempotency-Key.
- **Ngày đã chốt (S8):** hủy bản ghi của ngày trước hôm nay cần `record.void_closed_day` (🔒); chỉ có `record.void_same_day` → `403 day.locked`. Hủy trong ngày không cần reauth. Route: `POST /visits/{id}/void`, `POST /sales/{id}/void` (`record.void_same_day`), `POST /customer-packages/{id}/extend` (`package.extend_expiry`), `POST /customer-packages/{id}/cancel` (`package.cancel_refund` 🔒) đều có dòng trong `authz_matrix.rs`.
- **MFA (S2b):** role có `requires_mfa` (Owner mặc định, không tắt được) hoặc giữ `platform.tenant.manage` (PlatformAdmin) phải qua TOTP. Session **pending MFA** không có quyền nào: mọi route dùng `CurrentSession`/`AuthUser` trả `401 auth.mfa_required | auth.mfa_enrollment_required`; chỉ `/auth/mfa/verify`, `/auth/mfa/enroll/start|confirm`, `/auth/logout` nhận nó.
- **Thu tiền (security review r1, 2026-11-04):** `POST /sales` có `payments` khác rỗng cần **cả** `package.sell` **và** `payment.collect`, giống `POST /visits` (`visit.record` + `payment.collect` khi thu tiền). Seed mặc định Owner/Manager có cả hai nên không đổi hành vi; role tùy biến chỉ có `package.sell` thì bán được gói 0đ nhưng thu tiền → `403 auth.forbidden` (`app/tests/sales.rs::taking_money_needs_payment_collect`). Dòng `ROUTES` của `POST /sales` giữ `PackageSell` (cổng matrix kiểm permission chính; seed có đủ cả hai).
- **Nhập sổ cũ theo ngày (M2, quyết định EM 2026-11-05, spec AC8.6–8.8):** `GET /opening-balances/window` (`opening_balance.import`) cho biết hôm nay có mở không; `PUT /opening-balances/window {open}` (`opening_balance.manage_window`, Owner) mở cho ngày kinh doanh hôm nay hoặc đóng, có audit; tự hết hiệu lực lúc 0 giờ (Asia/Ho_Chi_Minh). `POST /opening-balances`: ngày đã mở → `opening_balance.import`; ngày chưa mở → chỉ người có `opening_balance.manage_window` và phải 🔒, người khác `403 opening_balance.closed`. Trần hoàn tiền của gói sổ cũ = `sold_price` chỉ khi người nhập có `package.cancel_refund`, nếu không = 0 cho tới khi Owner gửi `confirm_book_price: true` lúc hủy (thiếu → `422 refund.book_price_unconfirmed`). Matrix mở cửa sổ trong fixture để dòng Manager của `POST /opening-balances` kiểm đúng permission. Test: `app/tests/opening_balances.rs` (3 test M2) + `packages::opening_balance::tests::the_window_closes_at_midnight_in_the_spa_time_zone`.
- **Spa bị tạm ngưng (M3):** `tenant.status = 'suspended'` → không cấp permission nào, kể cả cho session đang mở: route nghiệp vụ trả `403 auth.not_a_member` (cùng mã với membership bị tắt), `/me` trả `active_tenant: null`, `permissions: []`. Yêu cầu MFA **không** mất theo spa bị ngưng (migration 0011). Test: `app/tests/auth.rs::suspending_a_spa_cuts_off_its_open_sessions`.
- **switch-tenant không phải re-auth (H1):** session mới sau `POST /auth/switch-tenant` giữ nguyên `recent_auth_at` và `absolute_expires_at` của session cũ (`app/tests/voids.rs::switching_tenant_does_not_refresh_the_reauth_window`, `auth.rs::switch_tenant_keeps_absolute_expiry_and_reauth_time`).
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
5. Thao tác 🔒 khi `recent_auth_at` quá 5 phút → `401` kèm code `auth.reauth_required` (`app/tests/voids.rs`: hủy gói + hoàn tiền, Owner hủy lượt ngày cũ; sau `POST /auth/reauth` → được). Đổi spa (`switch-tenant`) không làm mới cửa sổ này.
6. Manager giảm giá vượt ngưỡng → `403` code `discount.limit_exceeded`.
7. Session pending MFA gọi `/me` hoặc route nghiệp vụ → `401`; owner chưa enroll → `next = mfa_enroll`; manager → `next = done` (`app/tests/mfa.rs`).
