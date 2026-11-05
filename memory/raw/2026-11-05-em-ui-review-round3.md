# EM feedback — 2026-11-05: UI review round 3 (after PR #29 local preview)

Verbatim (EM, chat, with 4 screenshots: login emblem, sidebar logo, customer search focus ring, home "Hôm nay" tiles):

> bị lặp chữ hồi sinh sức khỏe và sắp đẹp này trong logo có sẵn rồi bỏ cái tự thêm đi [login screenshot] logo bị sai trong trang chính mày và nhiều chỗ khác cũng sai logo lắm làm cho đúng đi cứ để logo nguyên còn bạn muốn để thêm chữ viên phúc y đường ngoài thì tùy thôi đừng có bỏ ra
>
> trong báo cáo tôi thấy chỉ có báo cáo 7 ngày gần nhất tính từ ngày được chọn nhưng sau này tôi muốn báo cáo nó nhiều tùy chỉnh hơn như 30 ngày của 1 tháng, 1 năm kiểu vậy bạn thiết kết lại sao cho chỉnh chu và nó linh hoạt được đi mà nếu biểu đồ về xu hướng thì thay vì vẽ theo line graph ok hơn đó
>
> giả sử gói liệu trình unlimited hết hạn thì sao không có giới hạn thì chúng ta đã cover case này chưa
>
> với hiện tại khác không quay lại cả khách lẻ kiểu đăng kí theo buổi 1 chứ không phải dịch vụ cũng care và cả khách liệu trình từ buổi cuối chưa làm cũng phải care nhé
>
> tuy nhiên các route từ các trang này qua trang khác mọi thứ chưa hợp lý kiểu tôi bấm vào để xem infor chi tiết rồi nhảy qua trang khác muốn quay về xem thì không biết làm thế nào, cần nghiên cứu kĩ lại vụ này thật chuẩn chỉ
>
> bên cạnh đó cái lúc vào chi tiết từng hồ sơ khách giả sử sau này nhiều data muốn search trong đó thì có option nào không nhỉ
>
> với giới tính thì thường để bên cạnh tên luôn cho dễ phân biệt nhé [focus-ring screenshot] với đây cái vụ tôi bảo hiệu ứng bấm vào nó hiện cái link bị khó nhìn này che đi chữ ở trên
>
> tiếp đến là responsive nếu tăng giảm kích thước trang bạn nhớ làm sao để đừng có bị lỗi nhé check kĩ lại
>
> state management cũng phải làm
> [home tiles screenshot] mấy cái này check kĩ lại từ đầu đến cuối đừng có làm thừa thãi như này điều chỉnh cho đúng kich thước
>
> với như tôi bảo ấy trong từng loại page như "Theo dõi gói" nếu muốn search từ trong đó thì sao có cách nào chưa có search thì mỗi page phân tích lại cái nào cần thì add thêm
>
> với đã nghĩ đến việc vị nhập trùng số điện thoại hay gì thì tính sao chưa nếu chưa thì phải tính ví dụ báo đã có hay như thế nào ở trong đoạn thêm khách
>
> mấy icon khá ổn như avatar cái tên của user thì chưa ổn đâu cần cân nhắc lại
>
> à còn một vụ nữa bạn hay dùng và viết là thư giãn, an kiên lạc nhưng tôi muốn add thêm nó là thuộc danh mục nào nữa như gội đầu dưỡng sinh hay chăm sóc cổ vai gáy
>
> vụ follow up khách không quay lại, tôi đang nghĩ có cái đánh dấu đã follow up chưa vào ngày nào để dễ quản lý để biết khi nào nên follow tiếp khi nào không chứ không thể cứ thế chỉ xem ai lâu rồi không quay lại được
>
> tạm thời tôi chỉ thấy từng đó vấn đề bạn giúp tôi generate các subagent nghiên cứu làm và sửa cho tôi đi

## Lead's reading (each becomes a work item)
1. **Logo:**
   - Login: the emblem already contains the tagline. Remove the extra "HỒI SINH SỨC KHOẺ VÀ SẮC ĐẸP" text.
   - Sidebar and other places use a cut-up logo (mark + traced wordmark). Use the whole original logo unaltered. Adding a separate "Viên Phúc Y Đường" text next to it is optional; never strip parts of the logo.
2. **Reports:**
   - Flexible periods: day, week, month (30 days / calendar month), year, custom.
   - Polished design.
   - Trend charts as a **line graph**.
3. **Unlimited / no-expiry packages:** already covered by spec D2 (an empty expiry means it never expires). Verify the UI shows "Không hạn" and that the expiring/expired filters exclude these packages.
4. **"Lâu không quay lại" scope:**
   - Also include one-off customers (paid per session, no package).
   - Also include package customers who have not come back since their last session.
5. **Navigation:**
   - Going from a list to a detail page loses the way back.
   - Needs research and a proper pattern: breadcrumbs or back buttons, history-aware back, and list state kept in the URL.
6. **Search inside a customer profile** (history) once there is a lot of data.
7. **Gender shown next to the name** everywhere.
8. **Focus ring / press effect:** the thick ring on the search input covers the label text above it.
9. **Responsive:** resizing must never break the layout. Check carefully.
10. **State management:**
    - URL state for filters, search, tab and pagination.
    - The query cache stays consistent after mutations.
    - Remembered UI preferences.
11. **Tile sizing (home "Hôm nay"):** the tiles are oversized with empty space. Audit every component's size end to end.
12. **Search on every list page** that needs it (e.g. "Theo dõi gói"). Analyse each page.
13. **Duplicate phone on add-customer:**
    - Already implemented: 409 `customer.phone_taken` with a link to the existing profile.
    - Improve it by checking while the number is typed, before submit.
14. **User avatar / name chip in the top bar:** not good yet; rethink it.
15. **Service category shown with the service name** wherever services are listed (e.g. "Thư Giãn · Gội đầu dưỡng sinh").
16. **Follow-up tracking for inactive customers:**
    - Mark a customer as followed up, with date, method/result and an optional note.
    - Know when the next follow-up is due.
    - A new data type, so it needs backend work.
