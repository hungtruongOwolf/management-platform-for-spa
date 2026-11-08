# EM request — 2026-11-08: fix every live-test finding; a manager doesn't manage themselves

After the live test (raw notes [2026-11-08-live-test-manager-technician](2026-11-08-live-test-manager-technician.md) and [2026-11-08-live-test-owner](2026-11-08-live-test-owner.md)). Verbatim:

> tôi thấy test cũng khá ổn rồi bạn fix hết đống lỗi hiện có đi và nữa đó là nếu đã là account quản lý thì đâu cần show trong mục nhân viên account của quan lý mà muốn xem thông tin thì show như của KTV ấy

## Lead's reading
1. **Fix all open findings F1–F28** from the two live-test notes. F18 is resolved: the owner back-fills past days. For F12 and F6, which were open questions, the lead picks the conservative behaviour and reports it:
   - F12: a booking can't be created at a time already past, matching the server's `appointment.in_the_past` rule. Back-filling is done in Quầy by the owner.
   - F6: package-only sales of the day are visible in Quầy's day list, with their actions.
2. **A manager doesn't see themselves in Nhân viên.**
   - In the staff-management tabs (Sổ hằng ngày, Bảng công tháng, Đánh giá tháng, Nhân sự), the manager's own profile is not listed. They can't act on it anyway, by decision C.
   - The owner still sees and manages every manager.
   - The manager reads their own attendance, pay, evaluation and history the way a technician does, in "Công và lương của tôi".
   - Their own profile is the one linked to their login (F19).
