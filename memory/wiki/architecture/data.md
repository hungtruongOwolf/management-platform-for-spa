---
title: Data model (R1-1)
updated: 2026-11-05
sources: [product/specs/front-desk-and-packages.md, product/specs/management-views.md, architecture/system.md, decisions/ADR-0003-multi-tenancy.md]
---

# Data model R1-1 (PostgreSQL, schema `app`)

## Quy ước chung
- **PK:** `id uuid` dạng UUIDv7, sinh trong app. Bảng của tenant có `tenant_id uuid NOT NULL` và `UNIQUE (tenant_id, id)`. **FK luôn kép** `(tenant_id, x_id) → x(tenant_id, id)`, nên không thể trỏ chéo sang spa khác.
- **Role:** `app_rw` là role cấp cluster, **không** tạo trong migration (`infra/postgres-init/01-roles.sql` cho dev/CI; tạo tay một lần trên Supabase ở S11). App runtime kết nối bằng `APP_DATABASE_URL` (app_rw). Migration, compile-time check của sqlx và test dùng `DATABASE_URL` (owner).
- **RLS** bật trên mọi bảng tenant: `USING (tenant_id = current_setting('app.tenant_id', true)::uuid)`, `WITH CHECK` giống hệt. Role `app_rw` không phải owner bảng, không có `BYPASSRLS`.
- **Cột chung:** `created_at timestamptz NOT NULL DEFAULT now()`, `created_by uuid` (user). Bảng có thể bị sửa thêm `updated_at` và `version bigint NOT NULL DEFAULT 1` (optimistic lock, API dùng `If-Match`).
- **Tiền:** `bigint` đồng VND, `CHECK (>= 0)`. **Thời gian:** `timestamptz`. **Ngày nghiệp vụ** `business_date date` do server tính theo `tenant.timezone`, không bao giờ nhận từ client.
- **Văn bản:** chuẩn hóa NFC ở biên. Cột `search_text` = chữ thường, bỏ dấu, `đ→d`, do app tính.
- **Dữ liệu nhạy cảm:** `health_note`, `visit.note` mã hóa ở tầng app bằng AES-256-GCM (`*_ct bytea`, tiền tố phiên bản key). Key nằm trong env `APP_DATA_KEY` (base64, 32 byte), không bao giờ lưu trong DB. Ciphertext = `b'v'` + số phiên bản key + nonce 12 byte + ciphertext; AAD = tenant_id + customer_id nên không chép được ciphertext sang khách khác. Xoay key: thêm `APP_DATA_KEY_V{n}` khi cần (chưa làm).
- **Bảng append-only** (`payment`, `refund`, `audit_log`, `sale_line`): app role chỉ có `INSERT, SELECT`. `payment`, `sale_line` và `refund` có thêm trigger `app.reject_mutation` (BEFORE UPDATE OR DELETE), chặn cả schema owner (S6, S8).
- **Sửa sai bằng bút toán đảo (S8, 0009):** hủy hóa đơn thêm một dòng `payment.kind='reversal'` cho mỗi payment (cùng sale, method, amount; `business_date` = ngày hủy). Trigger `payment_reversal_mirrors_original` kiểm tra điều đó và chặn đảo một reversal; `UNIQUE (tenant_id, reversal_of)` chặn đảo 2 lần. Trạng thái chỉ đi tới: trigger `visit_voided_is_final`, `sale_voided_is_final`, `customer_package_cancelled_is_final` chặn mọi UPDATE sau khi `voided`/`cancelled`; `customer_package_item_no_restore_when_cancelled` chặn cộng buổi vào gói đã hủy. CHECK: sale `voided` phải có `voided_by`, `amount_paid = 0`, lý do 1–500; gói `cancelled` phải có `cancelled_by` + lý do.

## ER
```mermaid
erDiagram
  tenant ||--o{ branch : has
  user_account ||--o{ membership : "belongs to"
  tenant ||--o{ membership : has
  membership ||--o{ membership_role : has
  role ||--o{ membership_role : grants
  role ||--o{ role_permission : contains
  user_account ||--o{ session : has
  tenant ||--o{ staff : employs
  membership |o--o| staff : "may link"
  service_category ||--o{ service : groups
  service ||--o{ service_price : "priced by"
  package_plan ||--o{ package_plan_item : contains
  service ||--o{ package_plan_item : in
  customer ||--o{ customer_package : owns
  customer_package ||--o{ customer_package_item : contains
  customer ||--o{ visit : makes
  visit ||--o{ visit_line : contains
  visit_line }o--o| customer_package_item : "consumes session"
  visit_line }o--|| staff : "performed by"
  customer ||--o{ sale : buys
  sale ||--o{ sale_line : contains
  sale ||--o{ payment : "paid by"
  sale |o--o| visit : "one-off lines of"
  customer_package }o--o| sale : "sold in"
  customer_package ||--o{ refund : "refunded by"
```

## Bảng

### identity / org
| Bảng | Cột chính | Ràng buộc / index |
|---|---|---|
| `tenant` | name, slug, timezone (`Asia/Ho_Chi_Minh`), status, settings `jsonb` (ví dụ `manager_discount_max_pct=10`) | `UNIQUE(slug)`; không RLS (chỉ identity đọc) |
| `branch` | tenant_id, name, address, active | `UNIQUE(tenant_id, name)` |
| `user_account` *(toàn cục, ✅ 0002)* | phone_e164, full_name, password_hash (`$argon2id$`), status (`active`/`disabled`), failed_logins, locked_until. MFA (mfa_secret_ct, mfa_enabled) thêm ở S2b | `UNIQUE(phone_e164)`. Một người có thể làm ở nhiều spa (qua membership) |
| `membership` | tenant_id, user_id, status (liên kết với `staff` để làm "xem dữ liệu của mình" ở R2; S6 chưa thêm) | `UNIQUE(tenant_id, user_id)`; liệt kê spa của user khi login qua hàm `SECURITY DEFINER` `app.tenants_of_user` (chưa có tenant context) |
| `role` | tenant_id, key, name, is_system | `UNIQUE(tenant_id, key)`; seed `owner`, `manager`, `technician` |
| `role_permission` | tenant_id, role_id, permission `text` | PK `(tenant_id, role_id, permission)`; permission phải thuộc danh mục trong code (app kiểm tra khi ghi) |
| `membership_role` | tenant_id, membership_id, role_id | PK cả ba cột |
| `session` *(toàn cục)* | token_hash `bytea`, user_id, active_tenant_id?, created_at, last_seen_at, idle_expires_at, absolute_expires_at, recent_auth_at, ip, user_agent, revoked_at | `UNIQUE(token_hash)`; index `(user_id)` |
| `auth_token` *(toàn cục)* | user_id, purpose (`activate`/`reset`), token_hash, expires_at, used_at | `UNIQUE(token_hash)` |
| `platform_admin` *(toàn cục)* | user_id | |
| `support_grant` | tenant_id, platform_admin_id, granted_by, expires_at, revoked_at | index `(tenant_id, expires_at)` |
| `staff` ✅ 0005 | tenant_id, branch_id, full_name, phone_e164?, position (chữ tự do), status (`active`/`left`), joined_on?, left_on?, version | `CHECK (status='left') = (left_on IS NOT NULL)`; index `(tenant_id, status, full_name)`; không có DELETE. Context khác kiểm tra bằng `org::service::require_active` (khóa `FOR SHARE`) |

### catalog
| Bảng | Cột chính | Ràng buộc / index |
|---|---|---|
| `service_category` | tenant_id, name, sort_order | `UNIQUE(tenant_id, name)` |
| `service` | tenant_id, category_id, name, description, duration_min?, recommended, active, version | `UNIQUE(tenant_id, name)`; `CHECK (duration_min IS NULL OR duration_min > 0)` |
| `service_price` | tenant_id, service_id, variant (`all`/`female`/`male`), amount | PK `(tenant_id, service_id, variant)`. App ép quy tắc: hoặc chỉ có `all`, hoặc ≥ 1 trong `female`/`male`. Thiếu một giới tính = không áp dụng cho giới tính đó |
| `package_plan` | tenant_id, name, price, validity_months?, gift_note, active, version | partial `UNIQUE(tenant_id, name) WHERE active`; `CHECK (validity_months IS NULL OR validity_months > 0)` |
| `package_plan_item` | tenant_id, plan_id, service_id, sessions, bonus_sessions | `CHECK (sessions > 0 AND bonus_sessions >= 0)`; `UNIQUE(tenant_id, plan_id, service_id)` |
Buổi tặng đặt ở **từng dòng** dịch vụ, để gói combo không bị mơ hồ.

### crm
| Bảng | Cột chính | Ràng buộc / index |
|---|---|---|
| `customer` | tenant_id, full_name, search_text, phone_e164, gender? (`female`/`male`), birth_year?, address, source, note, health_note_ct?, anonymized_at?, version | partial `UNIQUE(tenant_id, phone_e164) WHERE anonymized_at IS NULL`; GIN `pg_trgm` trên `search_text` và trên `phone_e164` (tìm theo ≥ 3 chữ số) |

### packages
| Bảng | Cột chính | Ràng buộc / index |
|---|---|---|
| `customer_package` ✅ 0005 (+ `cancelled_by` 0009) | tenant_id, customer_id, plan_id?, name_snapshot, gift_note, sold_price, origin (`sale`/`opening_balance`), sale_id?, closer_staff_id?, starts_on, expires_on?, cancelled_at?, cancel_reason?, cancelled_by?, version | `CHECK (origin <> 'sale' OR (sale_id IS NOT NULL AND closer_staff_id IS NOT NULL))`; `CHECK (expires_on IS NULL OR expires_on >= starts_on)`; index `(tenant_id, customer_id)` |
| `customer_package_item` ✅ 0005 | tenant_id, customer_package_id, service_id, name_snapshot, sessions_purchased, sessions_bonus, sessions_total (generated = purchased + bonus), sessions_remaining, per_session_value, version | `CHECK (sessions_remaining BETWEEN 0 AND sessions_total)`; index `(tenant_id, customer_package_id)` |

**Trạng thái được suy ra, không lưu**, nên không bao giờ lệch:
- `cancelled` nếu có `cancelled_at`;
- `exhausted` nếu tổng `sessions_remaining` = 0;
- `expired` nếu `expires_on < business_today`;
- còn lại là `active`.

Thứ tự ưu tiên đúng như trên. Không cần job cập nhật trạng thái.

### operations
| Bảng | Cột chính | Ràng buộc / index |
|---|---|---|
| `visit` ✅ 0007 | tenant_id, branch_id, customer_id, business_date, check_in_at, note_ct?, status (`completed`/`voided`), void_reason?, voided_at?, voided_by?, sale_id?, created_by, version. **Không có giờ ra** (EM) | `CHECK (status <> 'voided' OR (void_reason, voided_at, voided_by NOT NULL))`; `UNIQUE(tenant_id, sale_id)` (một sale cho một lượt); index `(tenant_id, branch_id, business_date, check_in_at DESC)`, `(tenant_id, customer_id, check_in_at DESC)`. app_rw: SELECT/INSERT/UPDATE, không DELETE |
| `visit_line` ✅ 0007 | tenant_id, visit_id, position, service_id, service_name_snapshot, source (`package`/`one_off`), customer_package_id?, customer_package_item_id?, sessions_left_after?, staff_id, qty (1–50), price_variant?, unit_price, line_value, expiry_override_reason? | Một `CHECK` theo `source`: `package` cần package + item + `sessions_left_after`, không có `price_variant`; `one_off` không có package, có `price_variant`, `line_value = qty * unit_price`. `unit_price` = giá niêm yết (lẻ) hoặc `per_session_value` (gói); `line_value` = giá trị buổi thực trừ (tặng = 0) → cơ sở hoa hồng R2. index `(tenant_id, visit_id)`, `(tenant_id, staff_id)` (AC4.4), `(tenant_id, customer_package_item_id)` (lịch sử dùng gói). app_rw: SELECT/INSERT + `UPDATE (staff_id)` (sửa KTV) |

### sales
| Bảng | Cột chính | Ràng buộc / index |
|---|---|---|
| `sale` ✅ 0005 | tenant_id, branch_id, customer_id, business_date, subtotal, discount_amount, discount_reason?, total, amount_paid, status (`open`/`voided`), closer_staff_id?, void_reason?, version, **note_ct?** (0030, "Ghi chú gói" mã hóa, AAD `sale.note` + tenant + hóa đơn; chỉ hóa đơn gói) | `CHECK (total = subtotal - discount_amount AND total >= 0)`; `CHECK (discount_amount = 0 OR discount_reason IS NOT NULL)`; **`CHECK (amount_paid BETWEEN 0 AND total)`** |
| `sale_line` ✅ 0005 *(bất biến)* | tenant_id, sale_id, line_type (`package`/`service`; để mở cho `product`), service_id?, package_plan_id?, customer_package_id?, description_snapshot, qty, unit_price, line_total, position | `CHECK (line_total = qty * unit_price)`; CHECK cột theo `line_type`; `UNIQUE(tenant_id, sale_id, position)`; trigger chặn UPDATE/DELETE |
| `payment` ✅ 0005 *(append-only)* | tenant_id, branch_id, sale_id, business_date, kind (`payment`/`reversal`), method (`cash`/`bank_transfer`), amount, reversal_of?, reference?, paid_at | `CHECK (amount > 0)`; `CHECK ((kind='reversal') = (reversal_of IS NOT NULL))`; `UNIQUE(tenant_id, reversal_of)` (mỗi payment chỉ đảo được 1 lần); index `(tenant_id, branch_id, business_date)`, `(tenant_id, sale_id)` |
| `refund` ✅ 0009 *(append-only)* | tenant_id, branch_id, customer_id, customer_package_id, business_date, method (`cash`/`bank_transfer`), amount, reason, created_by | `CHECK (amount > 0)`; `reason` 1–500; `UNIQUE (tenant_id, customer_package_id)` (một gói hoàn 1 lần); trigger `refund_within_paid` (amount ≤ `sold_price`); trigger `reject_mutation`; app_rw chỉ `SELECT, INSERT`; index `(tenant_id, branch_id, business_date)` cho sổ quỹ |

**Công nợ:** hoãn (EM 2026-11-04). R1 luôn ghi `amount_paid = total`. Cột và `CHECK` được giữ lại để sau này bật công nợ bằng cách thêm view `receivable`, không cần đổi schema.

### kernel
| Bảng | Cột chính | Ràng buộc / index |
|---|---|---|
| `idempotency_record` | tenant_id, actor_user_id, key, request_hash, response_status, response_body `jsonb`, created_at | PK `(tenant_id, actor_user_id, key)`; claim + complete trong **cùng transaction** với use case; job dọn > 48 giờ qua hàm `SECURITY DEFINER` `app.purge_idempotency_records` (app_rw không cần BYPASSRLS) |
| `audit_log` *(append-only)* | tenant_id?, actor_user_id, actor_kind (`user`/`platform_admin`/`system`), action, entity_type, entity_id, before `jsonb`, after `jsonb`, reason?, ip, at | index `(tenant_id, entity_type, entity_id, at)`. Trường sức khỏe ghi `"[redacted]"` |

## Migrations (forward-only, `sqlx migrate`)
`0001_kernel` ✅ (pg_trgm, schema `app`, `app.current_tenant()`, tenant, branch, audit_log, idempotency_record, RLS + grants) → `0002_identity_org` → `0003_catalog` → `0004_crm` ✅ → `0005_sales_packages` ✅ (staff, sale, sale_line, payment, customer_package, customer_package_item) → `0006_mfa` ✅ (S2b) → `0007_visits` ✅ (S7: visit, visit_line) → `0009_void_lifecycle` ✅ (S8) → `0011_security_review` ✅ (H1, M3; M2 window + `refund_cap`) → `0012_simplify_owner_and_opening_balance` (EM 2026-11-05: bỏ `opening_balance_window`, quyền `opening_balance.manage_window`, cột `refund_cap`; trigger `refund_within_paid` về lại `amount ≤ sold_price`) → `0013_management_views` (R1-2, xem mục Read models) → `0014_follow_ups_and_search` (vòng 3, xem mục Follow-up) → `0015_long_range_reports` → `0016_appointments` (vòng 4, xem mục Lịch hẹn) → `0017_staff_management` (xem mục Quản lý nhân viên) → `0018_paper_book_sales` (tiền sổ giấy là hóa đơn, xem mục dưới) → `0019_calendar_timeline` (vòng 5: `appointment.group_id`, `appointment_setting`, quyền `appointment.settings`; xem mục Lịch hẹn) → `0020_visit_corrections` (Sửa lượt: `visit.corrects_visit_id`). 0008, 0010 bỏ trống. Từ 0016 crate `kernel` có `build.rs` (`rerun-if-changed=../../migrations`): trước đó thêm file migration mới không làm build lại migrator nhúng, `app migrate` lặng lẽ bỏ qua nó. 0011 đã có thể chạy trên staging nên không sửa, chỉ thêm 0012. Kế hoạch tách riêng migration RLS/grants ban đầu đã bỏ: mỗi migration tự bật RLS và cấp quyền cho bảng của nó.

Thay đổi về sau theo **expand → backfill theo lô → kiểm tra → contract**. Mỗi migration được test trên snapshot giống production.

## Lịch hẹn, nhập bù, giảm giá lượt (0016, vòng 4, [spec](../product/specs/appointments.md))
- **`app.appointment`**: id, tenant_id, branch_id, `customer_id?` **hoặc** (`guest_name`, `guest_phone` E.164) (CHECK đúng một), `business_date`, `starts_at`, `ends_at` (> starts, ≤ 8 giờ), `staff_id?`, `status` ∈ booked/arrived/done/no_show/cancelled, `note ≤ 500` (không phải dữ liệu sức khỏe), `status_reason` (bắt buộc với no_show/cancelled), `arrived_at` (bắt buộc khi arrived, cùng `staff_id`), `closed_at/closed_by` (đúng khi done/no_show/cancelled), `visit_id` (đúng khi done; `UNIQUE (tenant_id, visit_id)`, FK → visit), created_by/at, updated_at, `version`. FK tenant-scoped tới branch, customer, staff, visit.
  - `EXCLUDE USING gist (tenant_id =, staff_id =, tstzrange(starts_at, ends_at) &&) WHERE status IN ('booked','arrived') AND staff_id IS NOT NULL` (extension `btree_gist`, trusted). Lớp thứ hai sau khóa advisory theo (spa, ngày) của service.
  - Trigger `appointment_closed_is_final`: lịch đã done/no_show/cancelled không UPDATE được nữa.
  - Index `(tenant_id, business_date, starts_at)` (lịch ngày/tuần), `(tenant_id, customer_id, starts_at DESC)`.
  - app_rw: `SELECT, INSERT, UPDATE` (không DELETE). RLS `tenant_isolation`.
- **`app.appointment_service`** (tenant_id, appointment_id, position 0–4, service_id, service_name_snapshot): app_rw `SELECT, INSERT, DELETE` (thay danh sách khi đổi lịch). RLS.
- **`visit_line.discount_amount`** `bigint ≥ 0` (mặc định 0): phần giảm giá của lượt chia cho dòng lẻ. CHECK `visit_line_check` thay bằng bản mới: dòng gói `discount_amount = 0`; dòng lẻ `discount_amount ≤ qty × unit_price` và `line_value = qty × unit_price − discount_amount`. Dữ liệu cũ hợp lệ ngay (mặc định 0).
- **Quyền:** `appointment.manage` cho role hệ thống owner + manager, `visit.backdate` cho owner.
- **0019 (vòng 5, [spec §10](../product/specs/appointments.md)):**
  - `appointment.group_id uuid NULL`: các lịch đặt cùng lúc cho một nhóm / gia đình (mỗi người vẫn là một lịch, một lượt, một thanh toán). Index `(tenant_id, group_id) WHERE group_id IS NOT NULL`. Không có bảng nhóm (nhóm chỉ là id chung), nên không FK.
  - **`app.appointment_setting`** (tenant_id PK → tenant, `max_per_slot int NULL CHECK 1–200`, updated_by, updated_at): số khách tối đa mỗi khung giờ, **chỉ để cảnh báo**, NULL = không giới hạn. Bảng riêng thay vì `tenant.settings` để app_rw không cần UPDATE trên `app.tenant`. app_rw `SELECT, INSERT, UPDATE` (không DELETE), RLS `tenant_isolation`. Ghi bằng upsert, audit `appointment.settings` (before/after).
  - Quyền `appointment.settings` cho role hệ thống owner.
  - "Đang làm" **không** có cột: suy ra từ `arrived_at` và `visit.check_in_at` + thời lượng dịch vụ mỗi lần đọc lịch.
  - Lịch đọc thêm lượt vãng lai: `visit` các ngày trong chi nhánh mà không lịch nào trỏ tới (`NOT EXISTS … appointment.visit_id`, dùng index `UNIQUE (tenant_id, visit_id)`), qua `visit_day_idx`; dòng lượt một query (`visit_line_visit_idx`), không N+1.
- **0020 Sửa lượt ([spec US6b](../product/specs/front-desk-and-packages.md)):**
  - `visit.corrects_visit_id uuid NULL`, FK `(tenant_id, corrects_visit_id) → visit`, `UNIQUE (tenant_id, corrects_visit_id)` (mỗi lượt bị thay tối đa một lần, cũng là index tra ngược "corrected by"), `CHECK (corrects_visit_id <> id)`. Liên kết chỉ nằm ở **lượt mới**: lượt cũ đã `voided` không được UPDATE nữa (trigger 0009); `corrected_by_visit_id` trong DTO = join ngược.
  - Trigger `visit_correction_link` (BEFORE INSERT OR UPDATE): lượt bị thay phải `voided` và cùng `business_date`; liên kết bất biến.
  - `appointment_closed_is_final` thay bằng bản cho phép đúng một thay đổi trên lịch `done`: `visit_id` chuyển sang lượt có `corrects_visit_id` = lượt cũ (các cột khác trừ `version`, `updated_at` phải y nguyên).
  - Không đổi `sale`, `payment`, `visit_line`: tiền/buổi đi qua đúng đường hủy (reversal, cộng buổi) + ghi lượt mới; sổ doanh thu (`revenue_book`) hiện hóa đơn cũ đã hủy với lý do "Sửa lượt: …". Bất biến "hóa đơn có dòng gói = liệu trình" giữ nguyên vì lượt sửa chỉ tạo hóa đơn dịch vụ lẻ.
- Nhập bù không cần cột mới: `visit.check_in_at` vốn là giờ làm, `business_date` của visit/sale/payment = ngày của giờ làm; `created_at`/`paid_at` vẫn là lúc ghi thật.

## Tiền sổ giấy là hóa đơn (0018, EM 2026-11-05)
- `sale.source` (`app` mặc định | `paper_book`): đánh dấu ẩn cho audit và chạy lại; API không bao giờ trả.
- Bỏ CHECK cũ "gói `opening_balance` không có `sale_id`"; thay bằng trigger `customer_package_sale_link`: gói bán trong app không bao giờ đổi `sale_id`; gói sổ giấy chỉ được gắn **một lần** (NULL → id) vào hóa đơn `paper_book` của **cùng khách**; không gỡ, không gắn lại.
- Dòng `sale_line` loại gói không còn bắt buộc `package_plan_id` (gói sổ giấy có thể không khớp gói nào trong bảng giá); vẫn bắt buộc `customer_package_id`.
- Hóa đơn sổ giấy = 1 dòng gói + 1 `payment` (`business_date = paid_on`, `paid_at`/`created_at` = 12:00 ngày đó giờ spa). Vì là `payment` thường nên sổ doanh thu, báo cáo, lịch sử, "Đã chi" tự tính; append-only như mọi khoản tiền. Hủy hóa đơn này chỉ đảo tiền (`lock_packages_of_sale` chỉ khóa/hủy gói `origin = 'sale'`).

## Follow-up khách vắng + tìm không dấu (0014, vòng 3)
- Bảng `app.customer_follow_up` (id, tenant_id, customer_id → customer (tenant_id, id), contacted_on date, channel ∈ phone/zalo/in_person/other, outcome ∈ will_come/not_interested/no_answer/other, note ≤ 500, next_follow_up_on ≥ contacted_on, created_by → user_account, created_at). **Chỉ thêm**: trigger `reject_mutation` cho UPDATE/DELETE, app_rw chỉ `SELECT, INSERT`. RLS `tenant_isolation`. Index `(tenant_id, customer_id, contacted_on DESC, created_at DESC)` cho "lần liên lạc gần nhất" (LATERAL trong `/customers/inactive`).
- Quyền `customer.follow_up` cho role hệ thống owner + manager.
- Hàm `app.search_fold(text)` (IMMUTABLE): NFC → bỏ dấu tiếng Việt → đ→d → lowercase → gộp khoảng trắng; bản SQL của `crm::text::search_key` cho tên chỉ có dạng snapshot (dịch vụ, gói, danh mục, KTV). Test `follow_ups.rs::sql_folding_matches_the_rust_search_key` so hai bản trên mọi chữ có dấu, hoa/thường, NFC/NFD.

## Read models R1-2 (0013, [spec](../product/specs/management-views.md))
Không có bảng mới. 0013:
- **Quyền:** thêm `report.read`, `report.revenue.read` cho role hệ thống owner + manager đã có; xóa `opening_balance.import` khỏi role hệ thống manager.
- **`app.package_status(cancelled_at, sessions_remaining, expires_on, today) → text`** (`IMMUTABLE`): định nghĩa SQL duy nhất của trạng thái suy ra, khớp `PackageStatus::derive` (test so trạng thái của `GET /packages` với `GET /customer-packages/{id}`).
- **View `app.customer_package_state`**: gói + tổng/còn lại cộng từ items.
- **View `app.revenue_book`**: sổ doanh thu suy ra: một dòng mỗi hóa đơn (tiền nhận theo CK/TM, chưa thanh toán = total − đã nhận, mô tả dòng lẻ / gói), một dòng âm mỗi (hóa đơn, ngày hủy) từ các `reversal`, một dòng âm mỗi `refund`. Cột `seq` = UUIDv7 do API sinh (hóa đơn / reversal mới nhất / refund) để thứ tự trong ngày đúng thứ tự ghi, không phụ thuộc đồng hồ DB vs API. Nhánh hóa đơn dùng `LATERAL`, không `GROUP BY` cột ngoài khóa, nên điều kiện `tenant_id`/`business_date` của người gọi được đẩy xuống từng nhánh.
- Cả hai view `WITH (security_invoker = true)` (PG ≥ 15; dev/CI/Supabase đều PG ≥ 16): chạy bằng quyền người gọi, RLS của bảng gốc vẫn áp cho `app_rw`. `GRANT SELECT` cho `app_rw`.
- **Index** (bench 3 năm: mọi index mới đều được dùng, các index ngày cũ dẫn đầu bằng `branch_id` có 0 lượt quét): `sale/payment/visit/refund (tenant_id, business_date)`, `visit_line (tenant_id, customer_package_id) WHERE NOT NULL` (trang liệu trình, "dùng lần cuối"), `refund (tenant_id, customer_id)`.
- `customer_package.origin` vẫn giữ trong DB (gói sổ giấy không có hóa đơn nên không bao giờ thành doanh thu), nhưng không còn trong API.
- Dữ liệu demo: hóa đơn ghi lùi ngày (`create_sale_as_of`) có `created_at` = lúc seed, nên timeline khách sắp theo `business_date` trước rồi mới tới giờ.

## Quản lý nhân viên (0017, [spec](../product/specs/staff-management.md))
- `staff` + `user_id` (FK `user_account`, unique `(tenant_id, user_id)` khi khác NULL) + `shift_start`.
- `staff_pay` (PK `(tenant_id, staff_id)`): `base_salary`, `commission_rate_bp`, `loyalty_bonus_monthly`, `meal_allowance`, `version`. Nhạy cảm.
- `payroll_rules` (PK `tenant_id`, `rules jsonb`, `version`): hr::rules::PayrollRules, thiếu key = mặc định.
- `payroll_period` (PK `(tenant_id, period)`, `period` = ngày 1): `open` → `closed` (trigger `payroll_period_forward_only`, không DELETE).
- `attendance_day` (PK `(tenant_id, staff_id, work_date)`): `present|leave_p|leave_dx|absent_k`, giờ đến/về, `late_minutes`, ghi chú; CHECK giờ chỉ khi `present`.
- `staff_daily_review` (PK như trên): khen, phàn nàn (0–99), ghi chú.
- `staff_evaluation` (PK `(tenant_id, staff_id, work_date=ngày 1)`): `criteria jsonb` [{key,max,level,points}], `total_points`, `result_level`, CHECK Mức 3/Chưa đạt phải có `coaching_plan`.
- `staff_adjustment`: chỉ thêm (`reject_mutation`), `amount` và/hoặc `points`, `reason`, `corrects_period` < tháng ghi.
- `payslip`: bản chụp `detail jsonb` + `net_pay`, unique `(tenant_id, period, staff_id)`, chỉ thêm.
- Trigger `reject_closed_period()` trên 4 bảng ghi theo ngày/tháng: từ chối INSERT/UPDATE/DELETE khi tháng của `work_date` đã chốt, và không cho đổi `work_date`.
- Index: `visit_day_status_idx (tenant_id, business_date) WHERE completed`, `sale_closer_day_idx`.
- Mọi bảng: RLS `tenant_isolation`; grant tối thiểu (`payslip`, `staff_adjustment`: SELECT, INSERT).
- Seed demo (`app seed-demo`, lần đầu): ngày vào làm + lương cho 6 người, tài khoản KTV `0905000003` gắn vào "Nguyễn Thị Lan", chấm công từ ngày 1 tháng trước đến hôm qua, khen/phàn nàn ngẫu nhiên, đánh giá tháng trước cho 5 KTV (một người Chưa đạt có kế hoạch kèm), 5 khoản thưởng/phạt. Tháng trước để **mở** cho EM thử chốt.

## Upsell trong lượt (0021, vòng 6, [spec §10](../product/specs/front-desk-and-packages.md))
- `sale.visit_id uuid NULL` → FK `(tenant_id, visit_id)` → `visit` **DEFERRABLE INITIALLY DEFERRED** (lượt và các hóa đơn của nó ghi trong một transaction, `visit.sale_id` trỏ ngược). Index `(tenant_id, visit_id)`. Backfill từ `visit.sale_id` (tạm tắt `sale_voided_is_final`). Trigger `sale_visit_link`: gắn một lần lúc insert, cùng khách (người trả) với lượt, không đổi sau đó.
- `visit_line.sale_id` (hóa đơn đã thu dòng lẻ; backfill từ `visit.sale_id`) + `visit_line.referrer_staff_id` (người giới thiệu, upsell) — FK theo tenant; `CHECK (source = 'one_off' OR (sale_id IS NULL AND referrer_staff_id IS NULL))`. Index một phần cho cả hai.
- `customer_package`: bỏ `CHECK (origin <> 'sale' OR (sale_id IS NOT NULL AND closer_staff_id IS NOT NULL))`, thay bằng `customer_package_sale_origin CHECK (origin <> 'sale' OR sale_id IS NOT NULL)` — "khách tự chọn" = người chốt null.
- Mua cho người khác: không thêm cột; `customer_package.customer_id` (người dùng) có thể ≠ `sale.customer_id` (người trả).
- `visit.companions`, `appointment.companions` `text[] NOT NULL DEFAULT '{}'`, `CHECK (app.valid_companions(...))` (0–5 mục, 1–100 ký tự).
- `app.revenue_book` (CREATE OR REPLACE, thêm cột cuối): `visit_id`, `upsell_by` (người chốt của hóa đơn gói, hoặc người giới thiệu của các dòng lẻ).

## Người đi cùng được làm (0022, [ADR-0009](../decisions/ADR-0009-served-companions.md))
- Bỏ `appointment_check1` (khách có hồ sơ **hoặc** tên + SĐT). Thay bằng `appointment_customer_or_guest` (có hồ sơ và không có trường khách mới, **hoặc** không hồ sơ và có tên; SĐT tùy chọn) + `appointment_guest_phone_or_group` (`customer_id IS NOT NULL OR guest_phone IS NOT NULL OR group_id IS NOT NULL`): khách không SĐT chỉ tồn tại trong một nhóm, cạnh người đặt lịch có SĐT.
- Không có cột mới; người đi cùng được làm là một dòng `appointment` cùng `group_id`. `EXCLUDE` theo KTV áp dụng như mọi lịch.
- Kiểm trên dữ liệu giống thật: DB mới migrate tới 0021 → `seed-demo` (8 lịch, 1 khách mới) → 0022 áp dụng sạch (6 ms).

## Danh tính khách (0023, [ADR-0011](../decisions/ADR-0011-anonymous-walk-in-and-optional-phone.md))
- `customer.full_name`, `customer.phone_e164` bỏ `NOT NULL` (CHECK độ dài / định dạng giữ nguyên, chỉ áp khi có giá trị). `full_name NULL` = hồ sơ không tên ("Khách vãng lai"), `search_text = ''`. `customer_phone_uq` không đổi: NULL khác nhau nên nhiều hồ sơ không SĐT cùng tồn tại, SĐT có giá trị vẫn duy nhất trong hồ sơ còn hiệu lực.
- `customer.birth_date date` + `customer_birth_date_plausible` (≥ 1900-01-01; "không sau hôm nay" và "≤ 120 tuổi" do service kiểm vì phụ thuộc đồng hồ) + `customer_birth_year_matches_date`.
- `app.customer_label(text)` IMMUTABLE: `coalesce(full_name, 'Khách vãng lai')`; mọi read model đọc tên khách qua hàm này (lượt, lịch hẹn, "Khách của tôi", sổ doanh thu, danh sách gói, lịch sử). Twin Rust `crm::service::WALK_IN_LABEL`.
- `app.customer_package_extension` (chỉ thêm, trigger `reject_mutation`, RLS `tenant_isolation`, `GRANT SELECT, INSERT`): mỗi lần gia hạn = một dòng (hạn cũ, hạn mới, lý do, ngày nghiệp vụ, người làm); index `(tenant_id, customer_package_id, created_at)`. Migration copy các lần gia hạn cũ từ `audit_log`.
- Danh sách gần đây (`GET /customers` không `q`) và "Khách lâu chưa quay lại" lọc `full_name IS NOT NULL`.

## Nhân sự: đợt làm việc (0025, [ADR-0013](../decisions/ADR-0013-employment-periods.md))
- `app.staff_employment(id, tenant_id, staff_id, started_on?, ended_on?, end_reason?, ended_by?, created_at)`: mỗi đợt làm việc một dòng. `started_on NULL` = hồ sơ cũ không có ngày vào. CHECK `ended_on ≥ started_on`, `end_reason` chỉ khi đã kết thúc; `EXCLUDE USING gist (tenant_id =, staff_id =, daterange(coalesce(started_on, -infinity), ended_on, '[]') &&)` (không chồng đợt); unique partial `(tenant_id, staff_id) WHERE ended_on IS NULL` (một đợt mở); index `(tenant_id, staff_id, started_on)`.
- Trigger `staff_employment_forward_only`: đợt đã kết thúc không đổi, đợt mở chỉ sửa ngày vào hoặc được đóng; không DELETE (app_rw chỉ `SELECT, INSERT, UPDATE`). Trigger `staff_first_period` (`AFTER INSERT ON app.staff`): mọi hồ sơ mới có đợt đầu. Backfill từ `joined_on/left_on`. RLS `tenant_isolation`.
- `app.staff.status / joined_on / left_on` = **bản sao của đợt mới nhất** (service ghi cùng transaction), nên mọi picker vẫn lọc `status = 'active'`; index partial mới `staff_active_name_idx (tenant_id, full_name) WHERE status = 'active'`.
- `app.staff.note` (≤ 1000, chủ spa). Nghỉ việc đổi `app.membership.status` của tài khoản đã gắn (`disabled`), nhận lại đổi về `active` (app_rw đã có UPDATE từ 0002).
- Permission mới `staff.read` cho role hệ thống owner + manager (spa mới: `SystemRole::default_permissions`).
- Đọc: `org::employment::Employment` (đợt của một người) trả lời "ngày D / tháng M có làm" và thâm niên; `hr` (bảng công, đánh giá, bảng lương, `calc::summarize`) dùng nó thay `joined_on/left_on`. `StaffSheet.periods` (dates only) cho web.

## Nhật ký ghi chú (0027, [spec management-views §10](../product/specs/management-views.md))
- Không bảng mới, không chép ghi chú: `reporting::notes` UNION ALL 14 loại tại chỗ (lượt, hồ sơ khách ×2, chấm công, khen/phàn nàn, thưởng/phạt, chăm sóc lại, hủy lượt/hóa đơn/gói, đợt làm việc; Q-N1: lịch hẹn, đánh giá tháng, `staff.note`, gia hạn gói, giảm giá), mỗi nhánh có cờ quyền + `tenant_id = $1` + khoảng ngày trên cột có index; sắp `(business_date, at, kind, key) DESC` (ngày kinh doanh trước, nên nhóm theo ngày liền mạch), cursor keyset 4 phần.
- 0027 chỉ thêm index một phần: `visit (tenant_id, business_date) WHERE note_ct IS NOT NULL`, `visit / sale (tenant_id, voided_at) WHERE status = 'voided'`, `customer_package (tenant_id, cancelled_at) WHERE cancelled_at IS NOT NULL`, `customer (tenant_id, updated_at) WHERE note IS NOT NULL OR health_note_ct IS NOT NULL`, `customer_follow_up (tenant_id, contacted_on) WHERE note IS NOT NULL`, `staff_employment (tenant_id, ended_on) WHERE end_reason IS NOT NULL`.
- 0028 (Q-N1, 14 loại): `appointment (tenant_id, created_at) WHERE note IS NOT NULL`, `staff_evaluation (tenant_id, updated_at) WHERE comment IS NOT NULL OR coaching_plan IS NOT NULL`, `staff (tenant_id, updated_at) WHERE note IS NOT NULL`, `customer_package_extension (tenant_id, business_date)`, `sale (tenant_id, business_date) WHERE discount_reason IS NOT NULL`. CTE thêm hai cột `guest` (tên khách của lịch hẹn chưa có hồ sơ) và `about` (`about_date`: ngày hẹn, tháng đánh giá, hạn mới).
- 0029 (F21, 2026-11-08, 15 loại): `appointment (tenant_id, closed_at) WHERE status IN ('cancelled', 'no_show')` cho lý do hủy lịch / không đến (`status_reason`); không cột mới.
- Ghi chú mã hóa giải mã ở Rust (tìm chữ: quét ≤ 500 / request); ghi chú hồ sơ khách dùng `customer.updated_at` (chưa có cột người/lúc sửa riêng, Q-N2).

## Ghi chú gói (0030, F4 2026-11-08, [spec front-desk §10.15](../product/specs/front-desk-and-packages.md))
- `ALTER TABLE app.sale ADD COLUMN note_ct bytea` (nullable): ghi chú của **một hóa đơn gói** (giỏ 2–3 gói = 2–3 hóa đơn, mỗi hóa đơn một ghi chú). AES-256-GCM tầng app như `visit.note_ct`, associated data `sale.note` + tenant + id hóa đơn (`kernel::crypto::sale_note_aad`); không bản rõ trong DB, audit hay bảng idempotency. Hóa đơn dịch vụ lẻ của lượt luôn `NULL` (ghi chú của lượt là `visit.note_ct`); chỉ app quyết, không CHECK chéo bảng.
- Ghi lúc tạo hóa đơn (`sales::service::record_package_sale`, chỗ ghi duy nhất của mọi đường bán gói) và `PATCH /sales/{id}` (`repo::set_note`, `version + 1`). Hóa đơn đã hủy không đổi được (trigger `sale_voided_is_final`, 0009). app_rw đã có `UPDATE` trên `app.sale` (0005), không grant mới.
- Index một phần `sale_note_day_idx (tenant_id, business_date) WHERE note_ct IS NOT NULL` cho nhánh `package_note` của `GET /notes` (16 loại). Không backfill (hóa đơn cũ không có ghi chú).

## Rà soát 2026-11
Đo trên 3 năm dữ liệu + staging, findings và đề xuất (bỏ 8 index, gộp xác thực, sửa lịch sử khách, dọn session, job kiểm bất biến), danh sách cơ chế không được đổi: [db-review-2026-11](db-review-2026-11.md).
- **0033 (B1, EM duyệt 2026-11-09):** bỏ 8 index, nên các dòng index tương ứng trong bảng ở trên không còn đúng:
  - GIN trigram `customer_search_trgm`, `customer_phone_trgm`: dưới RLS không bao giờ dùng được, vì `LIKE` không leakproof. Tìm khách quét theo `tenant_id`.
  - `visit_line_visit_idx`, `sale_line_sale_idx`: tiền tố của `UNIQUE (tenant_id, x_id, position)`.
  - `visit_day_idx`, `sale_day_idx`, `payment_day_idx`, `refund_day_idx`: dẫn đầu `branch_id`. Bản `(tenant_id, business_date)` từ 0013 thay thế.
  - Không đụng constraint, unique hay đích FK nào. Hoàn tác: `CREATE INDEX CONCURRENTLY` ghi trong file migration. Test `kernel/tests/indexes.rs`.

## Seed
Dữ liệu tenant Viên Phúc Y Đường: 1 chi nhánh (Phường Đông Hà), 10 nhóm và 35 dịch vụ từ [service-catalog](../product/service-catalog.md), role hệ thống, tài khoản chủ spa. Giá "Ôn ấm tử cung" và các gói liệu trình: chờ EM.
