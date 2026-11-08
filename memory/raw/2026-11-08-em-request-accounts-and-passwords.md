# EM request — 2026-11-08: create staff and manager accounts in the app; change passwords

Verbatim:

> tiếp đến bây giờ giả sử 1 spa mới chưa có data chỉ có mỗi account của chủ spa thì làm sao để tạo mới cho quản lý và nhân viên tôi thấy hiện tại flow tạo account cho nhân viên và quản lý chưa hợp lý và chưa cho phép đổi mật khẩu của từng account chưa có feature đó

## Current state (lead)
- Logins can only be created with the server CLI `add-user`. In the app, the owner can only link an existing login to a staff profile (`PUT /staff/{id}/account`, AC16.1).
- There is no in-app password change for anyone, and no reset for a staff member who forgot their password.

## Lead's reading
A new spa starts with only the owner's account. The owner must be able to onboard everyone from the app.
1. **Create a login while adding or editing a staff profile** (Nhân sự, owner only). Inputs: the login phone and the role (Quản lý / Kỹ thuật viên). The role is the account role, which decides permissions (EM decision 2026-11-08: role comes from the account). The login is created and linked to the profile in one step.
2. **First password.** The system generates a temporary password and shows it to the owner once, to hand over. The owner never sees it again. The staff member must set their own password at first login.
3. **Change password (everyone).** "Giao diện & tài khoản" → Đổi mật khẩu: current password plus new password. Signs out other sessions.
4. **Reset a staff password (owner).** For someone who forgot theirs: a new temporary password, shown once, with a forced change at next login. Their existing sessions are revoked.
5. **Change an account's role** (owner): manager ↔ technician.
6. Security: audit everything; re-auth (step-up) for the owner's sensitive account actions; password policy; rate limits. A leaver's lock (ADR-0014) and rehire keep working. The owner's own forgotten password stays a recovery or CLI path, out of scope unless cheap.

## EM follow-up (same day) — approved, and drop the separate "link account" step
Verbatim:

> có một ý tôi muốn nói đó là việc mà gán account hơi thừa với flow hiện tại vì là đã tự chủ spa tạo account nên là không cần đến thế
>
> còn flow mới này ok bạn có thể implement nó

Lead's reading:
- The flow above (1–5) is approved.
- No separate "Gắn tài khoản" (link an existing login) step in the normal flow. The owner creates the account from the profile, so profile and account are one person by construction.
- The profile's account section shows either "Tạo tài khoản đăng nhập" or the account: phone, role, "Đặt lại mật khẩu", "Đổi vai trò".
- Data created before this (logins made by CLI and left unlinked) still needs a one-time way in, kept out of the main flow, e.g. a small owner-only notice listing logins with no profile and a "gắn vào hồ sơ" action. The lead decides the minimum that is sound.

## EM follow-up (same day) — forgotten passwords: option A
The lead offered two ways for a staff member who forgot their password: (A) a request from the login screen that the owner handles in the app, or (B) an SMS / Zalo code (later). Verbatim:

> ok cách A đi

Lead's reading:
- On the login screen, "Quên mật khẩu?" opens a form: phone → "Gửi yêu cầu".
- The server records a reset request for that login only if it exists, is active and is not an owner / `staff.manage` login. The answer is identical either way (generic, roughly constant time), so it never reveals whether the phone exists. Rate-limited per IP and per phone like login; audited without extra PII.
- The owner sees pending requests in the app (Tổng quan and Quản lý nhân sự → Hồ sơ nhân viên, e.g. "Nguyễn Thị Lan xin đặt lại mật khẩu"), each with "Đặt lại" (the normal owner reset: temporary password shown once, forced change, sessions revoked) and "Bỏ qua".
- Requests expire (24 h); a reset or a dismiss closes them. No SMS or Zalo now (option B later).
