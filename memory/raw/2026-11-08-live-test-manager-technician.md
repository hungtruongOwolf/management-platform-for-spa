# Live test 2026-11-08 (local, Chrome) — findings

The lead ran the EM-approved live test plan in Claude in Chrome against local (API 127.0.0.1:8080 at main 73a4571, Vite 5173): manager and technician done; owner (MFA) pending. The EM said: "ok tạm thời lưu lại mấy cái lỗi đó đi" — record the findings, fix later.

## Manager (0905000002)
- A1 PASS: login, manager nav.
- A2 PASS: anonymous walk-in saved without name/phone; session note shown in list; no "SĐT null"; profile shows "Chưa có SĐT"; later added name/phone/gender/DOB/note via Sửa hồ sơ.
- A13 PASS: DOB, profile note, health note (warning shown in Quầy).
- A3 PASS: search "test vang lai" without diacritics finds the customer.
- A4 PASS: package + "Làm 1 buổi hôm nay" saved together, 11/12 left; split cash 290k + CK 500k.

- A5 PASS: 2 packages in one save (2.980.000đ CK); ledger shows one row per package; today total 3.849.000đ matches.
- A6 PASS: new-customer booking requires phone ("Còn thiếu: SĐT khách mới, dịch vụ"); 70' exact; group of 2; "Đổi cả nhóm" default moved both 10:00→11:00; "Chỉ người này" moved only companion to 13:00; KTV busy → clear message; arrive → "Hoàn tác đã đến" (with confirm) → back to Chưa đến; Hủy lịch requires reason, shown on sheet.

- A7a PASS:
  - Sửa lượt (service Thư Giãn→Chuyên Sâu, KTV Lan→Hằng) requires reason, shows "Thu thêm 40.000đ"; old row "Đã sửa" struck through, new row 119k.
  - Hủy lượt requires reason ("Bắt buộc ghi lý do."); package session returned (Xông chân 11→12/12); row shows "Đã hủy · Đã trả lại buổi" + reason.
  - Hủy hóa đơn bán gói (Combo) with reason: moved to "Gói đã hết / đã hủy", Đã chi 3.889k→2.199k, buổi 33→23.
- A7b PASS: yesterday list shows "Ngày đã chốt: chỉ chủ spa hủy được lượt của ngày này."; row menu only "Mở hồ sơ khách"; API POST /visits/{id}/void → 403 day.locked; PATCH → 403 day.locked. Back-fill a visit for yesterday: Quầy only allows a time today (no date) → manager cannot back-fill yesterday.

- A8 PASS (with F19/F20): per-staff cards, 4 tiles; Lan Đi làm 09:20 → "Đi muộn 20 phút"; notes shown; khen +1; thưởng 50.000đ with Sửa/Xóa.
- A9 PASS: empty form, typed points, "Tối đa 40 điểm" validation, 89 Mức 1; Xem read-only scorecard (criteria, levels, plan, comment, "Chấm bởi … 19:14"); history 12 months; Từng ngày → month table filtered + day list.
- A10: manager has no payroll tab (as designed).
- A11 PASS: Nhân sự read-only, no add/edit/end; profile sheet has "Lịch sử đánh giá".
- A12 PASS: 12 kinds (no HR/end reasons); encrypted search "xong nong" finds health notes; kind filter "Hủy / sửa" shows void/edit reasons with links.
- A14 SKIPPED live: Chrome window maximized, resize had no effect (CI e2e covers 390 + axe).

## Technician (0905000003, Nguyễn Thị Lan)
- B1 PASS: nav = Tổng quan, Công và lương của tôi, Khách của tôi, Bảng giá, Mẫu gói, Cài đặt.
- B2 PASS: week grid incl. 01:47 visit; list view; detail shows session note, no phone/DOB/health; /me/appointments JSON has no phone/dob/health; corrected visit moved to Hằng, voided excluded.
- B3 PASS: summary (5.562.305đ tạm tính, +50.000đ thưởng, 89 Mức 1), day sheet (late 20', notes, bonus, visits+commission), "Xem phiếu đánh giá", own 12-month history without Chấm lại / notes link.
- B4 PASS: /nhat-ky-ghi-chu and /quay → "Không có quyền mở màn hình này"; API customers/notes/visits/staff/evaluations/history/ledger/customer → 403 (appointments → 400 validation first).

### Findings
- F19 (IMPORTANT): decision C relies on staff↔login link. Local manager profile "Nguyễn Thị Thu Hà" is NOT linked → manager could mark her own attendance "Đi làm" and sees "Chấm điểm" on her own evaluation. Nhân sự shows only a small hint "Chưa gắn: khi ghi công được tính như KTV". Proposal: block/obvious warning for unlinked "Quản lý" profiles; check staging linkage.
- F20: "Giờ & ghi chú" dialog still labels options with (P), (ĐX), (K); Bảng công tháng legend/column "P/ĐX/K" too.
- F22 (UX): technician week/day grid starts at the earliest visit hour (02:00) and doesn't auto-scroll to working hours / now.
- F23 (naming): every non-booked visit is labelled "Vãng lai" (even named customers) — clashes with "Khách vãng lai" (anonymous). Suggest "Không đặt lịch" / "Khách đến thẳng".
- F21 (gap): appointment cancel / no-show reasons are not in Nhật ký ghi chú.
- F1 (minor UX): in cart, after picking Nữ, the hint "Khách chưa có giới tính: chọn Nữ/Nam để tính giá." stays visible.
- F2 (minor): gender chosen in the cart for a walk-in is not saved to the profile (profile stays "Chưa rõ").
- F3 (minor): today list row for package+session reads "Gội đầu dưỡng sinh · Thư Giãn (trừ buổi)" with 790.000đ — doesn't say a package was sold; amount looks like a service price.
- F4 (minor): package-only cart has no note field.
- F5 (minor): toast after multi-package save concatenates two messages without separator.
- F6 (question): package-only sales don't appear in "Lượt khách hôm nay" (only in ledger) — can't act on them from Quầy list.
- F7 (UX dup): booking dialog has both a "Khách mới" tab and a "+ Khách mới" button.
- F8 (minor UI): booking dialog shows two vertical scrollbars (dialog + inner).
- F9 (minor): companion validation error shows immediately when adding a companion, after an earlier failed submit.
- F10 (minor): after booking for another day, the calendar stays on the current day (no jump to the booked day).
- F11 (minor): stale error ("Lan đã có lịch…") stays after changing date/time.
- F12 (decision): past time today → button becomes "Vẫn đặt lịch" and the booking IS created (10:00 when now 19:08). Confirm intended (back-fill) vs block.
- F14 (minor): "Lượt khách hôm nay (N)" counts replaced/voided rows.
- F15 (minor): voided package-session row detail still says "Trừ buổi · còn 11/12" (snapshot) after the session was returned.
- F16 (minor): customer timeline shows a voided visit twice (a "Đã hủy" event + the original row with reason).
- F17 (low): POST /visits/{id}/correct on a locked day with an invalid body returns 422 before 403 day.locked (validation before authz). No data leak.
- F18 (decision): no way to back-fill a forgotten visit for yesterday (manager or owner?) — check owner.
- F13 (minor): time options show "x/3 khách" for today but not when another date is picked first.
