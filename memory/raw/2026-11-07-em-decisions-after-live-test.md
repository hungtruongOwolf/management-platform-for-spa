# EM decisions — 2026-11-07: after the live full-case test (owner, manager, technician)

The lead posted a test report with findings 1–13 and decision questions A–E. The EM answered. Verbatim:

> đầu tiên tôi thấy một số vấn đề qua lượt test - khách vãng lai mà vẫn phải nhập tên với nhập số điện thoại lỡ đâu người ta không muốn thì sao
>
> tiếp đến có đoạn hủy lịch cái đó có thể làm thành button đặt lại vị trí cho hợp lý khong chứ bây giờ nó đang rất tệ là chỉ một cái tesxt chứ không hẳn là button nữa, thôi cũng phải là 1 button check kĩ lại đi
>
> hồ sơ sách có thể là ngày tháng năm sinh luôn không như mà optional thôi là được để sau này ai có info còn có chương trình giảm giá các thứ
>
> - A. Thời lượng làm tròn: dịch vụ 70 phút đang được giữ khung 75 phút, tức làm tròn lên ô 15 phút. Giữ như vậy hay để đúng 70 phút? cứ đúng thời gian 70' thôi
> - B. Trạng thái "Đang làm": lượt đã thu tiền xong vẫn hiện "Đang làm" cho tới hết khung giờ. Bạn muốn chuyển sang "Xong" ngay khi ghi lượt không? như hiện tại đợi hết giờ cũng được
> - C. Quản lý tự chấm cho mình: quản lý đang tự chấm công, tự khen và tự ghi thưởng cho chính mình được. Có cho phép không? chỉ chủ spa mới được chấm công cho quản lý quản lý có thể chấm công cho nhân viên
> - D. Form đánh giá mặc định 100 điểm: form đánh giá tháng mặc định mọi tiêu chí ở "Mức 1 (Xuất sắc)", tức 100 điểm. Đổi thành để trống, bắt người chấm chọn từng mục? tối da 100 nhưng tôi thấy bạn cứ để hard mức 1,2,3 bạn có thể viết note là chúng ta có mấy mức từ mấy điểm nhưng số điểm thì nên tự nhật
> - E. Hồ sơ không SĐT: vẫn chờ bạn quyết có cho người đi cùng có hồ sơ không SĐT không. nếu có thì có nếu khách không có thì thôi

## Lead's reading
- **Walk-in without identity:** a walk-in customer may decline to give a name or phone. Recording a visit must not require either.
- **Cancel appointment:** "Hủy lịch" becomes a real button, placed sensibly in the appointment sheet's action area.
- **Date of birth:** customer profiles get an optional date of birth, for later birthday promotions.
- **A:** a block lasts exactly the service duration (70 min stays 70 min). No rounding up to the 15-minute grid.
- **B:** keep the current behaviour. "Đang làm" stays until the time slot ends.
- **C:** only the owner records attendance, praise/complaints and bonus/penalty for a manager. A manager records them for technicians only, never for themselves or another manager. This is enforced on the server.
- **D:** the monthly evaluation keeps the 100-point maximum. Each criterion's score is typed in by the evaluator, not forced to a hard Mức 1/2/3 choice. A note explains the levels and their point ranges. A new form starts empty, never pre-filled at 100.
- **E:** phone is optional on a customer profile, companions included. Enter it when the customer has one.
- **Findings 1–13** from the report are approved for fixing.
