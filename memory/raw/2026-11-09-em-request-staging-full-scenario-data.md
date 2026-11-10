# EM request — xóa dữ liệu rác trên staging, đổ một kịch bản đầy đủ 12 tháng (2026-11-09)

Nguồn: brief của lead giao cho agent (lead chuyển yêu cầu của EM; file raw được agent thêm vì brief nhắc tới file này nhưng nó chưa có trong `memory/raw/`). Ghi lại theo brief, không diễn giải thêm.

## Mục tiêu
Staging bị xóa sạch dữ liệu nghiệp vụ rác và đổ lại **một kịch bản hoàn chỉnh, giống thật, 12 tháng lịch sử**. Mọi màn hình, mọi vai trò đều có dữ liệu có ý nghĩa. Lead tự chạy hai lệnh trên staging; agent build và kiểm ở local.

## 1. Lệnh mới `app reset-demo --tenant <slug>`
- Từ chối khi `APP_ENV=production` (giống `seed-demo`).
- Một transaction, chạy bằng schema owner (`DATABASE_URL`), xóa **toàn bộ dữ liệu nghiệp vụ của spa đó**: khách, ghi chú, lần liên lạc; lượt, dòng lượt, hóa đơn, dòng hóa đơn, thanh toán, hoàn tiền; gói của khách + item, gia hạn, hủy; lịch hẹn; nhân viên, đợt làm việc, chấm công, khen/phàn nàn, đánh giá, kỳ lương + phiếu lương; yêu cầu quên mật khẩu; `audit_log` của spa; mọi thứ khác theo spa là dữ liệu nghiệp vụ.
- Trigger append-only sẽ chặn: xử lý trong transaction (ví dụ `ALTER TABLE … DISABLE TRIGGER USER` rồi bật lại trước commit; owner sở hữu bảng). Không dùng `session_replication_role` (Supabase không có superuser).
- **Giữ:** `tenant` (kể cả `owner_mfa_required`), `branch`, role, role permission, `tenant_feature`; danh mục (dịch vụ, nhóm, giá, mẫu gói + item); `user_account`, `membership`, `membership_role` (đăng nhập chủ spa + quản lý và MFA); mọi thứ `platform_*`. `payroll_rules` / settings: giữ nếu là cấu hình chứ không phải lịch sử — agent quyết và ghi lại.
- In số dòng đã xóa theo từng bảng.
- Test Rust: seed, reset, kiểm bảng nghiệp vụ trống cho spa đó, bảng giữ lại không đổi, dữ liệu spa khác không bị đụng.

## 2. Nâng `seed-demo` thành kịch bản đầy đủ
Giữ nguyên tắc đi qua use case thật (validation, audit, payment append-only).
- **Dùng lại tài khoản có sẵn:** đăng nhập quản lý (`0905000002` trong runbook) và chủ spa, không tạo trùng.
- **Một đăng nhập KTV**, mật khẩu từ env (ví dụ `APP_NEW_TECHNICIAN_PASSWORD`), không bao giờ qua argv.
- **Nhân viên:** khoảng 5–6 KTV + hồ sơ nhân viên của quản lý gắn với đăng nhập quản lý. Đăng nhập KTV gắn với một người. Một KTV **nghỉ giữa tháng trước và có đăng nhập** (màn quá khứ ADR-0014 có dữ liệu), mật khẩu cũng từ env. Một KTV vào làm khoảng 4 tháng trước.
- **Lịch sử nhân sự:** 12 tháng chấm công mỗi ngày làm việc, gồm nghỉ phép, nửa ngày, đi muộn và các trạng thái quy tắc có; khen/phàn nàn (100 điểm) và đánh giá tháng; kỳ lương: 11 tháng trước **đã chốt** có phiếu lương, tháng này mở. Hoa hồng, thưởng gắn bó, quy tắc "tháng không trọn" đều phải xuất hiện.
- **Khách:** khoảng 150–250, tên Việt, SĐT trong dải demo dành riêng. Có: khách quen và khách có gói; khách lâu chưa quay lại (vắng 60–180 ngày) có lần liên lạc + ngày hẹn gọi lại; khách vãng lai không tên, khách không SĐT (ADR-0011); khách không ghi giới tính; sinh nhật tháng này; ghi chú sức khỏe mã hóa trên một phần.
- **Gói:** đủ mọi trạng thái màn hình lọc: đang dùng, sắp hết hạn (≤ 7 ngày), hết hạn, hết buổi, hủy + hoàn tiền, đã gia hạn; 2–3 gói nhập sổ giấy (đường `import-paper-book`).
- **Bán hàng:** tiền mặt, chuyển khoản, cả hai; giảm giá có lý do; một hóa đơn bị hủy + bán lại; bán gói kèm trong lượt.
- **Lượt:** khoảng 4–8 lượt/ngày trong 12 tháng, cuối tuần khác ngày thường; ghi chú buổi trên một số lượt; vài lượt hủy và vài lượt "Sửa lượt"; **lượt hôm nay** để `/quay` có dữ liệu, một số đang làm nếu có trạng thái đó.
- **Lịch hẹn:** quá khứ (hoàn tất gắn với lượt, không đến, hủy có lý do), hôm nay (sắp tới + một quá giờ), 2 tuần tới, một lịch nhóm / gia đình.
- **Một yêu cầu "Quên mật khẩu" đang chờ** để Tổng quan của chủ spa có.
- **"Hôm nay":** mọi thứ tính theo ngày chạy; báo cáo có đủ một năm để so sánh.
- **Tất định:** RNG có seed, cùng ngày chạy ra cùng dữ liệu.
- **Tốc độ trên staging:** qua Supabase session pooler ~150 ms mỗi round trip; 12 tháng ≈ 2.000+ lượt, chạy tuần tự có thể mất hàng giờ. Đếm round trip mỗi lượt ở local; chạy song song phần độc lập khi an toàn (khách khác nhau, ngày khác nhau, nhiều kết nối pool), giữ đúng quy tắc nghiệp vụ. Mục tiêu **< 45 phút** ở 150 ms RTT, báo ước tính kèm phép tính. Thêm `--months N` mặc định 12.
- Giữ guard "đã có dữ liệu demo"; sau `reset-demo` seed phải chạy sạch. In tóm tắt khi xong.

## 3. Test và tài liệu
- `demo_seed.rs` chạy với `--months` nhỏ, kiểm mọi nhóm kịch bản ở trên có mặt. Chạy reset + seed đủ 12 tháng trên DB riêng ở local, báo số lượng và thời gian.
- Runbook mục "Dữ liệu demo": lệnh mới, tên biến env (chỉ tên), những gì được tạo, quy trình reset (backup trước bằng `pg_dump`).
- `memory/index.md` nếu trang đổi, thêm mục `memory/log.md`.

## Ràng buộc
RAM ít (`CARGO_BUILD_JOBS=4`, không để server chạy, không chạy cả bộ e2e); DB local riêng, không dùng DB dev chung `vpyd`; không bí mật trong git / argv, không in mật khẩu, repo public nên không hostname / project ref; dải SĐT demo là dải giả; không force-push, merge chỉ qua `scripts/merge-when-green.sh`; **không đụng staging** (không reset, không seed, không deploy).
