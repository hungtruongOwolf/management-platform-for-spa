# EM decision — 2026-11-08: a visit keeps only its start time

The lead explained that a visit stores only its start time (`check_in_at`), with the end computed from the services' standard durations. It offered an optional "Giờ xong" field, or start/finish buttons. Verbatim:

> giờ bắt đầu là được rồi thời gian tự tính dư ra vài phút trong thực tế thì không cần care đâu logic hiện tại đã ổn rồi càng nhiều thao tác càng phiền phức

## Lead's reading
- Keep the current model: start time only; end = start + exact service durations (decision A). No end-time field, and no start/finish buttons.
- General rule: don't add operator steps for marginal accuracy.
