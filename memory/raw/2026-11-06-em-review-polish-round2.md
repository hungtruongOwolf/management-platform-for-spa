# EM review — 2026-11-06: polish round 2 (after PR #65)

Verbatim (images 46–50 are in the session scratchpad `images/` folder):

> bạn có thể style hiện tại hơn tí được không kiểu ở background của cả page có hover cái logo dạng mờ mờ bản to hay như thế nào đó ấy
>
> rồi thứ ngày có thể [Image #46] có cái link hay component cho nó rõ lên được không hiện đang là text tương tự như cái chủ spa luôn
>
> vo[Image #47]ới bạn có thấy đonạ 6 khách, 4 khách bị lỗi UI không cái góc của calen cũng bị
>
> rồi tiếp đến mấy cái dropdown mấy cái viền không cùng style bo góc với các button mọi thứ quá tệ
>
> với như tôi bảo ấy các show data thì ok nhưng nó đang bị 1 vấn đề đó là style các sắp xeepx [Image #48] ví dụ cái này xem ngày để bên cạnh luôn drop down để đẩy cái lượt khách hôm nay cao lên trong bảo đưuọc mà nếu để thế thì quá tệ
>
> [Image #49] rồi kích thước bây giờ không đồng đều chỗ thì to chỗ thì nhỏ bạn check từng page từng phần cân chọn lại kích thước cân bằng lại kích thước chữ, các compoenent mọi thứ lại chó tôi chứ tôi thấy bây giờ là chỗ to chỗ nhỏ rồi đó
>
> [Image #50] cái sửa lượt này cho thành một button đặt ở bên phải hay gì đó đi chứ đặt vị trí hiện tại rất ngu làm cho giống cái nút sửa trong liệu trình cũng được
>
> rppof cách show data bây giờ khá ổn rồi chưa vấn đề gì nhưng nó đang bị minimalism quá nếu mà bảo được bao nhiêu điểm thì có được tầm 5,6 điểm trên 10 thôi cần cải thiện nó cho thêm style, màu mè, hoa lá cành các layeer ẩn nếu mà chưa có data thì hiện cái gì đẹp dẹp như logo cộng chữ kiểu vậy hiện tại nó bị thiếu thiếu ấy hiệu ứng thử xem
>
> rồi có chỗ thì bạn dùng tên chữ trắng có chỗ chữ lại màu hồng check kĩ màu mè đi
>
> và còn rất nhiều vấn đề khác nữa bạn check kĩ giúp tôi đi

## What the images show
- **#46:** the top bar. The date "Thứ 6, 06/11/2026" and the user "Chủ spa (test) / Chủ spa" are plain text. The EM wants them as visible components (chip/button/link).
- **#47:** the calendar day view in dark mode.
  - The "6 khách" / "4 khách" count badges collide with the time labels in the Giờ column.
  - The calendar's top-left corner is broken: the border/radius doesn't close.
  - The "Tất cả KTV" select has a different radius and height from the buttons beside it.
- **#48:** /quay "Lượt khách hôm nay (14)". The "Xem ngày" date field sits on its own taller row above the table with a label, which wastes height. The EM wants it inline beside the title, as a compact control.
- **#49:** /quay search panel. The "Tìm khách theo SĐT hoặc tên" label is tiny next to a large input. The "Lượt khách đang ghi" title uses a different size. Sizes are unbalanced.
- **#50:** the customer profile timeline card. "Sửa lượt" is a bare pink text link at the bottom left. The EM wants a right-aligned button like the "Sửa" button on packages. The second line's amount also wraps under the text instead of sitting right-aligned.
