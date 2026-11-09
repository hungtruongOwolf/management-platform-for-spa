---
title: ADR-0017 One-statement request authentication (app.session_actor)
status: accepted
date: 2026-11-09
deciders: EM, agent
---

# ADR-0017: Xác thực mỗi request bằng một câu (`app.session_actor`)

## Context
- Mỗi request đã đăng nhập từng tốn **hai bước tách rời** trước câu nghiệp vụ đầu tiên:
  1. Tra session + tài khoản (một câu trên pool).
  2. Một transaction riêng để đọc quyền: `BEGIN; set_config`, `SELECT` quyền, `COMMIT`. Transaction là bắt buộc, vì RLS chỉ cho đọc `membership` / `role_permission` / `staff` khi đã đặt `app.tenant_id`, mà tenant chỉ biết sau khi đọc xong session.
- Tức là 4 lượt đi về API ↔ DB, mỗi lượt ~2,6 ms trên staging ([db-review §3](../architecture/db-review-2026-11.md)).
- EM duyệt gộp lại ([raw](../../raw/2026-11-09-em-decision-db-plan.md), Q-DB2), với điều kiện qua skill `secure` và có ADR.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **A. Hàm `SECURITY DEFINER` `app.session_actor(token_hash)`** trả session + cờ tài khoản + quyền + cờ người đã nghỉ + cài đặt spa | 1 câu, không cần transaction. Cùng mẫu với `app.tenants_of_user` / `app.user_requires_mfa` đã có. Logic quyền vẫn ở Rust | Hàm đọc vượt RLS nên phải tự lọc đúng spa / đúng người; phải pin `search_path` và giới hạn grant |
| B. Dùng chung một `TenantTx` cho xác thực + nghiệp vụ (extractor mở transaction, handler dùng tiếp) | Không đọc vượt RLS | Đổi chữ ký mọi handler; transaction mở trong lúc parse body. Chỉ bớt `COMMIT`/`BEGIN` (−2), vẫn còn tra session riêng |
| C. Giữ nguyên | Không rủi ro mới | Mất 3 lượt mỗi request; về lâu dài API và DB sẽ ở hai nơi xa nhau |
| D. Cache quyền trong bộ nhớ process | 0 lượt | Thu hồi quyền / khóa tài khoản / người nghỉ việc không có hiệu lực ngay (ADR-0014). Vercel nhiều instance thì cache không đồng bộ |

## Decision
Chọn **A**: migration 0034 tạo `app.session_actor(p_token_hash bytea)` (`LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp`), `REVOKE ALL … FROM PUBLIC`, `GRANT EXECUTE … TO app_rw`.

**Hàm chỉ trả về những gì hai câu cũ trả, không hơn:**
- Session còn sống: chưa thu hồi, chưa quá hạn rảnh lẫn hạn tuyệt đối, tài khoản `active`.
- Membership `active` của **đúng** `session.active_tenant_id` và **đúng** `session.user_id`, trong spa `active`.
- Quyền gộp từ role của membership đó.
- `former_staff` từ `staff.status = 'left'` của hồ sơ gắn với chính user đó trong chính spa đó.
- Timezone / chi nhánh / trần giảm giá của spa đó.

**Quyết định vẫn ở Rust** (`identity::service`), cùng thứ tự như trước:
- Session đang chờ MFA → `401 auth.mfa_required` / `mfa_enrollment_required`.
- Mật khẩu tạm → `403 auth.password_change_required` (ADR-0016).
- Không có spa đang chọn → `403 auth.no_active_tenant`.
- Không phải thành viên → `403 auth.not_a_member`.
- Người đã nghỉ → chỉ `staff.read_own_past` (`effective_permissions`, ADR-0014).

**Nơi dùng:**
- `AuthUser`: `service::authenticate`.
- `/me`: `resolve_with_access`.
- `CurrentSession` / `resolve` và `resolve_pending`: đọc qua cùng hàm, bỏ phần quyền.
- `actor_for` / `tenant_permissions` giữ lại cho tooling, demo và luồng đổi mật khẩu. Chúng vẫn đọc quyền dưới RLS.

## Consequences
**Tích cực:**
- GET đơn giản bớt 3 lượt đi về (~8 ms trên staging).
- `/me` còn 3 câu.
- Quyền vẫn suy ra **mỗi request** từ DB: thu hồi, khóa, người nghỉ có hiệu lực ngay ở request kế tiếp.

**Đánh đổi chấp nhận** (`secure`, xem [threat-model](../security/threat-model.md)): hàm đọc vượt RLS. Nếu kẻ tấn công điều khiển được `app_rw`, họ vốn đã đọc được mọi dòng `session` (có `token_hash`), vì bảng `session` không có RLS. Với hàm này, họ đọc thêm được **tên quyền** và cài đặt spa của từng session. Đây là dữ liệu ít nhạy cảm (không có khách, tiền hay sức khỏe), cùng loại với `tenants_of_user`. Không có định danh nào đến từ client, vì đầu vào là SHA-256 của bí mật trong cookie.

**Ràng buộc từ giờ:**
- Thêm điều kiện cho "session sống" hay "thành viên" thì sửa **cả** hàm này (migration mới, `CREATE OR REPLACE`) **và** test `app/tests/session_actor.rs`.
- `member_access` (đường cũ dưới RLS) và hàm phải trả cùng quyền cho cùng người. Test so hai đường trên role quản lý; các luồng `auth.rs` / `staff_logins.rs` / `authz_matrix.rs` chạy qua hàm.

**Hoàn tác:**
- Đổi `AuthUser` / `/me` về `resolve` + `actor_for` (chỉ đổi code). Hàm để lại vô hại, hoặc `DROP FUNCTION app.session_actor(bytea)`.

## Hiện trạng (kiểm lại 2026-11-09)
Migration 0039 (cờ tính năng, [ADR-0018](ADR-0018-platform-admin-console.md) D4) `CREATE OR REPLACE` hàm này để trả thêm `disabled_features` của spa; quyền bị rút theo cờ vẫn quyết ở Rust. Đúng ràng buộc "sửa cả hàm và test" ở trên.

## Revisit when
- Có nhiều spa và nhiều chi nhánh mỗi người.
- Quyền theo chi nhánh (spec #4) cần thêm cột.
- Muốn bỏ hẳn quyền đọc trực tiếp bảng `session` của `app_rw`: lúc đó mọi đường đọc session đi qua hàm definer.
