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
