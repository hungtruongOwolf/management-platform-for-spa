# EM decisions — 2026-11-08: Q-M1, Q-S7, group-move default; evaluation history is missing

After the deploy of #84–#89, the lead asked three questions. The EM answered and raised a gap. Verbatim:

> Q-M1 giữ nguyên, Q-S7 không cần, mặc định đổi cả nhóm mà thiếu một chỗ chưa add đó là trong mục nhân viên không có xem lại đánh giá hoặc lịch sử của đánh giá xem lại của các ngày trước hoặc thnags trước à hay có rồi mà tôi chưa biết xem

## Lead's reading
- **Q-M1, keep as is.** A cart of 2–3 packages saved in one checkout still writes one revenue-book row per package sale. No grouping key and no migration.
- **Q-S7, not needed.** The owner can lock a leaver's login only when ending employment ("Khóa tài khoản"). There is no separate lock/unlock button.
- **Group move default:** "Đổi cả nhóm" stays preselected.
- **Evaluation history (gap).** Nhân viên → Đánh giá has a month picker and shows each person's total and level for the chosen month. What it lacks:
  - There is no read-only view of a past (or closed) month's scorecard: criterion points, comment, coaching plan, serious issue. The only action is "Chấm điểm / Chấm lại", which is hidden once the month is closed.
  - There is no per-person history across months, so you can't see how someone's evaluations trended month by month.
  - There is no easy way to read back one person's past days, which means praise and complaints per day plus attendance.

  The EM wants a place in Nhân viên to review past evaluations: previous months, and the day-level history behind them.
