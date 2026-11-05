# EM feedback — 2026-11-05: review round 4 (after #31–#35 local preview)

Verbatim (EM, chat, with 3 screenshots: report chart tooltip wrapping "30 ngày qua"; price-list "Nhóm dịch vụ mới" label/input misaligned; customer profile dark mode — "(sổ cũ)" in a package name, "Đã chi 0đ", layout/capitalisation issues):

> thứ nhất thống tin cần chỉnh sửa cái ghi lượt khách bạn có thể làm như làm 1 option như dạng google calendar được không
>
> và bên cạnh đó là hiện tại chúng ta mới chỉ ghi là người đã làm vậy kiểu nếu đặt trước lịch rồi thì ghi ở đâu các thiết kế của Ghi lượt khách hiện thiếu cái đó. chúng ta nên có trạng thái là trước khi đến và sau khi làm xong. trước khi đến là đặt lịch. sau khi đến là biết ai là kỹ thuật viên đã thanh toán chưa. Và thêm nữa đó là có thể biết được là có một vài khách lẻ đến đột ngột thì cứ ghi thẳng bước cuối luôn không cần lên slot giờ nữa
>
> vì có một vấn đề sau này chỉ có 3,4 nhân chúng ta phải control được bao nhiêu khách sẽ làm vào khung thời gian nào
>
> tiếp đến là bạn có thể làm feature quản lý nhân viên, tính lương tính thường, phạt rồi đánh giá từng ngày như trong cái sheet của docs reference ấy
>
> tiếp nữa chắc bạn cũng hiểu đó là UI của mobile đâu thể y như web được mà vì kích thước nhỏ hơn một số thứ phải thể hiện theo kiểu khác nghiên cứu lại vụ UI/UX cho mobile chưa ổn lắm đâu
>
> UI [chart tooltip screenshot] lỗi này
>
> như tôi bảo ấy tôi thấy chủ spa chưa được thêm data cũ của khách làm ngày trước khi hết thời gian nhập của ngày trước mà vẫn chỉ cho nhập ngày hôm đó thôi. với hiểu cách lưu thời gian của bạn là thời gian nhập chứ không phải thời gian làm vào mấy giờ tôi thấy thiếu. Và thêm cái show là thời gian còn lại để nhập đủ số liệu ngày hôm nay là còn bao nhiêu thêm bộ đếm live và cũng như tính nó theo time local của máy đăng nhập nhé
>
> [price-list screenshot] lỗi UI này [profile screenshot] lỗi UI nhiều lắm check kĩ mấy cái viết hoa viết thường định dạng mọi thứ lại cho tôi, position các thứ cũng sai
> tôi bảo bạn check kĩ từ đầu đến cuối đi mà đừng để sai cái gì cả
>
> với tôi thấy trong cái lượt khách hôm nay đâu thể bấm vào xem được info khách hàng đâu tôi nghĩ cần thêm tính năm này và cách show bảng hơi nhỏ và hẹp. cái search khách và chọn trong ghi lượt khách xong nó hiện ra thông tin cơ bản của user nó nên hiện luôn ở dưới tên được bấm thay vì là component khác
>
> đặt lại các tên cho đúng đừng ghi là gói bán hay theo dõi gói mà nên cụ thể gói liệu trình
>
> với giả sử có giảm giá bao nhiêu % thì chưa thấy có feature đó trong Ghi lượt khách. logo trên thanh bar new tab của platform chưa đúng
>
> bạn làm cho tôi kĩ đi generate subagent ra làm cho đầy đủ và đúng

## Lead's reading (work items)
1. **Appointments / calendar.**
   - Add a Google-Calendar-like view (day/week; technicians as columns, time slots as rows) to the front desk.
   - Lifecycle:
     - đặt lịch (booked);
     - đã đến (arrived/in service), with the technician assigned;
     - hoàn tất (done), paid or unpaid;
     - plus no-show and cancelled.
   - Walk-ins skip booking and record directly as today.
   - Capacity: with 3–4 technicians, show and enforce how many customers each slot can take (per-technician overlap).
2. **Staff management.**
   - Covers staff records, daily attendance and evaluation, bonuses, penalties, and salary/commission calculation, mirroring the paper sheets in `docs for reference/sheet cho quản lý khách hàng/` and [compensation-rules](../wiki/product/compensation-rules.md). That page has 8 open contradictions that need EM answers.
3. **Mobile UI/UX.**
   - The mobile UI must not just be the web layout shrunk. Research mobile patterns and redesign per screen.
4. **UI bugs, with a full audit.**
   - The chart tooltip wraps its legend text.
   - The label and input are misaligned in "Nhóm dịch vụ mới".
   - The customer profile has:
     - "(sổ cũ)" in a package name (demo seed);
     - "Đã chi 0đ" for migrated packages;
     - inconsistent capitalisation and formatting, wrong positions.
   - The EM wants an end-to-end check of capitalisation, formatting and positioning.
5. **Owner back-dated entry and actual service time.**
   - The owner must be able to enter past days' data, e.g. visits done earlier that were not entered in time. Non-owners stay locked after the day ends.
   - Store **when the service happened** (giờ làm), not only the entry time.
   - Show a **live countdown** of the time left to complete today's entries, computed on the device's local clock. The server still enforces the spa's day in Asia/Ho_Chi_Minh.
6. **Today's visits list.**
   - Click a row to see the customer's info.
   - The list is too small and narrow.
   - After picking a customer in the search, show their basic info inline under the picked name instead of in a separate card.
7. **Naming.**
   - Use specific names, never "Gói bán" or "Theo dõi gói". Say "gói liệu trình" (e.g. "Mẫu gói liệu trình", "Gói liệu trình của khách").
8. **Discount (%) on visits** (one-off services at the front desk). It exists for package sales (D10) but not for visits.
9. **Browser-tab favicon** is still wrong.
