# EM review — 2026-11-06: redesign rollout (PR #59 Lịch hẹn, #60 Quản lý, #61 Khách hàng)

The EM reviewed the before/after page (artifact "Giao diện mới toàn app · duyệt đợt 2"). Verbatim:

> tôi thấy ok trừ cái lịch sử sử dụng của bản cũ ok hơn, Lịch tuần Ui của bản cũ cũng ok hơn, ngày chưa có lịch của bản mới lại bị dính tận 2 lần đặt lịch nếu bạn thêm cái note thì thêm thôi chứ không cần thêm button đâu, Gói liệu trình của khách bản cũ ok hơn , Bảng giá dịch vụ bản cũ ok hơn nhưng đẩy cái thêm nhóm dịch vụ mới lên trên ngang với mẫu gói liệu trình đi , Tổng quan bản cũ ok hơn , Báo cáo 30 ngày dùng bản mới như nhớ đổi màu nâu đỏ cho cái thực thu như bản cũ
>
> cái màu phối menu Navbar bản cũ ok hơn
>
> giúp tôi còn lại bản mới ok
>
> nhưng 1 vấn đề hiện tại bản cũ và bản mới chọn 2 option qua lại sẽ bị lẫn lộn về phối màu và style của toàn bộ cần phải check kĩ và cân đối lại điều chỉnh phối hợp cho thống nhất 1 bản cuối

## Lead's reading (per screen)
- **Old version kept:**
  - Customer usage history (Lịch sử sử dụng on the customer profile).
  - Week view of the calendar.
  - "Gói liệu trình của khách" page.
  - "Bảng giá dịch vụ" page. Change: move "Thêm nhóm dịch vụ mới" to the top, level with how "Mẫu gói liệu trình" places its add action.
  - "Tổng quan" (dashboard).
  - Sidebar/nav colour scheme.
- **New version, with changes:**
  - Empty calendar day: keep only the note, no second "Đặt lịch" button.
  - Báo cáo 30 ngày: the "Thực thu" figure gets the maroon colour it had in the old version.
- **New version kept:** everything else.
- **Then a unification pass.** Mixing old and new screens must not produce two visual languages. The final app uses one set of colours, type, buttons, fields, chips and spacing. Reverted screens keep their old layout but use the shared final tokens and components.
