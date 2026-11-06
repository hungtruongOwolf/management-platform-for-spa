---
title: Glossary — ubiquitous language (draft)
updated: 2026-11-05
sources: [raw/2026-11-04-idea-internal-platform.md, raw/2026-11-05-em-review-round4.md]
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
| Mẫu gói liệu trình | `PackagePlan` | Mẫu gói trong bảng giá: dịch vụ, số buổi, giá, quà tặng, hạn dùng. **Không** gọi là "gói bán" |
| Gói liệu trình của khách (nói gọn: gói liệu trình) | `CustomerPackage` | Gói khách đã mua: số buổi còn lại, hạn, trạng thái. **Không** gọi là "theo dõi gói" |
| Lượt phục vụ / Buổi | `Visit` | Một lần khách đến: giờ vào/ra, các dịch vụ, KTV |
| Dịch vụ dùng thêm | `Visit add-on` | Dịch vụ lẻ phát sinh trong một buổi |
| Hủy lượt | `POST /visits/{id}/void` | Lượt ghi nhầm bị hủy hẳn: buổi cộng lại, tiền ghi bút toán đảo, lượt còn trong lịch sử "Đã hủy" |
| Sửa lượt | `PATCH /visits/{id}` · `POST /visits/{id}/correct` | Chỉnh lượt đã ghi. Sửa trực tiếp (KTV, giờ làm, ghi chú) giữ lượt; đụng tiền/buổi thì **hủy-ghi lại**: lượt cũ "Đã sửa" (`corrected_by_visit_id`), lượt mới nối `corrects_visit_id`. Không gọi là "hủy" trong báo cáo |
| Chênh lệch khi sửa | `VisitCorrected.difference` | "Thu thêm …" / "Hoàn …" / "Không đổi tiền" = tổng mới − tổng cũ |
| Lịch hẹn | `Appointment` | Hẹn trước một buổi: Đã đặt (`booked`) → Đã đến (`arrived`) → Hoàn tất (`done`, thành một lượt khách); hoặc Không đến (`no_show`) / Đã hủy (`cancelled`) kèm lý do ([spec](specs/appointments.md)) |
| Khách vãng lai | walk-in | Khách đến không hẹn trước: ghi thẳng lượt khách, không qua lịch |
| Khách mới (trong lịch hẹn) | `guest` | Người hẹn chỉ có tên + SĐT, chưa có hồ sơ; tạo hồ sơ khi đến |
| Giờ làm | `visit.check_in_at` | Lúc dịch vụ được làm (khác lúc ghi `created_at`); quyết định ngày nghiệp vụ của lượt |
| Nhập bù | back-dating (`visit.backdate`) | Chủ spa ghi lượt của ngày đã qua; mọi số liệu về đúng ngày làm |
| Sức chứa / chỗ trống | capacity | Số KTV đang làm trừ số lịch đang mở trong cùng khung giờ |
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

## Tên màn hình và điều hướng (EM vòng 4 mục 7, 2026-11-05)
EM: "đặt lại các tên cho đúng đừng ghi là gói bán hay theo dõi gói mà nên cụ thể gói liệu trình" ([raw](../../raw/2026-11-05-em-review-round4.md)). Quy tắc: tên nói **cái gì** (danh từ cụ thể), không nói hành động mơ hồ ("theo dõi", "bán"); cùng một thứ dùng cùng một tên ở nav, tiêu đề trang (h1), breadcrumb, nút quay lại, command palette, lối tắt trang chủ. Tab bar điện thoại được dùng **nhãn ngắn** (≤ 10 ký tự, M3/HIG: một dòng) nhưng phải là phần đầu của tên đầy đủ.

| Route | Nhóm nav | Mục nav = tiêu đề trang (h1) | Nhãn ngắn (tab bar) | Trước đây |
|---|---|---|---|---|
| `/` | Quầy | Tổng quan | Trang chủ | — |
| `/quay` | Quầy | Ghi lượt khách | Quầy | — |
| `/quay?view=lich` (tab thứ hai của màn quầy) | Quầy | Lịch hẹn | Lịch hẹn (**thay "Trang chủ"** trên tab bar khi có `appointment.manage`, EM vòng 4; Tổng quan vào "Thêm") | — |
| `/khach-hang` | Khách hàng | Danh sách khách hàng (h1 "Khách hàng") | Khách | nav "Hồ sơ khách" |
| `/khach-hang/:id` | Khách hàng | tên khách; breadcrumb `Khách hàng › Tên` | — | — |
| `/khach-lau-khong-quay-lai` | Khách hàng | Khách lâu chưa quay lại | — | "Lâu không quay lại" / "Khách lâu không quay lại" |
| `/goi-lieu-trinh` | Gói liệu trình | Gói liệu trình của khách | — | nav "Theo dõi gói", h1 "Theo dõi gói của khách" |
| `/bao-cao` | Báo cáo | Báo cáo kinh doanh (tab "Sổ doanh thu") | Báo cáo | nav "Kinh doanh" |
| `/bang-gia` | Danh mục | Bảng giá dịch vụ | — | nav "Dịch vụ" |
| `/bang-gia/goi` | Danh mục | Mẫu gói liệu trình | — | nav "Gói bán", h1 "Gói liệu trình" |
| `/cai-dat` | Cài đặt | Giao diện & tài khoản | — | — |
| `/nhan-vien` (staff agent, PR #37) | Nhân viên | quản lý: "Nhân viên", tab "Sổ hằng ngày" / "Bảng công tháng" / "Đánh giá tháng" / "Bảng lương" / "Hồ sơ" (vòng 5); KTV (không quyền quản lý): "Công và lương của tôi" | **Công lương** (KTV) | — |
| `/khach-cua-toi` (vòng 5) | Nhân viên | Khách của tôi (KTV: lịch hẹn sắp tới + khách đã làm) | **Khách** (KTV) | "Lịch sử của tôi" |

Nút và chữ liên quan: "+ Tạo mẫu gói" (không "+ Tạo gói"), "Bán gói liệu trình" (không "Bán liệu trình mới"), thẻ trên hồ sơ "Gói liệu trình", báo cáo "Gói liệu trình bán ra" / "Mẫu gói bán chạy".
