---
title: ADR-0016 Tài khoản đăng nhập tạo từ hồ sơ, mật khẩu tạm, đổi / đặt lại mật khẩu
status: accepted (EM 2026-11-08)
date: 2026-11-08
deciders: EM, agent
---

# ADR-0016: Tài khoản đăng nhập tạo từ hồ sơ, mật khẩu tạm, đổi / đặt lại mật khẩu

## Context
- EM 2026-11-08 ([raw](../../raw/2026-11-08-em-request-accounts-and-passwords.md)): spa mới chỉ có tài khoản chủ spa; tài khoản quản lý / KTV chỉ tạo được bằng CLI `add-user`; không ai đổi được mật khẩu, không có đặt lại khi quên. EM duyệt luồng mới, bỏ bước "Gắn tài khoản" riêng, chọn "cách A" cho quên mật khẩu (chủ spa xử lý yêu cầu trong app, không SMS / Zalo).
- Vai trò lấy từ tài khoản ([ADR-0015 update](ADR-0015-own-profile-out-of-the-books.md)). `user_account` là bảng toàn cục (một SĐT = một tài khoản dù ở nhiều spa); membership + role theo spa.
- ADR-0004: Argon2id, session opaque, re-auth cho hành động bảo mật tài khoản. ADR-0014: người đã nghỉ giữ đăng nhập; `lock_login` khóa hẳn.

## Decision
1. **Tạo tài khoản từ hồ sơ** (`POST /staff/{id}/login`, `staff.manage` + re-auth ≤ 5 phút): SĐT (di động VN, chưa có tài khoản nào trong hệ thống) + vai trò (Quản lý / Kỹ thuật viên = role hệ thống `manager` / `technician`). Một transaction: `user_account` (tên = tên hồ sơ, `must_change_password`), `membership` + role, `staff.user_id`. SĐT đã có tài khoản (spa khác hoặc CLI) → `409 account.phone_taken`: không bao giờ gắn tài khoản của người khác rồi đặt lại mật khẩu của họ. Hồ sơ đã nghỉ / đã có tài khoản → `409`.
2. **Mật khẩu tạm**: server sinh (CSPRNG, 12 ký tự từ bảng chữ không nhầm lẫn, ≈ 59 bit), trả **một lần** trong response (`Cache-Control: no-store`), chỉ lưu hash Argon2id, không log, không audit. Tài khoản `must_change_password = true`.
3. **Bắt đổi mật khẩu**: đăng nhập trả `next: password_change` (không `me`); mọi route dùng session (`CurrentSession` / `AuthUser`) trả `403 auth.password_change_required` cho tài khoản `must_change_password` — kiểm ở server trong `service::resolve` (cùng câu SQL tìm session). Chỉ `POST /auth/password` và `POST /auth/logout` nhận session đó.
4. **Đổi mật khẩu** (`POST /auth/password`, mọi role, chính mình): mật khẩu hiện tại (sai → đếm vào khóa tài khoản như đăng nhập) + mật khẩu mới theo chính sách: 10–128 ký tự (đếm ký tự), không nằm trong danh sách mật khẩu phổ biến đi kèm binary (`too_common`, ADR-0019 D18), khác mật khẩu hiện tại, không phải SĐT; không bắt quy tắc ký tự (NIST SP 800-63B, như `identity::password`). Thành công: xóa `must_change_password`, thu hồi **mọi** session của tài khoản, cấp session mới cho trình duyệt này (xoay token). Giới hạn tần suất như đăng nhập (lớp `/auth/*` theo IP + khóa theo tài khoản).
5. **Chủ spa đặt lại mật khẩu** (`POST /staff/{id}/login/reset-password`, `staff.manage` + re-auth): mật khẩu tạm mới (một lần), `must_change_password`, thu hồi mọi session, xóa đếm sai / khóa tạm. Không bao giờ cho: tài khoản của chính mình hay tài khoản có `staff.manage` (chủ spa) (`422 account.owner_protected`), tài khoản còn thuộc spa khác (`422 account.other_spa`: mật khẩu đó cũng mở spa kia), tài khoản đã bị **khóa** khi cho nghỉ (`409 account.locked`: chỉ "Nhận lại làm" mở lại; đặt lại không được mở đường vào). Người đã nghỉ mà không khóa (ADR-0014) vẫn đặt lại được (để xem phiếu lương cũ).
6. **Đổi vai trò** (`PUT /staff/{id}/login/role`, `staff.manage` + re-auth): Quản lý ↔ Kỹ thuật viên; thay role của membership ở spa này. Quyền tính lại ở request kế tiếp (đã đọc mỗi request). Không đổi vai trò của chính mình hay của tài khoản có `staff.manage` (`422 account.owner_protected`), không đổi cho người đã nghỉ (`409 staff.already_left`).
7. **Quên mật khẩu, cách A** (EM: "ok cách A đi") (`POST /auth/password-reset-requests`, công khai): SĐT → luôn `202` cùng một body, cùng lượng việc (không lộ SĐT có hay không). Chỉ ghi yêu cầu khi tài khoản tồn tại, đang hoạt động và có membership đang hoạt động không có `staff.manage` (mỗi spa một yêu cầu đang chờ; gửi lại trong lúc chờ không tạo thêm). Hết hạn sau 24 giờ; đặt lại hoặc "Bỏ qua" đóng yêu cầu. Chủ spa thấy ở Tổng quan và Quản lý nhân sự → Hồ sơ nhân viên (`GET /staff/password-reset-requests`, `POST …/{id}/dismiss`, `staff.manage`). Giới hạn: lớp `/auth/*` theo IP + một yêu cầu đang chờ mỗi tài khoản mỗi spa. Audit `account.password_reset_requested` (actor `system`, không SĐT trong audit).
8. **Dữ liệu cũ** (tài khoản CLI chưa gắn hồ sơ): giữ nguyên `PUT /staff/{id}/account` (chỉ chủ spa, như trước) làm lối đi một lần: một thông báo chỉ hiện khi có tài khoản không phải chủ spa mà chưa gắn hồ sơ (`GET /staff/accounts` → `staff_id: null`, `manages_staff: false`), mỗi dòng chọn hồ sơ chưa có tài khoản + "Gắn vào hồ sơ". Bỏ dropdown gắn / "Bỏ gắn" khỏi luồng chính (form "Thêm nhân viên", sheet hồ sơ). Không thu hẹp route: các kiểm tra của nó (thành viên đang hoạt động, một tài khoản một hồ sơ, `If-Match`, audit) đã đủ, và thu hẹp sẽ làm gãy lối đi này.
9. Audit: `staff.login_create` (user_id, role), `staff.login_password_reset`, `staff.login_role` (before/after role), `account.password_change` (người tự đổi), `account.password_reset_requested`, `…_dismissed` — không bao giờ mật khẩu (tạm hay mới) hay hash.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| Chủ spa tự đặt mật khẩu ban đầu | Đơn giản | Chủ spa biết mật khẩu lâu dài; mật khẩu yếu |
| **Mật khẩu tạm một lần + bắt đổi** | Chủ spa không giữ mật khẩu thật; theo chuẩn onboarding | Cần cổng "bắt đổi" ở server |
| Link mời qua SMS / Zalo | Không ai thấy mật khẩu | Cần nhà cung cấp, chi phí, để sau (cách B) |
| Gắn tài khoản có sẵn (SĐT đã có ở spa khác) | Một người nhiều spa | Chủ spa spa này đặt lại được mật khẩu dùng ở spa khác — từ chối |

## Consequences
- Positive: spa mới tự onboarding trong app; mật khẩu tạm chỉ đi qua một response; mọi thay đổi có audit; quên mật khẩu không cần SMS.
- Negative / accepted: chủ spa thấy mật khẩu tạm (đến khi người đó đổi); một SĐT đã có tài khoản ở spa khác chưa thêm được từ app (CLI / hỗ trợ); chủ spa quên mật khẩu của mình vẫn qua đường hỗ trợ / CLI.
- Follow-ups: cách B (SMS / Zalo OTP). Danh sách mật khẩu phổ biến: đã làm 2026-11-09 (ADR-0019 D18, offline, không gọi dịch vụ ngoài).

## Revisit when
Có nhân viên làm ở nhiều spa, hoặc có kênh gửi tin (SMS / Zalo) cho cách B.

## EM decisions (2026-11-08)
Raw: [2026-11-08-em-decisions-accounts-open-questions](../../raw/2026-11-08-em-decisions-accounts-open-questions.md).
- Một người chỉ làm ở một spa: SĐT đã có tài khoản ở spa khác giữ `409 account.phone_taken`, không làm nhiều spa từ app.
- Chủ spa quên mật khẩu: khôi phục qua hỗ trợ / CLI (và mã khôi phục MFA); chưa làm trong app.
- Người đã nghỉ chưa bị khóa: chủ spa vẫn đặt lại mật khẩu được để họ xem phiếu lương cũ; người bị khóa giữ khóa tới khi "Nhận lại làm".
