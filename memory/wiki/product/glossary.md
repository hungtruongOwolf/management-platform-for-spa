---
title: Glossary — ubiquitous language (draft)
updated: 2026-11-04
sources: [raw/2026-11-04-idea-internal-platform.md]
---

# Glossary (draft — chốt trong `/idea`)

Code, DB và API dùng đúng tên tiếng Anh ở cột 2; giao diện dùng tiếng Việt ở cột 1.

| Tiếng Việt (UI) | Code term | Định nghĩa |
|---|---|---|
| Chủ spa | `Owner` | Toàn quyền nghiệp vụ trên spa (tenant) của mình |
| Quản lý | `Manager` | Vận hành chi nhánh: khách, lịch, chấm công, đánh giá |
| Kỹ thuật viên / Nhân viên | `Staff` | Người phục vụ khách; xem dữ liệu của chính mình |
| Lễ tân / Thu ngân | `Receptionist` *(chưa xác nhận)* | Nhập lượt khách, thu tiền |
| Khách hàng | `Customer` | Người dùng dịch vụ (tài khoản online về sau) |
| Quản trị nền tảng | `PlatformAdmin` | Người xây platform, hỗ trợ chủ spa |
| Chi nhánh | `Branch` | Một địa điểm (khu vực) của spa |
| Nhóm dịch vụ | `ServiceCategory` | Nhóm trên menu (Gội đầu dưỡng sinh, Chăm sóc da mặt, Dịch vụ bổ sung…) |
| Dịch vụ | `Service` | Một dịch vụ trên menu. Lưu ý: "Liệu trình giải cảm" là **dịch vụ**, không phải gói |
| Phương án giá | `ServicePrice` | Giá chung, hoặc giá theo Nữ/Nam; một phương án có thể không áp dụng |
| Liệu trình (gói) | `PackagePlan` | Mẫu gói bán ra: dịch vụ, số buổi, giá, quà tặng, hạn dùng |
| Liệu trình của khách | `CustomerPackage` | Gói khách đã mua: số buổi còn lại, đã trả, còn nợ |
| Lượt phục vụ / Buổi | `Visit` | Một lần khách đến: giờ vào/ra, các dịch vụ, KTV |
| Dịch vụ dùng thêm | `Visit add-on` | Dịch vụ lẻ phát sinh trong một buổi |
| Lịch hẹn | `Appointment` | Hẹn trước một buổi |
| Hóa đơn | `Sale` | Bán dịch vụ lẻ / liệu trình / sản phẩm |
| Thanh toán | `Payment` | TM (`cash`) hoặc CK (`bank_transfer`); cho phép nhiều lần |
| Công nợ / Chưa thanh toán | `Receivable` | Phần hóa đơn chưa trả |
| Sổ quỹ / Thu chi | `CashbookEntry` | Bút toán thu/chi; tồn quỹ theo tài khoản (TM, TK) |
| Chấm công | `AttendanceDay` | Một ngày của một NV: đi làm/nghỉ P/ĐX/K, giờ đến/về |
| Đánh giá tháng | `MonthlyEvaluation` | 6 tiêu chí → điểm → Mức 1/2/3/Chưa đạt |
| Hợp đồng | `EmploymentContract` | Chương trình, gói 12/24, cọc, lựa chọn lương, ngày bắt đầu |
| Chính sách lương thưởng | `CompensationPolicy` | Bảng số liệu + quy tắc, có phiên bản và ngày hiệu lực |
| Bảng lương tháng | `PayrollRun` / `Payslip` | Kết quả tính lương, có điều chỉnh tay kèm lý do, khóa sau khi duyệt |
| Hoa hồng | `Commission` | 3 loại: `service_performed`, `package_closed`, `product_sold` |
| Thưởng gắn bó | `LoyaltyBonus` | 500k/tháng cộng dồn, trả theo mốc |
| Thưởng Tết | `TetBonus` | Tổng mức của từng tháng |
| Cờ kỷ luật | `DisciplineFlag` | Cảnh báo tự động theo Nhóm A; người quyết định là chủ spa |
| Nhắc nhở | `Reminder` | Liệu trình sắp hết, khách vắng lâu, công nợ, lịch hẹn |
