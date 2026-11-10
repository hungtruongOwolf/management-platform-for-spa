---
title: Runbook — staging trên Vercel + Supabase
updated: 2026-11-09
sources: [vercel.json, apps/api/crates/app/src/config.rs, apps/api/crates/app/src/main.rs, decisions/ADR-0005-compute-hosting.md]
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

CLI `app` (danh sách đầy đủ ở đầu `apps/api/crates/app/src/main.rs`): `migrate`, `create-tenant`, `add-user`, `add-staff`, `seed-catalog`, `seed-demo`, `import-paper-book`, `reset-mfa`, `unlock`, `add-operator`, `reset-operator-mfa`, `disable-operator` / `enable-operator`, `openapi`. Mật khẩu chỉ qua env (`APP_NEW_USER_PASSWORD`, `APP_NEW_OPERATOR_PASSWORD`), không qua argv. Lệnh ghi qua use case (`seed-demo`, `import-paper-book`) cần cả `DATABASE_URL` (owner) và `APP_DATABASE_URL` + `APP_DATA_KEY`.

**Operator console** (`/admin`, ADR-0018): tạo operator bằng `add-operator --username U --name N` với `APP_NEW_OPERATOR_PASSWORD`; MFA cài ở lần đăng nhập đầu tại `/admin`.
5. Kiểm tra `app_rw` bị RLS chặn (không có tenant context thì thấy 0 dòng).

Ba bảng không có RLS là `user_account`, `session`, `mfa_recovery_code` (kiểm lại đến migration 0040). Đây là thiết kế: identity dùng chung cho mọi tenant. Bảng của console (`platform_*`, `role_template_permission`, `tenant_feature`) có RLS và chỉ ghi qua hàm có cổng operator.

## Dữ liệu demo
Staging trống thì EM không thử được luồng nào ([raw](../../raw/2026-11-05-em-brand-colors-and-demo-data.md)). Lệnh `app seed-demo` đổ dữ liệu giống thật vào một spa, đi qua đúng các use case của quầy (validation, audit, payment append-only đều giữ nguyên).

**Chạy** (sau khi đã có chủ spa và đã migrate; mật khẩu quản lý demo chỉ qua env):
```bash
set -a; . ./.env.staging; set +a
read -rs APP_NEW_USER_PASSWORD; export APP_NEW_USER_PASSWORD   # mật khẩu cho tài khoản quản lý demo
SQLX_OFFLINE=true DATABASE_URL="$STAGING_OWNER_URL" APP_DATABASE_URL="<app_rw URL>" APP_DATA_KEY="$STAGING_APP_DATA_KEY" \
  cargo run --manifest-path apps/api/Cargo.toml -p app -- seed-demo --tenant vien-phuc-y-duong
```
- Cần cả hai kết nối như các lệnh CLI khác: `DATABASE_URL` (schema owner: thêm nhân viên, thêm người dùng, nạp menu nếu còn trống) và `APP_DATABASE_URL` (app_rw, RLS: mọi khách, hóa đơn, lượt). `APP_DATA_KEY` phải **đúng key của API trên Vercel**, nếu không ghi chú sức khỏe sẽ không giải mã được trên web.
- Spa phải có chủ spa (`add-user --role owner`): nhập sổ cũ, tạo gói và hủy + hoàn tiền chạy dưới tên chủ spa, nên audit ghi đúng người.
- Local mất ~5 giây. Qua pooler Supabase ở us-west-2 thì mỗi lượt tốn vài chục round-trip: dự kiến **5–15 phút**. Lệnh in báo cáo khi xong.
- Lệnh từ chối chạy khi `APP_ENV=production`: dữ liệu bịa không bao giờ được lẫn vào sổ của spa thật.
- Chạy lần hai bị từ chối ("this spa already has demo data", exit 1, không ghi gì). `--force-again` thêm một lô khách mới (dải SĐT kế tiếp), dùng lại nhân viên và gói.

**Tạo ra gì** (RNG cố định, nên cùng một ngày chạy thì ra cùng dữ liệu):
| Gì | Số lượng |
|---|---|
| Đăng nhập quản lý | `0905000002` "Nguyễn Thị Thu Hà", role manager (không MFA), mật khẩu = `APP_NEW_USER_PASSWORD` |
| Nhân viên | 5 KTV (Lan, Thu, Hoa, Nam, Hằng) + 1 "Quản lý" để chọn làm người chốt |
| Menu + gói | Menu in nếu spa chưa có; 5 gói liệu trình (gội 10+2, cổ vai gáy 10+1, body 10+2, xông chân 10+2, combo gội + cổ vai gáy 5+5) |
| Khách | 40 khách tên thật kiểu Việt, SĐT dải dành riêng `0399 RR CCCC`, 14 khách có ghi chú sức khỏe (mã hóa), 2 khách chưa ghi giới tính |
| Nhập sổ cũ | 3 khách: 1 gói **sắp hết hạn** (còn 5 ngày), 1 gói **đã hết hạn** (hôm nay dùng tiếp với lý do override), 1 gói không hạn của khách có sẵn |
| Bán gói | 22 hóa đơn: 18 trong 3 tuần qua, 2 hôm nay, 1 **bị hủy** hôm nay + hóa đơn bán lại; khoảng 1/3 có giảm giá 5–10% kèm lý do; tiền mặt, chuyển khoản, hoặc cả hai |
| Lượt khách | ~120 lượt trong 21 ngày (4–7 lượt/ngày), trừ buổi liệu trình hoặc dịch vụ lẻ, đôi khi thêm dịch vụ bổ sung trả ngay; **12 lượt hôm nay** cho `/quay`, trong đó 1 lượt **bị hủy** |
| Hủy liệu trình | 1 gói đã dùng dở, chủ spa hủy + hoàn tiền mặt phần chưa dùng |

**Chưa có trong demo:** công nợ / trả một phần (US5 hoãn, R1 thu đủ ngay) và khách không có SĐT: D6 đã được thay bằng [ADR-0011](../decisions/ADR-0011-anonymous-walk-in-and-optional-phone.md) (SĐT tùy chọn, khách vãng lai không tên), nhưng `seed-demo` vẫn chỉ tạo khách có tên + SĐT. Lượt và hóa đơn của các ngày trước có `business_date` và giờ vào đúng ngày đó, nhưng `created_at` / `paid_at` là lúc chạy lệnh: đó là sự thật về lúc dữ liệu được ghi.

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
