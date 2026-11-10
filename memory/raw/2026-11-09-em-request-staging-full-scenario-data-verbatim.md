# EM request — test lại 3 vai trò trên staging + dọn data, seed kịch bản đầy đủ (2026-11-09)

EM (nguyên văn): "tôi đã tắt mfa trên staging rồi bạn có thể đăng nhập test lại 3 loại account KTV, quản lý và chủ spa có được không bên cạnh đó bạn giúp tôi là clean data chỉ để lại những case đủ nhất make full scenario chứ đừng có tạm bợ như hiện tại một đống test data rác"

Hiện trạng staging lúc hỏi: 47 khách, 122 lượt, 95 hóa đơn, 322 dòng audit; demo seed cũ 4 ngày (/quay hôm nay trống); không có tài khoản KTV; gần như không có chấm công / đánh giá / lương.

Lead đề xuất, EM chọn:
- **Phạm vi dọn:** "Đồng ý, xóa + seed lại". Backup toàn bộ DB staging ra file trước. Xóa hết dữ liệu nghiệp vụ (khách, lượt, hóa đơn, gói của khách, lịch hẹn, nhân viên, chấm công, lương, nhật ký spa). Giữ tài khoản chủ spa / quản lý / admin (kèm MFA và công tắc MFA chủ spa EM vừa tắt), menu 35 dịch vụ, 5 mẫu gói. Nâng cấp lệnh seed để đổ một bộ kịch bản đầy đủ.
- **Lịch sử:** **12 tháng**.
