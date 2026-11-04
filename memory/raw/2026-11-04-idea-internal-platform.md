# EM idea — internal spa platform — 2026-11-04

Source: EM chat message (faithful summary) + reference files in `docs for reference/`.

## EM message
- Explicitly "only my idea" — needs clarification and discussion.
- A platform with roles: customer, staff, manager, spa owner, platform admin (the builder, who edits and supports the owner later).
- Simple public homepage introducing the spa Viên Phúc Y Đường (dưỡng sinh đông y). Later: customer online booking with area selection; later: product shop.
- **First priority: internal features** for staff, manager, owner:
  - Daily staff evaluation, tracked per month.
  - Compensation (đãi ngộ), bonus and penalty computed automatically from rules (rule docs provided).
  - Customer management for manager/owner, replacing paper books; currently 4 kinds of paper sheets with no automatic calculation and no reminders when a treatment package ends or a customer still has sessions left. Free to reorganise them sensibly.
  - Bookkeeping entered on the platform instead of paper.
- Roles must be concrete; exact access rules discussed later, but code must make permissions easy to customise and extend.
- All code must be extensible and easy to change; consider architecture from the start.
- Concern: agents fix only the spot they look at and skip callers and stale tests; wants a way to always check related parts when adding, changing, or deleting. Asked about tools for code relationships (Obsidian or other).

## Reference files (read 2026-11-04)
- `sheet cho quản lý khách hàng/1.jpg, 2.jpg` — generic book "VIÊN PHÚC Y ĐƯỜNG": columns Ngày tháng | Khách | Dịch vụ | Giá dịch vụ | Nhân viên | Ghi chú.
- `thu chi/` — "SỔ THU CHI HẰNG NGÀY, Năm 2026" (Spa dưỡng sinh đông y công nghệ mới): Ngày tháng | Chứng từ (Thu, Chi) | Diễn giải | Số tiền thu (TM, CK, Tổng) | Số tiền chi (TM, CK, Tổng) | Tồn (TM, TK, Tổng).
- `sổ theo dõi doanh thu hằng ngày/` — Ngày tháng ghi sổ | Nội dung (Dịch vụ 1 lần, Liệu trình) | Số tiền (Chuyển khoản, Tiền mặt) | Chưa thanh toán | Ghi chú.
- `sổ theo dõi liệu trình khách hàng/` — "Phường Đông Hà, năm 2026". Per-customer header: Khách hàng, Số ĐT, Dịch vụ, Giá dịch vụ, Ngày đăng ký, Quà tặng, Ghi chú. Rows: STT | Ngày tháng/Giờ hẹn | Dịch vụ sử dụng | DV sử dụng thêm | Giá | Giờ ra | Ghi chú.
- `Bảng chấm công đánh giá tháng cho nhân viên Viên Phúc Y Đường.xlsx` — sheets "Bảng tháng" (daily attendance + monthly 6-criteria evaluation + results), "Tham chiếu lương" (salary/bonus tables), "Hướng dẫn". Notes commission is not computed because it needs per-day sales.
- `Nội dung chương trình đào tạo học viên v3.docx` — trainee program (4 months, deposit / no-deposit, 12/24-month commitment).
- `Nội dung chương trình tuyển dụng kĩ thuật viên v3.docx` — experienced technician program (1-month probation, 12/24-month commitment).
- `logo Viên phúc y đường transparent.png` — brand logo.
Rules extracted in full into `wiki/product/compensation-rules.md`.
