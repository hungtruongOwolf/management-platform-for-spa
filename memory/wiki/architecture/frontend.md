---
title: Frontend architecture (internal app)
updated: 2026-11-05
sources: [decisions/ADR-0001-tech-stack.md, decisions/ADR-0004-authentication.md, design/design-system.md, design/front-desk-flows.md]
---

# Kiến trúc frontend: app nội bộ

Stack ([ADR-0001](../decisions/ADR-0001-tech-stack.md)): TypeScript strict · React · Vite (SPA) · TanStack Router · TanStack Query · Tailwind (tokens từ [design-system](../design/design-system.md)) · shadcn/ui (Radix) · react-hook-form + zod · Motion · vite-plugin-pwa · Vitest + Testing Library + MSW · Playwright.

## Route map
| Route | Màn hình | Permission (UX gate) | Đợt |
|---|---|---|---|
| `/dang-nhap`, `/kich-hoat/:token`, `/dat-lai-mat-khau/:token` | Auth | public | R1 |
| `/quay` | Màn hình quầy (mặc định của Quản lý) | `visit.record` | R1 |
| `/khach-hang`, `/khach-hang/:id` | Danh sách / hồ sơ khách | `customer.read` | R1 |
| ~~`/cong-no`~~ | Công nợ: **hoãn** (EM 2026-11-04, R1 thu đủ) | — | sau |
| `/` | Tổng quan: chào, "Hôm nay", lối tắt (lazy) | session | R1 |
| `/nhap-so-cu` | Nhập liệu trình dở dang. **Không có trong nav** (EM 2026-11-05); chỉ chủ spa mở bằng URL tới khi import chuyển sang CLI vận hành, rồi xóa | `opening_balance.import` + `package.cancel_refund` | R1 |
| `/bao-cao` | Báo cáo / Tổng quan hôm nay: thẻ số, 7 ngày, cơ cấu tiền, KTV, dịch vụ dùng nhiều | `report.read` (tiền: `report.revenue.read`) | R1 |
| `/goi-lieu-trinh` | Theo dõi gói của khách (lọc sắp hết hạn / đang dùng / hết hạn / hết buổi / đã hủy) | `report.read` | R1 |
| `/nhac-khach` | Khách lâu chưa quay lại (còn buổi) | `report.read` | R1 |
| `/cai-dat` | Giao diện (theo thiết bị / sáng / tối) + tài khoản | session | R1 |
| `/bang-gia`, `/bang-gia/goi` | Dịch vụ, gói | `catalog.read` (sửa: `catalog.manage`) | R1 |
| `/so-quy` | Sổ quỹ, chốt quỹ | spec #2 | R1 |
| `/cai-dat/nhan-vien`, `/cai-dat/vai-tro` | Tài khoản, vai trò | spec #4 | R1 |
| `/toi` | Trang của KTV (lịch, số khách, phiếu lương) | self | R2/R3 |
URL tiếng Việt không dấu, ổn định để deep link (principle #10). Mọi route ngoài auth đều cần session.

## Màn quản lý và cờ nguồn dữ liệu (2026-11-05)
API của các màn quản lý (`GET /reports/daily`, `/reports/range`, `/customers/{id}/history`, `/customers/{id}/summary`, `/packages`, `/customers/inactive`) do branch `feat/management-views` làm. Frontend dựng trước khung và dữ liệu mẫu:
- `app/features.ts` `MANAGEMENT_DATA` = `VITE_MANAGEMENT_DATA` (`api` | `mock` | `off`); mặc định `mock` khi dev/test, `off` khi build production (không bao giờ hiện số bịa cho spa thật). Preview staging: build với `VITE_MANAGEMENT_DATA=mock` (banner "Dữ liệu mẫu" luôn hiện).
- `features/reports/api.ts`: mọi màn chỉ dùng các query option ở đây; `fetchOr` gọi API thật hoặc `mock.ts` (chunk riêng, chỉ tải khi `mock`). Đổi sang API = đổi cờ.
- `features/reports/contract.ts`: **tạm** chép kiểu DTO từ `apps/api/crates/reporting/src/dto.rs` của branch kia. TODO khi branch đó merge: `pnpm api:gen`, thay file này bằng re-export từ `@vpyd/api-client`, đặt `VITE_MANAGEMENT_DATA=api` làm mặc định, xóa `mock.ts` + `MockBanner`.
- Quyền UX: `canSeeManagement` = `report.read` (api) hoặc `sale.read` (mock, vì `/me` hôm nay chưa có `report.read`). Trường tiền là optional: không có trong response thì màn **không render** (không hiện 0đ), test `Entitlement.test.tsx`.
- Query keys: `['reports','daily',date]` (30 s), `['reports','range',from,to]` (60 s), `['customers','history',id]`, `['customers','summary',id]`, `['packages','overview',filter]`, `['customers','inactive',days]`. Ghi lượt/bán gói nên invalidate `['reports']` + `['customers','history',id]` khi nối API thật.

## Rendering & data
- **SPA tĩnh**, Caddy trả `index.html` cho mọi route không phải `/api/*`. Code-split theo route (`lazy`).
- **API client** `packages/api-client` sinh từ OpenAPI (typed fetch). Frontend **không tự định nghĩa** kiểu DTO; backend đổi contract → `pnpm typecheck` fail (principle #8).
- **TanStack Query** là nguồn sự thật cho server state. Query keys theo tenant ngầm (session) + resource:
  - `['catalog']` (staleTime 10 phút) · `['staff','active']` (10 phút) · `['customers','search',q]` (30 giây) · `['customer',id]` · `['visits','today']` · `['receivables']`.
- **Invalidation sau mutation:**
  - ghi lượt → `customer(id)`, `visits.today`, `receivables`;
  - bán gói → `customer(id)`, `receivables`;
  - thu nợ → `customer(id)`, `receivables`;
  - hủy → các key liên quan;
  - `409` → refetch `customer(id)`.
- **Không optimistic update cho tiền và buổi**: chờ server xác nhận rồi mới hiển thị kết quả cuối, vì số tiền phải đúng tuyệt đối. Chỉ hiển thị "trừ tạm" trong giỏ chưa lưu.
- **Idempotency:** mỗi form ghi sinh `Idempotency-Key` (UUID) khi mở; giữ nguyên khi retry; đổi khi lưu thành công.
- Client state: chỉ state UI cục bộ (giỏ lượt đang ghi trong component; backup vào `sessionStorage` để sống qua 401/reload). Không dùng global store.

## Forms
react-hook-form + zod schema. Schema validation **phản chiếu** lỗi server (server vẫn là nơi quyết định). Lỗi `422` problem+json map vào field theo `errors[].pointer`. Tiền nhập là số nguyên VND; SĐT chuẩn hóa trước khi gửi. Ngoại lệ duy nhất: form đăng nhập (2 field) dùng state React thường, xem "Perf budget".

## Perf budget (critical path)
Lighthouse CI chặn merge khi LCP > 2,5 s (mobile, Slow 4G, CPU 4×, `/dang-nhap` và `/quay`). Lighthouse dùng mô phỏng (lantern): **mọi request kết thúc trước lần paint LCP quan sát được đều bị tính**, kể cả font và API. Vì vậy các quy tắc sau áp dụng cho mọi PR:
- **Không zod, không react-hook-form trên critical path.** Bundle chính (`index-*.js` + chunk preload) chỉ chứa React, TanStack Router/Query, shell, `/dang-nhap` và `/quay`. Parse response trên đường khởi động (`/me`, login, MFA) bằng guard viết tay trong `features/auth/session.ts` (`parseMe`), vì kiểu đã có từ contract. Form dùng RHF + zod thì phải nằm trong chunk lazy: route lazy (`lazyRouteComponent`) hoặc `React.lazy` cho panel/dialog chỉ mở khi cần (ví dụ `CustomerForm` trong `/quay`). Component nhỏ mà màn chính cần thì tách module riêng (ví dụ `HealthNote`), đừng import từ file của màn lazy.
- **`/xac-thuc` là route lazy** (chỉ chủ spa đi qua; kéo theo `uqr`).
- **Font:** chỉ subset `latin` + `vietnamese`, chỉ woff2, `font-display: swap`, khai báo tay ở `src/fonts.css` (CSS của @fontsource có `latin-ext`, mà unicode-range của nó trùng chữ Việt nên trình duyệt tải thêm 4 file). Lần tải đầu (chưa có cache) font được tải **sau màn hình đầu tiên** (`app/fonts.ts`, 500 ms sau `onRendered`), text hiện bằng font hệ thống trước. Sau khi tải xong, cờ `localStorage` `vpyd.fonts-cached` cho các lần sau tải font ngay lúc khởi động (từ cache, không nháy font). Không preload font; không thêm weight mới nếu chưa đo lại LCP.
- **Font fallback khớp số đo (S10):** `packages/ui/src/tokens/tokens.css` khai báo `"Be Vietnam Pro Fallback"` (Arial / Liberation Sans, 400 và 600) và `"Lora Fallback"` (Georgia Bold) với `size-adjust` / `ascent-override` / `descent-override` đo trong Chromium, đứng ngay sau web font trong `--font-ui` / `--font-brand`. Lý do: lần tải đầu, lúc web font swap vào, dòng chữ xuống hàng khác đi và đẩy cả trang; với danh sách lượt dài (ngày đông khách, 100 lượt) CLS `/quay` lên 0,156 (> 0,1). Thêm weight hoặc đổi font thì đo lại (Chromium, canvas `measureText` trên một đoạn chữ Việt của giao diện: `size-adjust` = độ rộng web font ÷ độ rộng font local; ascent/descent của web font ÷ `size-adjust`) rồi sửa số.
- **`/api/v1/me` được preload** trong `index.html` (`<link rel="preload" as="fetch" crossorigin="anonymous">`), chạy song song với JS thay vì sau JS. `api()` phải giữ request GET `/me` khớp với preload (không thêm header, `credentials: same-origin`), nếu không trình duyệt sẽ gọi hai lần.
- Ngân sách JS critical path: ≤ 125 KB gzip. **Một chunk khởi động** (2026-11-05): `vite.config.ts` `build.rolldownOptions.output.codeSplitting.groups = [{ name: "app", tags: ["$initial"] }]` gom mọi module của màn đầu vào `index-*.js`. Không có nó, khi thêm màn lazy dùng chung module, rolldown tách ~10 chunk nhỏ được preload, mỗi chunk gzip riêng nên tốn thêm ~2 KB. Số đo (gzip -9 tổng entry + modulepreload): trước UI overhaul 121.6 KB (3 file), sau **120.3 KB (1 file)** dù thêm sidebar/top bar/tab bar/drawer/theme. Kiểm bằng `vite build` rồi cộng entry + mọi `modulepreload` trong `dist/index.html`.
- **Không đưa vào bundle chính:** trang chủ (lazy từ 2026-11-05), command palette (`React.lazy`, chỉ khi mở), mọi màn quản lý + chart (hand-rolled HTML, ~3 KB trong chunk `ReportsPage`), `mock.ts`. Icon: 1 sprite SVG file (`assets/icons.svg?no-inline`, `<use href>`), không thư viện icon. Logo: CSS mask từ file SVG, không inline vào JS. Chuỗi class dài viết thành class CSS (`components.css`) vì chuỗi trong JSX nằm trong JS.
- Emblem lớn (11.8 KB gzip) chỉ ở `/dang-nhap` và `/xac-thuc`, dạng CSS mask (không phải ứng viên LCP); shell và `/quay` chỉ dùng mark (2.5 KB) + wordmark (3.8 KB).
- Đo trước/sau bằng `pnpm lhci` trên cùng một máy; local chậm hơn CI khoảng 0,3–0,4 s. Kết quả 2026-11-04: [log](../../log.md).

## Auth & permission-aware UI
- Session là cookie `__Host-session` HttpOnly ([ADR-0004](../decisions/ADR-0004-authentication.md)). **JS không bao giờ đọc hay lưu token.** Fetch dùng `credentials: 'same-origin'`.
- `GET /api/v1/me` → user, tenant, branch, **danh sách permission** → `AuthContext`. Route guard và ẩn/hiện nút đọc từ đây. **Đây chỉ là UX**: server luôn kiểm tra lại; giao diện xử lý `403` đúng cách.
- Field nhạy cảm (`health_note`, `visit.note`) render **theo việc API có trả về hay không**, không dựa vào role ở client. Không có field thì không render gì.
- `401` từ bất kỳ request nào → xóa cache Query → `/dang-nhap?next=<url>`. Nút **Khóa nhanh** gọi logout.
- Thao tác nghiệp vụ (hủy/sửa ngày đã chốt, hủy gói + hoàn tiền, nhập sổ cũ) **không** hỏi lại mật khẩu (EM 2026-11-05): form dùng `useFormAction` (một mutation, hiện lỗi server). Hộp thoại nhập lại mật khẩu (`POST /auth/reauth`) chỉ dành cho thao tác bảo mật tài khoản (đổi mật khẩu, cài đặt MFA), dựng lại khi có các màn đó. `/nhap-so-cu` không có công tắc mở nhập.

## i18n
Chỉ tiếng Việt ở R1, nhưng mọi chuỗi đi qua `t('key')` (ví dụ `i18next`, resource `vi.json`) để thêm ngôn ngữ sau mà không sửa component. Định dạng tiền/ngày qua helper `formatVnd`, `formatDate` trong `packages/ui`.

## PWA
Icon đã có (`public/favicon.svg`, `favicon-32.png`, `apple-touch-icon.png`, `icons/icon-192|512|maskable-512.png`, sinh từ logo thật bằng `scripts/brand/build_logo_assets.py`) và `public/manifest.webmanifest`. Sau này `vite-plugin-pwa`: manifest (tên, icon từ logo, `display: standalone`, theme color = `primary`); service worker **chỉ precache app shell và font**, `/api/*` luôn `NetworkOnly` (không offline, theo EM). Có prompt "Có phiên bản mới, tải lại".

## Error handling
- Error boundary theo route: ErrorState + "Tải lại"; báo lỗi (Sentry, đã lọc PII) ở `/devops`.
- Lỗi mạng hoặc `5xx` → giữ form, banner thử lại. `503` → banner "Hệ thống tạm gián đoạn, ghi phiếu tạm" (failure mode trong [system](system.md)).

## Accessibility (WCAG 2.2 AA)
Ưu tiên HTML ngữ nghĩa; mọi chức năng dùng được bằng bàn phím (phím tắt quầy ở [front-desk-flows](../design/front-desk-flows.md)); focus-visible 3px; vùng chạm ≥ 44px; `aria-live` cho toast và kết quả tìm; dialog có focus trap; tương phản theo token; tôn trọng `prefers-reduced-motion`. Kiểm tra tự động bằng `@axe-core/playwright` trong e2e.

## Bảo mật frontend
CSP nonce-based do Caddy/API cấp (không `unsafe-inline` script); không `dangerouslySetInnerHTML`; không secret trong `VITE_*`; dependency audit trong CI.

## Cấu trúc thư mục
```
apps/web/
  src/
    app/            # router, providers (Query, Auth, Theme), AppShell, error boundary
    routes/         # file-based routes (TanStack Router): quay/, khach-hang/, cong-no/, nhap-so-cu/, bang-gia/, ...
    features/       # theo bounded context, khớp với crate backend
      customers/  packages/  visits/  sales/  catalog/  auth/
        components/  hooks/ (useCustomer, useRecordVisit...)  schemas/ (zod)  index.ts
    lib/            # api (wrap api-client: credentials, problem+json, idempotency), auth, i18n, format
    locales/vi.json
  e2e/              # Playwright: luồng quầy, bán gói, hủy, axe
packages/ui/
  src/tokens/       # tokens.css (light/dark), tailwind preset
  src/components/   # Button, Input, MoneyInput, PhoneInput, SearchBox, HealthNote, PackageRow, CartLine, Dialog, Toast...
  src/format/       # formatVnd, formatDate, formatPhone
packages/api-client/ # sinh từ apps/api OpenAPI; không sửa tay
```
> **Liên kết package (2026-11-04):** repo nằm trên ổ exFAT (không có symlink), nên `apps/web` dùng `@vpyd/ui` (và sau này `api-client`) qua **path alias** trong `vite.config.ts` + `tsconfig.json`, không khai báo dependency workspace.
```
```
Quy tắc: `features/*` chỉ import từ `packages/ui`, `lib`, `api-client` và `index.ts` của feature khác (không import sâu). ESLint `import/no-restricted-paths` ép quy tắc này.
