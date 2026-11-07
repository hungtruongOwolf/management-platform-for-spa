# EM decisions — 2026-11-07: HR rights, leaver login, visit-note visibility, group booking, server region

The EM answered the open questions after PRs #78, #79, #81 and #82. Verbatim:

> 1. Nhân sự: quản lý có được thêm KTV hoặc cho KTV nghỉ việc không? (Hiện chỉ xem.) không nhé chỉ spa mới có quyền edit
> 2. Nhân sự: người đã nghỉ có được đăng nhập để xem phiếu lương cũ không? (Hiện bị khóa ngay khi nghỉ.) có
> 3. Ghi chú buổi có thể chứa thông tin sức khỏe. Hiện chỉ chủ spa và quản lý xem được. Bạn đồng ý không? không thêm cả KTV đi vì họ có thể xem làm cho ai mà chỉ là không được xem thông tin cá nhân của khách ngoài giới tính và tên thôi
> 4. Giỏ chỉ có gói, không có dịch vụ: mỗi lần lưu bán được 1 gói. Có cần bán nhiều gói một lần không? cụ thể hơn có được không
> 5. Đặt lịch nhóm:
>    - Có cần nút hoàn tác "đã đến" không? ok
>    - Có bỏ bắt buộc SĐT khi đặt lịch cho khách mới không? không cần
>    - Đổi giờ lịch của người đặt thì cả nhóm có đổi theo không? có thể cho phép đổi nếu đi cùng nhau hoặc có thể chỉnh riêng
> 6. Chuyển server sang Singapore: bạn chọn A / B / C? (Mình đề xuất B.) vì tôi đang ở mỹ tôi chọn test server ở mỹ là đúng rồi sau này tôi public nó as product sử dụng chính thức và chạy prod thì mới cho chạy ở châu á

## Lead's reading
1. **Staff edits are owner-only.** Only the owner adds, edits, ends or rehires staff. Managers read the roster. This is the current default, now confirmed (Q-S5).
2. **A leaver keeps a login to see their own past pay** (Q-S6).
   - Ending employment no longer locks the login outright. It downgrades it to read-only access to their own past attendance and payslips for the months they were employed.
   - No other screen: no customers, appointments or Quầy, and no "Khách của tôi" for months after leaving.
   - Rehiring restores the role.
3. **Technicians see the session note on the visits they did**, e.g. in "Khách của tôi" and the day detail. They still see no other customer personal data: only name and gender. No phone, birth date, address, health profile or profile notes.
4. **Not decided.** The EM asked for a more concrete explanation of "a cart with only packages saves one package at a time" before deciding.
5. **Group booking:**
   - (a) Add "undo arrival" for a mistaken "Chỉ báo đã đến".
   - (b) Keep the phone required when booking a new customer.
   - (c) When moving a group member's booking, offer a choice: move the whole group together, or only this person.
6. **Server region.** Staging stays in the US (pdx1 + Supabase us-west-2) because the EM tests from the US. Production, when the product is launched for real, runs in Asia (Singapore). ADR-0012 stays Proposed, as the production plan.

## Follow-up — item 4 (same day)
The lead explained item 4 with an example: chị Lan buys 2 packages and no service (one for herself, one as a gift for her mother). Today that is 2 saves, 2 invoices and 2 payments. New behaviour: both go in the cart, one save, one invoice, one payment for the total.

EM, verbatim:

> ukm theo cái mới đi
