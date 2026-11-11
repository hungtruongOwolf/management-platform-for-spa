---
title: ADR-0019 Quyết định của lead cho các câu hỏi mở sau rà soát 2026-11-09 (EM ủy quyền)
status: accepted (lead decision under EM delegation 2026-11-09; EM có thể xem lại từng mục)
date: 2026-11-09
deciders: lead (EM ủy quyền: "cứ đúng nhất mà làm")
---

# ADR-0019: Quyết định cho câu hỏi mở của đợt rà soát 2026-11-09

## Context
- Rà soát toàn bộ ([audit-2026-11-09](../plans/audit-2026-11-09.md)) để lại khoảng 30 câu hỏi về nghiệp vụ và UX.
- EM ủy quyền cho lead ([raw](../../raw/2026-11-09-em-delegates-audit-decisions.md)): "chỗ nào nên sửa thì bạn sửa cho tôi đi cứ đúng nhất mà làm". Repo public/private để sau.
- Nguyên tắc lead dùng khi quyết:
  1. Không âm thầm làm mất tiền hoặc dữ liệu. Server từ chối rõ ràng tốt hơn ghi sai.
  2. Theo [compensation-rules](../product/compensation-rules.md) khi nó đã nói rõ.
  3. Theo thông lệ kế toán và tiền lương ở Việt Nam khi nó chưa nói.
  4. Bảo vệ dữ liệu cá nhân theo Nghị định 13/2023/NĐ-CP.
  5. Ít thao tác hơn cho người vận hành.

## Decision

### Lương (crate hr)
- **D1. Chốt lương bị chặn khi thiếu dữ liệu.** Hiện chỉ chặn khi thiếu chấm công. Từ nay chặn thêm khi:
  - nhân viên đang làm trong tháng chưa có điều khoản lương;
  - nhân viên cần đánh giá tháng chưa có đánh giá.

  Trả `409` kèm danh sách người còn thiếu.
  Lý do: nếu không chặn, tháng bị đóng băng với `tet_amount = 0` và không có cách sửa.
- **D2. Thưởng/phạt hoặc hiệu chỉnh cho tháng người đó không làm việc ngày nào:** server từ chối với `422 not_employed`. Không nhận rồi để rơi mất nữa.
- **D2b. Người chốt gói đã nghỉ việc, sau đó gói được hoàn tiền:**
  - Tháng lương của họ còn mở: trừ vào cơ sở hoa hồng của tháng đó, theo ADR-0010.
  - Tháng đã chốt: không thu hồi, spa chịu.

  Cả hai trường hợp đều được ghi audit và hiện trong báo cáo, không bị bỏ qua âm thầm.
- **D3. Lương thực nhận không bao giờ âm.**
  - Khoản trừ ngày nghỉ tối đa bằng lương cơ bản của tháng.
  - Phụ cấp ăn tính theo số ngày đi làm thực tế (0 ngày thì 0đ).
  - Riêng phần phụ cấp: nếu compensation-rules ghi phụ cấp là khoản cố định tháng thì theo compensation-rules, và ghi vào báo cáo.
- **D4. Vào làm giữa tháng (Q-S4).** Lương cơ bản tính theo tỷ lệ: `lương cơ bản × số ngày thuộc thời gian làm việc / số ngày trong tháng`, làm tròn đến nghìn đồng. Quy tắc 2 ngày nghỉ miễn trừ áp dụng như cũ. "Tháng làm việc" để tính thưởng gắn bó: tháng có từ 15 ngày làm việc trở lên mới được tính.
- **D5.** Giữ một `payroll_rules` cho mỗi spa cộng snapshot trong payslip. Snapshot đã giữ lịch sử của từng tháng, nên chưa cần `CompensationPolicy` có `effective_from`. Sửa [principles](../architecture/principles.md) #4 cho khớp.

### Bán hàng, lượt khách, báo cáo
- **D6. Hoàn tiền gói mua cho người khác:** khoản hoàn ghi cho **người trả tiền**, tức chi tiêu của họ giảm. Người dùng gói không có dòng âm.
- **D7. Báo cáo kỳ:**
  - Gói từ sổ cũ không có mẫu được gom theo tên gói.
  - Doanh thu dùng **số tiền thực trả**, không dùng giá niêm yết.
  - Gói bị hoàn sau đó tính theo số ròng: số bán trừ số hoàn. Cụ thể (lead 2026-11-09): doanh thu trừ số tiền đã hoàn; gói **hoàn toàn bộ** không còn được đếm vào "đã bán" (cả xếp hạng gói lẫn tổng `packages_sold`), gói hoàn một phần vẫn đếm.
- **D8. "Sửa lượt":**
  - Giữ gói đã bán trong checkout gốc, cả trên lượt mới và lịch hẹn.
  - Giữ **giá gốc** của các dòng; không tính lại theo bảng giá hôm nay.
  - Thêm dòng vào một ngày đã qua thì ghi tiền vào ngày đó (giữ như hiện tại). Ngày đã khóa thì vẫn theo quy tắc khóa ngày.
  - Đổi KTV của một dòng trong tháng đã chốt lương: từ chối với `409 payroll.period_closed` (mã đã dùng sẵn trong code và web cho tháng đã chốt; lead 2026-11-09 giữ mã này thay cho `payroll.month_closed` ghi lúc đầu).
  - Đổi sang gói: giảm giá đã có từ trước giữ nguyên, không kiểm lại theo hạn mức của người đổi. Chỉ phần giảm giá **mới** bị kiểm. Ngoại lệ này **chỉ** áp cho "Chuyển sang gói"; "Sửa lượt" thường vẫn kiểm toàn bộ giảm giá theo hạn mức của người sửa.
  - `discount_reason` hiện đủ lý do của mọi hóa đơn.
- **D9.** `PATCH /sales/{id}` với `{}` không được xóa ghi chú gói. Thiếu trường nghĩa là giữ nguyên; muốn xóa phải gửi `null` rõ ràng.
- **D10.** Thêm optimistic lock (`If-Match`, version) cho `PUT /staff/{id}/pay`, `PUT /payroll/rules` và `PATCH /sales/{id}`, web gửi kèm version.
- **D11.** `POST /customers` bắt buộc `Idempotency-Key`. Bấm đôi chỉ tạo một hồ sơ.
- **D12.** Siết CHECK SĐT trong DB cho khớp code: chỉ đầu số di động. Trước khi chạy migration, lead kiểm dữ liệu staging; dùng NOT VALID rồi VALIDATE.

### Dữ liệu cá nhân
- **D13. Audit khách không lưu PII rõ nữa.** *(Sửa một phần bởi [ADR-0021](ADR-0021-activity-log-for-the-owner.md), EM 2026-11-11: `customer.update` giữ tên + SĐT cũ → mới khi đổi; tạo khách và ngày sinh vẫn như D13.)*
  - Thay tên, SĐT và ngày sinh bằng cờ (`has_phone`, `has_birth_date`) cùng danh sách trường đã đổi.
  - Migration xóa PII khỏi các dòng audit cũ.
  - Ai đổi gì, lúc nào vẫn được giữ.

### Lịch hẹn và nhân viên
- **D14.** Quản lý đã gắn hồ sơ có cột trên lịch hẹn và có trong picker KTV, giống KTV. EM đã quyết là quản lý cũng làm cho khách. Cột được chọn theo **role của tài khoản** (technician hoặc manager), không theo chức danh. Chủ spa không có cột, trừ khi hồ sơ có đánh dấu "nhận khách".
- **D15.** Không còn xóa `POST /opening-balances`: M8 được bỏ, vì route đã được mở rộng có chủ ý.

### Bảo mật và vận hành
- **D16.** Chưa làm rate limit theo user. Rate limit theo IP cộng khóa theo tài khoản là đủ ở quy mô hiện tại.
- **D17.** Lượt tìm tài khoản xuyên spa của operator được ghi vào `platform_audit_log`, kèm chuỗi tìm đã che bớt. Escape `%` và `_` trong `LIKE`.
- **D18.** Kiểm mật khẩu yếu bằng danh sách mật khẩu phổ biến nằm trong binary, không gọi dịch vụ ngoài.
- **D19. Console nền tảng (Q-PA1…4):**
  - Audit spa chỉ hiện metadata; chưa làm support grant.
  - Chưa có cờ `packages`.
  - Subdomain `admin.` sẽ dùng khi có domain chính thức.
  - Giữ quy tắc "không khóa chủ spa cuối".

### UX web
- **D20.**
  - Ẩn "Đặt lịch" khi đã qua giờ đóng cửa hôm nay.
  - Kỳ báo cáo tự chọn theo đúng giới hạn API: 5 năm cho chủ spa.
  - Audit spa hiện nhãn tiếng Việt thay mã action.
  - Cột "Tài khoản" trong roster hiện SĐT đăng nhập.
  - Sheet tự đóng sau khi bấm "Cho nghỉ việc".
  - Đặt lại MFA vẫn làm được với tài khoản đang khóa.
  - Thêm skip-link tới nội dung chính.
  - `theme-color` theo theme đang chọn trong app.
  - Command palette cuộn tới mục đang chọn.
  - Thêm dịch vụ 0đ không cần `payment.collect`.
  - Nút "Không đến" tự cập nhật khi đã qua giờ hẹn.
  - Xóa 3 test chụp ảnh review cũ.
- **D21.** Ngân sách startup JS 125 KiB tính file entry **cộng** mọi modulepreload; script đo sửa cho khớp.

### Giữ nguyên (không đổi)
- Cửa sổ nhắc đánh giá tháng giữ như code hiện tại: cuối tháng đến ngày 10. Lý do: rộng hơn ngày 1–5 nên không làm ai lỡ hạn.
- Postgres local vẫn là 16 vì đổi phải tạo lại DB local. CI và staging chạy 17.
- Workflow deploy VPS giữ lại làm đường production dự phòng (ADR-0005).
- Chưa áp formatter cho toàn repo.
- Các dòng lượt cũ đã ghi 0đ do bug #125 không được sửa lại: chúng là dữ liệu demo hoặc staging.

## Consequences
- Có thay đổi contract:
  - `If-Match` cho `PUT /staff/{id}/pay`, `PUT /payroll/rules` và `PATCH /sales/{id}`;
  - `Idempotency-Key` cho `POST /customers`;
  - `409` mới khi chốt lương.

  Web cập nhật trong cùng PR.
- Có migration mới: CHECK SĐT, xóa PII khỏi audit khách và các thay đổi khác nếu cần. Lead chạy chúng trên local và staging sau khi kiểm dữ liệu.
- EM có thể đảo bất kỳ mục nào. Mỗi mục đều độc lập.
