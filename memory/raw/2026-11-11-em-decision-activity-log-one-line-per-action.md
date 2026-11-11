# EM — Nhật ký thao tác: ghi lượt có thu tiền là một dòng (2026-11-11)

Bối cảnh: trên màn "Nhật ký thao tác" (`/nhat-ky-thao-tac`, PR #180, ADR-0021), ghi một lượt có hóa đơn hiện hai dòng ("ghi lượt" và "thu tiền"), vì cùng một transaction ghi hai dòng audit (`visit.create` và `sale.create`); hủy lượt cũng vậy (`visit.void` + `sale.void`). Lead hỏi EM nên giữ hai dòng hay gộp.

Lời EM (nguyên văn):

> một dòng thôi vì khi ghi là thu tiền luôn rồi
