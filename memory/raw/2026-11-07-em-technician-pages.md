# EM feedback 2026-11-07 — màn của KTV (đăng nhập bằng tài khoản KTV, giao diện tối)

Nguồn: tin nhắn của EM, kèm 3 ảnh chụp màn hình (ảnh không lưu vào git).

## Nguyên văn

> [Image #54] cách sắp xếp nội dung và mọi thứ chưa hợp lý [Image #55] UI xấu không cân đối [Image #56] tôi nghĩ hiện như google calendar giống trong chủ spa thì UI ổn hơn đó

Yêu cầu thêm (cùng PR, chuyển qua lead):

> trong cái công và lương của tôi có thể xem chi tiết hơn ngày hôm đó như thế nào có được không nhỉ

## Mô tả ảnh

- **Ảnh 54** — "Công và lương của tôi", phần cuối phiếu lương: bảng "Dịch vụ đã làm" một danh sách phẳng xếp theo giá trị, cột Dịch vụ / Loại / Lượt / Giá trị; cùng một dịch vụ ("An Kiên Lạc", "Thư Giãn") hiện hai lần vì khác Loại (Dịch vụ lẻ / Buổi liệu trình); các cột cách nhau rất xa. Bảng "Giới thiệu / chốt được" một dòng gói. Dòng "⚠ Chưa đánh giá tháng" nằm lẻ loi ở cuối trang. Không có tổng nhóm, không thấy số tiền thành hoa hồng thế nào.
- **Ảnh 55** — khối "Ngày công": 5 ô tổng (Đi làm 7 ngày, Đi muộn 1 lần · 14′, Nghỉ P/ĐX 0/0, Nghỉ K 0, Khen/Phàn nàn 6/1), dưới là lịch tháng T2…CN: ô rộng và thấp, số ngày góc trên trái, dấu ✓ ở giữa phía dưới, ngày đi muộn có gạch đỏ dưới đáy, hôm nay viền vàng, ngày tương lai là ô nền đậm trống; dòng chú thích chữ ở cuối. EM: xấu, không cân đối.
- **Ảnh 56** — "Khách của tôi": card "Lịch hẹn sắp tới" (trạng thái trống) và card "Khách đã làm · Tháng 11/2026" là danh sách dài nhóm theo ngày (giờ, tên khách, dịch vụ). EM muốn hiện như Google Calendar giống màn Lịch hẹn của chủ spa.

## Yêu cầu ngày chi tiết (lead diễn giải)

Bấm một ngày trên lịch Ngày công → xem chi tiết ngày đó (sheet trên desktop, bottom sheet trên điện thoại, dùng được bằng bàn phím, có ngày trước / ngày sau): chấm công (trạng thái, giờ vào/ra, phút muộn, ghi chú), khen / phàn nàn, thưởng / phạt của ngày, lượt đã làm (giờ, dịch vụ, lẻ / buổi liệu trình, giá trị, hoa hồng nếu tính được), gói đã chốt / giới thiệu trong ngày. Chỉ dữ liệu của chính KTV, không SĐT khách; server lấy người từ session.
