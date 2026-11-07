---
title: ADR-0009 Served companions are appointments of the group
status: accepted
date: 2026-11-07
deciders: EM (problem), agent (lead's decision)
---

# ADR-0009: Người đi cùng được làm = một lịch hẹn trong nhóm

## Context
EM 2026-11-07 ([raw](../../raw/2026-11-07-em-review-round3-small.md)): ở form đặt lịch, chọn người đi cùng nhưng không gán được KTV làm cho họ. Vòng 6 (Q-U5) chỉ có người đi cùng dạng ghi chú (`companions text[]`), còn đặt nhóm (`POST /appointments/group`, `group_id`) là khái niệm riêng. Người đi cùng thường chưa có hồ sơ, đôi khi chỉ biết "Chồng của chị Hà" (không SĐT). Ràng buộc: lượt khách cần `customer_id` (SĐT `NOT NULL`, duy nhất mỗi spa); hoa hồng "làm" lấy từ `visit_line.staff_id`; lịch dựa trên `EXCLUDE` theo KTV.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Người đi cùng có dịch vụ = lịch hẹn riêng trong nhóm; khách mới chỉ có tên (SĐT tùy chọn) khi ở trong nhóm | Dùng lại toàn bộ: thẻ trên lịch, `EXCLUDE` KTV, khách đến, ghi lượt, lương. Migration chỉ nới một CHECK | Ghi lượt vẫn cần tạo hồ sơ có SĐT |
| B. Thêm `companion_services` (dịch vụ + KTV) vào chính lịch hẹn của khách | Một dòng lịch | Lịch nhiều KTV: phải viết lại chồng giờ, thẻ, khách đến, ghi lượt; lượt của ai? |
| C. Ghi dịch vụ của người đi cùng vào lượt của khách chính | Không cần hồ sơ | Sai lịch sử khách, có thể trừ gói của người khác (D5), báo cáo khách sai |
| D. Cho hồ sơ khách không SĐT | Ghi lượt không cần SĐT | Đụng tìm kiếm, unique index, follow-up, mọi DTO `phone: String`; EM chưa quyết (Q11/Q-C9) |

## Decision
Chọn **A**. Migration `0022_served_companions`: bỏ `appointment_check1`, thay bằng `appointment_customer_or_guest` (khách mới cần tên; SĐT tùy chọn) và `appointment_guest_phone_or_group` (khách không SĐT phải có `group_id`). API: `GuestCreate.phone` tùy chọn cho `people[1..]` và cho `POST /appointments` có `group_with` (tham gia nhóm của một lịch đang mở, cùng ngày; lịch chưa có nhóm thì mở nhóm); người đặt lịch vẫn bắt buộc SĐT. `PATCH /appointments` nhận `companions` (ghi chú). `AppointmentDto.customer_phone` thành nullable. Web gộp "+ Thêm người" và "+ Người đi cùng (ghi chú)" thành một danh sách "Người đi cùng".

## Consequences
- Positive: không có kiểu dữ liệu mới; mọi quy tắc trùng KTV, sức chứa, ngày chốt, RLS, audit, idempotency giữ nguyên; hoa hồng "làm" đúng KTV vì lượt của người đi cùng là lượt riêng.
- Negative / accepted trade-offs: người đi cùng chỉ có tên phải được tạo hồ sơ (có SĐT) trước khi ghi lượt. `group_with` là hai bước khi sửa lịch (PATCH rồi POST); web giữ kết quả bước đã xong và dùng `Idempotency-Key` riêng cho từng người nên gửi lại không đặt trùng.
- **Cập nhật 2026-11-07 ([ADR-0010](ADR-0010-anonymous-walk-in-and-optional-phone.md), quyết định E):** hồ sơ khách không bắt buộc SĐT, nên người đi cùng chỉ có tên được "Tạo hồ sơ" ngay ở check-in nhóm mà không cần SĐT; Q-C9 đã trả lời.
- Follow-ups: tách / rời nhóm chưa có (hủy lịch của người đó là đủ).

## Revisit when
EM muốn ghi lượt cho người không SĐT, hoặc một người cần nhiều KTV trong một lịch (Q7 của spec lịch hẹn).
