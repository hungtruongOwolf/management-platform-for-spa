# EM answers — 2026-11-05: round-4 polish questions (PR #39)

Verbatim (EM, chat, replying to the lead's 4 questions):

> 1. "Đã chi" của khách chuyển từ sổ giấy: có cần cộng cả số tiền khách đã trả ngoài giấy không? Nếu có thì phải lưu thêm số tiền đó khi nhập dữ liệu cũ. ok
> 2. Icon trên tab trình duyệt: ... dùng bản rút gọn (hoa sen + vòng tròn) chỉ cho icon tab, còn mọi chỗ khác giữ logo đầy đủ không? ok
> 3. Thanh tab dưới cùng trên điện thoại: khi có lịch hẹn, có nên thay "Trang chủ" bằng "Lịch hẹn" không? ok
> 4. Tên dịch vụ: giữ cách viết hoa như trên menu in (ví dụ "Gội đầu Thư Giãn") hay đổi thành "Gội đầu thư giãn"? như trên menu nhé

## Decisions
1. The paper-book migration records the amount the customer already paid on paper (`carried_over_paid`). "Đã chi" on the customer summary includes it, shown as its own part.
2. The browser-tab favicon (16/32/48 px) uses a simplified mark: lotus and ring. The full logo stays everywhere else, including the PWA/app icons and the apple-touch icon.
3. The phone tab bar shows "Lịch hẹn" instead of "Trang chủ" once appointments ship.
4. Service and plan names keep the printed menu's capitalisation (proper names). UI copy stays in sentence case.
