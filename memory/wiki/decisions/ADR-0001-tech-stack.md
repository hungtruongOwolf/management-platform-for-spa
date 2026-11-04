---
title: ADR-0001 Tech stack
status: accepted (amended 2026-11-04 at /architect)
date: 2026-11-04
deciders: EM, agent
---

# ADR-0001: Tech stack

## Context
EM muốn backend nhanh, scale tốt và frontend làm được UI/UX đẹp, hiệu ứng, 3D. Bảo mật là bắt buộc. Discovery cho thấy **R1 là app nội bộ, bắt buộc đăng nhập, không cần SEO**. Trang giới thiệu công khai (cần SEO, hiệu ứng, 3D) thuộc giai đoạn sau. Xem [research/tech-stack](../research/tech-stack.md).

## Options considered
| Option | Pros | Cons |
|---|---|---|
| Rust/Axum + Next.js cho mọi thứ | Một frontend framework; SSR sẵn cho trang công khai | App nội bộ không cần SSR; thêm Node server; auth phức tạp hơn (BFF, chuyển tiếp cookie giữa server components và API) |
| **Rust/Axum + Vite React SPA (app nội bộ); site công khai riêng về sau** | App nội bộ là file tĩnh, cùng origin với API: auth bằng cookie đơn giản nhất, không CORS, không Node server; PWA dễ | Trang công khai sau này là một app thứ hai |
| Go + React | Backend viết nhanh hơn | Đảm bảo kiểu yếu hơn Rust |
| Toàn TypeScript | Một ngôn ngữ | Hiệu năng thấp hơn, nhiều lỗi runtime hơn |

## Decision
- **Backend:** Rust + Axum + SQLx + PostgreSQL. Redis **hoãn lại** cho tới khi thật sự cần.
- **App nội bộ (R1–R3):** TypeScript + React + **Vite** SPA, TanStack Router + TanStack Query, Tailwind + shadcn/ui, Motion, react-hook-form + zod, PWA (vite-plugin-pwa, không offline).
- **Site công khai (sau này):** Next.js hoặc Astro, chọn khi làm trang giới thiệu. Dùng chung design-system package và React ecosystem (GSAP, R3F cho 3D) nên không mất gì về hiệu ứng.
- API là trust boundary duy nhất.

## Consequences
- Positive: ít thành phần, auth an toàn và đơn giản, deploy là copy file tĩnh, chi phí thấp.
- Negative: hai frontend về sau (app + site). Đây là đánh đổi có chủ đích vì hai thứ có nhu cầu khác nhau.
- Follow-up: chọn framework cho site công khai khi làm trang giới thiệu.

## Revisit when
App nội bộ cần SSR, hoặc muốn gộp site công khai và app vào một domain/app.
