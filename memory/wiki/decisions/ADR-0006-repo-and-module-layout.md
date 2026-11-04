---
title: ADR-0006 Repository & module layout
status: accepted
date: 2026-11-04
deciders: EM, agent
---

# ADR-0006: Cấu trúc repo & module

## Context
EM yêu cầu code dễ mở rộng, dễ sửa, và agent không được sửa lan man. Ranh giới module cần được **compiler ép**, không chỉ dựa vào quy ước.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| Một crate Rust, chia module bằng thư mục | Đơn giản | Mọi thứ đều nhìn thấy nhau; ranh giới dễ bị phá |
| **Cargo workspace: mỗi bounded context một crate** | Compiler ép ranh giới (chỉ thấy `pub` API); compile song song; agent nhìn phạm vi rõ | Nhiều `Cargo.toml` hơn |
| Microservices | Tách deploy | Quá sớm; đắt; phức tạp |

## Decision
Monorepo:
```
apps/
  api/                    # Cargo workspace
    crates/
      kernel/             # TenantId, BranchId, Money, BusinessDate, AppError, can(), audit, idempotency
      identity/  org/  catalog/  crm/  packages/  operations/  sales/  ledger/  notifications/
      app/                # binary: config, router, wiring, jobs
    migrations/
  web/                    # Vite React SPA (app nội bộ)
packages/
  api-client/             # TS client sinh từ OpenAPI
  ui/                     # design system (tokens + components), dùng chung với site công khai sau này
infra/                    # docker-compose, Caddyfile, backup scripts
memory/  .claude/  CLAUDE.md
```
- Mỗi crate có `domain` (kiểu dữ liệu, bất biến), `service` (use case + `can()`), `repo` (SQL, luôn nhận `TenantId`), `http` (route, DTO). Crate chỉ phụ thuộc `kernel` và **public service API** của crate khác, không bao giờ dùng repo của crate khác.
- Quan hệ phụ thuộc giữa các crate là đồ thị không vòng; `cargo deny`/CI kiểm tra.
- OpenAPI sinh từ code (`utoipa`), commit vào repo; client TS sinh lại trong CI và frontend typecheck theo nó.

## Consequences
- Positive: ranh giới rõ, impact analysis dễ (crate nào phụ thuộc crate nào là thấy ngay), thay đổi cục bộ.
- Negative: chút boilerplate khi thêm context mới.

## Revisit when
Một context cần scale hoặc deploy riêng → tách crate đó thành service.
