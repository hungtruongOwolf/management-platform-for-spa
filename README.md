# Viên Phúc Y Đường platform

Nền tảng vận hành cho spa dưỡng sinh đông y: màn hình quầy, liệu trình khách hàng, sổ quỹ, chấm công và lương. Kiến trúc, quyết định và kế hoạch nằm trong [`memory/`](memory/README.md) (bắt đầu từ `memory/index.md`). Quy tắc làm việc cho agent: [`CLAUDE.md`](CLAUDE.md).

## Cấu trúc
```
apps/api/        Rust (Axum) — Cargo workspace, mỗi bounded context một crate (ADR-0006)
apps/web/        React + Vite SPA (app nội bộ)
packages/ui/     design tokens + components dùng chung
packages/api-client/  TS client sinh từ OpenAPI (từ S3)
infra/           docker-compose (dev), Caddyfile (production)
prototypes/      prototype HTML, không phải code production
```

## Chạy local
Yêu cầu: Rust stable, Node 22, pnpm 10, Docker Desktop (đang chạy).
```bash
cp .env.example .env        # rồi chỉnh nếu cần
pnpm install
pnpm dev:db                 # Postgres 16 ở 127.0.0.1:5432
pnpm dev:api                # Rust API ở 127.0.0.1:8080 (tự đọc .env)
pnpm dev:web                # SPA ở http://localhost:5173, proxy /api → API
```
Mở http://localhost:5173: dòng "API: đã kết nối" cho biết SPA → API hoạt động.

## Kiểm tra trước khi tạo PR
```bash
pnpm check                  # cargo fmt/clippy/test + lint + typecheck + test frontend
```
CI (`.github/workflows/ci.yml`) chạy cùng các bước trên, cộng thêm `cargo deny`, `pnpm audit`, gitleaks. PR không merge khi CI đỏ.

## Lưu ý môi trường
Repo nằm trên ổ exFAT (không hỗ trợ symlink), nên:
- pnpm dùng `node-linker=hoisted` (`.npmrc`);
- package nội bộ (`@vpyd/ui`) được dùng qua path alias trong `vite.config.ts` và `tsconfig.json`, không qua link.
