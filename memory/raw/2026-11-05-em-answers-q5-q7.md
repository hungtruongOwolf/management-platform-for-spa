# EM answers — 2026-11-05: management-views Q5–Q7 (round 3)

Verbatim (EM, chat):

> 1. Báo cáo xa nhất 2 năm có đủ không? ... 2 năm ok rồi, còn vụ 3-5 năm kia thì lâu lâu mới xem và chỉ chủ spa mới có tính năng này
> 2. Quy tắc "không nghe máy thì gọi lại sau 3 ngày" ...? hmm cái này trước tiên bạn cứ tự chốt tôi đang không hiểu feature này lắm, 1 là discuss giải thích thêm 2 là bạn làm ra tôi review như thế nào mới feedback
> 3. KTV có được ghi follow-up không? ... KTV đâu có khả năng quản lý khách đâu mà follow up

## Lead's reading / decisions
- **Q5:**
  - Everyone gets up to 2 years.
  - 3–5 years is owner-only and rarely used, so it is computed on demand (slower is acceptable). No rollup table yet.
- **Q6:** EM delegates the defaults to the lead and will review the built feature.
  - Lead's decision for the next follow-up date:
    - `no_answer`: +3 days.
    - `will_come`: +7 days, as a check-in if they still haven't come. A visit starts a new absence streak, so the reminder disappears by itself if they come.
    - `not_interested`: no next date. The customer leaves the "cần liên hệ" list for 90 days, then comes back once.
    - `other`: no automatic date.
  - A date entered by hand always wins.
- **Q7:** Technicians do not follow up. Only owner and manager, as built.
