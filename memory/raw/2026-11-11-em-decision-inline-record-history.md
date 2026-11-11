# EM — hiện "ai làm" ngay trên bản ghi (có công tắc), tạm ẩn màn Nhật ký thao tác (2026-11-11)

Bối cảnh (tóm tắt đề xuất của lead, trước lời EM):
- Trước đó cùng ngày EM quyết "ai làm" chỉ cần có data để truy vết, không hiện trên các màn khác ([raw](2026-11-11-em-actor-trace-data-only.md)); thay vào đó có màn "Nhật ký thao tác" chỉ chủ spa (`/nhat-ky-thao-tac`, PR #180, ADR-0021), một thao tác một dòng (PR #182, ADR-0021 D9).
- Lead đề xuất thêm: trên mỗi bản ghi hiện một dòng nhỏ, màu nhạt, kiểu "Ghi bởi Hà · 15:10 · đã sửa 1 lần"; bấm vào thì mở lịch sử của chính bản ghi đó (ai, lúc nào, đổi gì, lý do), mặc định đóng.
- Chỗ đặt đề xuất: lượt (Quầy, hồ sơ khách), hóa đơn trong Sổ doanh thu, gói của khách, lịch hẹn, hồ sơ khách (tên / SĐT cũ → mới), ô chấm công + khen / phàn nàn, bảng giá / mẫu gói / quy tắc lương.
- Danh sách chỉ hiện dấu nhỏ ("đã sửa", "đã hủy"); tên người và lịch sử đầy đủ chỉ ở phần mở chi tiết.

Lời EM (nguyên văn):

> 1. [...] nếu có thể control được thấy bởi quản lý, hoặc bơi chủ spa và bật tắt được thì làm đi cho tôi làm kiểu có thể bật tắt ấy và default là hiện cho chủ spa. nhưng trước khi làm thì cần phân tích có nào cần add, chỗ nào không UI mọi thứ thế nào nhé đảm bảo ổn hết rồi mới làm review đi review lại kĩ nhiều lần và không cần hỏi ý kiến của tôi

> 2. [...] nó là công cụ phụ cứ giữ nhưng tạm thời ẩn ở chủ spa đi là bằng cách tắt từ admin bạn làm như thế nào đó cũng được như off show cái nhật kí thao tác đi

> 3. [...] danh sách chỉ hiện một dấu nhỏ như "đã sửa" hoặc "đã hủy", còn tên người và lịch sử đầy đủ để ở chỗ mở chi tiết. ok cũng được

Làm rõ của EM (cùng ngày, chuyển qua coordinator, nguyên văn):

> công tắc bật tắt là admin control nhé chơ không phải ở account của spa và quản lý đâu

Ghi chú: quyết định này **đảo** quyết định "không hiện ai làm trên các màn khác" của raw `2026-11-11-em-actor-trace-data-only.md`.
