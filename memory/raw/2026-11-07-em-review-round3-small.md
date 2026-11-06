# EM review — 2026-11-07: follow-ups after PRs #66–#68

Verbatim (images 51 and 52 are in the session scratchpad `images/`):

> sửa 2 lỗi đó đi
>
> và bên cạnh đó hình như lối tắt thì thiếu mục nhân viên trong cái tổng quan ấy check kĩ để bỏ cho đủ
>
> trong cái dặt lịch hẹ chọn người đi cùng nhưng ai làm cho người đi cùng thì không assign được người làm cho người đi cùng rất tệ
>
> [Image #51] lỗi UI
>
> t[Image #52] thứ nhất lỗi ở spacing chữ khen và button
> thứ hai tôi thấy cái chấm công này nội dung đâu có gì nhiều đâu mà làm UI rất bự rất khó nhìn có thể sắp xếp nó gọn lại được không dễ tương tác dễ nhìn hơn

## Context
- **"2 lỗi đó"**, the two leftovers reported after #67:
  - at 390px, the appointment sheet's "Khách đến · ghi lượt" button wraps to two lines;
  - the "Còn … để nhập số liệu" clock has a gap before the time.
- **Image 51:** the "Ghi chú buổi (không bắt buộc)" textarea in focus, dark mode. It shows a thick gold focus ring, a squashed height, a resize grip in the corner, and a radius that doesn't match the controls.
- **Image 52:** one staff card in Nhân viên · Chấm công (day). It holds the name and role, a "Chưa chấm công" chip, a "Giờ & ghi chú" button, a 4-option segmented row (Đi làm / Nghỉ có phép P / Nghỉ đột xuất ĐX / Nghỉ không phép K), "Ghi nhận trong ngày" with Khen and Phàn nàn steppers, and "+ Ghi thưởng / phạt". The "Khen" label sits too close to its stepper. The card is far taller than its content needs.
