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
| `sale` ✅ 0005 | tenant_id, branch_id, customer_id, business_date, subtotal, discount_amount, discount_reason?, total, amount_paid, status (`open`/`voided`), closer_staff_id?, void_reason?, version | `CHECK (total = subtotal - discount_amount AND total >= 0)`; `CHECK (discount_amount = 0 OR discount_reason IS NOT NULL)`; **`CHECK (amount_paid BETWEEN 0 AND total)`** |
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
`0001_kernel` ✅ (pg_trgm, schema `app`, `app.current_tenant()`, tenant, branch, audit_log, idempotency_record, RLS + grants) → `0002_identity_org` → `0003_catalog` → `0004_crm` ✅ → `0005_sales_packages` ✅ (staff, sale, sale_line, payment, customer_package, customer_package_item) → `0006_mfa` ✅ (S2b) → `0007_visits` ✅ (S7: visit, visit_line) → `0009_void_lifecycle` ✅ (S8) → `0011_security_review` ✅ (H1, M3; M2 window + `refund_cap`) → `0012_simplify_owner_and_opening_balance` (EM 2026-11-05: bỏ `opening_balance_window`, quyền `opening_balance.manage_window`, cột `refund_cap`; trigger `refund_within_paid` về lại `amount ≤ sold_price`) → `0013_management_views` (R1-2, xem mục Read models) → `0014_follow_ups_and_search` (vòng 3, xem mục Follow-up) → `0015_long_range_reports` → `0016_appointments` (vòng 4, xem mục Lịch hẹn) → `0017_staff_management` (xem mục Quản lý nhân viên). 0008, 0010 bỏ trống. Từ 0016 crate `kernel` có `build.rs` (`rerun-if-changed=../../migrations`): trước đó thêm file migration mới không làm build lại migrator nhúng, `app migrate` lặng lẽ bỏ qua nó. 0011 đã có thể chạy trên staging nên không sửa, chỉ thêm 0012. Kế hoạch tách riêng migration RLS/grants ban đầu đã bỏ: mỗi migration tự bật RLS và cấp quyền cho bảng của nó.

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
- Nhập bù không cần cột mới: `visit.check_in_at` vốn là giờ làm, `business_date` của visit/sale/payment = ngày của giờ làm; `created_at`/`paid_at` vẫn là lúc ghi thật.

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

## Seed
Dữ liệu tenant Viên Phúc Y Đường: 1 chi nhánh (Phường Đông Hà), 10 nhóm và 35 dịch vụ từ [service-catalog](../product/service-catalog.md), role hệ thống, tài khoản chủ spa. Giá "Ôn ấm tử cung" và các gói liệu trình: chờ EM.
