---
title: ADR-0005 Compute hosting
status: accepted (provider chọn ở /devops sau khi kiểm tra giá)
date: 2026-11-04
deciders: EM, agent
---

# ADR-0005: Nơi chạy API và app

## Context
Cần chạy Rust API + file tĩnh SPA, gần Supabase Singapore, chi phí thấp, vận hành đơn giản cho một người. Tải rất nhỏ (xem [system](../architecture/system.md) §7).

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **1 VPS Singapore + Docker Compose + Caddy** | Rẻ nhất ($6–12/tháng); TLS tự động; kiểm soát toàn bộ | Tự vá OS, tự theo dõi |
| PaaS (Fly.io / Render / Railway) | Ít vận hành, deploy bằng git | Đắt hơn; vùng/giá thay đổi; khó kiểm soát header/network |
| Serverless | Scale về 0 | Rust cold start; khó chạy job nền và chốt quỹ theo lịch |

## Decision
**Một VPS ở Singapore**, Docker Compose gồm: `caddy` (TLS, header bảo mật, file tĩnh, proxy `/api`) + `api` (Rust, image distroless, user non-root). Chỉ mở port 443/80; SSH bằng key, tắt đăng nhập bằng mật khẩu; cập nhật bảo mật tự động. Nhà cung cấp (DigitalOcean / Vultr / Hetzner / Linode) chốt ở `/devops` sau khi kiểm tra giá hiện tại.

## Consequences
- Positive: chi phí thấp, đơn giản, một máy dễ hiểu.
- Negative: một điểm lỗi duy nhất. Chấp nhận được vì có phương án giấy tạm, có runbook dựng lại < 1 giờ, và uptime alert.

## Revisit when
Nhiều spa, cần uptime cao hơn → 2 instance + load balancer (API stateless, session ở DB nên scale ngang được).

## Update 2026-11-04 (S11 prep)
- Provider khuyến nghị: **DigitalOcean Basic 2 GiB / 1 vCPU ($12/tháng)**, so sánh giá và nguồn ở [runbooks/staging-setup](../runbooks/staging-setup.md) §1. EM chốt khi mua.
- **Staging** đặt VPS **cùng vùng với project Supabase staging (Mỹ)**, vì đi xuyên Thái Bình Dương mất khoảng 170 ms mỗi query. **Production** vẫn là Singapore (VPS SGP1 + project Supabase mới ở `ap-southeast-1`).
- Triển khai: image build trên GitHub Actions rồi push lên GHCR; deploy bằng `workflow_dispatch` qua SSH; `migrate` chạy one-shot trước `api`; runtime distroless non-root.

## Update 2026-11-05: staging lên Vercel + Supabase (EM)
EM muốn bắt đầu đơn giản: "dùng vercel và supabase cho đơn giản trước" ([raw](../../raw/2026-11-05-em-simplify-owner-and-opening-balance.md)). VPS, Caddy, R2 và domain riêng hoãn lại.
- **Một project Vercel** (`vien-phuc-y-duong`) dùng [Services](https://vercel.com/docs/services) (beta, mọi plan): service `web` là bản build Vite tĩnh, service `api` là `apps/api/Dockerfile` chạy bằng [Container Runtime](https://vercel.com/docs/functions/runtimes/container) (beta). `vercel.json` ở gốc repo rewrite `/api/*` sang `api`, phần còn lại sang `web`. Vẫn **một origin**, nên cookie `__Host-session`, CSRF origin check và CSP `'self'` giữ nguyên. Header bảo mật trước đây nằm trong Caddyfile nay chuyển vào `vercel.json`.
- **Region `pdx1`**, cạnh Supabase `us-west-2`.
- **TLS DB**: dùng `sslmode=verify-full` với Supabase Root 2021 CA, file công khai được bake vào image ở `/etc/ssl/certs/supabase-root-2021.crt`. Fingerprint SHA-256 `80:70:25:AD:…:CA:FA` khớp giữa file tải từ Supabase và chain mà pooler trả về.
- **Pool**: `APP_DB_MAX_CONNECTIONS=3` mỗi instance, vì Vercel chạy nhiều instance nhỏ trỏ vào cùng một pooler.
- **Job nền**: tick đầu tiên của job purge idempotency chạy ngay khi instance khởi động. Vercel scale về 0 sau 5 phút không có traffic, nên lần chạy lúc khởi động là đủ.
- **Đánh đổi chấp nhận**:
  1. Cold start vài giây sau khi nghỉ.
  2. Rate limit theo IP nằm trong bộ nhớ của từng instance. Khóa tài khoản sau 5 lần sai vẫn lưu ở DB nên vẫn giữ.
  3. Log request của Vercel có thể chứa query string (`?q=` tìm khách), tức là L5 chưa giải được trên Vercel. Ghi nhận, cần xem lại trước production.
  4. Gói Hobby chỉ cho dùng phi thương mại. Khi spa dùng thật thì phải lên Pro, hoặc chuyển API sang nơi khác.
  5. Gói Supabase Free không có backup tự động. Phải lên Pro trước khi nhập dữ liệu thật.
- Migration chạy từ máy dev hoặc CI bằng owner URL (`.env.staging`), không chạy trong container.
- Runbook: [vercel-staging](../runbooks/vercel-staging.md).

## Update 2026-11-07: đo latency → đề xuất ADR-0012
Đo staging ([performance-latency-2026-11](../architecture/performance-latency-2026-11.md)): từ VN mỗi request API tốn ~200–450 ms chỉ vì vùng Oregon, và lần gọi đầu sau ~45 s nghỉ chậm thêm 1,2–1,7 s (cold start container). Đánh đổi 1 ("cold start vài giây sau khi nghỉ") trên thực tế xảy ra gần như mỗi lượt khách. Đề xuất dời API + DB sang Singapore với API luôn chạy: [ADR-0012](ADR-0012-colocate-api-db-singapore.md) (proposed, chờ EM).
