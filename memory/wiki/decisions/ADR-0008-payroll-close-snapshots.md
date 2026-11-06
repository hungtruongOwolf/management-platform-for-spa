---
title: ADR-0008 Payroll months close once into immutable payslip snapshots
status: proposed
date: 2026-11-05
deciders: EM, agent
---

# ADR-0008: Chốt lương một lần, lưu bản chụp phiếu lương, sửa bằng khoản điều chỉnh

## Context
[Spec staff-management](../product/specs/staff-management.md) US5. Lương phụ thuộc vào nhiều thứ có thể đổi về sau: chấm công, đánh giá, thưởng/phạt, điều kiện lương, quy tắc của spa, và các lượt khách (hoa hồng). Lượt khách còn có thể bị chủ spa hủy sau khi ngày đã chốt. Trong khi đó, tiền lương đã trả thì không được đổi theo.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Luôn tính lại trực tiếp | Không có bảng mới | Phiếu lương đã trả bị đổi khi ai đó sửa dữ liệu cũ, không kiểm được |
| B. Chốt tháng → lưu phiếu lương dạng jsonb chỉ thêm, khóa mọi bản ghi của tháng, không mở lại; sửa bằng khoản thưởng/phạt ghi vào tháng đang mở (`corrects_month`) | Giống sổ sách giấy ("không tẩy, chỉ ghi thêm"); kiểm được; lịch sử đọc nhanh | Sai sót phải sửa ở tháng sau |
| C. Chốt nhưng cho mở lại (có audit) | Linh hoạt | Phiếu đã trả có thể đổi; cần thêm quy trình duyệt |

## Decision
Chọn **B**. Khóa ở hai lớp:
- service: khóa dòng `payroll_period` FOR SHARE khi ghi, FOR UPDATE khi chốt;
- DB: trigger `reject_closed_period` và `payroll_period_forward_only`, cùng `reject_mutation` trên `payslip` và `staff_adjustment`.
Bản chụp lưu toàn bộ phiếu lương như đã hiển thị, gồm các số đầu vào. Quy tắc dùng khi chốt được ghi trong audit `payroll.close`.

## Consequences
- Positive: phiếu lương đã chốt không bao giờ đổi. KTV và chủ spa cùng nhìn một con số.
- Negative / accepted trade-offs: chốt nhầm thì không mở lại được; lượt bị hủy sau khi chốt chỉ được bù bằng khoản điều chỉnh ghi tay.
- Follow-ups: chi trả theo mốc (Thưởng gắn bó 12/24, thưởng cam kết) sẽ đọc từ các bản chụp này.

## Revisit when
EM cần quy trình "mở lại tháng" có duyệt, hoặc khi có bảng lương nhiều chi nhánh.
