# EM request — bật/tắt MFA chủ spa từ /admin (2026-11-09)

EM hỏi (nguyên văn): "bên admin có chứ năm bật tắt MFA kiểu bước authen thứ 2 của chủ spa không có thể implement cái này tôi với được không"

Lead trả lời: chưa có. Hiện MFA của chủ spa là bắt buộc cứng (`role_owner_requires_mfa`, ADR-0004); /admin chỉ có "Reset MFA". Lead nêu rủi ro (chủ spa thấy ghi chú sức khỏe, lương, quyền hoàn tiền) và đưa 3 hướng:
1. Công tắc theo từng spa trong /admin, mặc định BẬT, tắt cần step-up + lý do + nhật ký; bật lại dùng tiếp authenticator cũ (khuyến nghị).
2. Công tắc theo từng tài khoản.
3. Giữ bắt buộc, thêm "nhớ thiết bị 30 ngày".

EM chọn: **1. Công tắc theo từng spa.**
