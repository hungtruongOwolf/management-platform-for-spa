# EM decisions — 2026-11-08: open questions after PR #103 (accounts and passwords)

The lead asked three questions after PR #103 (ADR-0016). Verbatim:

> a. Có cần cho một người làm ở nhiều spa không? không nhé
> b. Chủ spa quên mật khẩu thì tạm khôi phục qua hỗ trợ hoặc trên máy chủ, vậy được chưa? ok
> c. Người đã nghỉ mà tài khoản chưa bị khóa, chủ spa vẫn được đặt lại mật khẩu để họ xem phiếu lương cũ. Bạn muốn giữ vậy không? ok

## Lead's reading
- a. One person works for one spa. A phone that already has a login elsewhere stays `409 account.phone_taken`, with no multi-spa membership from the app.
- b. An owner who forgets their own password recovers through support or the CLI (and the MFA recovery codes). No in-app owner recovery for now.
- c. Keep it as it is: a leaver who was not locked at exit can have their password reset by the owner, so they can still read their past payslips. A locked leaver stays locked until rehired.
