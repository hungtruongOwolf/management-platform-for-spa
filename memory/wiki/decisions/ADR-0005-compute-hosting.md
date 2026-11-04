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
