---
title: Tech stack research — backend & frontend
updated: 2026-11-04
sources: [raw/2026-11-04-kickoff-request.md]
---

# Nghiên cứu tech stack

> **Đã chốt ở [ADR-0001](../decisions/ADR-0001-tech-stack.md)** (2026-11-04): app nội bộ dùng **Vite React SPA** thay cho Next.js, vì R1 không cần SEO. Next.js/Astro dành cho site công khai sau này. Trang này giữ lại làm tư liệu.

Tiêu chí của EM: **backend nhanh, scale tốt**; **frontend làm UI/UX đẹp, nhiều hiệu ứng, 3D, prototype nhanh**. Kèm theo đó là các tiêu chí ngầm: bảo mật, agent (AI) code hiệu quả, chi phí vận hành, tuyển người sau này.

> Ghi chú: version/giá cụ thể của thư viện và dịch vụ cần kiểm tra lại khi bắt đầu code `(chưa xác minh tại 2026-11)`.

## 1. Backend

| Lựa chọn | Hiệu năng | Tốc độ phát triển | An toàn | Hệ sinh thái web | Nhận xét |
|---|---|---|---|---|---|
| **Rust + Axum** | ★★★★★ | ★★★ | ★★★★★ | ★★★★ | Nhanh nhất nhóm, RAM thấp, latency ổn định (không GC). Compiler bắt lỗi rất sớm |
| Go (chi/Echo) | ★★★★ | ★★★★★ | ★★★★ | ★★★★ | Đơn giản, compile nhanh, rất hợp backend; type system yếu hơn Rust |
| TypeScript (Bun/Node + Hono/NestJS) | ★★★ | ★★★★★ | ★★★ | ★★★★★ | Dùng chung ngôn ngữ với frontend; chậm hơn và runtime errors nhiều hơn |
| Elixir (Phoenix) | ★★★★ | ★★★★ | ★★★★ | ★★★ | Realtime tuyệt vời; cộng đồng nhỏ |

**Sự thật cần biết về Rust:**
- Với hầu hết app web, nút thắt nằm ở **database, network và kiến trúc** chứ không ở ngôn ngữ. Ưu điểm thực tế của Rust là chi phí server thấp (1 instance nhỏ chịu tải lớn), p99 latency ổn định, và **cả một lớp bug bị loại bỏ ngay lúc compile** (null, data race, memory safety). Điều này có giá trị lớn với yêu cầu bảo mật.
- Nhược điểm: học khó, compile chậm hơn, viết CRUD chậm hơn Go/TS. Khi agent viết code thì nhược điểm này giảm đáng kể vì compiler đóng vai một reviewer cực kỳ nghiêm khắc.
- **Kết luận: Rust là lựa chọn hợp lý.** Đồng ý với EM.

**Stack Rust đề xuất:**
- Web framework: **Axum** (do team Tokio làm, middleware `tower` dùng chung cho rate limit, timeout, tracing, CORS). Actix-web cũng nhanh, nhưng hệ sinh thái tower giúp ghép các lớp bảo mật gọn gàng hơn.
- Database: **PostgreSQL** + **SQLx** (SQL được kiểm tra ngay lúc compile, không cần ORM nặng). Cache/session/rate-limit dùng **Redis** (hoặc Valkey).
- Auth: `tower-sessions` + Argon2id cho session cookie; hoặc dùng OIDC provider (Zitadel/Keycloak tự host, Clerk/Auth0 managed). Cần quyết định trong `/architect`.
- Validation `serde` + `validator`; OpenAPI `utoipa`; background jobs `apalis`; observability `tracing` + OpenTelemetry.
- Media/video: object storage (S3 / Cloudflare R2) + pipeline transcode/DRM (tự làm bằng ffmpeg + Shaka Packager, hoặc dịch vụ như Mux / Cloudflare Stream / Bunny Stream / VdoCipher). Xem `.claude/skills/secure/content-protection.md`.

## 2. Frontend

Hiệu năng render của các framework hiện đại **chênh nhau rất ít** so với tác động của ảnh, font, bundle size và 3D. Thứ quyết định khả năng "làm UI đẹp, hiệu ứng, 3D" là **hệ sinh thái thư viện**.

| Lựa chọn | Hiệu ứng / 3D | SEO / SSR | Hiệu năng | Hệ sinh thái & agent | Nhận xét |
|---|---|---|---|---|---|
| **Next.js (React)** | ★★★★★ | ★★★★★ | ★★★★ | ★★★★★ | R3F, drei, Motion, GSAP, shadcn đều React-first. Agent viết React tốt nhất |
| React + Vite (SPA) | ★★★★★ | ★★ | ★★★★ | ★★★★★ | Hợp app sau đăng nhập; SEO kém |
| SvelteKit | ★★★★ (Threlte) | ★★★★★ | ★★★★★ | ★★★ | Nhẹ, nhanh, code gọn; thư viện 3D/UI ít hơn |
| Astro | ★★★ (islands) | ★★★★★ | ★★★★★ | ★★★★ | Rất tốt cho site nội dung; app tương tác nhiều thì kém hơn |
| Nuxt (Vue) | ★★★★ (TresJS) | ★★★★★ | ★★★★ | ★★★★ | Ổn, nhưng hệ sinh thái 3D/motion nhỏ hơn React |
| Leptos / Dioxus (Rust WASM) | ★★ | ★★★ | ★★★★ | ★★ | Hấp dẫn vì "full Rust" nhưng thư viện UI/3D còn non → **không khuyến nghị** cho mục tiêu UI/UX |

**Đề xuất: TypeScript + Next.js (App Router) + React**, kèm bộ công cụ sáng tạo:
- **Styling / design system:** Tailwind CSS v4 + shadcn/ui (dựng trên Radix: accessible, tự sở hữu code, chỉnh tùy ý).
- **Animation UI:** **Motion** (trước đây là Framer Motion) cho layout/gesture/transition; **GSAP** (miễn phí toàn bộ, kể cả plugin, từ 2025) với ScrollTrigger/SplitText cho scroll storytelling; **Lenis** cho smooth scroll.
- **3D:** **Three.js** qua **React Three Fiber** + **drei** (helpers) + **postprocessing**; `@react-three/rapier` cho physics; Theatre.js để biên đạo cảnh 3D theo timeline; **Spline** để designer dựng 3D nhanh rồi nhúng vào. Three.js đã có WebGPU renderer + TSL shaders cho hiệu ứng nặng.
- **Data/state:** Server Components + TanStack Query; react-hook-form + zod.
- **Prototype "insight":** Figma (thiết kế), v0 / Claude Artifacts / skill `prototype` (prototype code chạy được để test cảm giác), Storybook (catalog component).
- **Test:** Vitest, Testing Library, Playwright.

## 3. Kiến trúc kết hợp (phác thảo, sẽ chốt ở `/architect`)
```
Browser ──HTTPS──> Next.js (SSR, UI, SEO) ──> Rust/Axum API ──> PostgreSQL / Redis / Object storage
        └──────────── media qua CDN (signed URL + DRM licence do API cấp) ────────────┘
```
- Rust API là **trust boundary duy nhất**: mọi authN/authZ đều nằm ở đây. Next.js chỉ chuyển tiếp session cookie và không giữ quyền quyết định.
- Cách khác nếu muốn giảm số dịch vụ: Vite SPA tĩnh được Rust serve trực tiếp. Đổi lại, SEO kém đi; chỉ hợp khi sản phẩm chủ yếu là app sau đăng nhập.

## Câu hỏi để chốt
1. Có cần SEO mạnh không (site public, blog, trang dịch vụ)? → nếu có thì Next.js; nếu chỉ là app nội bộ/sau đăng nhập thì Vite SPA cũng được.
2. Có mobile app native không? → nếu có, cân nhắc React Native/Expo để tái dùng kiến thức React.
3. Video là tài sản kinh doanh chính? → quyết định mức DRM và nhà cung cấp.
