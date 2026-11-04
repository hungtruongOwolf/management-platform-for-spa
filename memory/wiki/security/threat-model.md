---
title: Threat model (R1)
updated: 2026-11-04
sources: [architecture/system.md, product/specs/front-desk-and-packages.md]
---

# Threat model R1 (STRIDE theo trust boundary)

**Tài sản cần bảo vệ:** tiền (thanh toán, công nợ, sổ quỹ) · số buổi liệu trình khách đã trả · dữ liệu khách (SĐT, ghi chú sức khỏe) · dữ liệu nhân sự và lương (R2) · tài khoản chủ spa và platform admin · tính cách ly giữa các spa.

**Tác nhân:**
- (A1) người ngoài trên internet;
- (A2) nhân viên có tài khoản nhưng vượt quyền (ví dụ KTV muốn xem lương người khác, quản lý muốn xóa khoản thu);
- (A3) người dùng của spa khác;
- (A4) kẻ chiếm được tài khoản (lộ mật khẩu, máy quầy không khóa);
- (A5) sự cố nội bộ: deploy lỗi, mất dữ liệu.

## TB1: Browser ↔ Caddy/API (internet)
| STRIDE | Mối đe dọa | Biện pháp | Test |
|---|---|---|---|
| S | Dò hoặc nhồi mật khẩu (A1) | Argon2id, rate limit IP + tài khoản, lỗi đồng nhất, MFA cho chủ/admin (ADR-0004) | Đăng nhập sai lần thứ N → bị chậm hoặc khóa tạm |
| S | Lộ mật khẩu chủ spa (A4): kẻ tấn công chỉ có mật khẩu | TOTP bắt buộc; session sau mật khẩu chỉ là **pending MFA** (10 phút, bị `CurrentSession`/`AuthUser` từ chối, không lộ spa/permission, không gọi được `logout-all`/reauth) | `tests/mfa.rs`: session pending gọi `/me`, `/services`, `/customers` → 401; mutation đã kiểm |
| S | Dò mã TOTP 6 số | Mã sai tính vào khóa tài khoản (tăng nguyên tử trong SQL); nhập lại mật khẩu không reset bộ đếm; rate limit IP `/auth/*` | 5 mã sai → khóa, mã đúng cũng bị từ chối; 3 sai + login lại + 2 sai → khóa |
| S | Phát lại mã TOTP đã thấy/bị phishing | Mỗi bước thời gian chỉ chấp nhận 1 lần (`mfa_last_step`, UPDATE nguyên tử); mã khôi phục dùng 1 lần | Cùng mã ở lần đăng nhập thứ hai → 401; mutation đã kiểm |
| S | Enroll trước chủ spa (TOFU) khi có mật khẩu mà owner chưa enroll | Enroll ngay lần đăng nhập đầu; đã enroll thì không enroll lại được (409), chỉ operator reset; xác nhận gắn đúng ciphertext đã kiểm mã | `enroll/start` sau khi đã bật → 409 |
| I | Lộ DB (dump/backup) làm lộ secret TOTP / mã khôi phục | Secret AES-256-GCM (`APP_DATA_KEY` ngoài DB), AAD gắn user id; mã khôi phục chỉ lưu SHA-256 (80-bit entropy) | Ciphertext không chứa secret; hash không chứa mã |
| S | Đánh cắp session qua XSS | Cookie `HttpOnly`; CSP nonce-based, không `unsafe-inline`; React escape; không `dangerouslySetInnerHTML` | CSP header có mặt; XSS payload trong tên khách hiển thị dạng text |
| S | CSRF | `SameSite=Strict` + kiểm tra `Origin` | Request ghi với `Origin` lạ → 403 |
| T | Sửa request trong DevTools: đổi `price`, `tenant_id`, `staff_id`, `sessions_remaining`, `is_paid` (A2) | DTO chỉ nhận trường được phép ghi; giá lấy từ bảng giá phía server; giảm giá cần permission + lý do; tenant/user từ session | Gửi `tenant_id`/`price` lạ → bị bỏ qua hoặc `422` |
| T | Gửi lại request (replay / bấm đôi) → ghi đôi tiền hoặc trừ đôi buổi | Idempotency-Key; khóa lạc quan (optimistic lock); `CHECK sessions_remaining >= 0` | Gửi lặp cùng key → 1 bản ghi; hai request song song trừ buổi cuối → 1 thành công |
| R | Quản lý chối đã hủy hoặc giảm giá | Audit log mọi thao tác tiền/buổi (ai, khi nào, trước/sau, lý do); payment append-only | Mỗi lệnh ghi đều có dòng audit tương ứng |
| I | IDOR: đổi ID trong URL để xem khách hoặc liệu trình của spa khác (A3) hay ghi chú sức khỏe khi không có quyền (A2) | Query luôn scope theo tenant + RLS; ghi chú sức khỏe bị loại khỏi response nếu thiếu permission; ID là UUIDv7 | Ma trận test: tenant B gọi ID của tenant A → 404; thiếu quyền → không có field `health_note` |
| I | Lộ thông tin qua lỗi hoặc stack trace | Lỗi dạng problem+json chung chung; chi tiết chỉ nằm trong log | Gây lỗi 500 → response không có stack/SQL |
| D | Spam request làm chậm hệ thống | Rate limit theo IP (S1: 5/giây, burst 60) + theo route (S2+); body ≤ 64 KB; timeout 10 giây; phân trang có trần. **Ràng buộc:** IP lấy từ `X-Forwarded-For` (Caddy đặt), nên API **chỉ được nhận traffic từ Caddy**: không publish port API, chỉ lắng nghe trong network nội bộ của Docker | Vượt ngưỡng → 429 |
| E | Gọi endpoint quản trị bằng tài khoản thường (A2); route guard frontend bị bỏ qua | `can()` ở service layer cho mọi route; route không khai báo permission thì CI fail; endpoint platform admin tách riêng + MFA | Test mỗi route với role không đủ quyền → 403 |

## TB2: API ↔ Postgres (Supabase)
| STRIDE | Mối đe dọa | Biện pháp |
|---|---|---|
| S | Lộ chuỗi kết nối DB | Secret trong env của VPS (file quyền 600), không vào git; gitleaks trong CI; xoay mật khẩu khi nghi ngờ |
| T | SQL injection | Chỉ dùng `sqlx::query!` với tham số bind; sort/filter qua allowlist |
| T | Bug hoặc người sửa thẳng dữ liệu tiền | Role `app_rw` chỉ DML, không DDL; không có `UPDATE/DELETE` trên `payment`, `audit_log` (REVOKE); migration chạy bằng role riêng |
| I | Truy cập qua Supabase Data API / anon key | Data API tắt; bảng ở schema `app`; RLS bật trên mọi bảng (ADR-0002) |
| I | Bug quên lọc tenant | RLS theo `app.tenant_id` (ADR-0003) + newtype `TenantId` bắt buộc trong repo |
| D | Hết connection | Pool giới hạn; dùng Supavisor; timeout query |

## TB3: API ↔ R2 (backup)
| STRIDE | Mối đe dọa | Biện pháp |
|---|---|---|
| I | Lộ file backup | Bucket private; mã hóa file trước khi upload (age/gpg); key chỉ cho phép ghi vào bucket backup |
| T | Kẻ xâm nhập xóa backup | Bucket versioning + object lock (retention 30 ngày); key upload không có quyền xóa |
| D | Backup lỗi âm thầm | Alert nếu không có backup mới trong 26 giờ; diễn tập khôi phục hằng tháng |

## TB4: Máy chủ VPS
| Mối đe dọa | Biện pháp |
|---|---|
| Chiếm quyền SSH | Chỉ SSH key, tắt root login, fail2ban; firewall chỉ mở 80/443/SSH |
| Lỗ hổng OS/package | Cập nhật bảo mật tự động; image distroless, user non-root |
| Lỗ hổng dependency | `cargo audit`, `cargo deny`, `pnpm audit` trong CI; Renovate |

## Mối đe dọa nội bộ & vận hành
| Mối đe dọa | Biện pháp |
|---|---|
| Máy quầy bỏ quên, đang đăng nhập (A4) | Idle timeout 12 giờ; nút khóa nhanh; thao tác nhạy cảm (hủy ngày đã chốt, hoàn tiền) yêu cầu nhập lại mật khẩu |
| Dùng chung tài khoản → mất trách nhiệm | Mỗi người một tài khoản (vision rủi ro #5); audit theo user |
| Platform admin xem dữ liệu spa tùy ý | Mặc định không có quyền đọc; chủ spa cấp support grant 24 giờ (thu hồi được); mọi truy cập đều audit và chủ spa xem được (ADR-0003) |
| Mất dữ liệu (A5) | Backup Supabase + dump đêm ra R2 + diễn tập khôi phục |

## Còn mở
- Danh sách thao tác yêu cầu nhập lại mật khẩu: chốt ở `/backend-design`.
