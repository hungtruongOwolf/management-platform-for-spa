# EM decision — 2026-11-05: simplify opening balance + owner re-auth; deploy simpler

Verbatim (EM, chat, replying to the lead's summary of the M2 implementation in PR #23):

> nếu trong ngày thì cứ nhập thôi còn nếu quá ngày thì không được chỉnh sửa hay nhập thông tin của ngày đó hoặc các ngày trước nữa đâu cần chủ spa mở gì đâu cái đấy tự động mà
> nếu account là chủ spa thì cứ allow hết đi đâu cần làm cho nó rườm rà như vậy nhập lại mật khẩu

On the refund cap / "Đã đối chiếu giá trong sổ" checkbox:

> cái này tính sau

On hosting (DigitalOcean VPS + domain + R2):

> dùng vercel và supabase cho đơn giản trước được không hiện tại chưa gì làm rườm rà quá không ổn

## Lead's interpretation

- No "open import today" window. Data of a business day can be entered/edited that same day; after midnight (Asia/Ho_Chi_Minh) that day and earlier days are locked automatically — for non-owner roles.
- Owner accounts: allowed everything on business data, without re-entering the password (closed-day corrections/voids, opening-balance import, package cancel + refund).
  Re-auth stays only for account-security actions (change password, MFA settings) — not day-to-day work.
  Owner sessions still need MFA at login; every action stays in the audit log.
- Refund cap for paper-book packages: deferred. Refund limit returns to the sold (book) price, no checkbox.
- Hosting: start with Vercel + Supabase; no VPS/domain/R2 yet.
