---
title: Rà soát thiết kế database (2026-11)
updated: 2026-11-09
sources: [raw/2026-11-08-em-request-db-design-review.md, architecture/data.md, architecture/backend.md, architecture/performance-latency-2026-11.md, decisions/ADR-0003-multi-tenancy.md, decisions/ADR-0004-authentication.md, decisions/ADR-0012-colocate-api-db-singapore.md, decisions/ADR-0013-employment-periods.md]
---

# Rà soát thiết kế database (2026-11-08)

EM nhờ rà kỹ schema (bảng, index, FK, lưu trữ, toàn vẹn, xung đột, độ trễ) **trước khi đổi gì** ([raw](../../raw/2026-11-08-em-request-db-design-review.md)). Trang này là **rà soát + đề xuất**: không migration nào được chạy, không merge gì. Mọi số đo đều có script trong `infra/perf/db/` để đo lại.

## Tóm tắt
- **Toàn vẹn dữ liệu đang rất chắc.** RLS + FK kép theo tenant, CHECK tiền, trigger chỉ-thêm, trigger trạng thái chỉ đi tới, `EXCLUDE` chống đặt trùng KTV, idempotency, `If-Match`, thứ tự khóa. 11 phép kiểm bất biến trên staging (chỉ đọc) đều **0 dòng lệch**. Không có đề xuất nào nới lỏng chúng (§6).
- **Thời gian Postgres chạy query không phải nút thắt.** Trên bản sao local 3 năm của một spa đông (60k lượt, 43k hóa đơn, 47k lịch hẹn, 208k dòng audit), 29/40 câu nóng chạy **≤ 3 ms** (11 câu còn lại 3–135 ms, đều là màn quản lý hoặc lịch sử khách), câu xác thực chạy mỗi request **0,07–0,35 ms**.
- **Nút thắt phía DB là số lượt đi về API ↔ DB.** Một GET đơn giản đang tốn **~10 lượt đi về**, trong đó chỉ **1** là truy vấn nghiệp vụ. Trên staging mỗi lượt ~2,6 ms, nên `GET /staff` mất 26 ms ở server dù Postgres chỉ chạy < 1 ms. Ba trong số đó là **ping ẩn** của pool sqlx. Gộp lại (§5 A1–A4) đưa GET đơn giản từ ~10 xuống ~4 lượt: **−15 ms mỗi request trên staging**, `/me` từ ~10 xuống ~3 lượt.
- **Một câu sẽ chậm dần theo dữ liệu:** lịch sử khách (`GET /customers/{id}/history`) quét **toàn bộ** hóa đơn và lượt của spa mỗi lần mở hồ sơ: **66–94 ms ở 3 năm**, tăng tuyến tính. Viết lại bằng `UNION` hai lần tra index: **0,4–0,6 ms** (×100), không cần đổi schema.
- **Index:** 2 GIN trigram trên `customer` **không bao giờ dùng được** dưới RLS (toán tử `LIKE` không leakproof), 2 index trùng tiền tố, 4 index ngày dẫn đầu bằng `branch_id` không được dùng. Bỏ 8 index này: **−31 MB ở 3 năm (−15 % dung lượng index)**, bớt một lần ghi index cho mỗi lượt, dòng lượt, hóa đơn, dòng hóa đơn, thanh toán. Plan không đổi (đã thử trong transaction rồi rollback).
- **Dung lượng không phải vấn đề:** 3 năm ≈ **397 MB** (heap 176 MB, index 208 MB, audit 126 MB = 32 %). Supabase Pro có sẵn 8 GB đĩa. Việc cần làm là **vệ sinh dữ liệu cá nhân**: bảng `session` (IP + user agent) **không bao giờ được dọn**.

## 1. Cách đo
- **Đọc tĩnh:** 29 migration (0001…0032; 0008, 0010, 0031 bỏ trống) và 257 câu SQL trong `apps/api/.sqlx`. Danh mục index, FK, constraint, trigger và grant lấy từ một DB local mới migrate tới 0032.
- **Local (3 năm, một spa đông):** DB tạm `vpyd_dbreview` trong container `vpyd-dev-postgres-1` (PG 16), seed bằng `infra/perf/db/seed-busy-spa.sql`: 1 095 ngày, 30–80 lượt/ngày (60 265 lượt, 72 281 dòng lượt), 3 gói/ngày, 47 433 lịch hẹn, 10 nhân viên (chấm công, đánh giá, thưởng phạt, phiếu lương 35 tháng đã chốt), 8 000 khách, 208 213 dòng audit cỡ như staging, 3 288 session, 400 bản ghi idempotency trong 48 giờ. Mọi constraint và trigger vẫn áp dụng khi seed. Xong thì drop DB.
- **Plan đúng như API chạy:** `infra/perf/db/explain_hot.py` lấy nguyên văn câu SQL từ `.sqlx`, `PREPARE` rồi `EXPLAIN (ANALYZE, BUFFERS) EXECUTE` với **role `app_rw`** và `set_config('app.tenant_id', …)`, nên plan có cả điều kiện RLS. Tham số planner đặt bằng staging (`random_page_cost 1.1`, `effective_cache_size 384MB`, `work_mem 2184kB`). Owner thì bỏ qua RLS nên plan của owner gây hiểu lầm (xem F5).
- **Staging, chỉ đọc:** `infra/perf/db/staging-readonly.sh` chạy file SQL trong `BEGIN READ ONLY … ROLLBACK`. Chuỗi kết nối đọc từ `.env.staging` vào biến môi trường và đưa vào container bằng `-e PGURL`, nên không xuất hiện trong argv, log hay git. Lấy catalog, `pg_stat_user_tables/indexes`, `pg_stat_statements` (từ 2026-10-29), cỡ payload, plan sổ doanh thu (chỉ SELECT) và 11 phép kiểm bất biến.
- **Server-Timing:** `infra/perf/endpoints.mjs 6` (2026-11-08, từ Mỹ, tài khoản quản lý demo) cho tổng thời gian server, thời gian DB và số câu lệnh mỗi request.
- `pg_stat_statements` không bật được ở local (cần `shared_preload_libraries`, container dùng chung). `pgstattuple` không cần, vì `VACUUM` xong thì không còn bloat để đo. Staging đã có `pg_stat_statements`.

## 2. Bức tranh tổng thể
**Staging (2026-11-08):** PG 17.11, 41 bảng + 2 view (EM đếm 43), 147 index, **15 MB**. Cache hit 99,97 %, `data_checksums = on`. Pooler: **Supavisor session mode, cổng 5432** ([runbook](../runbooks/vercel-staging.md)), **không phải** transaction mode: SQLx cần prepared statement, mà transaction mode không hỗ trợ ([Supabase](https://supabase.com/docs/guides/database/connecting-to-postgres)). Đề bài ghi "transaction pooler" là nhầm, nên giữ nguyên session mode.

**Local 3 năm (sau `VACUUM ANALYZE`):**

| Bảng | Dòng | Heap | Index (số index) | Index / heap |
|---|---|---|---|---|
| `audit_log` | 208 213 | 102 MB | 24 MB (2) | 0,23 |
| `visit` | 60 265 | 13 MB | 40 MB (10) | 3,1 |
| `visit_line` | 72 281 | 14 MB | 38 MB (9) | 2,7 |
| `appointment` | 47 433 | 11 MB | 28 MB (9) | 2,5 |
| `sale` | 43 451 | 8,9 MB | 23 MB (10) | 2,6 |
| `sale_line` | 55 467 | 8,6 MB | 16 MB (4) | 1,9 |
| `payment` | 44 223 | 7,0 MB | 11 MB (6) | 1,7 |
| `customer` | 8 000 | 1,2 MB | 12 MB (6) | **9,9** (2 GIN trigram = 8,6 MB) |
| Toàn DB | | 176 MB | **208 MB** | 1,18 |

Index chiếm nhiều hơn dữ liệu. Phần lớn là chủ ý: `PK (id)` + `UNIQUE (tenant_id, id)` làm đích cho FK kép (§6). Phần bỏ được nằm ở F5–F6.

## 3. Độ trễ: DB chạy bao lâu, đi về bao nhiêu lượt

### 3a. Server-Timing trên staging (2026-11-08, n = 6, p50)
| Endpoint | Câu lệnh đếm được | `db` (ms) | Tổng server (ms) | Lượt đi về ước tính |
|---|---|---|---|---|
| `GET /me` | 5 | 13,8 | 26,5 | ~10 |
| `GET /staff`, `/customers?q=`, `/service-categories` | 5 | 13,0–13,4 | 25,8–26,3 | ~10 |
| `GET /services`, `/package-plans` | 6 | 16,3–16,6 | 29,9–30,6 | ~11 |
| `GET /visits?date=` (Quầy) | 8 | 26,1 | 38,5 | ~14 |
| `GET /reports/daily` | 8 | 24,7 | 37,0 | ~14 |
| `GET /appointments` (ngày / tuần) | 12 | 34,3–34,9 | 47,8–47,9 | ~18 |
| `GET /attendance?month=` | 13 | 35,7 | 50,4 | ~19 |

"Câu lệnh đếm được" là các câu sqlx ghi event (`app::server_timing`). Hai thứ **không** được đếm nhưng vẫn là một lượt đi về mỗi lần: `BEGIN; set_config(...)` (sqlx gửi bằng `queue_simple_query`) và **ping của pool**. `/me` có 5 câu đếm được + 4 ping + 1 `BEGIN` = 10 lượt ≈ 26,5 ms, tức **~2,6 ms mỗi lượt** (Vercel `pdx1` → Supavisor → Postgres). Phần Rust còn lại < 1 ms.

**DB time so với lượt đi về:** Postgres chạy các câu của `GET /staff` chưa tới 1 ms (staging `pg_stat_statements`: tra session 0,17 ms, quyền 0,4 ms, danh sách nhân viên 0,3 ms; local 3 năm tương tự). Tức là **trên 95 % thời gian server là chờ mạng giữa API và DB**. Với người dùng ở Việt Nam, request còn cộng thêm 180–400 ms Thái Bình Dương ([perf](performance-latency-2026-11.md)). Bớt lượt đi về giúp ~15 ms mỗi request, tức 3–6 % cảm nhận từ VN hôm nay, và ~10 % từ Mỹ. Khi production chạy API + DB cùng Singapore (ADR-0012), mạng xa không còn nên tỷ lệ này lớn hơn nhiều.

### 3b. Một request đã đăng nhập hôm nay đi về những gì
```
pool checkout → ping                      ┐ CurrentSession
SELECT session JOIN user_account          ┘ (mỗi 60 s: + checkout, ping, UPDATE session ~5 ms do commit WAL)
pool checkout → ping                      ┐ AuthUser::actor_for → tenant_permissions
BEGIN; set_config(tenant, timeout)        │
SELECT quyền (array_agg) + cờ người đã nghỉ│
COMMIT                                    ┘
pool checkout → ping                      ┐ handler: TenantTx
BEGIN; set_config(tenant, timeout)        │
[SELECT tenant profile]  (mọi lệnh ghi)   │
… các câu nghiệp vụ …                     │
COMMIT                                    ┘
```
Ping đến từ mặc định `test_before_acquire = true` của sqlx: mỗi lần lấy một kết nối rảnh từ pool, sqlx gửi một `Sync` và chờ `ReadyForQuery` (`sqlx-core 0.9 pool/inner.rs::check_idle_conn`, `sqlx-postgres connection::ping`). Mỗi request lấy kết nối 3 lần, nên tốn 3 ping. Chính docs của sqlx chỉ cách chỉ ping khi kết nối đã rảnh lâu (`before_acquire` + `idle_for`, `PoolOptions::before_acquire`).

### 3c. Postgres trên dữ liệu 3 năm (app_rw + RLS, plan đúng câu của API, lần chạy ấm)
| Màn / câu | Exec (ms) | Ghi chú |
|---|---|---|
| Tra session / quyền / tenant profile | 0,07 / 0,35 / 0,06 | index `session_token_hash_key`, `role_permission_pkey` |
| Quầy `GET /visits?date=`: lượt / dòng / gói / thanh toán | 1,1 / 2,1 / 0,8 / 0,3 | planning 2 / 5 / 1,7 / 0,9 ms |
| Lịch tuần: lịch / dịch vụ / khách vãng lai | 1,2 / 1,0 / 1,2 | |
| Tìm khách theo tên / số / gần đây | 1,2 / 1,0 / 2,6 | **seq scan** 8 000 khách (F5) |
| **Lịch sử khách (trang 1)** | **79–94** | **F4**: seq scan 43k hóa đơn + hash anti-join 60k lượt |
| Lịch sử: dòng lượt / tóm tắt / số gói | 0,8 / 0,5 / 0,2 | |
| Khách lâu chưa quay lại | 64–68 | gom `max(business_date)` mọi lượt; màn quản lý, trong ngân sách |
| Danh sách gói | 12–14 | |
| Báo cáo 30 ngày: lượt / thanh toán / gói | 30–40 / 8–10 / 2,3 | F7 |
| Báo cáo 366 ngày theo tháng | 131–135 | hiếm, ngân sách 300 ms |
| Sổ doanh thu tháng: dòng / tổng ngày | 12–14 / 2,5–2,9 | F16 |
| Nhật ký ghi chú 30 ngày | 7,4–8,3 | planning **8 ms** (16 nhánh UNION, F8) |
| Bảng công / khen-chê / đánh giá tháng | 0,1–0,2 | |
| Ngày có lượt trong tháng | 3,4–12,8 | F7 |
| Lương: hoa hồng / dòng dịch vụ tháng | 4–9 / 16–24 | F7 |
| `/me/*` (lịch sắp tới, lượt hôm nay, lịch tuần) | 0,3–1,1 | |

Điều kiện RLS không tốn gì: planner gộp `tenant_id = $1` và `tenant_id = app.current_tenant()` thành một **One-Time Filter**, còn index tenant-first vẫn được dùng làm Index Cond.

## 4. Findings
Thang điểm: **Ảnh hưởng** (L = độ trễ, S = dung lượng / ghi, I = toàn vẹn / riêng tư) × **Rủi ro khi sửa** × **Công sức**. Cao / TB / Thấp.

| # | Finding | Bằng chứng | Ảnh hưởng | Rủi ro | Công |
|---|---|---|---|---|---|
| F1 | ~10 lượt đi về cho một GET đơn giản, chỉ 1 là nghiệp vụ | §3a, §3b | L **Cao** (95 % thời gian server) | — | — |
| F2 | 3 ping ẩn mỗi request (`test_before_acquire`) | §3b, nguồn sqlx 0.9 | L **Cao**: ~8 ms/request staging | Thấp | **Rất thấp** (cấu hình pool) |
| F3 | Xác thực tốn 2 transaction tách rời: tra session (1) + quyền (BEGIN, SELECT, COMMIT); profile thêm 1 câu ở lệnh ghi | §3b; bản nháp 1 câu chạy 0,34–0,44 ms (`experiments.sql` D) | L **Cao**: −3…4 lượt | TB (đụng xác thực: `secure` + ADR) | TB |
| F4 | Lịch sử khách: nhánh hóa đơn `s.customer_id = $2 OR EXISTS(gói của khách)` buộc **seq scan** toàn bộ `sale` và hash toàn bộ `visit` | 66–94 ms ở 3 năm; `UNION` 0,4–0,6 ms (`experiments.sql` B) | L **Cao**, tăng tuyến tính theo tuổi spa | Thấp (chỉ đổi SQL, đã có test lịch sử) | Thấp |
| F5 | GIN trigram `customer_search_trgm`, `customer_phone_trgm` không dùng được với `app_rw` | Staging 0 scan dù có tìm; local: owner dùng Bitmap Index Scan, `app_rw` thì Seq Scan (`experiments.sql` A). `textlike` / `similarity_op` không `LEAKPROOF`, nên điều kiện của người dùng không được đẩy lên trước policy RLS ([PG RLS](https://www.postgresql.org/docs/17/ddl-rowsecurity.html)). Chỉ superuser đặt được `LEAKPROOF` ([CREATE FUNCTION](https://www.postgresql.org/docs/17/sql-createfunction.html)), mà Supabase không cho superuser. | S TB: 8,6 MB / 3 năm, GIN cập nhật mỗi lần ghi khách; L **không đổi** (đằng nào cũng seq scan 1,1–1,5 ms) | Thấp | Rất thấp |
| F6 | Index thừa: `visit_line_visit_idx` (tiền tố của `UNIQUE (tenant_id, visit_id, position)`), `sale_line_sale_idx` (tiền tố của `UNIQUE (tenant_id, sale_id, position)`), `visit_day_idx` / `sale_day_idx` / `payment_day_idx` / `refund_day_idx` (dẫn đầu `branch_id`, đã có bản `(tenant_id, business_date)` từ 0013) | Staging: 4 index ngày branch 0 scan (trừ `sale_day_idx` 212 lần trên dữ liệu tí hon, planner chọn tùy ý). Local workload: 0 scan. Plan sau khi bỏ không đổi (`experiments.sql` F: index unique thay thế, 0,4–0,8 ms) | S TB: 22,5 MB + mỗi insert bớt 1 index | Thấp | Rất thấp |
| F7 | Truy vấn tháng / khoảng ngày nối `visit_line` bằng seq scan (dòng lượt không có ngày) | Lương dòng dịch vụ 16–24 ms, báo cáo 30 ngày 30–40 ms (trong đó 2,8 ms là subplan quét **toàn bộ** `visit_corrects_once`, index gần như toàn NULL) | L Thấp (trong ngân sách), tăng tuyến tính | — | — |
| F8 | Câu "tham số tùy chọn" (`$n IS NULL OR …`) giữ custom plan nên planning chạy lại mỗi lần | Generic plan = seq scan 5,7–6,3 ms, custom 0,1 ms; sau 6 lần chạy vẫn là custom ([PREPARE](https://www.postgresql.org/docs/17/sql-prepare.html)) → **đúng**. Planning 5 ms (dòng lượt), 8 ms (nhật ký) | L Thấp | — | — |
| F9 | `session` không bao giờ được dọn: không có job, `app_rw` không có quyền DELETE, giữ IP + user agent mãi | Code: chỉ `jobs::spawn_idempotency_purge`. [backend](backend.md) §Jobs ghi "dọn session hết hạn mỗi giờ" ⚠ lệch với code. Staging 13 dòng, 0 còn sống | I TB (riêng tư, tối thiểu hóa dữ liệu), S rất thấp | Thấp | Thấp |
| F10 | Dọn idempotency chạy **mỗi lần instance khởi động** | Staging: 217 lần / 10 ngày, mean 6 ms, 191 buffer | L/S Thấp | — | — |
| F11 | Dung lượng: 3 năm ≈ 397 MB, audit 32 %, không dùng TOAST, ciphertext 75–95 B | §2; staging payload: `visit.create` 677 B/dòng audit, idempotency 1,4 KB/body | S Thấp | — | — |
| F12 | Các bản sao lưu chủ ý (`staff.status/joined_on/left_on`, `payment.branch_id`, `sale.amount_paid`, liên kết ba chiều lượt ↔ hóa đơn ↔ dòng lượt, `customer_package.closer_staff_id/sold_price`, `search_text`, `business_date`) | `invariants.sql`: 11 phép kiểm, **0 dòng lệch** trên staging | I: hiện sạch; chưa có job phát hiện lệch | Thấp | Thấp |
| F13 | 29 FK không có index dẫn đầu bên con; cột "người làm" (`created_by`, `voided_by`, `closed_by`, `cancelled_by` của lượt, hóa đơn, thanh toán, lịch, gói) không có FK, trong khi cột của HR / follow-up thì có | Danh mục FK; bên cha không bao giờ bị xóa / đổi khóa, nên không cần index ([PG FK](https://www.postgresql.org/docs/17/ddl-constraints.html)). Riêng `membership.user_id` được hàm definer `tenants_of_user` / `user_requires_mfa` quét (2–3 ms, 110–262 buffer mỗi lần login và `/me`) | I Thấp, L Thấp | Thấp | Thấp |
| F14 | Xung đột / cô lập: READ COMMITTED + khóa tường minh + constraint, đủ và đúng | §6; `hr::period::require_open` khóa `FOR SHARE` trước khi ghi, trigger là lưới an toàn | — (giữ) | — | — |
| F15 | Kết nối bị dựng lại liên tục: 471 `DISCARD ALL` và 218 lần kiểm role lúc khởi động trong 10 ngày. Kết nối mới thì cache prepared statement của sqlx trống, mỗi câu tốn thêm một lượt Parse/Describe | Staging `pg_stat_statements` | L TB trên Vercel (cold start) | — | Hạ tầng (ADR-0012) |
| F16 | Sổ doanh thu: trên staging tí hon, planner chạy lại cả view `revenue_book` cho **từng khách** (loops = 47, 8,7k buffer, 20 ms cho 28 dòng); ở 3 năm thì thành hash join 12–14 ms | `staging-ledger-explain.sql` (chỉ SELECT) và local | L Thấp | — | — |

### Chi tiết đáng chú ý
- **F4, lịch sử khách.** Một chữ `OR` giữa hai bảng làm Postgres không dùng được `sale_customer_idx` lẫn `customer_package_customer_idx`. Hai `NOT EXISTS` sau đó (hóa đơn của lượt, hóa đơn của dòng lượt) thành hash anti-join trên **toàn bộ** `visit`. Cách sửa: `s.id IN (SELECT id FROM sale WHERE customer_id = $2 UNION SELECT sale_id FROM customer_package WHERE customer_id = $2)`. Plan mới: tra 2 index, 10 + 5 dòng, rồi kiểm từng hóa đơn theo index. Ý nghĩa giữ nguyên, kể cả trường hợp gói mua cho người khác. Quầy mở hồ sơ khách rất thường xuyên, nên đây là thắng lớn nhất về độ trễ thuần DB.
- **F5, trigram và RLS.** Đây không phải lỗi index mà là giới hạn bảo mật có chủ đích của Postgres: điều kiện không leakproof của người dùng phải chạy **sau** policy, nên không thành Index Cond. Không có cách sạch nào trên Supabase (không có superuser để đặt `LEAKPROOF`). Một hàm `SECURITY DEFINER` để tìm kiếm thì làm yếu lớp RLS, nên **không** đề xuất. Ở 8 000 khách, seq scan 1,1–1,5 ms. Ở 50 000 khách (nhiều chi nhánh) ước chừng 7–10 ms, vẫn ổn.
- **F2/F3, gộp xác thực.** Bản nháp một câu (`experiments.sql` D) nối session → user → tenant → membership → quyền → cờ người đã nghỉ → profile (timezone, chi nhánh, trần giảm giá) và chạy 0,34–0,44 ms. Khi đưa vào app, câu này phải chạy **trước** khi biết tenant, nên cần một hàm `SECURITY DEFINER` (`app.session_actor(token_hash)`, `SET search_path = app, pg_temp`, `REVOKE … FROM PUBLIC`, `GRANT EXECUTE … TO app_rw`). Cùng mẫu với `app.tenants_of_user` đã có. Hàm chỉ đọc membership của **đúng** `session.active_tenant_id` và `session.user_id`. Đầu vào là hash của bí mật trong cookie, nên người gọi được hàm thì vốn đã là chủ session. Không có định danh nào đến từ client. Cần `secure` review và ADR (bổ sung ADR-0003/0004).

## 5. Đề xuất theo thứ tự
Thứ tự theo (lợi ích ÷ rủi ro). Chưa làm gì. Bước nào đụng schema hoặc xác thực cần EM duyệt.

| Bước | Việc | Loại | Lợi ích dự kiến | Hoàn tác |
|---|---|---|---|---|
| **A1** | Pool: `test_before_acquire(false)` + `before_acquire` chỉ ping khi `idle_for > 30 s` (đúng mẫu trong docs sqlx), thêm `idle_timeout` < timeout rảnh của Supavisor | Code (`app/src/main.rs`), không migration | **−3 lượt/request ≈ −8 ms** trên staging (GET đơn giản 26 → ~18 ms) | Bỏ hai dòng cấu hình |
| **A2** | Viết lại nhánh hóa đơn của lịch sử khách bằng `UNION` | Code (`reporting::customer`), `cargo sqlx prepare` | **79–94 → < 1 ms** ở 3 năm; hết tăng tuyến tính | Revert commit |
| **A3** | `app.session_actor(token_hash)` (migration chỉ thêm hàm) + `identity::resolve/actor_for` dùng nó: session + cờ MFA / đổi mật khẩu + quyền + người đã nghỉ trong **một** câu, không cần transaction | Migration 0033 (hàm) + code + test authz / cách ly / `request_plumbing` + `secure` + ADR | **−3 lượt nữa** (−8 ms); `/me` ~10 → ~3 lượt | Code quay về đường cũ; hàm để lại vô hại hoặc `DROP FUNCTION` |
| **A4** | Đưa timezone / chi nhánh / trần giảm giá vào `Actor` (từ A3), bỏ câu `profile` trong transaction | Code | −1 lượt ở lệnh ghi và màn dùng profile | Revert |
| A5 | (tùy chọn) Gộp touch session vào A3 (`UPDATE … WHERE last_seen_at < now() − 60 s`) | Theo A3 | −2 lượt + một commit WAL ~5 ms, mỗi phút một lần | Revert |
| **B1** | Migration 0034 bỏ 8 index (F5 + F6), `SET LOCAL lock_timeout = '3s'`. Hoặc `DROP INDEX CONCURRENTLY`, mỗi file một câu với dòng đầu `-- no-transaction` (sqlx 0.9 hỗ trợ; một chuỗi nhiều câu thì Postgres coi là một transaction ngầm và từ chối `CONCURRENTLY`) | Migration | **−31 MB / 3 năm (−15 % index)**; mỗi lần ghi lượt / dòng lượt / hóa đơn / dòng hóa đơn / thanh toán bớt 1 index, ghi khách bớt 2 GIN; plan không đổi | Tạo lại bằng `CREATE INDEX CONCURRENTLY` (định nghĩa bên dưới); 3 năm dữ liệu dựng < 1 s mỗi index |
| **C1** | `app.purge_sessions(interval)` (`SECURITY DEFINER`, như `purge_idempotency_records`) + gọi cùng job dọn idempotency: xóa session đã thu hồi hoặc hết hạn quá 30 ngày | Migration + code + test | Tối thiểu hóa dữ liệu cá nhân (IP / UA); sửa lệch giữa wiki và code | Bỏ lời gọi job; hàm vô hại |
| **C2** | Chạy `infra/perf/db/invariants.sql` định kỳ (hằng ngày, chỉ đọc) và báo khi có dòng | Job / CI | Phát hiện lệch bản sao trước khi thành lỗi tiền (skill data-integrity §7) | Tắt job |
| C3 | Index `membership (user_id)` khi có spa thứ hai | Migration nhỏ | Login / `/me` không quét membership | Drop index |
| C4 | (tùy chọn) FK cho cột "người làm" bằng `ADD CONSTRAINT … NOT VALID` rồi `VALIDATE CONSTRAINT` ở migration sau; `ALTER ROLE app_rw SET idle_in_transaction_session_timeout = '30s'` (runbook, role không nằm trong migration) | Migration / runbook | Toàn vẹn nhất quán; chặn transaction treo giữ khóa | Drop constraint / reset setting |
| D | Theo dõi, chưa sửa: F7 (seq scan `visit_line` theo tháng), F8 (planning), F16 (plan sổ doanh thu), "khách lâu chưa quay lại" (64–68 ms). Chạy lại `explain_hot.py` khi dữ liệu gấp 3 lần hoặc mỗi năm một lần | Đo | — | — |

**Câu B1 (dự kiến):**
```sql
SET LOCAL lock_timeout = '3s';
DROP INDEX app.customer_search_trgm;   -- GIN, không dùng được dưới RLS (F5)
DROP INDEX app.customer_phone_trgm;    -- GIN, không dùng được dưới RLS (F5)
DROP INDEX app.visit_line_visit_idx;   -- tiền tố của visit_line_tenant_id_visit_id_position_key
DROP INDEX app.sale_line_sale_idx;     -- tiền tố của sale_line_tenant_id_sale_id_position_key
DROP INDEX app.visit_day_idx;          -- (tenant, branch, day, check_in) → visit_tenant_day_idx
DROP INDEX app.sale_day_idx;           -- → sale_tenant_day_idx
DROP INDEX app.payment_day_idx;        -- → payment_tenant_day_idx
DROP INDEX app.refund_day_idx;         -- → refund_tenant_day_idx
-- Hoàn tác (mỗi câu một migration -- no-transaction):
-- CREATE INDEX CONCURRENTLY customer_search_trgm ON app.customer USING gin (search_text gin_trgm_ops);
-- CREATE INDEX CONCURRENTLY customer_phone_trgm ON app.customer USING gin (phone_e164 gin_trgm_ops);
-- CREATE INDEX CONCURRENTLY visit_line_visit_idx ON app.visit_line (tenant_id, visit_id);
-- CREATE INDEX CONCURRENTLY sale_line_sale_idx ON app.sale_line (tenant_id, sale_id);
-- CREATE INDEX CONCURRENTLY visit_day_idx ON app.visit (tenant_id, branch_id, business_date, check_in_at DESC);
-- CREATE INDEX CONCURRENTLY sale_day_idx ON app.sale (tenant_id, branch_id, business_date);
-- CREATE INDEX CONCURRENTLY payment_day_idx ON app.payment (tenant_id, branch_id, business_date);
-- CREATE INDEX CONCURRENTLY refund_day_idx ON app.refund (tenant_id, branch_id, business_date);
```
`DROP INDEX` thường chỉ cần khóa `ACCESS EXCLUSIVE` trong vài ms. Ở tải của một spa thì không ai thấy, còn `lock_timeout` giúp migration không đứng chờ sau một query dài. Một lần dựng index `CONCURRENTLY` bị lỗi sẽ để lại index `INVALID`: drop rồi dựng lại ([CREATE INDEX](https://www.postgresql.org/docs/17/sql-createindex.html)). Khi có nhiều chi nhánh, `(tenant_id, business_date)` + lọc `branch_id` vẫn đủ cho vài chi nhánh. Chỉ thêm lại index theo chi nhánh khi số đo cần.

**Tổng hiệu quả dự kiến (staging, cùng phép đo §3a):** GET đơn giản ~10 → ~4 lượt (26 → ~11 ms ở server), Quầy `/visits` ~14 → ~8 lượt (38 → ~22 ms), `/me` ~10 → ~3 lượt, hồ sơ khách bớt 80–90 ms DB ở 3 năm. Đo lại bằng `infra/perf/endpoints.mjs` (Server-Timing) trước và sau mỗi bước (skill `perf`).

**Cố ý không đề xuất:**
- Bỏ `PK (id)` hoặc `UNIQUE (tenant_id, id)` trùng nhau: giữ an toàn FK kép, lợi ích vài MB.
- Bỏ các bản sao (F12): chúng là sổ / bản chụp, hoặc giúp lọc theo index. Bỏ đi thì thêm join và thêm rủi ro.
- Chuyển sang transaction pooler: phá prepared statement, không lợi gì ở tải này.
- Hàm definer để tìm khách bằng trigram: làm yếu RLS.
- Thêm `business_date` vào `visit_line`: phi chuẩn hóa vì 10–20 ms trong màn tháng.
- Gom "Quầy" thành một câu `json_agg`: −4 lượt nhưng SQL khó bảo trì. Chỉ làm khi A1–A4 xong mà số đo vẫn thiếu.

## 6. Không được đổi (bảo vệ toàn vẹn / xung đột)
Những thứ dưới đây trông như "thừa" hoặc "tốn" khi nhìn riêng lẻ, nhưng là lớp chặn cuối cùng. Mọi đề xuất ở §5 đều giữ nguyên chúng.

| Cơ chế | Vì sao trông thừa | Vì sao phải giữ |
|---|---|---|
| RLS `tenant_isolation` trên mọi bảng tenant + `app_rw` không `BYPASSRLS` + `set_config(..., true)` trong transaction ([ADR-0003](../decisions/ADR-0003-multi-tenancy.md)) | Query nào cũng đã có `tenant_id = $1` | Lớp thứ hai khi code quên tenant; transaction-local nên không rò sang request sau trên cùng kết nối pool |
| Điều kiện `tenant_id = $1` tường minh trong mọi query | Trùng với policy RLS | Planner dùng nó để chạy index tenant-first; Supabase cũng khuyên lọc tường minh dù đã có RLS ([RLS performance](https://supabase.com/docs/guides/database/postgres/row-level-security-performance)) |
| `UNIQUE (tenant_id, id)` trên mọi bảng + FK kép `(tenant_id, x_id)` | Trùng index với PK `(id)` | Ở mức DB không thể trỏ sang bản ghi của spa khác |
| `version` + `If-Match` (412/409) | Thêm một cột, một điều kiện | Chống lost update giữa máy quầy và điện thoại KTV |
| `idempotency_record` PK `(tenant_id, actor_user_id, key)` + claim/complete trong **cùng** transaction + hash request; giữ 48 giờ | Ghi thêm một dòng mỗi POST tiền / buổi | Bấm hai lần hay retry mạng cũng không thu tiền / trừ buổi hai lần; request trùng chờ khóa PK. **Không rút ngắn 48 giờ** |
| Trigger chỉ-thêm `reject_mutation` (payment, refund, sale_line, follow-up, gia hạn, payslip; payroll_period không DELETE) + grant chỉ `INSERT, SELECT` cho `audit_log` | Trigger không bao giờ thật sự chạy | Sổ tiền không sửa được, kể cả bởi owner |
| `payment_reversal_mirrors_original` + `UNIQUE (tenant_id, reversal_of)`; refund `UNIQUE (tenant_id, customer_package_id)` + `refund_within_paid` | Trùng logic với service | Đảo tiền đúng một lần, đúng số; hoàn tiền một lần, không vượt giá bán |
| Trigger trạng thái chỉ đi tới: `visit_voided_is_final`, `sale_voided_is_final`, `customer_package_cancelled_is_final`, `appointment_closed_is_final`, `payroll_period_forward_only`, `staff_employment_forward_only`, `customer_package_item_no_restore_when_cancelled` | Service đã kiểm | Bản ghi đã hủy / đã chốt không "sống lại" do bug hay SQL tay |
| `EXCLUDE USING gist` lịch hẹn theo KTV + khóa advisory theo (spa, ngày); `EXCLUDE` các đợt làm việc không chồng nhau | GiST tốn hơn btree | Không đặt trùng KTV ngay cả khi hai request chạy song song |
| Thứ tự khóa visit/sale → gói (theo id) → item (theo id), `FOR SHARE` gói trước khi trừ buổi, `UPDATE … WHERE sessions_remaining >= qty`, `CHECK (0 ≤ remaining ≤ total)` | Thêm một câu `FOR SHARE` | Hết deadlock S10, không bao giờ âm buổi |
| `reject_closed_period` trên 4 bảng HR + `FOR SHARE` kỳ lương trong service | Hai lớp kiểm cùng một điều | Trigger là lưới an toàn; khóa `FOR SHARE` chặn ghi chen vào lúc đang chốt |
| FK `sale.visit_id` `DEFERRABLE INITIALLY DEFERRED`, `UNIQUE visit_corrects_once`, trigger `visit_correction_link`, `sale_visit_link`, `customer_package_sale_link` | Phức tạp | Liên kết lượt ↔ hóa đơn ↔ gói bất biến; mỗi lượt chỉ sửa được một lần |
| CHECK tiền: `total = subtotal − discount`, `amount_paid BETWEEN 0 AND total`, `line_total = qty × unit_price`, `visit_line_check` theo `source` | — | Không thể ghi số tiền sai hình dạng |
| `statement_timeout = 5s` mỗi `TenantTx`; READ COMMITTED + khóa tường minh | — | Không query nào giữ khóa lâu; mức cô lập đủ vì bất biến nằm ở constraint + khóa |
| Mã hóa `visit.note_ct`, `customer.health_note_ct`, `sale.note_ct` (AES-256-GCM, AAD theo bản ghi) | Không tìm được bằng SQL | Không có bản rõ dữ liệu sức khỏe trong DB, audit hay idempotency |
| Bản chụp: `service_name_snapshot`, `description_snapshot`, `name_snapshot`, `per_session_value`, `sold_price`, `unit_price`, `payment.business_date` của reversal | Trùng với catalog | Lịch sử và tiền không đổi khi bảng giá đổi, nên không phải dư thừa |
| Unique một phần: `customer_phone_uq`, `package_plan_active_name_uq`, `staff_user_idx`, `staff_employment_open_idx`, `password_reset_request_open_idx` | — | Bất biến nghiệp vụ: một SĐT một hồ sơ còn hiệu lực, một đợt mở, … |
| Supavisor **session mode** + prepared statements của sqlx | — | Transaction mode không hỗ trợ prepared statement ([Supabase](https://supabase.com/docs/guides/database/connecting-to-postgres)) |

## 7. Cần EM quyết
- **Q-DB1:** Duyệt làm A1 + A2 ngay? Chỉ đổi code, không migration, rủi ro thấp.
- **Q-DB2:** Duyệt hướng A3 (hàm `SECURITY DEFINER` gộp xác thực, kèm ADR + `secure` review) trước khi build?
- **Q-DB3:** Duyệt migration B1 (bỏ 8 index)?
- **Q-DB4:** Giữ session đã hết hạn / đã thu hồi bao lâu rồi xóa (đề xuất **30 ngày**)? Có cần chính sách giữ `audit_log` không? Câu này là pháp lý / riêng tư, không phải dung lượng: 3 năm mới 126 MB.

## 8. Tooling (`infra/perf/db/`)
| File | Làm gì | Chạy ở đâu |
|---|---|---|
| `staging-readonly.sh <file.sql>` | Chạy một file SQL trong `BEGIN READ ONLY … ROLLBACK`; URL từ `.env.staging` (hoặc `PERF_ENV_FILE`) qua `-e PGURL` | staging (chỉ đọc) |
| `staging-stats.sql` | Phiên bản, cấu hình, cỡ bảng / index, số scan, ghi, HOT | mọi môi trường |
| `staging-statements.sql` | `pg_stat_statements` của `app_rw`: gọi nhiều, tốn thời gian, tốn buffer | staging |
| `staging-payloads.sql` | Cỡ audit theo action, idempotency, session, ciphertext, độ rộng dòng (chỉ cỡ, không nội dung) | staging |
| `staging-ledger-explain.sql` | Plan sổ doanh thu tháng (chỉ SELECT) | staging |
| `invariants.sql` | 11 phép kiểm bản sao / liên kết; 0 dòng = sạch | mọi môi trường (owner) |
| `seed-busy-spa.sql` | Seed 3 năm một spa đông vào DB **tạm** (~400 MB, vài phút) | chỉ local |
| `explain_hot.py` | Sinh script `EXPLAIN (ANALYZE, BUFFERS)` cho 40 câu nóng, nguyên văn từ `.sqlx`, với `app_rw` + RLS | chỉ local |
| `experiments.sql` | Trigram owner vs `app_rw`, viết lại lịch sử, generic plan, bản nháp gộp xác thực, bỏ index (rollback) | chỉ local |

**C1 + C2 + C4 đã làm (PR 4, migration 0035–0037, 2026-11-09):**
- Session đã thu hồi / hết hạn bị xóa sau 30 ngày (`app.purge_sessions`).
- Kiểm bất biến hằng ngày (`app.invariant_violations`, log ERROR khi có dòng lệch). Trên dữ liệu `seed-demo` cả 11 phép kiểm đều 0. Trên seed 3 năm, phép kiểm `sessions_remaining` báo 2 918 dòng, đúng như dự kiến, vì seed này đặt số buổi giả.
- Mỗi job chạy một lần mỗi khoảng cho cả hệ thống (`app.claim_maintenance`, sửa F10).
- FK cột "người làm" (`NOT VALID` → `VALIDATE`; staging 0 dòng mồ côi, `infra/perf/db/actor_column_orphans.sql`).
- `idle_in_transaction_session_timeout = 30s` trong `TenantTx`.
- `audit_log` giữ, không có hạn. Index `membership (user_id)` để sau.
