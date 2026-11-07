---
title: ADR-0014 A leaver keeps a login to their own past pay (derived "former staff" permission)
status: accepted (EM decision Q-S6 2026-11-07; design = lead's decision, EM to review)
date: 2026-11-07
deciders: EM (what), agent (how)
---

# ADR-0014: Người đã nghỉ vẫn đăng nhập, chỉ xem công và lương của chính mình (quyền "former staff" suy ra lúc request)

## Context
EM 2026-11-07 ([raw](../../raw/2026-11-07-em-decisions-hr-notes-group-region.md) câu 2, Q-S6): "người đã nghỉ có được đăng nhập để xem phiếu lương cũ không? … có".

Trước bản này, `POST /staff/{id}/end` đặt `membership.status = 'disabled'` (ADR-0013): request kế tiếp của tài khoản đó không còn quyền gì. Yêu cầu mới:
- người đã nghỉ vẫn đăng nhập được;
- chỉ thấy **ngày công và phiếu lương của chính mình** ("Công và lương của tôi", tháng + chi tiết ngày), chỉ các tháng có làm;
- không khách hàng, Quầy, lịch hẹn, danh mục, "Khách của tôi"; không tháng sau khi nghỉ;
- nhận lại làm thì quyền cũ trở lại; chủ spa vẫn khóa hẳn được.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Đổi role của membership sang một role "Cựu nhân viên" khi nghỉ, đổi lại khi nhận lại | Dùng đúng mô hình role-as-data | Phải nhớ role cũ để trả lại; hai nơi lưu trạng thái (role + đợt làm việc) dễ lệch; sửa role bằng tay là lộ quyền |
| B. Cờ trên client / trong session (`is_former`) | Đơn giản | Trái non-negotiable 3; session cũ giữ quyền cũ |
| C. **Quyền suy ra lúc request**: `identity::repo::member_permissions` đọc luôn `staff.status` của hồ sơ gắn với tài khoản; `left` ⇒ tập quyền cố định `FORMER_STAFF = {staff.read_own_past}` thay cho quyền của role | Một nguồn sự thật (đợt làm việc, ADR-0013); deny by default (mọi endpoint khác không thấy permission nào của role); session đang mở đổi quyền ngay request kế tiếp; nhận lại làm tự trả quyền | Truy vấn quyền thêm một `EXISTS` (index `staff_user_idx`, cùng một round trip) |
| D. Giữ `disabled`, thêm endpoint công khai xem phiếu lương bằng link | Không đụng auth | Link là bí mật mang theo được; lương là dữ liệu nhạy cảm |

## Decision
Chọn **C**.
- `kernel::permission::effective_permissions(roles, former_staff)`: nếu hồ sơ gắn tài khoản đã nghỉ (không còn đợt mở, `staff.status = 'left'`) **và** role không có `staff.manage` ⇒ chỉ `[staff.read_own_past]`; ngược lại = quyền của role (một role không bao giờ cấp được `staff.read_own_past`). Tài khoản chủ spa không bao giờ bị hạ quyền (roster đã từ chối cho chủ spa nghỉ; gắn chủ spa vào một hồ sơ đã nghỉ cũng không khóa spa).
- `staff.read_own_past` không có trong role nào (không migration quyền). Chỉ hai endpoint nhận nó: `GET /me/staff` và `GET /me/staff/day`. Với người đã nghỉ: tháng mặc định = tháng nghỉ việc, ngày mặc định = ngày làm cuối; tháng không có ngày làm (sau khi nghỉ, hoặc trước ngày vào) ⇒ `403 staff.not_employed_in_month`; `MyStaff.left_on` cho web biết giới hạn. Chi tiết ngày của người đã nghỉ **không** kèm ghi chú buổi (họ không còn phục vụ khách đó).
- `POST /staff/{id}/end` không còn khóa tài khoản; body có `lock_login` (mặc định `false`, checkbox "Khóa tài khoản") để chủ spa khóa hẳn (`membership.status = 'disabled'` như trước). Audit ghi `login_locked` / `login_former_staff`.
- `POST /staff/{id}/rehire` giữ nguyên: đợt mới ⇒ `status = 'active'` ⇒ quyền của role trở lại ở request kế tiếp; mở khóa tài khoản nếu đã khóa.
- Migration **0026**: mở lại các membership `disabled` của người đã nghỉ (trước đây chỉ `end` ghi `disabled`, nên đó là khóa theo quy tắc cũ chứ không phải lựa chọn).
- Web: `mayOpen.myPay` (`staff.read_own` hoặc `staff.read_own_past`), `mayOpen.formerStaff`; nav tự rút còn Tổng quan, "Công và lương của tôi", Cài đặt; chọn tháng nằm cạnh dữ liệu (không qua tháng nghỉ việc).

## Consequences
- Positive: không có trạng thái thứ hai phải đồng bộ; test authz matrix có một lớp "former staff" chạy qua mọi route (`a_former_staff_member_reaches_only_their_past_pay`).
- Negative / accepted: tài khoản đã nghỉ vẫn đăng nhập được (bề mặt tấn công nhỏ: hai endpoint chỉ đọc dữ liệu của chính họ). Muốn chặn hẳn thì chủ spa chọn "Khóa tài khoản" lúc cho nghỉ. Chưa có nút khóa / mở khóa riêng sau khi đã cho nghỉ (nhận lại làm thì mở).
- Follow-ups: nếu sau này có role tùy biến, `staff.manage` vẫn là ranh giới "không bao giờ hạ quyền". Nút "Khóa / mở khóa tài khoản" độc lập (Q-S7).

## Revisit when
Có nhiều chi nhánh (người nghỉ ở một chi nhánh nhưng còn làm ở chi nhánh khác), hoặc pháp lý yêu cầu thời hạn giữ quyền xem phiếu lương.
