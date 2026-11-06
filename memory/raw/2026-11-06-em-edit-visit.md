# EM request 2026-11-06: sửa lượt khách

Verbatim (EM, 2026-11-06):

> tôi thấy 1 vấn đề nữa ghi lượt khách nhưng không có khả năng edit lỡ trong ngày hôm đấy bị lỗi gì thì sao thì phải cho chỉnh chứ

Lead's brief to the implementing agent (summary): build "Sửa lượt". Direct edits with no money
impact (technician per line, service time within the same business day, session note) through
`PATCH /visits/{id}` with optimistic concurrency; edits that change money or package use go
through one atomic "correct visit" = void the old visit + record the corrected one, linked, one
transaction, one Idempotency-Key, reusing the existing use cases. Same day → `record.void_same_day`
(manager + owner); earlier days → owner only (`record.void_closed_day`). Technicians cannot edit.
Audit both. A voided or already-corrected visit cannot be corrected again. Lead's design, EM to
review the result.
