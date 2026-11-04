---
title: ADR-0004 Authentication & sessions
status: accepted
date: 2026-11-04
deciders: EM, agent
---

# ADR-0004: Đăng nhập & session

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
- **MFA TOTP bắt buộc** cho Chủ spa và Platform admin; tùy chọn cho người khác. *(Hiện thực ở ticket S2b; S2 đã có mật khẩu, khóa tài khoản, session, reauth.)*
- Phân quyền: permission-based, role là dữ liệu ([principles](../architecture/principles.md) #2). Permissions được nạp vào `AuthUser` mỗi request từ session.

## Consequences
- Positive: không có service phụ; token không bao giờ nằm trong JS (chống XSS lấy cắp).
- Negative: tự viết luồng auth. Giảm rủi ro bằng checklist `secure/authn-authz.md` và test tích hợp đầy đủ.
- Follow-up: khách hàng đăng nhập (OTP SĐT) khi có đặt lịch online, sẽ làm ADR riêng.

## Revisit when
Cần SSO, đăng nhập mạng xã hội, hoặc lượng user lớn đến mức nên dùng provider.
