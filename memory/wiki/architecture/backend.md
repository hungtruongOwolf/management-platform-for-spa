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
| `customer.phone_taken` | 409 | Kèm `existing_id` (ID hồ sơ đã có) |
| `package.no_sessions_left` / `package.cancelled` | 409 | |
| `package.expired` | 409 | Gửi kèm `expiry_override_reason` + có permission → cho qua |
| `payment.amount_mismatch` | 422 | Tổng thanh toán khác số phải trả (R1 luôn thu đủ) |
| `discount.reason_required` | 422 | |
| `discount.limit_exceeded` | 403 | |
| `staff.inactive` | 422 | Người chốt / KTV không tồn tại, đã nghỉ, hoặc thuộc spa khác (S6 ✅) |
| `package.item_not_found` | 422 | Dòng liệu trình không có hoặc của khách khác (AC4.8) |
| `idempotency.key_reuse_mismatch` | 422 | Cùng `Idempotency-Key`, khác body |
| `price.variant_not_available` | 422 | Ví dụ dịch vụ chỉ dành cho Nữ chọn cho khách Nam |
| `price.gender_required` | 422 | Khách chưa có giới tính, dịch vụ có giá Nữ/Nam, không gửi `price_variant` (S7) |
| `visit.check_in_in_future` / `visit.check_in_not_today` | 422 | Giờ vào > now + 5 phút / khác ngày nghiệp vụ hôm nay (S7) |
| `visit.voided` | 409 | Sửa lượt đã hủy (S7, phòng cho S8) |
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
| `GET /staff?status=active` | Picker KTV, người chốt; `status` = `active` (mặc định), `left` hoặc `all`; chỉ trả tên, chức vụ, trạng thái (S6 ✅) | `staff.read_basic` | — | read |
| `GET /customers?q=` | Tìm theo SĐT (≥ 3 số) / tên (≥ 2 ký tự, không dấu) (US2) | `customer.read` | — | search |
| `POST /customers` | Tạo khách (US2) | `customer.write` (+ `health_notes.write` nếu gửi ghi chú) | — | write |
| `GET /customers/{id}` | Hồ sơ: liệu trình (trạng thái suy ra), tổng nợ; `health_note` chỉ khi có quyền (US2) | `customer.read` | — | read |
| `PUT /customers/{id}` | Sửa hồ sơ, thay toàn bộ (If-Match); không gồm ghi chú sức khỏe | `customer.write` | — | write |
| `PUT /customers/{id}/health-note` | Đặt/xóa ghi chú sức khỏe (If-Match) | `customer.health_notes.write` | — | write |
| `GET /customers/{id}/visits` | Lịch sử buổi | `visit.read` | — | read |
| `POST /customers/{id}/anonymize` 🔒 | Ẩn danh hóa | `customer.anonymize` | ✓ | write |
| `GET /customer-packages/{id}` | Chi tiết + items, trạng thái suy ra (S6 ✅; lịch sử sử dụng thêm ở S7) | `customer.read` | — | read |
| `GET /customers/{id}/packages` | Liệu trình của khách, mới nhất trước, còn X/Y buổi (S6 ✅) | `customer.read` | — | read |
| `POST /customer-packages/{id}/extend` | Gia hạn + lý do (If-Match) (US7) | `package.extend_expiry` | ✓ | write |
| `POST /customer-packages/{id}/cancel` 🔒 | Hủy + hoàn tiền (US7) | `package.cancel_refund` | ✓ | write |
| `POST /opening-balances` | Nhập liệu trình dở dang (US8, S9 ✅): `customer_id` **hoặc** `customer{…như POST /customers}` + `packages[{package_plan_id?, name?, sold_price, starts_on, expires_on?, gift_note?, items[{service_id, sessions_purchased, sessions_bonus?, sessions_remaining}]}]` (1–10). `201 {customer_id, customer_created, customer_packages}`; không sale/payment; lỗi field có tiền tố `customer.` / `packages[j].`; bắt buộc `Idempotency-Key` | `opening_balance.import` (+ `customer.write` khi tạo khách) | ✓ | write |
| `POST /sales` | Bán gói: `lines[{package_plan_id, gift_note?}]` (1–10), `closer_staff_id`, `discount{amount, reason}?`, `payments[{method, amount, reference?}]` (0–2, mỗi hình thức 1 lần). `201` trả `SaleDto` kèm `customer_packages` (US3, S6 ✅) | `package.sell` (+ `sale.discount*`) | ✓ | write |
| `GET /sales/{id}` | Chi tiết hóa đơn: lines, payments, customer_packages (S6 ✅) | `sale.read` | — | read |
| `POST /sales/{id}/void` | Hủy hóa đơn + lý do (US6) | `record.void_same_day` / `_closed_day` 🔒 | ✓ | write |
| `POST /visits` | Ghi lượt: `lines` (1–20), `check_in_at?`, `note?`, `payments[]` (US4). `201` trả `VisitDto` (không bao giờ có `note`) (S7 ✅) | `visit.record` (+ `payment.collect` khi có tiền, `customer.health_notes.write` khi có ghi chú, `package.use_expired_override` khi có lý do) | ✓ | write |
| `GET /visits?date=` | Lượt của một ngày nghiệp vụ (mặc định hôm nay), mới nhất trước, kèm lines + payments, **không có ghi chú** (S7 ✅) | `visit.read` | — | read |
| `GET /visits/{id}` · `PATCH /visits/{id}` | Xem (có `note` nếu có `customer.health_notes.read`, `ETag`) / sửa ghi chú và KTV từng dòng (If-Match; không đụng buổi hay tiền; ngày khác hôm nay → `403 day.locked` trừ khi có `record.void_closed_day`) (S7 ✅) | `visit.read` / `visit.record` (+ `customer.health_notes.write` khi sửa ghi chú) | — | read/write |
| `POST /visits/{id}/void` | Hủy lượt + lý do (US6) | `record.void_same_day` / `_closed_day` 🔒 | ✓ | write |
| `POST /support-grants` · `DELETE /support-grants/{id}` | Cấp / thu hồi quyền hỗ trợ | `support_grant.manage` 🔒 | — | write |

**Thân `POST /visits` (rút gọn):**
```json
{ "customer_id": "…", "check_in_at": "2026-11-05T09:12:00+07:00", "note": "…",
  "lines": [
    { "service_id": "…", "source": "package", "customer_package_item_id": "…", "staff_id": "…", "qty": 1,
      "expiry_override_reason": null },
    { "service_id": "…", "source": "one_off", "staff_id": "…", "qty": 1, "price_variant": "male" }
  ],
  "payments": [ { "method": "bank_transfer", "amount": 100000, "reference": null } ] }
```
- **Không có giờ ra** và **không có giảm giá** trong lượt ở R1 (EM 2026-11-04; giảm giá dịch vụ lẻ để sau, xem S7 "Kết quả").
- **Server tự quyết định giá:** `one_off` lấy giá của dịch vụ (khóa `FOR SHARE`) theo giới tính của khách. Khách chưa có giới tính: dịch vụ giá chung thì không cần gì, dịch vụ có giá Nữ/Nam thì bắt buộc gửi `price_variant` (thiếu → `422 price.gender_required`). Khách đã có giới tính mà gửi `price_variant` khác → `422 validation_failed` (`lines[i].price_variant`). Dịch vụ không có giá cho giới tính đó → `422 price.variant_not_available`. Field lạ (`unit_price`, `total`) bị bỏ qua.
- `package`: `customer_package_item_id` bỏ trống → server chọn gói còn dùng được (chưa hủy, chưa hết hạn, còn buổi) chứa dịch vụ, **hết hạn sớm nhất trước, không thời hạn sau cùng, rồi mua sớm nhất** (AC4.3); không có → `422 package.item_not_found`. Item không chứa `service_id` của dòng → `422 validation_failed` (`not_in_package`). `unit_price` = `per_session_value`, `line_value` = giá trị buổi thực trừ (buổi tặng = 0).
- Các item được trừ theo thứ tự `item_id` để hai lượt nhiều dòng tranh cùng các item không deadlock. Dòng `one_off` tạo một `sale` (không người chốt, `line_type='service'`) qua `sales::service::record_service_sale` trong cùng transaction; `sum(payments)` phải bằng tổng dòng lẻ (lượt chỉ trừ buổi mà gửi payment → `422 payment.amount_mismatch`).
- Ghi chú buổi mã hóa AES-256-GCM (`kernel::crypto`, AAD `visit.note` + tenant + visit). Response lưu trong idempotency và audit **không bao giờ** chứa ghi chú.
- `POST /sales` (S6): field lạ trong body (ví dụ `unit_price`, `total`) bị **bỏ qua**, không báo lỗi; mọi số tiền tính từ catalog. Request hash của idempotency = JSON đã parse rồi serialize lại (key đã sắp xếp), nên khác khoảng trắng hay thứ tự key vẫn là cùng một request.
- `check_in_at` (mặc định = lúc lưu): quá 5 phút trong tương lai → `422 visit.check_in_in_future`; khác ngày nghiệp vụ hiện tại (theo múi giờ của spa) → `422 visit.check_in_not_today`. Ghi lượt cho ngày cũ chưa hỗ trợ (để S8 / sửa ngày đã chốt).

## 4. Use case & tính toàn vẹn dữ liệu (áp dụng [data-integrity](../../../.claude/skills/backend-design/data-integrity.md))
Mọi use case chạy trong **một transaction**, mở bởi `kernel::TenantTx`. `TenantTx` chạy `SET LOCAL app.tenant_id` và `SET LOCAL statement_timeout = '5s'`, giữ `actor` và `business_date`, và **không gọi mạng ra ngoài** trong lúc transaction mở.

| Write path | Bất biến | Chiến lược xung đột / trùng lặp |
|---|---|---|
| Trừ buổi (`POST /visits`) | `0 ≤ sessions_remaining ≤ total` | Update nguyên tử `… - qty WHERE sessions_remaining >= qty`; 0 dòng → `409`. Có `CHECK` constraint. Idempotency |
| Hoàn buổi khi hủy lượt | Không vượt `sessions_total` | `+ qty WHERE sessions_remaining + qty <= sessions_total`; lượt chuyển `voided` qua `WHERE status='completed'`, nên mỗi lượt chỉ hủy được 1 lần |
| Thu tiền (payments[] trong sale/visit) | R1: `sum(payments) = total`, `amount_paid = total` | Kiểm tra trong service trước khi insert; `CHECK (amount_paid BETWEEN 0 AND total)` vẫn giữ để bật công nợ sau này. Idempotency |
| Hủy hóa đơn | Payment không bị xóa | Mỗi payment sinh một `reversal` (`UNIQUE reversal_of`); `amount_paid` về 0; `WHERE status='open'` |
| Bán gói | Snapshot giá / tên / buổi | Transaction duy nhất tạo sale + lines + customer_package + items + payments |
| Nhập sổ cũ (S9) | `sessions_remaining ≤ purchased + bonus`; không sale, không payment | Transaction duy nhất: khách (tìm hoặc tạo qua `crm::service::create_in`) + các gói `origin=opening_balance` + audit. Idempotency |
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
| US2 | 2.1–2.7 | `GET/POST /customers`, `GET/PUT /customers/{id}`, `PUT /customers/{id}/health-note` |
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
