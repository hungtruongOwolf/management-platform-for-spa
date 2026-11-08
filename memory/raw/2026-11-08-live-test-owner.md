# Live test 2026-11-08 — owner part (follow-up)

Continues [2026-11-08-live-test-manager-technician](2026-11-08-live-test-manager-technician.md). The EM entered the owner MFA code in Chrome, then the lead ran the owner cases (C1–C6) and re-checked F19 as the manager.

## Owner (0905000001, MFA by EM)
- Home: today 2.199.000đ (= 3.849k − 1.690k Combo void + 40k correction) ✓.
- Linked manager login → profile "Nguyễn Thị Thu Hà". Re-login as manager: own card locked "Chỉ chủ spa chấm công, ghi khen/phàn nàn và thưởng/phạt cho quản lý" → F19 confirmed as a data-linking dependency only.
- C2 PASS: add staff "Test KTV Mới" (start 01/10/2026 → "1 tháng", salary, owner-only note); salary/commission columns + Bảng lương tab owner-only.
- End employment blocked while the person has future bookings ("Nhân viên còn lịch hẹn sau ngày nghỉ việc…"); after reassigning tomorrow's booking (KTV picker saves on change) → ended with reason; "Nhận lại làm" offered; KTV pickers exclude the leaver; leaver still in today's daily book (last day).
- C3 PASS: leaver login → nav = Tổng quan, Công và lương của tôi, Cài đặt; banner "Bạn đã nghỉ việc từ…"; next-month disabled; /khach-cua-toi screen blocked; API me/appointments 403, me/staff 200, month 12 403, me/evaluations/history 200, customers 403, notes 403.
- C4 PASS: owner sees 14 kinds incl. staff_note + employment_end.
- C6 PASS: owner has "Đổi giờ / nhập bù" with a date → back-filled a visit on 07/11 ("nhập bù ngày 07/11/2026"); closed-day list has full menu (Sửa/Thêm/Hủy) for owner; voided it with reason.
- C1 PASS: owner adds khen for the manager.
- NOT DONE: rehire Lan (needs owner MFA again) — local data: Lan is currently "Đã nghỉ"; C5 owner scoring the manager; C7 report deep-check.


### New findings
- F18 RESOLVED: owner can back-fill past days ("Đổi giờ / nhập bù" + Ngày làm); manager cannot (by design).
- F24 (minor): end-employment refusal for future bookings doesn't say which bookings or link to them.
- F25 (check): staff change log after ending shows "Ngày vào làm: 01/03/2025 → trống" (looks like start date cleared; profile still shows 01/03/2025).
- F26 (minor): day list orders by recorded time, so a back-filled 15:00 visit sits above 03:52.
- F27 (bug): voided back-fill detail "Ghi lúc 06:27:27 8/11/2026" uses the browser's timezone (US) instead of spa time (actual 19:27 ICT) and a different date format.
- F28 (minor): leaver's header still says "Kỹ thuật viên" (no "Đã nghỉ" hint in the account chip).
