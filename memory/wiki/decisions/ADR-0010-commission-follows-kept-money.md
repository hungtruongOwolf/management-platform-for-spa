---
title: ADR-0010 Upsell commission follows the money the spa keeps
status: accepted (lead's decision — EM to review)
date: 2026-11-07
deciders: agent (lead's brief: conservative reading), EM to confirm
---

# ADR-0010: Hoa hồng chốt gói đi theo số tiền spa giữ lại

## Context
Test thật 2026-11-07: Võ Thị Hằng vẫn nhận 7% hoa hồng chốt trên 1.290.000đ của gói Xông chân, dù gói đã hủy và hoàn khách 1.000.000đ. Bảng lương tính `hoa hồng chốt = tỷ lệ × Σ giá gói` trên mọi hóa đơn gói còn mở; hủy gói có hoàn tiền (`POST /customer-packages/{id}/cancel`) không hủy hóa đơn, nên bảng lương không thấy khoản hoàn.

[compensation-rules](../product/compensation-rules.md) §3 chỉ ghi "% × giá trị liệu trình mới tư vấn và chốt", không nói gì về gói bị hủy. Mặc định đã build cho Q-C3 ([staff-management §6](../product/specs/staff-management.md) #3): "chốt = tổng hóa đơn (R1 luôn thu đủ, nên **tiền đã thu** = giá trị bán)". Tức là hoa hồng chốt vốn được hiểu là tính trên tiền spa thu được.

Ràng buộc: tháng đã chốt lương không bao giờ đổi (ADR-0008). Gói có thể hủy ở tháng sau tháng bán.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Thu hồi toàn bộ hoa hồng khi gói bị hủy | Đơn giản, chi ít nhất | Gói hủy **không hoàn tiền** (khách bỏ, spa giữ đủ tiền) vẫn mất hoa hồng: phạt người bán vì việc không do họ |
| B. Trừ theo số tiền hoàn (cơ sở = giá gói − tiền hoàn) | Khớp mặc định Q-C3 "tính trên tiền đã thu"; không bao giờ trả hoa hồng trên tiền đã trả lại khách; hủy không hoàn thì giữ nguyên | Một dòng âm trên phiếu lương tháng hoàn tiền; tháng đó có thể âm nếu người bán không bán gì |
| C. Giữ như cũ | Không đổi gì | Trả hoa hồng trên tiền spa đã trả lại: lỗi tiền (EM báo) |

## Decision
Chọn **B**. Khoản hoàn của một gói được trừ khỏi cơ sở hoa hồng chốt của **người chốt gói đó**, ghi vào **tháng của ngày hoàn tiền** (`refund.business_date`) dưới dạng một dòng `package_refund` có giá trị âm trên phiếu lương (`upsell_lines`, "Hoàn tiền gói (trừ lại)"); `upsell_package_base` = gói chốt − hoàn. Hoàn cùng tháng bán: hai dòng triệt tiêu, hoa hồng tính trên phần spa giữ (ví dụ Hằng: 7% × (1.290.000 − 1.000.000) = 20.300đ). Hoàn ở tháng sau: tháng bán giữ nguyên (có thể đã chốt), tháng hoàn bị trừ lại `tỷ lệ × tiền hoàn` (có thể làm hoa hồng chốt của tháng đó âm).

Đây là cách đọc thận trọng mà lead yêu cầu khi quy tắc không rõ: không bao giờ trả hoa hồng trên tiền đã trả lại khách, nhưng cũng không lấy lại hoa hồng trên tiền spa vẫn giữ.

Hủy hóa đơn gói (`POST /sales/{id}/void`, chỉ khi chưa dùng buổi) vẫn như cũ: hóa đơn `voided` không còn trong cơ sở của tháng bán; nếu tháng đó đã chốt thì chủ spa ghi điều chỉnh (AC5.6).

## Consequences
- Positive: bảng lương khớp sổ tiền; phiếu lương ghi rõ dòng trừ lại; không cần migration (đọc `app.refund` có sẵn); tháng đã chốt không đổi.
- Negative / accepted trade-offs: hoa hồng chốt của một tháng có thể âm (thực lĩnh giảm tương ứng). Tỷ lệ dùng để trừ là tỷ lệ hiện tại của người chốt, không phải tỷ lệ của tháng bán (điều kiện lương sửa được giữa chừng; hiếm).
- Follow-ups: EM xác nhận B (hay muốn A, hoặc chỉ trừ khi hoàn trong N ngày). Nếu chọn A: đổi truy vấn `hr::repo::upsell_lines` (trừ `sold_price` thay vì `refund.amount`, kể cả hủy không hoàn).

## Revisit when
EM chọn quy tắc khác cho gói hủy, hoặc có trả góp (tiền thu < giá bán), lúc đó cơ sở hoa hồng chốt phải tính trên tiền thực thu từng tháng.
