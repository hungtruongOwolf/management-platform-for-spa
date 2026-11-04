---
title: Backend design (R1-1)
updated: 2026-11-04
sources: [product/specs/front-desk-and-packages.md, architecture/system.md, architecture/data.md, security/authz-matrix.md]
---

# Backend design R1-1

Contract-first: các bảng dưới đây là hợp đồng API, hiện thực bằng `utoipa` → `apps/api/openapi.json` (lệnh `app openapi`, commit vào repo) → `packages/api-client/src/schema.ts` (openapi-typescript). `pnpm api:gen` sinh lại cả hai; CI fail nếu lệch (S3 ✅).

## 1. Quy ước API
- REST + JSON, `snake_case`, prefix `/api/v1`. Tiền là số nguyên VND. Thời gian là ISO 8601 có offset. Ngày nghiệp vụ dạng `YYYY-MM-DD`.
- **Lỗi:** RFC 9457 `application/problem+json`: `{type, title, status, code, detail, errors?: [{field, code}]}`. `code` ổn định, frontend dịch sang tiếng Việt.
- **Danh sách:** cursor (`?cursor=&limit=`), `limit` mặc định 20, tối đa 100 (server ép). Sort và filter chỉ theo allowlist.
- **Idempotency:** mọi `POST` có tạo tiền hoặc trừ buổi bắt buộc header `Idempotency-Key` (UUID). Thiếu key → `400 idempotency.key_required`.
  - Cùng key, cùng body → trả lại response cũ.
  - Cùng key, khác body → `422 idempotency.key_reuse_mismatch`.
  - Request đầu vẫn đang chạy → request trùng **chờ** (khóa PK của Postgres) cho tới khi request đầu commit, rồi replay; nếu request đầu rollback thì request trùng chạy mới. (`409 idempotency.in_progress` chỉ là nhánh phòng thủ.) Hiện thực: `kernel::idempotency` (S1).
- **Optimistic lock:** DTO có trường `version`; ghi xong trả về `ETag: "<version>"`. Lệnh sửa (`PUT`, thay toàn bộ bản ghi) bắt buộc `If-Match`: thiếu → `400 precondition.if_match_required`, lệch phiên bản → `412 conflict.stale_version` (S4 ✅).
- **Rate-limit classes:** `auth` (5/phút/tài khoản, 20/phút/IP) · `search` (120/phút/user) · `write` (60/phút/user) · `read` (300/phút/user). Vượt → `429 rate_limited`.

## 2. Mã lỗi nghiệp vụ
| code | HTTP | Khi nào |
|---|---|---|
| `auth.unauthenticated` / `auth.reauth_required` | 401 | Chưa đăng nhập / thao tác 🔒 cần nhập lại mật khẩu |
| `auth.forbidden` | 403 | Thiếu permission |
| `not_found` | 404 | Không tồn tại **hoặc thuộc tenant khác** |
| `validation_failed` | 422 | Kèm `errors[]` |
| `customer.phone_taken` | 409 | Kèm `existing_customer_id` |
| `package.no_sessions_left` / `package.cancelled` | 409 | |
| `package.expired` | 409 | Gửi kèm `expiry_override_reason` + có permission → cho qua |
| `payment.amount_mismatch` | 422 | Tổng thanh toán khác số phải trả (R1 luôn thu đủ) |
| `discount.reason_required` | 422 | |
| `discount.limit_exceeded` | 403 | |
| `staff.inactive` | 422 | |
| `price.variant_not_available` | 422 | Ví dụ dịch vụ chỉ dành cho Nữ chọn cho khách Nam |
| `sale.has_used_sessions` | 409 | Hủy hóa đơn gói đã dùng buổi |
| `refund.exceeds_paid` | 422 | |
| `day.locked` | 403 | Quản lý sửa ngày đã chốt |
| `conflict.stale_version` | 412 | |

## 3. Endpoints
| Method & path | Mục đích (story) | Permission | Idem | Rate |
|---|---|---|---|---|
| `POST /auth/login` | Đăng nhập (SĐT, mật khẩu) → `200` + body `/me` + Set-Cookie; sai SĐT/mật khẩu/khóa/disabled đều → `401 auth.invalid_credentials` (S2 ✅; TOTP ở S2b) | công khai | — | auth (~20/phút/IP + khóa theo tài khoản) |
| `POST /auth/logout`, `/auth/logout-all` | Đăng xuất | đã đăng nhập | — | write |
| `POST /auth/reauth` | Nhập lại mật khẩu → `recent_auth_at` | đã đăng nhập | — | auth |
| `POST /auth/activate` | Kích hoạt / đặt lại mật khẩu bằng token (**spec #4**; R1 tạo tài khoản bằng CLI `app add-user`) | công khai | — | auth |
| `GET /me` | User, tenant đang dùng, permissions, memberships | đã đăng nhập | — | read |
| `POST /auth/switch-tenant` | Đổi spa đang làm việc; **xoay token** (đổi quyền); spa không phải thành viên → `404` | membership | — | write |
| `GET /service-categories` · `GET /services?active=` · `GET /package-plans?active=` | Bảng giá (US1) | `catalog.read` | — | read |
| `POST /service-categories` · `PUT /service-categories/{id}` | Thêm / đổi tên + thứ tự nhóm (S4 ✅) | `catalog.manage` | — | write |
| `POST /services` · `PUT /services/{id}` (If-Match) | Dịch vụ + `prices` (all \| female/male). **Ngừng bán = `active:false` trong PUT**, không có endpoint xóa (S4 ✅) | `catalog.manage` | — | write |
| `POST /package-plans` · `PUT /package-plans/{id}` (If-Match) | Gói: items `service × sessions + bonus` (S4 ✅) | `catalog.manage` | — | write |
| `GET /staff?status=active` | Picker KTV, người chốt | `staff.read_basic` | — | read |
| `GET /customers?q=` | Tìm theo SĐT (≥ 3 số) / tên (≥ 2 ký tự, không dấu) (US2) | `customer.read` | — | search |
| `POST /customers` | Tạo khách (US2) | `customer.write` (+ `health_notes.write` nếu gửi ghi chú) | — | write |
| `GET /customers/{id}` | Hồ sơ: liệu trình (trạng thái suy ra), tổng nợ; `health_note` chỉ khi có quyền (US2) | `customer.read` | — | read |
| `PATCH /customers/{id}` | Sửa (If-Match) | `customer.write` | — | write |
| `GET /customers/{id}/visits` | Lịch sử buổi | `visit.read` | — | read |
| `POST /customers/{id}/anonymize` 🔒 | Ẩn danh hóa | `customer.anonymize` | ✓ | write |
| `GET /customer-packages/{id}` | Chi tiết + lịch sử sử dụng (US7) | `customer.read` | — | read |
| `POST /customer-packages/{id}/extend` | Gia hạn + lý do (If-Match) (US7) | `package.extend_expiry` | ✓ | write |
| `POST /customer-packages/{id}/cancel` 🔒 | Hủy + hoàn tiền (US7) | `package.cancel_refund` | ✓ | write |
| `POST /opening-balances` | Nhập liệu trình dở dang (US8) | `opening_balance.import` | ✓ | write |
| `POST /sales` | Bán gói: lines, closer, discount, payments[] (US3) | `package.sell` (+ `sale.discount*`) | ✓ | write |
| `GET /sales/{id}` | Chi tiết hóa đơn | `sale.read` | — | read |
| `POST /sales/{id}/void` | Hủy hóa đơn + lý do (US6) | `record.void_same_day` / `_closed_day` 🔒 | ✓ | write |
| `POST /visits` | Ghi lượt: lines, check_in/out, note, discount, payments[] (US4) | `visit.record` (+ `payment.collect`, `sale.discount*`, `package.use_expired_override` khi cần) | ✓ | write |
| `GET /visits?date=` | Lượt trong ngày (mặc định hôm nay) | `visit.read` | — | read |
| `GET /visits/{id}` · `PATCH /visits/{id}` | Xem / sửa giờ ra, ghi chú (If-Match) | `visit.read` / `visit.record` | — | read/write |
| `POST /visits/{id}/void` | Hủy lượt + lý do (US6) | `record.void_same_day` / `_closed_day` 🔒 | ✓ | write |
| `POST /support-grants` · `DELETE /support-grants/{id}` | Cấp / thu hồi quyền hỗ trợ | `support_grant.manage` 🔒 | — | write |

**Thân `POST /visits` (rút gọn):**
```json
{ "customer_id": "…", "check_in_at": "2026-11-05T09:12:00+07:00", "check_out_at": null, "note": "…",
  "lines": [
    { "service_id": "…", "source": "package", "customer_package_item_id": "…", "staff_id": "…", "qty": 1 },
    { "service_id": "…", "source": "one_off", "staff_id": "…", "qty": 1, "price_variant": "male", "expiry_override_reason": null }
  ],
  "discount": { "amount": 0, "reason": null },
  "payments": [ { "method": "bank_transfer", "amount": 100000, "reference": null } ] }
```
- **Server tự quyết định giá:** `one_off` lấy `service_price` theo `price_variant`. Mặc định là giới tính của khách; khách chưa có giới tính thì bắt buộc gửi `price_variant`. Client **không gửi** đơn giá.
- `package` lấy `per_session_value` của liệu trình.
- `check_in_at`: không quá 5 phút trong tương lai, và phải thuộc ngày nghiệp vụ hiện tại, trừ khi có quyền void/sửa ngày cũ.

## 4. Use case & tính toàn vẹn dữ liệu (áp dụng [data-integrity](../../../.claude/skills/backend-design/data-integrity.md))
Mọi use case chạy trong **một transaction**, mở bởi `kernel::TenantTx`. `TenantTx` chạy `SET LOCAL app.tenant_id` và `SET LOCAL statement_timeout = '5s'`, giữ `actor` và `business_date`, và **không gọi mạng ra ngoài** trong lúc transaction mở.

| Write path | Bất biến | Chiến lược xung đột / trùng lặp |
|---|---|---|
| Trừ buổi (`POST /visits`) | `0 ≤ sessions_remaining ≤ total` | Update nguyên tử `… - qty WHERE sessions_remaining >= qty`; 0 dòng → `409`. Có `CHECK` constraint. Idempotency |
| Hoàn buổi khi hủy lượt | Không vượt `sessions_total` | `+ qty WHERE sessions_remaining + qty <= sessions_total`; lượt chuyển `voided` qua `WHERE status='completed'`, nên mỗi lượt chỉ hủy được 1 lần |
| Thu tiền (payments[] trong sale/visit) | R1: `sum(payments) = total`, `amount_paid = total` | Kiểm tra trong service trước khi insert; `CHECK (amount_paid BETWEEN 0 AND total)` vẫn giữ để bật công nợ sau này. Idempotency |
| Hủy hóa đơn | Payment không bị xóa | Mỗi payment sinh một `reversal` (`UNIQUE reversal_of`); `amount_paid` về 0; `WHERE status='open'` |
| Bán gói | Snapshot giá / tên / buổi | Transaction duy nhất tạo sale + lines + customer_package + items + payments |
| Hoàn tiền | Tổng hoàn ≤ tổng đã thu của gói | Khóa `customer_package FOR UPDATE`, kiểm tra, insert `refund`, set `cancelled_at`, đưa `sessions_remaining` về 0 |
| Sửa khách / dịch vụ / gói / gia hạn | | Optimistic lock `version` + `If-Match` → `412` |
| Tạo khách | SĐT duy nhất trong tenant | Partial `UNIQUE` → `23505` → `409 customer.phone_taken` |
| Mọi mutation | Có dấu vết | `audit_log` insert trong cùng transaction |

- Retry transaction **tối đa 3 lần** với lỗi `40001`/`40P01`.
- Bảng chỉ có `INSERT` cho `payment`, `refund`, `audit_log`.
- Không cần outbox ở R1-1 vì không có hệ thống ngoài. Sẽ thêm khi có Zalo/SMS (spec #3).

## 5. Layering (ADR-0006)
```
http (Axum route, DTO, extractor AuthUser/TenantCtx/IdempotencyKey/IfMatch)
  └─▶ service (use case, can(actor, perm), quy tắc nghiệp vụ, mở TenantTx)
        └─▶ repo (SQL qua sqlx::query!, nhận &mut TenantTx + TenantId)
domain: kiểu dữ liệu & bất biến thuần (Money, PhoneE164, SessionCount, PriceVariant, PackageStatus::derive)
```
- **Đồ thị phụ thuộc crate:** `operations → {packages, sales, catalog, crm}` (chỉ qua `service` API, truyền chung `&mut TenantTx`) · `sales → {catalog, crm}` · `packages → {catalog, crm}` · mọi crate → `kernel`. Không vòng; `ledger` (spec #2) chỉ **đọc** `sales`.
- **Cross-cutting ở `app`:** config (validate khi khởi động, fail-fast), middleware tower (tracing request-id, timeout 10 giây, giới hạn body 64 KB, rate limit, kiểm tra `Origin` cho request ghi), error mapping `AppError → problem+json` tại một chỗ, header bảo mật do Caddy đặt.
- **Tiện ích domain:** chuẩn hóa SĐT VN → E.164 (`phonenumber`); tạo `search_text` (NFC → lowercase → bỏ dấu → `đ→d`); mã hóa field AES-256-GCM (`aes-gcm`, key qua `secrecy`).

## 6. Background jobs (in-process, `tokio`)
| Job | Lịch | Retry |
|---|---|---|
| Dọn `idempotency_record` > 48 giờ, session hết hạn, auth token hết hạn | Mỗi giờ | Idempotent, chạy lại thoải mái |
| Backup `pg_dump` → mã hóa → R2 | 02:00 hằng ngày (cron trên VPS, ở `/devops`) | Alert nếu không có backup mới trong 26 giờ |
Nhắc nhở (spec #3) sẽ dùng bảng job trên Postgres (`FOR UPDATE SKIP LOCKED`) có dead-letter.

## 7. Truy vết acceptance criteria → endpoint
| Story | AC chính | Endpoint |
|---|---|---|
| US1 | 1.1–1.9 | `/services`, `/service-categories`, `/package-plans` |
| US2 | 2.1–2.7 | `GET/POST/PATCH /customers` |
| US3 | 3.1–3.7 | `POST /sales` |
| US4 | 4.1–4.10, 4.2b/c | `POST/GET/PATCH /visits` |
| US5 | hoãn | Không có endpoint thu nợ / công nợ ở R1; không có endpoint sửa/xóa payment |
| US6 | 6.1–6.4 | `POST /visits/{id}/void`, `POST /sales/{id}/void` |
| US7 | 7.1–7.4 | `/customer-packages/{id}/extend`, `/cancel` |
| US8 | 8.1, 8.3–8.5 | `POST /opening-balances` (tạo `customer_package` với `origin=opening_balance`, không có sale hay payment; báo cáo lọc theo origin) |
| G1–G6 | | Middleware + extractor + `kernel` |

## 8. Áp dụng `secure` (tóm tắt)
- AuthN, session, cookie, CSRF: ADR-0004.
- AuthZ: `can()` ở service layer, ma trận [authz-matrix](../security/authz-matrix.md).
- Tenant isolation: ADR-0003.
- Không nhận giá hay identity từ client (§3).
- Field nhạy cảm: mã hóa và bị loại khỏi response nếu thiếu quyền.
- Audit, append-only, rate limit, giới hạn body.
- Lỗi không lộ thông tin nội bộ.
- Mọi mục trong [threat-model](../security/threat-model.md) TB1–TB2 đều có ít nhất một biện pháp ở đây.
