# EM review — 2026-11-07: flows too complex in places, gaps elsewhere; HR management; latency

The EM tested on staging (logged in as the manager, dark theme). The images are in the session scratchpad `images/` as 58, 59, 61, 62 and 63; they are not in git. Verbatim:

> [Image #58] sao lại bảo là SĐT null đổi thành kiểu không có hay như thế nào đó chứ
>
> [Image #59] trong phần bán gói liệu trình nó bị một vấn đề là giả sử hôm nay khách muốn làm và tính luôn vào lúc mua thì sao nhỉ không thấy vụ đó
>
> [Image #61] đoạn này hơi vô lý nhỉ đã chọn option là 2 người mà không cho chọn là ai và cũng lỗi UI
>
> [Image #62] rồi đoạn này cũng lỗi UI
>
> tôi thấy một vấn đề sao không đặt lịch là 2 người rồi dừng ở đó ai muốn chính KTV thì update, hoặc ghi lại người ta đến làm thì bấm từng cái một chỉ cần lưu thông tin 2 người đó đi cùng nhau là được chứ nhất thiết phải làm một đống steps để ra phức tạp làm gì nhỉ
>
> rồi phần mỗi lượt mua lượt làm người ta mua multiple liệu trình hoặc multiple gói thì nó cứ kéo dài một loạt xuống dưới quá khó nhìn có thể cái nào chọn xong thì lưu bỏ form đi thành 1 cái widget hay component gì đó cho dễ nhìn có được không
>
> rồi 1 vấn đề nữa là khách vãng lai không cho ghi note không cho thêm thông tin gì
>
> tôi thấy ghi lượt khách hay gì đó luôn phải có note hiện tôi thấy chúng ta mới có note mỗi cho phần thông tin khách hàng thôi không ổn
>
> [Image #63] chỗ này lại trùng lặp 2 button 2 gói liệu trình
>
> tôi thấy cần bạn check kĩ càng lại flow có những chỗ nên đơn giản thì bạn làm phức tạp có những chỗ thì chưa cover hết case
>
> có những cái thì lại vấn đề về UI bị trùng nhiều cách sắp xếp chưa hợp lý cần bạn check kĩ lại đó với hiện tại mới có chấm công mới có đánh giá nhưng lại không có quản lý nhân sự trong mục nhân viên cần điề chỉnh và bổ sung cho hợp lý 1 tí kiểu giả sử để sau này chúng tôi biết người đó làm bao lâu rồi cần remove hay add vào list của spa thì còn quản lý được chứ chứ bây giờ là như hardcode cứng luôn quá tệ
>
> với bị một cái nữa là hiện tại latency của các API và UI đang rất chậm cách nào cải thiện và make nó tối ưu và tốt hơn không giúp tôi với nghiên cứu kĩ các architecture để xem chúng ta ứng dụng gì vào để cải thiện nó lên nữa nào

## Images
- **58.** Quầy, "Lượt khách hôm nay". A walk-in row's detail line reads "SĐT null · Giới thiệu: Khách tự chọn".
- **59.** The Quầy cart while selling a package ("Gói bán hôm nay": Cổ vai gáy An Kiên Lạc 10 buổi + tặng 1, 2.290.000đ). The panel shows the package template, referrer, gift, "Mua cho người khác", "Thêm giảm giá gói", the hint "Bán thêm gói: thêm dịch vụ của lượt, hoặc lưu hóa đơn gói này trước", payment, "Ghi chú buổi", and "Lưu hóa đơn gói". There is no visible way to use one session today from the package being bought.
- **61.** Lịch hẹn → "Khách đến cả nhóm · 2 người". The modal shows the first person (KTV làm, payment, "Bán gói cho …") and the second person (no profile, a "Tạo hồ sơ" banner). The footer button reads "Ghi 1 lượt". The layout is unbalanced, and you can't pick which people actually arrived.
- **62.** The appointment detail sheet for a companion with no profile. It shows the info rows, "Cùng nhóm (2 người)", a yellow "Tạo hồ sơ khách" banner, "KTV làm" with "Chỉ báo đã đến (ghi lượt sau)", "Không đến / Hủy lịch", then "Đổi lịch / Khách đến · ghi lượt". It is crowded and the hierarchy is unclear.
- **63.** Quầy with a walk-in selected. The left customer card has "+ Bán gói liệu trình", and the right cart also has "+ Bán gói liệu trình". There is no note field and no way to add information for the walk-in.

## Lead's reading
1. **Never show raw `null`.** A missing phone reads "Chưa có SĐT", or the item is omitted.
2. **Selling a package can include using a session today, and it must be obvious.** "Làm 1 buổi hôm nay" sits on the package line, sale and today's session are saved together, and the session is deducted from the new package.
3. **Simplify group booking.**
   - A group booking only records that N people come together; each person has their own service and optional KTV.
   - On arrival, each person is checked in individually from their own card. Assigning or changing a KTV is a plain update.
   - Drop the mandatory multi-step "Khách đến cả nhóm" modal. Keep only an optional shortcut, and if one is kept, it lets the user pick who arrived.
4. **The cart collapses finished items.** Each finished service, package or session collapses into a compact summary row/widget with Sửa / Xóa. Only the item being edited is expanded.
5. **Walk-ins.** You can add a note and optional info (name, phone, gender) later, from the cart and the profile.
6. **Every visit has a note.** Shown in the cart for every visit type and visible in visit details/lists. Not only customer-profile notes.
7. **Remove duplicate actions.** For example, a single "Bán gói liệu trình" entry point.
8. **Audit every flow.** Simplify what is over-complex, cover the missing cases, and fix duplicated UI or illogical layout.
9. **Staff management (HR) in Nhân viên.**
   - Add, edit and deactivate/remove staff from the spa's list.
   - Start date, so tenure ("làm bao lâu") can be shown, and an end date or reason when someone leaves.
   - Role and status, with history kept (soft-deactivate, never hard delete).
   - Nothing hard-coded.
10. **Latency of the API and UI.** Measure where the time goes, research architectures, and propose improvements with evidence. Apply the safe quick wins; the EM decides the bigger architecture or vendor changes.
