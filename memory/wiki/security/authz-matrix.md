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
| `opening_balance.import` | ✓ | ✓ | — | — |
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

- 🔒 = yêu cầu **xác thực lại** (nhập mật khẩu trong 5 phút gần nhất, `session.recent_auth_at`).
- **grant** = chỉ khi có `support_grant` còn hiệu lực do chủ spa cấp; mọi truy cập đều audit. Platform admin **không bao giờ** đọc được ghi chú sức khỏe.
- Technician ở R1-1 không có quyền thao tác quầy. Quyền "xem dữ liệu của chính mình" thuộc R2.

## Ánh xạ endpoint → permission
Danh sách đầy đủ ở [backend](../architecture/backend.md) §3 (cột "Permission"). CI fail nếu có route chưa khai báo permission (trừ `/auth/*` công khai và `/health`).

## Test bắt buộc
1. Mỗi endpoint × mỗi role: role không có permission → `403`.
2. Mỗi endpoint đọc hoặc ghi theo ID: dùng ID của **tenant khác** → `404`.
3. `GET /customers/{id}` khi thiếu `customer.health_notes.read` → response **không có** key `health_note`.
4. Platform admin không có grant → `403` trên mọi endpoint dữ liệu tenant; grant hết hạn → `403`.
5. Thao tác 🔒 khi `recent_auth_at` quá 5 phút → `401` kèm code `auth.reauth_required`.
6. Manager giảm giá vượt ngưỡng → `403` code `discount.limit_exceeded`.
