---
title: Runbook — staging trên Vercel + Supabase
updated: 2026-11-09
sources: [vercel.json, apps/api/crates/app/src/config.rs, apps/api/crates/app/src/main.rs, apps/api/crates/app/src/demo/reset.rs, decisions/ADR-0005-compute-hosting.md, raw/2026-11-09-em-request-staging-full-scenario-data.md]
---

# Staging trên Vercel + Supabase

Quyết định và các đánh đổi xem ở [ADR-0005](../decisions/ADR-0005-compute-hosting.md), mục Update 2026-11-05.

## Thành phần
| Gì | Ở đâu |
|---|---|
| Web (Vite tĩnh) + API (container Rust, `apps/api/Dockerfile`) | Vercel project `vien-phuc-y-duong`, region `pdx1` (`vercel.json` → `services.web` + `services.api`, `/api/*` rewrite vào `api`) |
| Database | Supabase project staging ở `us-west-2`, kết nối qua Session pooler cổng 5432 (ref và host nằm trong `.env.staging`, không ghi ở wiki) |
| Cấu hình | `vercel.json` (services, rewrites, security headers), `.vercelignore` (không upload `.env*`, `.claude`, `memory`) |
| Bí mật | `.env.staging` ở gốc repo, đã gitignore, giữ owner URL, mật khẩu `app_rw`, mật khẩu chủ spa, `STAGING_APP_DATA_KEY` |

## Biến môi trường trên Vercel (Production)
Tên biến khớp `apps/api/crates/app/src/config.rs` (đọc lúc khởi động, sai thì API không chạy) và `main.rs`:

| Biến | Giá trị staging | Ghi chú |
|---|---|---|
| `APP_ENV` | `staging` | `development` / `staging` / `production`; image Docker mặc định `production` |
| `APP_BIND_ADDR` | `0.0.0.0:8080` | bắt buộc (image đã đặt sẵn) |
| `PORT` | `8080` | cho Vercel, API không đọc |
| `APP_PUBLIC_ORIGIN` | origin `https://…` của deployment, không `/` cuối | bắt buộc; kiểm `Origin` của request ghi |
| `APP_ADMIN_ORIGIN` | không đặt | tùy chọn (ADR-0018): origin riêng cho `/api/v1/admin/*` khi có subdomain; phải khác `APP_PUBLIC_ORIGIN` |
| `APP_DB_MAX_CONNECTIONS` | `3` | mặc định 10; nhỏ vì nhiều instance dùng chung pooler |
| `RUST_LOG` | `info` | mặc định `info` |
| `APP_DATABASE_URL` (sensitive) | user `app_rw.<ref>`, `?sslmode=verify-full&sslrootcert=/etc/ssl/certs/supabase-root-2021.crt` | ngoài development bắt buộc `verify-full`/`verify-ca` |
| `APP_DATA_KEY` (sensitive) | 32 byte base64 | khóa mã hóa trường; phải trùng key dùng khi chạy CLI ghi dữ liệu |

`APP_RATE_PER_SECOND` / `APP_RATE_BURST` **không được đặt**: ngoài `development` API từ chối khởi động nếu có. `APP_DB_INSECURE_TLS_FOR_LOCAL_TEST` chỉ cho prodtest local.

Đặt giá trị bằng stdin, không bao giờ đưa vào argv: `printf '%s' "$VAL" | npx vercel env add NAME production --sensitive`.

## Dựng DB lần đầu (đã làm 2026-11-05)
Script `staging-bootstrap.sh` (scratchpad của lead, nên chuyển vào `infra/` sau) làm các bước:
1. Tạo role `app_rw` NOBYPASSRLS.
2. `app migrate`.
3. REVOKE quyền của `anon`/`authenticated` trên schema `app`, để Data API của Supabase không chạm được dữ liệu.
4. `create-tenant --slug vien-phuc-y-duong`, `add-user` chủ spa `0905000001`, `seed-catalog`.

CLI `app` (danh sách đầy đủ ở đầu `apps/api/crates/app/src/main.rs`): `migrate`, `create-tenant`, `add-user`, `add-staff`, `seed-catalog`, `seed-demo`, `reset-demo`, `import-paper-book`, `reset-mfa`, `unlock`, `add-operator`, `reset-operator-mfa`, `disable-operator` / `enable-operator`, `openapi`. Mật khẩu chỉ qua env (`APP_NEW_USER_PASSWORD`, `APP_NEW_OPERATOR_PASSWORD`, và cho tài khoản demo `APP_NEW_TECHNICIAN_PASSWORD`, `APP_NEW_FORMER_TECHNICIAN_PASSWORD`), không qua argv. Lệnh ghi qua use case (`seed-demo`, `import-paper-book`) cần cả `DATABASE_URL` (owner) và `APP_DATABASE_URL` + `APP_DATA_KEY`.

**Operator console** (`/admin`, ADR-0018): tạo operator bằng `add-operator --username U --name N` với `APP_NEW_OPERATOR_PASSWORD`; MFA cài ở lần đăng nhập đầu tại `/admin`.
5. Kiểm tra `app_rw` bị RLS chặn (không có tenant context thì thấy 0 dòng).

Ba bảng không có RLS là `user_account`, `session`, `mfa_recovery_code` (kiểm lại đến migration 0040). Đây là thiết kế: identity dùng chung cho mọi tenant. Bảng của console (`platform_*`, `role_template_permission`, `tenant_feature`) có RLS và chỉ ghi qua hàm có cổng operator.

## Dữ liệu demo
Staging trống thì EM không thử được luồng nào ([raw](../../raw/2026-11-05-em-brand-colors-and-demo-data.md)); từ 2026-11-09 staging dùng **một kịch bản đầy đủ 12 tháng** ([raw](../../raw/2026-11-09-em-request-staging-full-scenario-data.md)). Hai lệnh CLI, không bao giờ chạy cho production (xem `APP_ENV` dưới):
- `app reset-demo --tenant S`: xóa toàn bộ dữ liệu nghiệp vụ của một spa (một transaction, schema owner).
- `app seed-demo --tenant S [--months N] [--workers W]`: đổ kịch bản qua đúng các use case (validation, audit, payment append-only đều giữ nguyên). `N` mặc định 12 (2–24), `W` mặc định 8 use case chạy song song.

**Biến môi trường** (chỉ tên; giá trị ở `.env.staging`, đọc bằng `read -rs`, không bao giờ qua argv):
| Biến | Lệnh | Ghi chú |
|---|---|---|
| `DATABASE_URL` | cả hai | schema owner: reset, thêm người dùng / nhân viên, nạp menu nếu trống |
| `APP_DATABASE_URL` | `seed-demo` | `app_rw` (RLS): mọi khách, hóa đơn, lượt, lịch, chấm công, lương |
| `APP_DATA_KEY` | `seed-demo` | phải **đúng key của API trên Vercel**, nếu không ghi chú sức khỏe / ghi chú buổi không giải mã được trên web |
| `APP_NEW_USER_PASSWORD` | `seed-demo` | mật khẩu quản lý demo `0905000002`; chỉ cần khi tài khoản **chưa có** |
| `APP_NEW_TECHNICIAN_PASSWORD` | `seed-demo` | KTV demo `0905000003`; chỉ cần khi chưa có |
| `APP_NEW_FORMER_TECHNICIAN_PASSWORD` | `seed-demo` | KTV đã nghỉ `0905000004`; chỉ cần khi chưa có |
| `APP_ENV` | cả hai | `seed-demo` từ chối `production`; `reset-demo` chỉ chạy khi đặt **rõ** `development` hoặc `staging` (không đặt cũng từ chối) |

Tài khoản đã có (cùng SĐT) được **dùng lại, giữ mật khẩu và MFA**; thiếu mật khẩu cho tài khoản chưa có thì lệnh dừng trước khi ghi gì.

**Quy trình reset + seed trên staging** (lead chạy; agent không đụng staging):
```bash
set -a; . ./.env.staging; set +a
# 1. Backup trước (Supabase free không có backup hằng ngày). File chứa dữ liệu staging: để NGOÀI repo, xóa khi xong.
pg_dump "$STAGING_OWNER_URL" --schema=app --format=custom --file="$HOME/staging-before-reset.dump"
# 2. Xóa dữ liệu nghiệp vụ của spa (APP_ENV phải đặt rõ)
APP_ENV=staging SQLX_OFFLINE=true DATABASE_URL="$STAGING_OWNER_URL" \
  cargo run --manifest-path apps/api/Cargo.toml -p app -- reset-demo --tenant vien-phuc-y-duong
# 3. Đổ kịch bản (mật khẩu chỉ cần cho tài khoản chưa có)
read -rs APP_NEW_TECHNICIAN_PASSWORD; export APP_NEW_TECHNICIAN_PASSWORD
read -rs APP_NEW_FORMER_TECHNICIAN_PASSWORD; export APP_NEW_FORMER_TECHNICIAN_PASSWORD
SQLX_OFFLINE=true DATABASE_URL="$STAGING_OWNER_URL" APP_DATABASE_URL="<app_rw URL>" APP_DATA_KEY="$STAGING_APP_DATA_KEY" \
  cargo run --manifest-path apps/api/Cargo.toml -p app -- seed-demo --tenant vien-phuc-y-duong
```
- `reset-demo` in số dòng đã xóa theo bảng. Nó tắt trigger append-only của các bảng nghiệp vụ trong transaction (`ALTER TABLE … DISABLE TRIGGER USER`, owner sở hữu bảng; không dùng `session_replication_role` vì Supabase không có superuser) rồi bật lại trước commit; lỗi giữa chừng thì rollback cả trigger. `ALTER TABLE` khóa bảng trong vài giây: API đang chạy sẽ chờ (`lock_timeout` 15 s).
- Rollback từ backup: `pg_restore --clean --if-exists --schema=app -d "$STAGING_OWNER_URL" "$HOME/staging-before-reset.dump"`.
- `seed-demo` từ chối spa đã có khách demo (SĐT `0399 00…`, exit 1, không ghi gì): chạy `reset-demo` trước. Không còn `--force-again`.

**Xóa gì / giữ gì** (`apps/api/crates/app/src/demo/reset.rs`; test `every_tenant_table_is_classified_for_reset` fail khi có bảng theo spa mới chưa được xếp loại):
| Xóa (theo spa) | Giữ |
|---|---|
| `customer`, `customer_follow_up`; `visit`, `visit_line`; `sale`, `sale_line`, `payment`, `refund`; `customer_package`, `customer_package_item`, `customer_package_extension` (hủy gói là cột của gói); `appointment`, `appointment_service`; `staff`, `staff_employment`, `staff_pay`, `attendance_day`, `staff_daily_review`, `staff_evaluation`, `staff_adjustment`, `payroll_period`, `payslip`; `password_reset_request`; `idempotency_record`; `audit_log` của spa | `tenant` (kể cả `owner_mfa_required`), `branch`, `role`, `role_permission`, `tenant_feature`; danh mục `service_category`, `service`, `service_price`, `package_plan`, `package_plan_item`; `user_account`, `membership`, `membership_role`, `mfa_recovery_code`, `session`; **cấu hình** `payroll_rules` (quy tắc lương của chủ spa) và `appointment_setting` (giờ mở cửa, độ dài khung); mọi bảng `platform_*`, `role_template_permission`, `maintenance_run` |

`payroll_rules` / `appointment_setting` được giữ vì là cài đặt, không phải lịch sử: một lần sửa quy tắc lương không nên mất khi làm sạch dữ liệu demo. `idempotency_record` bị xóa vì nó là bản chụp phản hồi của các lần ghi vừa bị xóa.

**Tạo ra gì** (RNG cố định, mọi ngày tính theo ngày chạy: cùng một ngày thì ra cùng dữ liệu; số liệu dưới là lần chạy local 2026-11-10):
| Gì | Lần chạy local 12 tháng |
|---|---|
| Cửa sổ | Từ ngày 1 của tháng cách đây 11 tháng tới hôm nay (`--months 12`: 2025-12-01 → 2026-11-10); **11 tháng đã chốt lương**, tháng này mở |
| Đăng nhập | Chủ spa có sẵn; quản lý `0905000002` (gắn hồ sơ "Nguyễn Thị Thu Hà"); KTV `0905000003` (gắn "Nguyễn Thị Lan"); KTV đã nghỉ `0905000004` ("Đỗ Thị Ngân", nghỉ ngày 18 tháng trước, chỉ còn xem công lương quá khứ, ADR-0014). 1 yêu cầu "Quên mật khẩu" của `0905000003` đang chờ ở Tổng quan chủ spa |
| Nhân viên | 7 hồ sơ: quản lý, 5 KTV đang làm (Lan, Thu, Hoa: vào làm ngày 20 bốn tháng trước; Nam: hay nghỉ / đi muộn; Hằng), 1 đã nghỉ; hoa hồng 3/5/7/10 %, thưởng gắn bó 500k cho 5 người, tiền ăn |
| Sổ nhân sự | 2.161 ngày công (140 ngày P / ĐX / K, 291 lần đi muộn), 349 dòng khen / phàn nàn, 22 thưởng / phạt (tiền hoặc điểm), 70 đánh giá tháng (Nam có tháng Mức 3 / Chưa đạt kèm kế hoạch kèm cặp), 70 phiếu lương. Tháng không trọn (ADR-0019 D4): người mới 12 ngày → lương cứng theo tỷ lệ, không thưởng gắn bó; người nghỉ 18 ngày → theo tỷ lệ, có thưởng gắn bó |
| Khách | 220 (40 viết tay + sinh tự động, tên Việt, SĐT dải demo `0399 00 CCCC`): 16 khách vãng lai không tên, 13 có tên không SĐT, 7 hồ sơ mới chưa ghi giới tính, 22 sinh nhật tháng này, 59 có ghi chú sức khỏe (mã hóa); 64 khách vắng 60–180 ngày, 36 lần liên lạc kèm ngày hẹn gọi lại (một số đã tới hạn) |
| Gói | 75 đang dùng (4 sắp hết hạn ≤ 7 ngày), 30 hết hạn, 32 hết buổi, 3 đã hủy (2 hủy + hoàn tiền mặt / chuyển khoản, 1 bán nhầm bị hủy hóa đơn), 1 đã gia hạn; 5 gói của 4 khách nhập sổ giấy (`import_opening_balance`, như `import-paper-book`) |
| Bán hàng | 135 hóa đơn gói (69 bán kèm trong lượt), 78 hóa đơn giảm giá có lý do; 900 lần trả tiền mặt, 540 chuyển khoản, 68 hóa đơn trả cả hai; 11 bút toán đảo; 1 hóa đơn hủy hôm nay + bán lại |
| Lượt | 1.977 lượt trong 12 tháng (4–6 ngày thường, 6–8 cuối tuần), gói hoặc dịch vụ lẻ, dịch vụ thêm, người giới thiệu, người đi cùng; 268 có ghi chú buổi; 8 hủy, 5 "Sửa lượt" đổi tiền, 6 "Sửa lượt" đổi KTV; **hôm nay** khoảng 8 lượt tới vài phút trước giờ chạy (lượt cuối đang làm), 1 lượt dùng gói hết hạn có lý do |
| Lịch hẹn | 71 hoàn tất thành lượt, 14 không đến, 14 hủy có lý do (khoảng 2 lịch / tuần); hôm nay 1 quá giờ chưa đến, 1 đã đến, vài lịch sắp tới (tùy giờ chạy, trong giờ mở cửa); 59 lịch trong 14 ngày tới (một số khách mới); 1 lịch gia đình 3 người |

Số "hôm nay" phụ thuộc giờ chạy (chạy trước 9:00 thì không có lịch quá giờ / đã đến). Lệnh in đúng bảng đếm này (đọc thẳng từ bảng) khi xong.

**Chưa có trong demo:** nửa ngày công (quy tắc chấm công chỉ có 4 trạng thái Đi làm / P / ĐX / K, không có nửa ngày); công nợ / trả một phần (R1 thu đủ ngay); bán sản phẩm (chưa có tính năng). Lượt và hóa đơn của các ngày trước có `business_date` và giờ vào đúng ngày đó, nhưng `created_at` / `paid_at` / thời điểm audit là lúc chạy lệnh: đó là sự thật về lúc dữ liệu được ghi.

**Tốc độ:** local (Postgres trên máy, build debug) **14,5 s** cho 12 tháng, khoảng 52.600 câu SQL, **21,5 câu mỗi lượt** (kể cả hóa đơn, hủy, sửa của lượt đó); lệnh in thời gian và số câu SQL từng phase. Qua Supabase session pooler ~150 ms mỗi round trip: chạy tuần tự sẽ là 52.600 × 0,15 s ≈ 2,2 giờ. Seed chạy 8 use case song song (`--workers`, mặc định 8; mỗi khách một chuỗi theo thời gian, các ngày của lịch hẹn và các tháng chấm công song song; lỗi kết nối tạm thời được thử lại với cùng idempotency key), ước tính **≈ 18 phút**: khách 1.744 × 0,15 / 8 ≈ 33 s; sổ giấy 91 × 0,15 ≈ 14 s (tuần tự); quầy 41.222 × 0,15 / 8 ≈ 13 phút (chuỗi dài nhất ngắn hơn nhiều); lịch hẹn 5.095 × 0,15 / 8 ≈ 1,6 phút; sổ nhân sự + 11 lần chốt lương tuần tự ≈ 2,5 phút; setup < 30 s. Pool `app_rw` của lệnh = `workers + 2` kết nối: nếu pooler báo hết kết nối thì chạy `--workers 6` (≈ 24 phút).

## Deploy
- Từ máy dev: `npx vercel deploy --prod --yes` ở gốc repo. Vercel build cả hai service: Rust release build trong Docker, nên lần đầu lâu.
- Có migration mới thì chạy **trước** khi deploy:
  `set -a; . ./.env.staging; set +a; SQLX_OFFLINE=true DATABASE_URL="$STAGING_OWNER_URL" pnpm db:migrate`
- Sau này: kết nối Git integration (`vercel git connect`, EM duyệt GitHub app) để push lên `main` là tự deploy.

## Kiểm tra sau deploy
Thay `<staging>` bằng origin trong `APP_PUBLIC_ORIGIN`:
- `npx vercel ls` phải có deployment **mới** (cột Age vài phút) và output deploy có `Aliased`. Lệnh deploy từng thoát 0 mà không tạo deployment (2026-11-09), khi đó các URL dưới vẫn 200 vì đang chạy bản cũ; gọi thêm một route chỉ bản mới có (không cookie → 401, bản cũ → 404).
- `curl -s <staging>/api/v1/health` → 200.
- `curl -s -o /dev/null -w '%{http_code}' <staging>/api/v1/me` → 401; `<staging>/api/v1/admin/me` → 401; `<staging>/admin` → 200.
- Mở `/dang-nhap`, đăng nhập chủ spa (mật khẩu ở `.env.staging`), cài MFA lần đầu.
- `curl -sI <staging>/` phải có CSP và HSTS (từ `vercel.json`).

## Trước khi spa dùng thật
- Lên Supabase **Pro** để có backup hằng ngày, hoặc dựng job backup riêng.
- Lên Vercel **Pro**: Hobby chỉ cho dùng phi thương mại.
- Xem lại chuyện query string nằm trong log của Vercel (L5).
- Production ở Singapore: tạo Supabase project mới `ap-southeast-1`, Vercel region `sin1`.
