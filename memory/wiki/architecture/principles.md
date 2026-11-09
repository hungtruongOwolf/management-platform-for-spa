---
title: Architecture principles — extensibility & changeability
updated: 2026-11-09
sources: [raw/2026-11-04-idea-internal-platform.md]
---

# Nguyên tắc kiến trúc: dễ mở rộng, dễ thay đổi

Yêu cầu của EM: code phải mở rộng và chỉnh sửa được dễ dàng, không làm cho xong rồi sau khó đổi. Các nguyên tắc dưới đây áp dụng từ ngày đầu và được `/architect` chốt thành ADR.

1. **Modular monolith theo bounded context.** Một Rust binary, chia module độc lập: `identity` (tài khoản, quyền), `org` (spa, chi nhánh, nhân sự, hợp đồng), `crm` (khách hàng), `catalog` (dịch vụ, liệu trình, sản phẩm), `operations` (lịch hẹn, lượt phục vụ), `sales` (hóa đơn, thanh toán, công nợ), `ledger` (sổ quỹ), `workforce` (chấm công, đánh giá), `payroll` (lương, hoa hồng, thưởng), `notifications` (nhắc nhở). Module chỉ gọi nhau qua public interface của module. Không module nào đọc bảng của module khác. Khi cần tách thành service riêng thì tách được mà không phải viết lại.
2. **Quyền là dữ liệu, không phải `if role == ...`.** Code chỉ hỏi `can(actor, permission, resource)`. Permission là chuỗi cố định (`customer.read`, `payroll.approve`...). Role (Chủ spa, Quản lý, KTV, Lễ tân...) là tập permission lưu trong DB, chủ spa/admin tùy chỉnh được. Quyền luôn có phạm vi (scope): tenant → chi nhánh → "của chính mình". Thêm role mới chỉ cần thêm dữ liệu, không cần sửa code.
3. **Đa spa / đa chi nhánh sẵn trong dữ liệu.** Mọi bảng nghiệp vụ có `tenant_id` (+ `branch_id` khi cần) ngay từ đầu, kể cả khi mới chạy cho 1 spa. Thêm cột từ đầu thì gần như miễn phí, còn thêm sau khi đã có dữ liệu thì rất đắt.
4. **Chính sách là dữ liệu, tháng đã chốt giữ bản chụp** ([ADR-0019](../decisions/ADR-0019-audit-decisions-2026-11-09.md) D5 thay bản cũ). Số ngày nghỉ miễn trừ, mức thưởng Tết, quy tắc đi muộn, thưởng gắn bó... là **một** `payroll_rules` mỗi spa (jsonb, có `version`, sửa bằng `If-Match`), không hard-code. Điều kiện lương của từng người (`staff_pay`) cũng là dữ liệu. Đổi quy tắc thì áp dụng cho mọi tháng **chưa chốt**. Tháng đã chốt không tính lại: phiếu lương là bản chụp đầy đủ số đầu vào, còn quy tắc đã dùng được ghi trong audit `payroll.close` ([ADR-0008](../decisions/ADR-0008-payroll-close-snapshots.md)). Chưa cần `CompensationPolicy` có `effective_from`; làm khi cần tính lại một tháng cũ theo quy tắc cũ (ví dụ chi trả theo mốc 12/24 tháng đọc từ nhiều tháng).
5. **Công thức là hàm thuần (pure function), test bằng bảng ví dụ.** `calculate_payslip(inputs, policy) -> Payslip` không đụng DB, không đụng thời gian thực. Mỗi ví dụ trong văn bản của Spa (lương ngày 178.571đ, thưởng Tết 4.500.000đ...) trở thành một test case. Thêm chương trình hợp đồng mới thì viết thêm một strategy, các strategy cũ giữ nguyên.
6. **Sự kiện gốc, báo cáo suy ra.** Chỉ nhập Visit, Sale/Payment, Cash entry. Doanh thu, tồn quỹ, số khách của NV, hoa hồng đều được tính ra, không ai nhập lại. Sổ quỹ và thanh toán là **append-only**: sửa sai bằng bút toán điều chỉnh, không xóa.
7. **Kết quả tính toán có thể ghi đè nhưng phải để dấu vết.** Mọi số tự tính (lương, điểm, mức) cho phép quản lý ghi đè, kèm lý do và người sửa (audit). Bảng lương **khóa** sau khi duyệt.
8. **Contract-first API + client sinh tự động.** OpenAPI là hợp đồng giữa backend và frontend. Client TypeScript được sinh từ đó, nên backend đổi field thì frontend báo lỗi type ngay khi build.
9. **Tính năng mới có cờ bật/tắt (feature flag)** theo tenant: đặt lịch online, bán hàng, nhắc Zalo... Nhờ vậy có thể bật dần mà không cần nhánh code riêng.
10. **API-first, đa kênh.** Web app, native app sau này và các tích hợp (Zalo, đặt lịch online) đều dùng chung một Rust API, chung tài khoản và phân quyền. Không có logic nghiệp vụ nào chỉ nằm ở frontend. URL của web app ổn định để làm deep link (universal links / app links) khi có native app.

## Hiện trạng trong code (kiểm 2026-11-09)
Các nguyên tắc trên là định hướng từ ngày đầu; chỗ code đi khác hoặc chưa tới:
- **#1:** danh sách module ở trên là bản dự kiến. Crate thật (`apps/api/crates/`): `kernel`, `identity`, `org`, `hr` (thay cho `workforce` + `payroll`), `catalog`, `crm`, `packages`, `sales`, `operations`, `reporting`, `platform`, `app`; chưa có `ledger`, `notifications` ([ADR-0006](../decisions/ADR-0006-repo-and-module-layout.md), [backend §5](backend.md)). Ngoại lệ có chủ ý của "không đọc bảng của module khác": `reporting` là read model, đọc thẳng các bảng sổ.
- **#2:** đã làm: permission là chuỗi trong `kernel::permission`, role là dữ liệu (`role_permission`), mẫu role do admin nền tảng sửa (`role_template_permission`, ADR-0018). Phạm vi "của chính mình" có (`staff.read_own`, `/me/*`); phạm vi theo **chi nhánh** chưa có (mới một chi nhánh).
- **#4:** khớp với nguyên tắc (đã sửa theo ADR-0019 D5): một `payroll_rules` mỗi spa cộng bản chụp phiếu lương.
- **#5:** đã làm: `hr::calc::compute` là hàm thuần, test theo ô Excel.
- **#9:** đã có cờ theo spa (`app.tenant_feature`, `kernel::feature`, ADR-0018 D4), do admin nền tảng bật/tắt.
