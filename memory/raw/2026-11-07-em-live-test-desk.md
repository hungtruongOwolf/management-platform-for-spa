# EM live test — Quầy + shared UI (2026-11-07, via lead)

Items handed to the desk agent (verbatim intent, lead's wording):

1. Split payment: when cash > total the field only gets a red border; show a message explaining the problem.
2. "Chuyển sang gói liệu trình" opens with two empty package slots ("Gói thứ 2"); open with one.
3. Sửa lượt with a money change shows both "Thu thêm 50.000đ" and "Phải trả 349.000đ", inviting the cashier to collect 349k. Show the difference prominently, the new total as secondary context.
4. Time inputs in Sửa lượt, nhập bù and staff attendance "Giờ & ghi chú" use the browser's 12-hour AM/PM input; typing "15:00" produced 15:58. One shared 24-hour time input everywhere `type="time"` is used.
5. Visit details: "Tiền mặt + CK" doesn't show the amounts ("Tiền mặt 20.000đ · CK 25.000đ").
6. A notice such as "Đã hủy lượt…" persists after the next save.
7. Draft restore: the Nữ/Nam toggle isn't restored although the price is still the Nam price.
8. Changing the customer clears the cart immediately with no confirmation; ask first when the cart has items.
9. The Quầy cart still has the old free-text "Người đi cùng (ghi chú)", inconsistent with ADR-0009; remove/replace, keep existing data readable.
10. A technician opening /quay by URL sees the full Quầy UI with permission errors; show "Không có quyền" or redirect, for every restricted route; hide "Thử lại" on 403.
11. Login phone field accepts letters; digits only (inputmode numeric), paste of formatted numbers still works.
