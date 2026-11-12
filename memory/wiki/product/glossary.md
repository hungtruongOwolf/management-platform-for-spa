---
title: Glossary — ubiquitous language (draft)
updated: 2026-11-11
sources: [raw/2026-11-04-idea-internal-platform.md, raw/2026-11-05-em-review-round4.md, raw/2026-11-06-em-upsell-and-convert-to-package.md]
---

# Glossary (draft — chốt trong `/idea`)

Code, DB và API dùng đúng tên tiếng Anh ở cột 2; giao diện dùng tiếng Việt ở cột 1. Dòng ghi *(chưa build)* là thuật ngữ đã chốt cho phần chưa làm; khi tên trong code khác tên khái niệm, cột 2 ghi cả tên thật (bảng / field).

| Tiếng Việt (UI) | Code term | Định nghĩa |
|---|---|---|
| Chủ spa | `Owner` | Toàn quyền nghiệp vụ trên spa (tenant) của mình |
| Quản lý | `Manager` | Vận hành chi nhánh: khách, lịch, chấm công, đánh giá |
| Kỹ thuật viên / Nhân viên | `Staff` | Người phục vụ khách; xem dữ liệu của chính mình |
| Lễ tân / Thu ngân | `Receptionist` *(chưa xác nhận; chưa có role này — role hệ thống hiện là `owner` / `manager` / `technician`, quản lý kiêm thu ngân)* | Nhập lượt khách, thu tiền |
| Khách hàng | `Customer` | Người dùng dịch vụ (tài khoản online về sau) |
| Quản trị nền tảng | `PlatformAdmin` (code: operator, bảng `platform_operator`) | Người xây platform, hỗ trợ chủ spa; danh tính riêng, không phải thành viên spa ([spec platform-admin](specs/platform-admin.md), ADR-0018) |
| Chi nhánh | `Branch` | Một địa điểm (khu vực) của spa |
| Nhóm dịch vụ | `ServiceCategory` | Nhóm trên menu (Gội đầu dưỡng sinh, Chăm sóc da mặt, Dịch vụ bổ sung…) |
| Dịch vụ | `Service` | Một dịch vụ trên menu. Lưu ý: "Liệu trình giải cảm" là **dịch vụ**, không phải gói |
| Phương án giá | `ServicePrice` | Giá chung, hoặc giá theo Nữ/Nam; một phương án có thể không áp dụng |
| Mẫu gói liệu trình | `PackagePlan` | Mẫu gói trong bảng giá: dịch vụ, số buổi, giá, quà tặng, hạn dùng. **Không** gọi là "gói bán" |
| Gói liệu trình của khách (nói gọn: gói liệu trình) | `CustomerPackage` | Gói khách đã mua: số buổi còn lại, hạn, trạng thái. **Không** gọi là "theo dõi gói" |
| Lượt phục vụ / Buổi | `Visit` | Một lần khách đến: giờ vào/ra, các dịch vụ, KTV |
| Dịch vụ dùng thêm | `Visit add-on` | Dịch vụ lẻ phát sinh trong một buổi |
| Hủy lượt | `POST /visits/{id}/void` | Lượt ghi nhầm bị hủy hẳn: buổi cộng lại, tiền ghi bút toán đảo, lượt còn trong lịch sử "Đã hủy" |
| Sửa lượt | `PATCH /visits/{id}` · `POST /visits/{id}/correct` | Chỉnh lượt đã ghi. Sửa trực tiếp (KTV, giờ làm, ghi chú) giữ lượt; đụng tiền/buổi thì **hủy-ghi lại**: lượt cũ chip **"Đã thay"** + chữ "Đã thay bằng lượt mới." (`corrected_by_visit_id`; trước ADR-0022 là "Đã sửa"), lượt mới nối `corrects_visit_id`. Không gọi là "hủy" trong báo cáo |
| Ai ghi, ai sửa / Ghi bởi · Tạo bởi | `record.history.read` · `GET /record-history/*` · `RecordTrace` (web) | Dòng nhỏ cuối chi tiết một bản ghi: "Ghi bởi Hà · 10:07 · đã sửa 1 lần" (lượt, hóa đơn, gói, lịch hẹn, ô chấm công / khen chê, thưởng / phạt), "Tạo bởi Hà · 03/02/2025" (hồ sơ khách), "Nhập từ sổ giấy · …", "Chưa có ai ghi" (ô chưa ai ghi); bấm thì mở lịch sử của chính bản ghi đó. "Ghi bởi" ở mọi nơi, không "Bán bởi". Chủ spa mặc định thấy, quản lý chỉ khi operator bật, KTV không bao giờ ([ADR-0022](../decisions/ADR-0022-inline-record-history.md), [spec](specs/record-history.md)) |
| Đã sửa / Đã thay / Đã hủy (chip) | `edited` · `corrected_by_visit_id` · `status = voided` | Một chip mỗi hàng, loại trừ nhau theo thứ tự: **Đã thay** = bị Sửa lượt thay; **Đã hủy** = hủy và không bị thay; **Đã sửa** = điều đã ghi bị đổi (giờ, KTV, Sửa lượt, chuyển sang gói; lịch hẹn đổi giờ hoặc KTV, chỉ ở dạng Danh sách) và chưa hủy, chỉ người có `record.history.read` thấy. Thêm dịch vụ, gia hạn, dùng gói hết hạn, ghi chú, khách đến / hoàn tất / hủy lịch không phải "sửa" (spec D9). Trong lịch sử: ô chấm công / khen chê ghi lại lần sau là sửa; sửa một khoản thưởng / phạt (kể cả chuyển người) là một dòng "sửa khoản thưởng" |
| Trả lại (tiền khi hủy) | `AMOUNT_LABEL` của `visit.void` / `sale.void` | Tiền một lần hủy trả lại khách, trong lịch sử bản ghi và Nhật ký thao tác (thay chữ kế toán "Đảo", ADR-0022) |
| Chênh lệch khi sửa | `VisitCorrected.difference` | "Thu thêm …" / "Hoàn …" / "Không đổi tiền" = tổng mới − tổng cũ |
| Gói bán hôm nay *(draft vòng 6)* | `package_sales[]` của lượt | Gói bán ngay trong giỏ của lượt; "Dùng gói này cho buổi hôm nay" trừ buổi hôm nay từ gói đó ([spec §10](specs/front-desk-and-packages.md)) |
| Chuyển sang gói liệu trình *(draft vòng 6)* | `convert-to-package` | Lượt lẻ đã trả được thay bằng lượt trừ buổi của gói vừa mua; khách đưa thêm phần chênh |
| Thêm dịch vụ vào lượt *(draft vòng 6)* | visit addition | Thêm dòng / bán gói vào lượt đã lưu, hóa đơn mới nối `sale.visit_id`; không phải sửa sai |
| Người trả / người dùng gói *(draft vòng 6)* | `sale.customer_id` / `customer_package.customer_id` | Mua gói cho người khác: hóa đơn ghi người trả, gói thuộc người dùng; không dùng chung (D5) |
| Lịch hẹn | `Appointment` | Hẹn trước một buổi: Đã đặt (`booked`) → Đã đến (`arrived`) → Hoàn tất (`done`, thành một lượt khách); hoặc Không đến (`no_show`) / Đã hủy (`cancelled`) kèm lý do ([spec](specs/appointments.md)) |
| Khách vãng lai | walk-in | Khách đến không hẹn trước: ghi thẳng lượt khách, không qua lịch. Khách không cho tên / SĐT → **hồ sơ không tên** (dưới) |
| Hồ sơ không tên ("Khách vãng lai") | customer with `full_name` NULL (`named: false`) | Hồ sơ của khách vãng lai không cho tên và SĐT (EM 2026-11-07, [ADR-0011](../decisions/ADR-0011-anonymous-walk-in-and-optional-phone.md)); mọi nơi hiện "Khách vãng lai" (`app.customer_label`). Không có trong danh sách gần đây / gọi lại; không là chủ gói được (`customer.name_required`) |
| Ngày sinh | `customer.birth_date` | Tùy chọn (ưu đãi sinh nhật sau này); không sau hôm nay, ≤ 120 tuổi; năm sinh theo ngày sinh |
| Gia hạn | `POST /customer-packages/{id}/extend` · `customer_package_extension` | Dời hạn gói về sau, có lý do; một mục "Gia hạn" (`package_extended`) trên dòng thời gian khách |
| Khách mới (trong lịch hẹn) | `guest` | Người hẹn chỉ có tên (+ SĐT nếu có), chưa có hồ sơ; tạo hồ sơ khi đến (SĐT không bắt buộc từ 2026-11-07) |
| Giờ làm | `visit.check_in_at` | Lúc dịch vụ được làm (khác lúc ghi `created_at`); quyết định ngày nghiệp vụ của lượt |
| Nhập bù | back-dating (`visit.backdate`) | Chủ spa ghi lượt của ngày đã qua; mọi số liệu về đúng ngày làm |
| Số khách tối đa (mỗi khung giờ) | `appointment_setting.max_per_slot` | Chủ spa đặt tùy chọn (1–200, để trống = không giới hạn; `appointment.settings`). Lịch hiện "n / tối đa" và **chỉ cảnh báo** khi đủ, không bao giờ chặn đặt lịch. Thay cho "sức chứa = số KTV − số lịch mở" (migration 0019, [spec lịch hẹn §10](specs/appointments.md)) |
| Hóa đơn | `Sale` | Bán dịch vụ lẻ / liệu trình / sản phẩm |
| Thanh toán | `Payment` | TM (`cash`) hoặc CK (`bank_transfer`); cho phép nhiều lần |
| Công nợ / Chưa thanh toán | `Receivable` *(chưa build: R1 thu đủ, `sale.amount_paid` = `total`)* | Phần hóa đơn chưa trả |
| Sổ quỹ / Thu chi | `CashbookEntry` *(chưa build)* | Bút toán thu/chi; tồn quỹ theo tài khoản (TM, TK) |
| Chấm công | `AttendanceDay` | Một ngày của một NV: Đi làm (`present`) / Nghỉ có phép (`leave_p`) / Nghỉ đột xuất (`leave_dx`) / Nghỉ không phép (`absent_k`), giờ đến/về. UI chỉ hiện chữ + icon + màu, không còn mã P/ĐX/K của sổ giấy (F20, EM 2026-11-08) |
| Đánh giá tháng | `MonthlyEvaluation` (bảng `staff_evaluation`) | 6 tiêu chí → điểm → Mức 1/2/3/Chưa đạt |
| Hợp đồng | `EmploymentContract` *(chưa build, [staff-management](specs/staff-management.md) §8)* | Chương trình, gói 12/24, cọc, lựa chọn lương, ngày bắt đầu. Khác **đợt làm việc** (`staff_employment`, ADR-0013): chỉ từ ngày – đến ngày |
| Chính sách lương thưởng | `CompensationPolicy` (code: `payroll_rules` theo spa + `staff_pay` theo người) | Bảng số liệu + quy tắc. Hiện chưa có phiên bản / ngày hiệu lực: sửa áp dụng cho mọi tháng chưa chốt, tháng đã chốt giữ bản chụp `payslip` |
| Bảng lương tháng | `PayrollRun` / `Payslip` (code: `payroll_period` / `payslip`) | Kết quả tính lương; điều chỉnh tay là khoản thưởng/phạt kèm lý do (`staff_adjustment`); chốt tháng (`POST /payroll/close`) thì khóa |
| Hoa hồng | `Commission` | 3 loại theo quy chế: làm (`service_commission`), giới thiệu / chốt gói + dịch vụ lẻ (`closing_commission`), bán sản phẩm *(chưa build)* |
| Thưởng gắn bó | `LoyaltyBonus` (phiếu lương: `loyalty_bonus_accrued`) | 500k/tháng cộng dồn, trả theo mốc (chi trả theo mốc chưa build; phiếu chỉ ghi nhận) |
| Thưởng Tết | `TetBonus` (phiếu lương: `tet_level`, `tet_amount`) | Tổng mức của từng tháng (phiếu ghi nhận mức tháng, không cộng vào thực lĩnh) |
| Cờ kỷ luật | `DisciplineFlag` (hiện chỉ có cờ phiếu lương `absent_k5_plus`) | Cảnh báo tự động theo Nhóm A; người quyết định là chủ spa |
| Nhắc nhở | `Reminder` *(chưa build như một module; khách vắng lâu có danh sách gọi lại `customer_follow_up`)* | Liệu trình sắp hết, khách vắng lâu, công nợ, lịch hẹn |

## Tên màn hình và điều hướng (EM vòng 4 mục 7, 2026-11-05)
EM: "đặt lại các tên cho đúng đừng ghi là gói bán hay theo dõi gói mà nên cụ thể gói liệu trình" ([raw](../../raw/2026-11-05-em-review-round4.md)). Quy tắc: tên nói **cái gì** (danh từ cụ thể), không nói hành động mơ hồ ("theo dõi", "bán"); cùng một thứ dùng cùng một tên ở nav, tiêu đề trang (h1), breadcrumb, nút quay lại, command palette, lối tắt trang chủ. Tab bar điện thoại được dùng **nhãn ngắn** (≤ 10 ký tự, M3/HIG: một dòng) nhưng phải là phần đầu của tên đầy đủ.

| Route | Nhóm nav | Mục nav = tiêu đề trang (h1) | Nhãn ngắn (tab bar) | Trước đây |
|---|---|---|---|---|
| `/` | Quầy | Tổng quan | Trang chủ | — |
| `/quay` | Quầy | Ghi lượt khách | Quầy | — |
| `/quay?view=lich` (tab thứ hai của màn quầy) | Quầy | Lịch hẹn | Lịch hẹn (**thay "Trang chủ"** trên tab bar khi có `appointment.manage`, EM vòng 4; Tổng quan vào "Thêm") | — |
| `/khach-hang` | Khách hàng | Danh sách khách hàng (breadcrumb của hồ sơ: "Khách hàng") | Khách | nav "Hồ sơ khách" |
| `/khach-hang/:id` | Khách hàng | tên khách; breadcrumb `Khách hàng › Tên` | — | — |
| `/khach-lau-khong-quay-lai` | Khách hàng | Khách lâu chưa quay lại | — | "Lâu không quay lại" / "Khách lâu không quay lại" |
| `/goi-lieu-trinh` | Gói liệu trình | Gói liệu trình của khách | — | nav "Theo dõi gói", h1 "Theo dõi gói của khách" |
| `/bao-cao` | Báo cáo | Báo cáo kinh doanh (tab "Tổng quan" / "Sổ doanh thu"; h1 theo tab) | Báo cáo | nav "Kinh doanh" |
| `/nhat-ky-ghi-chu` | Báo cáo | Nhật ký ghi chú (khi bật feature `notes_journal`) | — | — |
| `/nhat-ky-thao-tac` | Báo cáo | Nhật ký thao tác (**ẩn mặc định** sau cờ `activity_log`, operator bật lại theo spa, ADR-0022; chỉ chủ spa, `audit.read`; đọc `audit_log`: ai làm gì, lúc nào; một thao tác = một dòng, các dòng audit cùng transaction như lượt + hóa đơn được gộp khi đọc; ADR-0021) | — | — |
| `/bang-gia` | Danh mục | Bảng giá dịch vụ | — | nav "Dịch vụ" |
| `/bang-gia/goi` | Danh mục | Mẫu gói liệu trình | — | nav "Gói bán", h1 "Gói liệu trình" |
| `/cai-dat` | Cài đặt | Giao diện & tài khoản | — | — |
| `/nhan-vien` (staff agent, PR #37) | Nhân viên | chủ spa + quản lý: "Quản lý nhân sự", tab "Sổ hằng ngày" / "Bảng công tháng" / "Đánh giá tháng" / "Bảng lương" / "Hồ sơ nhân viên"; KTV (không quyền quản lý): "Công và lương của tôi" | **Công lương** (KTV) | mục/h1 "Nhân viên" (đổi 2026-11-08, PR #98); tab "Hồ sơ" → "Nhân sự" → "Hồ sơ nhân viên" |
| `/nhan-vien/cua-toi` (EM 2026-11-08) | Nhân viên | Công và lương của tôi (quản lý: tháng của chính mình) | Công lương | — |
| `/khach-cua-toi` (vòng 5) | Nhân viên | Khách của tôi (KTV, và quản lý từ 2026-11-08: lịch hẹn sắp tới + khách đã làm) | **Khách** (KTV) | "Lịch sử của tôi" |

Nút và chữ liên quan: "+ Tạo mẫu gói" (không "+ Tạo gói"), "Bán gói liệu trình" (không "Bán liệu trình mới"), thẻ trên hồ sơ "Gói liệu trình", báo cáo "Gói liệu trình bán ra" / "Mẫu gói bán chạy".
