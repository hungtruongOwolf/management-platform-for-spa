---
title: Runbook — staging trên Vercel + Supabase
updated: 2026-11-05
---

# Staging trên Vercel + Supabase

Quyết định và các đánh đổi xem ở [ADR-0005](../decisions/ADR-0005-compute-hosting.md), mục Update 2026-11-05.

## Thành phần
| Gì | Ở đâu |
|---|---|
| Web (Vite tĩnh) + API (container Rust) | Vercel project `vien-phuc-y-duong`, team `hungtruongowolfs-projects`, region `pdx1` |
| Database | Supabase project `zyyiukcysjntjgjabiqe` (us-west-2), Session pooler `aws-1-us-west-2.pooler.supabase.com:5432` |
| Cấu hình | `vercel.json` (services, rewrites, security headers), `.vercelignore` (không upload `.env*`, `.claude`, `memory`) |
| Bí mật | `.env.staging` ở gốc repo, đã gitignore, giữ owner URL, mật khẩu `app_rw`, mật khẩu chủ spa, `STAGING_APP_DATA_KEY` |

## Biến môi trường trên Vercel (Production)
`APP_ENV=staging`, `APP_BIND_ADDR=0.0.0.0:8080`, `PORT=8080`, `APP_DB_MAX_CONNECTIONS=3`, `RUST_LOG=info`, `APP_PUBLIC_ORIGIN=https://vien-phuc-y-duong.vercel.app`. Hai biến sensitive:
- `APP_DATABASE_URL`: user `app_rw.<ref>` với `?sslmode=verify-full&sslrootcert=/etc/ssl/certs/supabase-root-2021.crt`.
- `APP_DATA_KEY`.

Đặt giá trị bằng stdin, không bao giờ đưa vào argv: `printf '%s' "$VAL" | npx vercel env add NAME production --sensitive`.

## Dựng DB lần đầu (đã làm 2026-11-05)
Script `staging-bootstrap.sh` (scratchpad của lead, nên chuyển vào `infra/` sau) làm các bước:
1. Tạo role `app_rw` NOBYPASSRLS.
2. `app migrate`.
3. REVOKE quyền của `anon`/`authenticated` trên schema `app`, để Data API của Supabase không chạm được dữ liệu.
4. `create-tenant vien-phuc-y-duong`, `add-user` chủ spa `0905000001`, `seed-catalog`.
5. Kiểm tra `app_rw` bị RLS chặn (không có tenant context thì thấy 0 dòng).

Ba bảng không có RLS là `user_account`, `session`, `mfa_recovery_code`. Đây là thiết kế: identity dùng chung cho mọi tenant.

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

**Chưa có trong demo (do spec, không phải do lệnh):** công nợ / trả một phần (US5 hoãn, R1 thu đủ ngay) và khách không có SĐT (D6: SĐT bắt buộc). Lượt và hóa đơn của các ngày trước có `business_date` và giờ vào đúng ngày đó, nhưng `created_at` / `paid_at` là lúc chạy lệnh: đó là sự thật về lúc dữ liệu được ghi.

## Deploy
- Từ máy dev: `npx vercel deploy --prod --yes` ở gốc repo. Vercel build cả hai service: Rust release build trong Docker, nên lần đầu lâu.
- Có migration mới thì chạy **trước** khi deploy:
  `set -a; . ./.env.staging; set +a; SQLX_OFFLINE=true DATABASE_URL="$STAGING_OWNER_URL" pnpm db:migrate`
- Sau này: kết nối Git integration (`vercel git connect`, EM duyệt GitHub app) để push lên `main` là tự deploy.

## Kiểm tra sau deploy
- `curl -s https://vien-phuc-y-duong.vercel.app/api/v1/health`
- Mở `/dang-nhap`, đăng nhập chủ spa (mật khẩu ở `.env.staging`), cài MFA lần đầu.
- `curl -sI https://vien-phuc-y-duong.vercel.app/` phải có CSP và HSTS.

## Trước khi spa dùng thật
- Lên Supabase **Pro** để có backup hằng ngày, hoặc dựng job backup riêng.
- Lên Vercel **Pro**: Hobby chỉ cho dùng phi thương mại.
- Xem lại chuyện query string nằm trong log của Vercel (L5).
- Production ở Singapore: tạo Supabase project mới `ap-southeast-1`, Vercel region `sin1`.
