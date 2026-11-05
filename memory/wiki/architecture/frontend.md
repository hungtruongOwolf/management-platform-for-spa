---
title: Frontend architecture (internal app)
updated: 2026-11-05
sources: [raw/2026-11-05-em-ui-review-round3.md, decisions/ADR-0001-tech-stack.md, decisions/ADR-0004-authentication.md, design/design-system.md, design/front-desk-flows.md]
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
| ~~`/nhap-so-cu`~~ | **Đã xóa** (EM 2026-11-05): dữ liệu sổ giấy nhập qua CLI `app import-paper-book`; `POST /opening-balances` chỉ chủ spa | — | — |
| `/bao-cao` | Báo cáo theo kỳ (Hôm nay … 12 tháng, Tùy chọn), so với kỳ trước, xu hướng dạng line, chi tiết theo ngày/tuần/tháng, KTV, top dịch vụ/gói; tab **Sổ doanh thu** | `report.read` (tiền + sổ: `report.revenue.read`) | R1 |
| `/goi-lieu-trinh` | Theo dõi gói của khách: tìm (tên khách, tên gói, SĐT) + lọc trạng thái; gói không hạn hiện "Không hạn" | `report.read` | R1 |
| `/khach-lau-khong-quay-lai` | Khách liệu trình + khách lẻ lâu không quay lại: tìm, ngưỡng 7–90 ngày, loại khách, phạm vi, tình trạng liên hệ; "Đã liên hệ" ghi follow-up | `report.read` (ghi: `customer.follow_up`) | R1 |
| `/cai-dat` | Giao diện (theo thiết bị / sáng / tối) + tài khoản | session | R1 |
| `/bang-gia`, `/bang-gia/goi` | Dịch vụ, gói | `catalog.read` (sửa: `catalog.manage`) | R1 |
| `/so-quy` | Sổ quỹ, chốt quỹ | spec #2 | R1 |
| `/cai-dat/nhan-vien`, `/cai-dat/vai-tro` | Tài khoản, vai trò | spec #4 | R1 |
| `/toi` | Trang của KTV (lịch, số khách, phiếu lương) | self | R2/R3 |
URL tiếng Việt không dấu, ổn định để deep link (principle #10). Mọi route ngoài auth đều cần session.

## Màn quản lý (2026-11-05, cập nhật vòng 3)
Kiểu từ `packages/api-client` (contract r3b của `feat/followup-and-reports-api`). Mọi màn chỉ dùng query option trong `features/reports/api.ts` (follow-up: `features/customers/followUps.ts`); `features/reports/contract.ts` chỉ re-export.
- **Quyền UX:** `canSeeManagement(me)` = `report.read`. Trường tiền chỉ có khi có `report.revenue.read`; thiếu thì màn **không render** chỗ tiền: không thẻ "Thực thu", không chỉ số "Thực thu" trên biểu đồ, không cột tiền trong bảng chi tiết, không thẻ cơ cấu, không tab "Sổ doanh thu" (kể cả khi URL có `tab=ledger`). Test `Entitlement.test.tsx`. Nút "Đã liên hệ" / "Ghi lần liên hệ" chỉ hiện khi có `customer.follow_up`.
- **Báo cáo `/bao-cao`** (EM vòng 3 mục 2):
  - Kỳ: Hôm nay · 7 ngày · 30 ngày · Tháng này · Tháng trước · Năm nay · 12 tháng · Tùy chọn (từ–đến, tối đa 2 năm = 732 ngày, giới hạn của API). `features/reports/period.ts` (thuần, có unit test) tính `from/to`, bucket (≤ 31 ngày: ngày; ≤ 92: tuần; ≤ 2 năm: tháng; hơn: năm) và **kỳ trước cùng độ dài** (Tháng này ↔ cùng kỳ tháng trước; Năm nay ↔ cùng kỳ năm trước; còn lại: dịch lùi đúng số ngày). Client tự tính `from/to` thay vì gửi `preset`, để kỳ so sánh luôn khớp.
  - Hàng KPI (thực thu, khách, lượt, buổi, gói bán) có `Delta` so với kỳ trước (2 request `/reports/range` song song).
  - Biểu đồ xu hướng: **line chart SVG tự viết** (`LineChart.tsx`, không thư viện): một trục; kỳ này là đường nâu đỏ + vùng 10%, kỳ trước đường xám (dạng *emphasis* của skill dataviz; validator: tách CVD ΔE 23, tương phản ≥ 3:1 ở cả light/dark; check band/chroma chỉ áp dụng cho bảng màu categorical). Đổi chỉ số Thực thu / Khách / Lượt bằng segmented (không dual-axis). Crosshair + tooltip khi rê chuột và khi focus (← → Home End); bảng "Chi tiết theo ngày/tuần/tháng" chính là bảng thay thế cho screen reader.
  - Đổi kỳ giữ khung cũ mờ đi (`placeholderData: keepPreviousData` + `.is-refreshing`), không nháy skeleton.
  - Tab **Sổ doanh thu**: `GET /reports/ledger` (100 dòng/trang, "Xem thêm"), cột như sổ giấy, mỗi ngày một dòng cộng từ `days[]`, dòng hủy/hoàn đỏ + badge, dòng gốc đã hủy gạch ngang. Kỳ > 93 ngày: không gọi API, hướng dẫn chọn kỳ ngắn hơn.
- **URL state** (`lib/searchState.ts` của shell; schema ở `features/reports/search.ts`, `features/customers/search.ts`): mặc định không lên URL; filter `replace` (không đầy lịch sử Back).

| Màn | Cần tìm / lọc? | Có gì (URL key) |
|---|---|---|
| `/bao-cao` | kỳ + tab + chỉ số | `tab`, `period`, `from`, `to` (chỉ khi `custom`), `metric` |
| `/goi-lieu-trinh` | **có**: nhiều gói, quản lý tìm theo khách hoặc tên gói khi khách gọi tới | ô tìm `q` (tên khách, tên gói, SĐT; debounce 300 ms, ≥ 2 ký tự), `status` |
| `/khach-lau-khong-quay-lai` | **có**: danh sách gọi điện hằng ngày | `q`, `days` 7/14/30/60/90, `kind` (mọi khách / liệu trình / khách lẻ), `scope` (mọi khách / chỉ còn buổi), `follow` (Cần liên hệ (mặc định) / Chưa liên hệ / Đã hẹn / Tất cả). Server sắp theo ngày hẹn khi lọc "Cần liên hệ"/"Đã hẹn", còn lại theo số ngày vắng |
| Hồ sơ khách (lịch sử) | **có** khi khách nhiều năm (EM mục 6) | `hq` (dịch vụ, danh mục, gói, KTV), `hkind` (loại sự kiện), `hfrom`/`hto`; "Xóa bộ lọc"; timeline phân trang "Xem thêm lịch sử" |
| `/khach-hang` | có sẵn (shell): `q` | — |
| `/bang-gia`, `/bang-gia/goi` | **chưa cần**: 35 dịch vụ / 10 danh mục và ~5 gói vừa một màn, đã nhóm theo danh mục; quầy chọn dịch vụ qua `<select>` nhóm theo danh mục. Thêm ô lọc khi menu > ~80 dịch vụ | — |
| `/quay` | tìm khách đã có; danh sách lượt hôm nay phân trang 30 | — |

- **Follow-up** (EM mục 16): `FollowUpForm` (state thường, không RHF): ngày liên hệ (mặc định hôm nay, lùi tối đa 30 ngày), cách liên hệ, kết quả, ghi chú ≤ 500, **hẹn lại** điền sẵn gợi ý (Không nghe máy +3 ngày như server; Hẹn sẽ quay lại / Khác +7; Chưa có nhu cầu: không hẹn), sửa hoặc "Không hẹn" được. Mở bằng `<dialog>` trên danh sách vắng, inline trên hồ sơ (thẻ "Chăm sóc lại" có timeline các lần liên hệ). `POST` có `Idempotency-Key` sinh lúc mở form.
- **SĐT trùng khi gõ** (EM mục 13): `CustomerForm` gọi `GET /customers/phone-check?phone=` sau 400 ms ngừng gõ, chỉ khi số đủ 10 chữ số; hiện "Số này đã có hồ sơ: <tên> — Mở hồ sơ" (vùng `aria-live`) trước khi bấm Lưu; khi sửa hồ sơ không báo trùng với chính nó. 409 `customer.phone_taken` lúc lưu vẫn là chốt chặn cuối (giữ nguyên).
- **Giới tính cạnh tên** (mục 7): `app/CustomerName` ở bảng gói, danh sách vắng, sổ doanh thu, thẻ khách ở quầy (có "Chưa rõ"), lượt hôm nay, tiêu đề hồ sơ.
- **Danh mục trước tên dịch vụ** (mục 15): `lib/names.ts` `serviceLabel(name, category_name)` → "Gội đầu dưỡng sinh · Gội đầu thảo dược": giỏ ở quầy, lượt hôm nay, lịch sử, mục trong gói (thẻ quầy + hồ sơ), top dịch vụ. Picker ở quầy đã nhóm theo danh mục (`<optgroup>`).
- **Gói không hạn** (mục 3): `expires_on = null` → "Không hạn" mọi nơi (`expiryText`, `NO_EXPIRY`); server: `package_status` không bao giờ trả `expired` khi hạn null, và lọc "Sắp hết hạn" loại hạn null. Có test fixture gói không hạn.
- **Query keys:** `['reports','range',from,to,granularity]` (60 s, giữ dữ liệu cũ khi đổi kỳ), `['reports','ledger',from,to]` (infinite), `['packages','overview',status,q]` (infinite), `['customers','inactive',days,scope,kind,follow,q]` (infinite), `['customers','history',id,q,kind,from,to]` (infinite), `['customers','summary',id]`, `['customers','follow-ups',id]`, `['customers','phone-check',digits]` (30 s).
- **Invalidation:**
  - follow-up → `['customers','inactive']` + follow-ups + summary của khách đó;
  - ghi / hủy lượt, bán gói, hủy / gia hạn gói → `lib/invalidate.ts` `invalidateManagementViews`: `['reports']`, `['packages','overview']`, `['customers','inactive']`, history + summary của khách. Chỉ đánh dấu stale; trên `/quay` không có query nào trong số này đang mount nên không tốn request ở lần vẽ đầu.
- **Test:** `fixtures.ts` + `testServer.ts` giả API đúng contract, gồm follow-up GET/POST giữ trong bộ nhớ, lọc và sắp xếp như server, phone-check. E2E `e2e/specs/management.spec.ts` dùng `support/db.ts` `backdateCustomer` để có khách vắng 20 ngày.

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
- Ngân sách JS critical path: ≤ 125 KB gzip. **Một chunk khởi động** (2026-11-05): `vite.config.ts` `build.rolldownOptions.output.codeSplitting.groups = [{ name: "app", tags: ["$initial"] }]` gom mọi module của màn đầu vào `index-*.js`. Không có nó, khi thêm màn lazy dùng chung module, rolldown tách ~10 chunk nhỏ được preload, mỗi chunk gzip riêng nên tốn thêm ~2 KB. Số đo (gzip -9 tổng entry + modulepreload): trước UI overhaul 121.6 KB (3 file), sau **120.1 KB (1 file)** dù thêm sidebar/top bar/tab bar/drawer/theme. Kiểm bằng `vite build` rồi cộng entry + mọi `modulepreload` trong `dist/index.html`.
- **Không đưa vào bundle chính:** trang chủ (lazy từ 2026-11-05), command palette (`React.lazy`, chỉ khi mở), mọi màn quản lý + chart (hand-rolled HTML, ~3 KB trong chunk `ReportsPage`). Icon: 1 sprite SVG file (`assets/icons.svg?no-inline`, `<use href>`), không thư viện icon. Logo: CSS mask từ file SVG, không inline vào JS. Chuỗi class dài viết thành class CSS (`components.css`) vì chuỗi trong JSX nằm trong JS.
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
