---
title: ADR-0004 Authentication & sessions
status: accepted
date: 2026-11-04
deciders: EM, agent
---

# ADR-0004: Đăng nhập & session

> **Sửa đổi bởi [ADR-0020](ADR-0020-owner-mfa-per-spa.md) (2026-11-09):** MFA của chủ spa nay theo công tắc của từng spa (`tenant.owner_mfa_required`, mặc định bật, operator đổi ở `/admin`). Đăng nhập và reauth hỏi mã khi và chỉ khi `app.user_requires_mfa` (role `requires_mfa` ở spa có policy bật, nghiêm nhất thắng, spa tạm ngưng vẫn tính); đã enroll mà không bắt buộc thì không bị hỏi.

## Context
Người dùng R1–R3 là nhân sự spa (chủ, quản lý, KTV): vài người mỗi spa, do chủ spa tạo tài khoản. Không có đăng ký công khai và đăng nhập mạng xã hội. KTV có thể không có email. Khách hàng đăng nhập là giai đoạn sau. Ràng buộc từ ADR-0002: không dùng Supabase Auth/Data API.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Tự quản lý trong Rust: mật khẩu Argon2id + session opaque trong Postgres** | Không thêm service; kiểm soát hoàn toàn; hợp với cookie cùng origin | Tự chịu trách nhiệm code auth (giảm bằng thư viện chuẩn + checklist `secure`) |
| OIDC provider (Zitadel / Keycloak tự host) | Chuẩn, đủ tính năng | Thêm một service phải vận hành và vá; quá mức cần cho vài user |
| Managed (Clerk / Auth0) | Ít code | Chi phí theo user; phụ thuộc bên ngoài; dữ liệu nằm ở bên thứ ba |
| Supabase Auth | Có sẵn | Ngược ADR-0002; thường đi kèm JWT trong localStorage |

## Decision
- **Đăng nhập bằng SĐT (hoặc username) + mật khẩu.** Mật khẩu ≥ 10 ký tự, kiểm tra với danh sách mật khẩu bị lộ, băm **Argon2id** (`argon2` crate, chạy trong `spawn_blocking`).
- **Session opaque:** token ngẫu nhiên 256-bit; DB chỉ lưu hash SHA-256 của token. Cookie `__Host-session`, `HttpOnly; Secure; SameSite=Strict; Path=/`. Idle timeout 12 giờ, tuyệt đối 30 ngày. Xoay token khi đăng nhập hoặc đổi quyền. Có "đăng xuất mọi thiết bị".
- **CSRF:** `SameSite=Strict` + kiểm tra header `Origin` trên mọi request ghi.
- **Chống dò mật khẩu:** rate limit theo IP và theo tài khoản, chậm dần, thông báo lỗi giống nhau.
- **Tài khoản do chủ spa/quản lý tạo**, kèm link kích hoạt một lần (token 128-bit, hạn 24 giờ, lưu hash) gửi qua Zalo/tin nhắn thủ công. Quên mật khẩu: quản lý/chủ spa tạo link đặt lại, không cần email.
- **MFA TOTP bắt buộc** cho Chủ spa và Platform admin; tùy chọn cho người khác. *(Hiện thực ở S2b.)* *(Từ ADR-0020: chủ spa bắt buộc khi spa bật công tắc, mặc định bật.)*
  - Bắt buộc là **dữ liệu**: `role.requires_mfa` (role hệ thống `owner` luôn true, có CHECK), hoặc (đến 0040) role giữ `platform.tenant.manage`, nhánh này bỏ ở 0041; ~~user đã tự bật MFA cũng luôn phải nhập mã~~ (bỏ ở [ADR-0020](ADR-0020-owner-mfa-per-spa.md): chỉ hỏi mã khi `user_requires_mfa`, tức role `requires_mfa` ở spa có `owner_mfa_required` bật; "tự bật MFA" chưa từng có đường làm).
  - Đăng nhập 2 bước: mật khẩu đúng → session **pending MFA** (10 phút, chỉ dùng được cho `/auth/mfa/*` và logout) → mã TOTP hoặc mã khôi phục → session đầy đủ mới (xoay token). Owner chưa có MFA bị bắt enroll ngay (QR + khóa nhập tay, xác nhận bằng mã đầu tiên).
  - TOTP RFC 6238 (SHA-1, 6 số, 30 giây, ±1 bước), mỗi bước chỉ dùng 1 lần. Secret mã hóa AES-256-GCM bằng `APP_DATA_KEY`, AAD gắn user id. 10 mã khôi phục 80-bit, lưu SHA-256, dùng 1 lần, chỉ hiện 1 lần.
  - Mã sai tính vào cùng bộ đếm khóa tài khoản; mật khẩu đúng không reset bộ đếm khi MFA còn dang dở.
  - Reauth (🔒) với tài khoản có MFA yêu cầu thêm mã. *(Từ ADR-0020: khi MFA đang bắt buộc.)* Từ 2026-11-05 re-auth chỉ dùng cho thao tác bảo mật tài khoản (xem update bên dưới).
  - Mất thiết bị: operator chạy `app reset-mfa` sau khi xác minh danh tính ngoài hệ thống (TOFU: ai có mật khẩu trước khi owner enroll có thể enroll trước — giảm bằng cách owner enroll ngay khi nhận tài khoản; ở R1 tài khoản do operator tạo).
- Phân quyền: permission-based, role là dữ liệu ([principles](../architecture/principles.md) #2). Permissions được nạp vào `AuthUser` mỗi request từ session.

## Update 2026-11-04: security review r1
- **Xoay token ≠ xác thực lại (H1).** Mỗi lần phát session, code phải nói rõ loại (`identity::service::Issue`): `MfaPending`, `Authenticated` (đăng nhập/MFA xong: `recent_auth_at = now`, hạn tuyệt đối mới 30 ngày), `Rotated` (switch-tenant: **giữ** `recent_auth_at` và `absolute_expires_at` của session cũ, idle = min(now + 12h, hạn tuyệt đối)). Cột `session.recent_auth_at` không còn `DEFAULT now()` (migration 0011), nên quên truyền giá trị là lỗi NOT NULL chứ không phải một cửa sổ 🔒 miễn phí.
- **Spa bị tạm ngưng (M3):** không cấp permission cho session nào, kể cả session đang mở (`repo::member_permissions` kiểm `tenant.status = 'active'`) → `403 auth.not_a_member`. Ngược lại, `app.user_requires_mfa` **bỏ** điều kiện spa active: owner của spa đang ngưng vẫn phải MFA, để session đăng nhập trong lúc ngưng không thành session owner không-MFA khi spa mở lại.
- **Cấp role cần MFA (L4):** `provisioning::add_user` thu hồi mọi session của user khi role được cấp có `requires_mfa`; lần đăng nhập sau đi qua MFA. *(Từ ADR-0020: chỉ khi spa có policy bật; bật policy cũng thu hồi session của người vừa chuyển sang bắt buộc.)*
- **Khóa tài khoản (L2), chọn cách nhỏ và an toàn:** giữ khóa **theo tài khoản** (không đổi sang theo tài khoản + IP), thêm lệnh operator `app unlock --phone P` (xác minh danh tính ngoài hệ thống trước, giống `reset-mfa`). Lý do: khóa theo (tài khoản, IP) cho kẻ tấn công có nhiều IP (IPv6, botnet) 5 lần đoán mỗi IP, tức làm yếu chống dò mật khẩu phân tán, đúng thứ mà khóa tài khoản sinh ra để chặn; spa có 1 chi nhánh, operator gọi được, và khóa tối đa 24 giờ. Rủi ro còn lại: ai biết SĐT chủ spa có thể khóa lại liên tục (DoS). Nếu điều đó xảy ra thực tế thì cân nhắc: khóa theo (tài khoản, IP) + trần toàn cục theo tài khoản cao hơn, hoặc cho thiết bị đã từng đăng nhập thành công một hạn mức riêng.
- **Rate limit (L1):** bucket theo IPv4, hoặc theo **/64** với IPv6; map bucket được dọn mỗi 60 giây (`retain_recent`).
- **TOFU khi enroll MFA lần đầu (L3), chỉ ghi nhận, chưa sửa:** ai có mật khẩu owner trước khi owner tự enroll thì enroll được authenticator của mình. Giảm thiểu hiện tại: operator tạo tài khoản và owner enroll ngay khi nhận (ở R1 tài khoản do operator tạo trực tiếp). Hướng sửa sau: mã kích hoạt một lần do operator đưa tận tay, bắt buộc cho lần enroll đầu.

## Update 2026-11-05: chủ spa không nhập lại mật khẩu cho thao tác nghiệp vụ
- **Quyết định (EM):** "nếu account là chủ spa thì cứ allow hết đi đâu cần làm cho nó rườm rà như vậy nhập lại mật khẩu" ([raw](../../raw/2026-11-05-em-simplify-owner-and-opening-balance.md)). Hủy/sửa bản ghi ngày đã chốt, hủy gói + hoàn tiền, nhập sổ cũ: chỉ cần permission, **không** reauth. `kernel::corrections::authorize_closed_day`: ngày cũ thiếu `record.void_closed_day` → `403 day.locked`, có → được. `Actor` bỏ cờ `recent_auth` và `require_recent_auth`.
- **Giữ re-auth** cho thao tác bảo mật tài khoản (đổi mật khẩu, cài đặt MFA; chưa có route): guard `identity::AuthUser::require_recent_auth`, `POST /auth/reauth` giữ nguyên. H1 (xoay token ≠ xác thực lại) vẫn có hiệu lực và vẫn có test (`auth.rs::a_rotated_session_needs_a_real_reauth`).
- **Đánh đổi được EM chấp nhận:** ai cầm được session chủ spa (máy chủ spa bỏ quên khi đang đăng nhập, cookie bị đánh cắp) hủy được bản ghi ngày cũ và hoàn tiền mà không bị hỏi mật khẩu. Đây là rủi ro tiền thật (hoàn tiền mặt) và rủi ro sửa sổ sách ngày cũ.
- **Biện pháp còn lại:** MFA TOTP bắt buộc khi đăng nhập cho chủ spa; cookie `__Host-` HttpOnly + SameSite=Strict + kiểm Origin; idle timeout 12 giờ, hạn tuyệt đối 30 ngày (switch-tenant không kéo dài, H1); nút khóa nhanh; mọi hủy/hoàn có lý do bắt buộc và nằm trong `audit_log` append-only (ai, lúc nào, IP, trước/sau); hoàn ≤ giá đã bán, mỗi gói hoàn tối đa một lần (trigger DB).
- **Xem lại khi:** có lần hoàn/hủy bất thường trong audit, spa có nhiều chủ/quản lý dùng chung máy, hoặc có báo cáo sổ quỹ (spec #2) cho phép đối soát; lúc đó cân nhắc re-auth chỉ cho hoàn tiền trên một ngưỡng, hoặc idle timeout ngắn hơn cho session chủ spa.

## Hiện trạng (kiểm lại 2026-11-09)
Một số điểm ở Decision đã được thay, phần còn lại vẫn đúng:
- **Tạo tài khoản và quên mật khẩu**: không có link kích hoạt / link đặt lại. Thay bằng [ADR-0016](ADR-0016-accounts-and-passwords.md): chủ spa tạo tài khoản từ hồ sơ với mật khẩu tạm một lần, bắt đổi ở lần đăng nhập đầu; "Quên mật khẩu?" gửi yêu cầu cho chủ spa (`POST /auth/password-reset-requests`).
- **Đổi mật khẩu** đã có route `POST /auth/password` (ADR-0016). Câu "chưa có route" ở update 2026-11-05 không còn đúng.
- **Mật khẩu**: độ dài 10–128 ký tự và không nằm trong danh sách mật khẩu phổ biến (`identity::password::check_policy`). Danh sách đi kèm binary, không gọi dịch vụ ngoài ([ADR-0019](ADR-0019-audit-decisions-2026-11-09.md) D18, 2026-11-09): các mục ≥ 10 ký tự của danh sách NCSC 100k (SecLists, MIT) cộng một danh sách tiếng Việt tự giữ; so khớp không phân biệt hoa thường; lỗi `422 too_common`.
- **Quyền mỗi request** đọc trong một câu `app.session_actor(token_hash)` ([ADR-0017](ADR-0017-one-statement-request-auth.md)), không còn nạp riêng từng phần.
- **Spa bị tạm ngưng**: đăng nhập trả `403 auth.spa_suspended` khi mọi membership còn hiệu lực đều ở spa đang ngưng ([ADR-0018](ADR-0018-platform-admin-console.md)); session đang mở vẫn không nhận permission nào (M3).
- **Platform admin** không còn là role trong spa: operator đăng nhập riêng (`__Host-op-session`, TOTP luôn bắt buộc, ADR-0018). Nhánh "role giữ `platform.tenant.manage` phải MFA" trong `app.user_requires_mfa` đã bị bỏ ở migration 0041 (permission đó không còn cấp được từ 0040).
- **MFA chủ spa theo spa** ([ADR-0020](ADR-0020-owner-mfa-per-spa.md), 0050): `tenant.owner_mfa_required` (mặc định true), đổi bằng `PUT /admin/tenants/{id}/owner-mfa`; tắt giữ secret + mã khôi phục, bật lại dùng authenticator cũ.
- Lệnh operator hiện có: `app reset-mfa`, `app unlock`, `app add-operator`, `app reset-operator-mfa`, `app disable-operator` / `enable-operator` (`apps/api/crates/app/src/main.rs`).

## Consequences
- Positive: không có service phụ; token không bao giờ nằm trong JS (chống XSS lấy cắp).
- Negative: tự viết luồng auth. Giảm rủi ro bằng checklist `secure/authn-authz.md` và test tích hợp đầy đủ.
- Follow-up: khách hàng đăng nhập (OTP SĐT) khi có đặt lịch online, sẽ làm ADR riêng.

## Revisit when
Cần SSO, đăng nhập mạng xã hội, hoặc lượng user lớn đến mức nên dùng provider.
