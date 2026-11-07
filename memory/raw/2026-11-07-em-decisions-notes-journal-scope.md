# EM decisions — 2026-11-07: notes journal scope (Q-N1, Q-N2)

After PR #86 ("Nhật ký ghi chú") the lead asked two questions. Verbatim:

> - Q-N1: Nhật ký có cần thêm các loại ghi chú khác không? Ví dụ ghi chú lịch hẹn, nhận xét đánh giá tháng, ghi chú nhân sự, lý do gia hạn gói và lý do giảm giá. Hiện chưa có các loại này. có đi
> - Q-N2: Ghi chú hồ sơ khách và ghi chú sức khỏe hiện không lưu ai ghi và ghi lúc nào. Nhật ký đang hiện thời điểm hồ sơ được sửa lần cuối. Có cần lưu riêng người ghi và thời gian cho từng ghi chú không? không cần đâu

## Lead's reading
- **Q-N1: yes.** The journal also lists:
  - appointment (booking) notes;
  - monthly evaluation comments;
  - staff HR notes;
  - package-extension reasons;
  - discount reasons.

  Each new kind keeps the read rule it already has where it is written or shown.
- **Q-N2: no.** Profile notes and health notes keep a single value on the profile. The journal goes on showing the profile's last-change time, with no per-note author or timestamp. No schema change for this.
