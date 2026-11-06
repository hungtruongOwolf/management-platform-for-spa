# EM review round 5 — 2026-11-06 (local preview after #37/#38/#41)

Verbatim (EM, chat), with two screenshots: (1) `/quay` "Lượt khách hôm nay" list on desktop, every row has "Chi tiết" + "Hủy" buttons, columns misaligned; (2) left sidebar where long labels are cut off ("Danh sách khách hàng", "Khách lâu chưa quay lạ…", "Gói liệu trình của khác…") and a horizontal scrollbar appears.

> bạn thấy cái table này nó có ghê không, không hàng không lối không một cấu trúng gì nó rất tệ ấy bạn hiểu không cái chi tiết và hủy thì để vào trong cái ... rồi lúc nào cần thì mở ra xem hoặc không thì bỏ cái chi tiết đi mà bấm vào từng row là nó tự hiện ra được như thế user behavioral ok hơn, tiếp đến là cái navbar bên trái bạn làm sao để nó không bị nằm sâu bị mất chữ chứ ?
>
> rồi cái đoạn lịch hẹn tôi bảo ấy nó không chỉ là lịch hẹn mà nó cũng có thể lưu lịch sử làm của ghi lượt khách nữa cái đó phải sync với nhau thì với biết được ai đang làm ai chưa làm chứ bạn design mỗi để đặt lịch sao được, với trong chỗ đặt lịch mới cho khách bạn để là thời lượt theo dịch vụ nhưng chọn dịch vụ thì lại thời gian đâu có tự update theo như dịch vụ đâu chỗ này hơi fail nhé, với giả sửa khách đi gia đình đặt lúc cho 2,3 người thì chưa thấy đâu case đó được thiết kế như thế nào nếu mà cứ để 2,3 người đến thì nhập 2,3 lần thì đâu còn của 1 gia đình nhập nhanh nữa dẫn đến mấy giờ cao điểm là toang
>
> [console] :5173/api/v1/me:1 Failed to load resource: the server responded with a status of 401 (Unauthorized)
> bị lỗi này
>
> với tôi chọn đủ infor xong bấm đặt lịch nó không được
> [console: a Chrome extension's "[WindowMode] detectAndStoreWindowModeAvailability" log]
> nó cứ bị gì ấy check kĩ lại xem nào với tôi không hiểu bạn để 5,5,5,5,5 rất nhiều trong UI calendar và giờ bắt đầu để làm gì tôi đâu cần đâu cái tôi cần biết trong khi giờ đấy có bao nhiêu người rồi còn max bao nhiêu cái đó spa tự biết không cần hardcode lỡ sau này nhiều nhân viên hơn thì sao, để sau này thêm tính năng kiểu hôm nay có bao nhiêu kĩ thuật viên đi làm tì có thể nhận được bao nhiêu khách trong 1 khung giờ thì cái đó để chủ spa tự nhập còn không thì thôi
>
> với vụ Khách lâu chưa quay lại bạn để cái button cái phone đấy chúng ta đâu dùng đến đâu bỏ nó đi tạm thời không cần chỉ cần đánh dấu là được rồi
>
> với trong cái nhân viên ấy bạn chia ra 3 phần đó là chấm công đánh giá và thưởng phạt tôi thấy hơi khó chịu, có nhiều người không biết phải điền đủ 3 phần có thể làm cho nó thành 1 page bấm vào tự điền hay có thê nghiên cứu lại flow được không hơi tệ
>
> với cái có công và lương rồi nhưng mà ngày công bạn thiết kế nó như một list nhưng mà nếu làm tận 30 ngày nó là một cái sớ siêu dài và rất khó xem sao không để thành một cái table hay như thế nào đó để dễ theo dõi
>
> với sao account ktv không có tính năng đó là xem đã làm cho những ai, ai đặt lịch mình vậy, như lịch sử luôn ấy tôi thấy thiếu feature này
>
> check kĩ mọi thứ rồi soạn subagent ra sửa cho tôi

## Lead's notes
- The `/me` 401 is the normal "not signed in yet" probe before login (not a bug); the WindowMode line comes from a browser extension, not the app.
- `POST /appointments` works when called directly (lead verified 2026-11-06 with the demo manager), so "đặt lịch không được" is a web-side bug to reproduce in the browser.
- Items: (1) visits table → aligned columns, row click opens details, actions in a "⋯" menu; (2) sidebar labels never truncated; (3) calendar and visits are one timeline: a booked appointment becomes the visit, walk-ins appear too, and the calendar shows who is waiting / in service / done; (4) duration follows the chosen services; (5) group/family booking in one entry, check-in all together; (6) no hard-coded per-slot capacity: show how many people are booked per slot, optional max set by the owner; drop the start-time clutter; (7) fix booking submit; (8) inactive customers: remove the phone/call button, keep "mark contacted"; (9) staff: one guided daily flow instead of three tabs; (10) attendance month as a grid/table; (11) technician sees own history (customers served, who is booked with them).
