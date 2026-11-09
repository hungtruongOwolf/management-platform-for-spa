# Viên Phúc Y Đường platform

Nền tảng vận hành cho spa dưỡng sinh đông y, thay cho các sổ giấy đang dùng ở quầy: **nhập một lần, mọi sổ tự sinh**. Quầy ghi lượt khách, bán và trừ buổi gói liệu trình, thu tiền; từ đó hệ thống tự ra sổ doanh thu, báo cáo, lịch sử khách, công và lương nhân viên. Thiết kế đa spa (multi-tenant) từ đầu, chạy thật cho một spa, có console để operator bán cho nhiều spa.

- Stack: Rust (Axum, SQLx) + PostgreSQL có RLS (16 ở local/CI, 17 trên Supabase) · React + Vite + TanStack Router/Query · Vercel + Supabase.
- Kiến thức dự án (spec, ADR, thiết kế, nhật ký) nằm trong wiki [`memory/`](memory/README.md), bắt đầu từ [`memory/index.md`](memory/index.md). Quy tắc làm việc cho agent: [`CLAUDE.md`](CLAUDE.md).

## Tài liệu

| Trang | Nội dung |
|---|---|
| [docs/system-design.md](docs/system-design.md) | Thành phần, vòng đời request, ranh giới tin cậy và bảo mật, triển khai, job nền, ngân sách hiệu năng |
| [docs/database-design.md](docs/database-design.md) | Nhóm bảng + ER, RLS và hàm `SECURITY DEFINER`, ràng buộc toàn vẹn, mã hóa, index, migration |
| [docs/features.md](docs/features.md) | Tính năng theo vai trò và theo khu vực, tên màn hình, cờ tính năng, console nền tảng |
| [docs/README.md](docs/README.md) | Mục lục tài liệu và đường dẫn vào wiki |

## Tính năng chính theo vai trò

| Vai trò | Tóm tắt |
|---|---|
| **Chủ spa** | Toàn quyền trong spa (bắt buộc MFA): mọi việc của quản lý, cộng bảng giá và mẫu gói, gia hạn / hủy + hoàn tiền gói, hủy bản ghi ngày đã qua, nhập bù, bảng lương và chốt tháng, hồ sơ nhân viên + tạo tài khoản + đặt lại mật khẩu, báo cáo 3–5 năm |
| **Quản lý / thu ngân** | Quầy (ghi lượt, trừ buổi, dịch vụ lẻ, giỏ bán gói, giảm giá trong trần, sửa / hủy trong ngày), lịch hẹn, khách hàng + ghi chú sức khỏe + gọi lại khách vắng, báo cáo kinh doanh + sổ doanh thu + nhật ký ghi chú, chấm công / đánh giá / thưởng phạt cho KTV (không thấy lương) |
| **Kỹ thuật viên** | "Công và lương của tôi" (ngày công, đánh giá, phiếu lương), "Khách của tôi" (lịch hẹn và lượt mình làm), xem bảng giá |
| **Người đã nghỉ** | Vẫn đăng nhập được để xem công và phiếu lương của các tháng đã làm (quyền do server suy ra) |
| **Operator nền tảng** | Console `/admin` riêng (MFA luôn bắt buộc, xác thực lại trước mỗi thao tác ghi): tạo / tạm ngưng spa, bật tắt 6 tính năng theo spa, mẫu vai trò, tài khoản xuyên spa, nhật ký nền tảng; chỉ thấy số đếm, không thấy dữ liệu khách |

Chi tiết từng màn: [docs/features.md](docs/features.md).

## Cấu trúc repo
```
apps/api/                 Rust Cargo workspace (ADR-0006)
  crates/kernel           nền dùng chung: TenantTx, Money, Permission/Actor, idempotency, audit, crypto, feature flags
  crates/identity         tài khoản, session, MFA, role/permission, /auth, /me
  crates/org | hr         hồ sơ + đợt làm việc | chấm công, đánh giá, lương
  crates/catalog | crm    bảng giá, mẫu gói | khách hàng, ghi chú sức khỏe, gọi lại
  crates/packages | sales gói của khách | hóa đơn, thanh toán, giỏ bán gói
  crates/operations       lượt khách, lịch hẹn
  crates/reporting        tổng quan, báo cáo, sổ doanh thu, lịch sử khách, nhật ký ghi chú
  crates/platform         console nền tảng (/api/v1/admin)
  crates/app              binary: router, middleware, job bảo trì, CLI
  migrations/             SQL 0001–0040 (forward-only)
  openapi.json            hợp đồng API sinh từ code
apps/web/                 React + Vite SPA (src/app, src/features/*, src/admin = console /admin), e2e/ Playwright
packages/ui/              design tokens + component dùng chung
packages/api-client/      type TypeScript sinh từ openapi.json
infra/                    docker-compose dev/prod, postgres-init (role app_rw), Caddy, backup/, perf/ (đo latency + DB), prodtest/
scripts/                  merge-when-green.sh, merge-local-gate.sh, ci-changes.sh, api-spec.mjs, check-extra.mjs
docs/                     tài liệu tổng quan (trang này link tới)
memory/                   wiki dự án: raw/ (nguồn bất biến), wiki/, index.md, log.md
docs for reference/       tài liệu nghiệp vụ gốc của spa (sổ, menu, bảng chấm công)
prototypes/               prototype HTML, không phải code production
vercel.json               services web + api, rewrites, security headers (staging)
```

## Chạy local
Yêu cầu: Rust stable (`rust-toolchain.toml`), Node ≥ 22, pnpm 10, Docker Desktop đang chạy.

```bash
cp .env.example .env        # rồi điền APP_DATA_KEY: openssl rand -base64 32
pnpm install
pnpm dev:db                 # Postgres 16 ở 127.0.0.1:5432 (tạo role app_rw ở lần chạy đầu)
pnpm db:migrate             # áp migration bằng DATABASE_URL (schema owner)

# tạo spa, nạp menu, tạo chủ spa (mật khẩu đọc từ env, không nằm trong argv / lịch sử shell)
cargo run --manifest-path apps/api/Cargo.toml -p app -- create-tenant --slug vien-phuc-y-duong --name "Viên Phúc Y Đường" --branch "Phường Đông Hà"
cargo run --manifest-path apps/api/Cargo.toml -p app -- seed-catalog --tenant vien-phuc-y-duong
APP_NEW_USER_PASSWORD='…' cargo run --manifest-path apps/api/Cargo.toml -p app -- add-user --tenant vien-phuc-y-duong --phone 09xxxxxxxx --name "…" --role owner

pnpm dev:api                # Rust API ở 127.0.0.1:8080 (đọc .env)
pnpm dev:web                # SPA ở http://localhost:5173, proxy /api → API (cùng origin như production)
```
Mở http://localhost:5173/dang-nhap, đăng nhập bằng SĐT của chủ spa. Lần đầu chủ spa phải cài MFA (quét QR bằng app authenticator, cất mã khôi phục).

**Dữ liệu demo (tùy chọn).** Đổ khách, gói, hóa đơn, lượt 21 ngày, nhân viên và chấm công giống thật vào spa vừa tạo; tạo thêm tài khoản quản lý demo dùng mật khẩu trong env. Lệnh từ chối chạy khi `APP_ENV=production` và khi spa đã có dữ liệu demo ([vercel-staging § Dữ liệu demo](memory/wiki/runbooks/vercel-staging.md)):
```bash
APP_NEW_USER_PASSWORD='…' cargo run --manifest-path apps/api/Cargo.toml -p app -- seed-demo --tenant vien-phuc-y-duong
```

**Console nền tảng `/admin` (tùy chọn).** Tạo operator rồi mở ngay http://localhost:5173/admin/dang-nhap để cài TOTP:
```bash
APP_NEW_OPERATOR_PASSWORD='…' cargo run --manifest-path apps/api/Cargo.toml -p app -- add-operator --username ops --name "…"
```

Các lệnh CLI khác của binary `app`: `add-staff`, `unlock`, `reset-mfa`, `reset-operator-mfa`, `disable-operator` / `enable-operator`, `import-paper-book`, `openapi` (xem `apps/api/crates/app/src/main.rs`).

## Database
- `DATABASE_URL`: schema owner, dùng cho migration, compile-time check của sqlx và test. `APP_DATABASE_URL`: role `app_rw`, API chạy bằng role này nên RLS luôn áp dụng. API không bao giờ kết nối bằng owner.
- `APP_DATA_KEY` mã hóa ghi chú sức khỏe và secret MFA. Mất key là mất dữ liệu đó: giữ trong secret store và backup.
- Sửa query xong chạy `pnpm db:prepare` và commit `apps/api/.sqlx` (CI build ở chế độ offline).
- Đổi API thì chạy `pnpm api:gen` (sinh `openapi.json` + type ở `packages/api-client`).
- Thiết kế chi tiết: [docs/database-design.md](docs/database-design.md).

## Kiểm tra và merge
```bash
pnpm check                  # cargo fmt/clippy/test + lint + typecheck + test frontend + check:extra
pnpm e2e                    # Playwright: tự dựng API :8091 + web :5181 trên DB riêng vpyd_e2e (tạo DB một lần, xem testing.md)
pnpm lhci                   # Lighthouse: LCP ≤ 2,5 s cho /dang-nhap và /quay
```
CI (`.github/workflows/ci.yml`) chạy các bước trên theo phạm vi file thay đổi, cộng `cargo deny`, `pnpm audit`, gitleaks, E2E chia shard, perf (API p95 + Lighthouse). Nhánh `main` chưa bật branch protection, nên merge bằng `bash scripts/merge-when-green.sh <pr>`: script chỉ merge khi mọi check đã xanh. Khi GitHub Actions tắt: `scripts/merge-local-gate.sh` ([local-merge-gate](memory/wiki/runbooks/local-merge-gate.md)). Quy ước test: [testing](memory/wiki/conventions/testing.md).

## Triển khai
Staging chạy trên Vercel (web tĩnh + API container, region `pdx1`) với Supabase `us-west-2` qua session pooler. Migration chạy trước khi deploy; deploy bằng `npx vercel deploy --prod`. Biến môi trường, bootstrap DB và kiểm tra sau deploy: [vercel-staging](memory/wiki/runbooks/vercel-staging.md). Production dự kiến đặt API + DB cùng Singapore ([ADR-0012](memory/wiki/decisions/ADR-0012-colocate-api-db-singapore.md)). Không bao giờ đưa secret, URL DB hay mật khẩu vào git hoặc tài liệu.

## Lưu ý môi trường
Repo nằm trên ổ exFAT (không hỗ trợ symlink), nên:
- pnpm dùng `node-linker=hoisted` (`.npmrc`);
- package nội bộ (`@vpyd/ui`) được dùng qua path alias trong `vite.config.ts` và `tsconfig.json`, không qua link.
