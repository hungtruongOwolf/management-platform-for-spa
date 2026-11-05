# EM correction — 2026-11-05: paper-book money counts as revenue

Context: the lead proposed recording what migrated customers paid on paper as `carried_over_paid`, shown in "Đã chi" but excluded from revenue, the ledger and reports.

Verbatim (EM, chat):

> cứ tính là tiền chứ nó là data thôi mà vì hiện tại app đâu có tính năng link với bên app hay nhận tiền confirm đâu nó chỉ như lưu data hình thứ thôi nên cứ tính doanh thu mọi thứ 1 thể đi cho tôi

## Lead's reading
- The app records money; it does not move or confirm it. Paper-book money is the same kind of record, so it counts as revenue like everything else.
- The migration therefore books each paper purchase as a real sale with its payment, on the **original purchase date** taken from the paper book (the CSV carries the date and, optionally, the method; the default is cash). It then appears in "Đã chi", the ledger and the reports for that day.
- It is still marked as migrated internally (`source = paper_book`) for audit and re-run idempotency, but the UI does not distinguish it (EM 2026-11-05: "không cần phân biệt").
