---
title: Design system — internal app (R1)
updated: 2026-11-05
sources: ["docs for reference/logo Viên phúc y đường transparent.png", "docs for reference/Menu mặt trước - mới.png", product/personas.md, raw/2026-11-05-em-brand-colors-and-demo-data.md, raw/2026-11-05-em-management-views-logo-ui.md]
---

# Design system: app nội bộ

## Hướng thiết kế: "Trà thất Đông y" (2026-11-05, thay "Trà thất yên tĩnh")
EM thấy UI cũ "hơi xấu" và giao việc phối 4 màu đặc trưng của spa: **nâu đỏ, vàng, trắng, kem**, sao cho "sinh động và tinh tế" ([raw](../../raw/2026-11-05-em-brand-colors-and-demo-data.md)). Hướng mới lấy cảm hứng trực tiếp từ menu in: **khung app (header, dải đầu trang đăng nhập, khung chào ở trang chủ) dùng nền nâu đỏ đậm + chữ kem + vàng kim**, còn **vùng làm việc** giữ nền kem + card trắng để đọc số tiền cả ngày không mỏi mắt.
- **Quy tắc 70 / 20 / 10:** kem + trắng ~70% (nền, card), nâu đỏ ~20% (header, nút chính, tiêu đề), vàng ~10% (viền focus, gạch chân mục nav đang chọn, thanh số buổi, ★, logo, đường kẻ). Vàng **không** dùng làm nền lớn, cũng không dùng cho chữ nhỏ trên nền sáng (chỉ dùng `accent-text`).
- Tính từ định hướng: ấm áp, sang, điềm tĩnh, rõ ràng, nhanh. "Sinh động" đến từ khung nâu đỏ có gradient nhẹ, ánh sơn mài trên nút chính, viền vàng và micro-motion, **không** từ nhiều màu.
- Lý do giữ vùng làm việc sáng: quản lý dùng app 8–10 giờ/ngày; nền đỏ đô toàn trang như menu in quá nặng (giữ cho site công khai sau này).

## Màu (semantic tokens)
Mọi màu đều qua token (`packages/ui/src/tokens/tokens.css`), component không dùng hex trực tiếp. **`packages/ui/src/tokens/contrast.test.ts` đọc chính file tokens và kiểm mọi cặp chữ/UI theo WCAG AA (chữ 4.5:1, UI 3:1) ở cả light và dark**, đồng thời kiểm 2 khối dark (media query và `data-theme="dark"`) giống hệt nhau. Sửa token mà vỡ tương phản thì CI fail.

| Token | Light | Dark | Dùng cho | Tương phản (light / dark) |
|---|---|---|---|---|
| `bg` | `#FAF4E8` kem | `#18110F` | nền trang (+ `--texture` chấm vàng mờ 22px) | |
| `surface` | `#FFFFFF` trắng | `#221815` | card, input | |
| `surface-2` | `#F5EBDA` kem đậm | `#2D211D` | hover, vùng phụ, skeleton | |
| `border` | `#E6D6BD` | `#453530` | viền card (trang trí) | |
| `border-strong` | `#9C8670` | `#7D675C` | **viền input/select/textarea** (WCAG 1.4.11) | 3.47 / 3.28 |
| `text` | `#2B1B17` | `#F6EDE1` | chữ chính | 15.1 / 16.1 |
| `text-muted` | `#6D5A50` | `#C4B2A3` | chữ phụ (kể cả trên `surface-2`) | 5.5 / 7.6 (tệ nhất) |
| `primary` | `#7A2620` nâu đỏ | `#E8A592` | nút chính, link, tiêu đề trang | 9.0 trên kem / 9.1 |
| `primary-hover` | `#5F1C17` | `#F2BFAE` | hover nút chính | |
| `on-primary` | `#FFFFFF` | `#2A0F0B` | chữ trên nút chính | 9.9 / 8.7 |
| `primary-soft` | `#F6E4DC` | `#3B2420` | nền mục chọn, gợi ý tìm kiếm | primary trên nó 8.0 / 7.0 |
| `accent` | `#B07D24` vàng kim | `#E0B456` | vòng focus, viền trái gói, thanh buổi, ★ | 3.3 trên kem (chỉ UI) / 9.6 |
| `accent-text` | `#8A6114` | `#E6C06A` | **chữ màu vàng** (eyebrow tiêu đề, "★ Khuyên dùng", badge vàng) | 4.9 trên `accent-soft` / 8.0 |
| `accent-soft` | `#FBF0D6` | `#3A2A12` | nền badge vàng, avatar chữ cái, `::selection` | |
| `danger` / `danger-soft` | `#B3261E` / `#FCEBE7` | `#FF8F80` / `#3B1D19` | lỗi, công nợ, huỷ | 5.7 / 6.9 |
| `warning-bg` / `-text` / `-border` | `#FFF3DA` / `#7A4A00` / `#DDAE52` | `#3A2A12` / `#F2CF8A` / `#8A6418` | **lưu ý sức khỏe**, hết hạn | 6.8 / 9.3 |
| `success` / `success-soft` | `#2D6A3E` / `#E8F3E9` | `#8BD3A0` / `#17301F` | "Đang dùng", không nợ | 5.7 / 8.1 |
| `info` / `info-soft` | `#2F5B78` / `#E7F0F6` | `#9CC8E6` / `#172A38` | thông tin trung tính (dự phòng) | 6.3 / 8.3 |
| `focus` | `#B07D24` | `#E0B456` | outline 3px | ≥ 3.3 |
| `chrome-bg` → `chrome-bg-2` | `#5A1D18` → `#74271F` | `#2B1310` → `#3A1915` | gradient header, dải login, khung chào | |
| `chrome-text` | `#FBF1E1` | `#F6EDE1` | chữ trên khung nâu đỏ | 9.2 / 13.6 (tệ nhất) |
| `chrome-text-muted` | `#E6CDB4` | `#D9C2AD` | nav chưa chọn, ngày, tên user | 6.7 / 9.2 |
| `chrome-accent` | `#E3B455` | `#E3B455` | wordmark, logo, gạch chân nav, viền dưới header, **focus ring trong khung** | 5.3 / 8.2 |

Ghi chú:
- **Vàng trên trắng/kem không đạt 4.5:1** (`#B07D24` chỉ 3.3–3.6), nên chữ vàng luôn dùng `accent-text` (`#8A6114`), còn `accent` chỉ cho viền, thanh, icon.
- Trong khung nâu đỏ, focus ring đổi sang `chrome-accent` (đặt `--focus` cục bộ) vì vàng thường chỉ ~2.7:1 trên nâu đỏ.
- Toast lỗi ở dark: chữ `surface` trên `danger` (7.85), sửa lỗi cũ chữ trắng trên `#FF8A80` (~2:1).
- Dark mode theo `prefers-color-scheme`, có thể ghi đè bằng `data-theme="light|dark"`.

## Thương hiệu trong app (logo thật, 2026-11-05)
EM: "logo dùng cho đúng chúng ta đã có logo rồi" ([raw](../../raw/2026-11-05-em-management-views-logo-ui.md)). Logo sen tự vẽ của PR #27 đã bỏ; mọi chỗ dùng **logo thật**.
- **Nguồn:** `docs for reference/logo Viên phúc y đường transparent.png` (1024², chỉ có bản raster). Phần nét nằm trong **kênh alpha** (alpha > 127 là nét chuẩn); "ánh vàng phát sáng" chỉ là RGB của các pixel trong suốt. Nên không phải vẽ lại: `scripts/brand/build_logo_assets.py` trace kênh alpha bằng potrace rồi tách phần theo connected component (vòng, sen, lá, âm dương, chữ, tagline). Chạy lại script khi có file gốc mới.
- **Asset web** (`apps/web/src/assets/brand/`, SVG đen, dùng làm **CSS mask** nên tô bằng `currentColor`: vàng trên nâu đỏ, nâu đỏ trên kem; không tốn JS; Vite hash vào `/assets`, cache vĩnh viễn; CSP `img-src 'self'`):

| Asset | Gồm | Gzip | Dùng ở |
|---|---|---|---|
| `mark.svg` (`.brand-mark`, `BrandMark`) | vòng + sen + 2 lá + âm dương (không chữ) | 2.5 KB | sidebar 40px, top bar điện thoại 32px, drawer, empty state, watermark trang chủ |
| `wordmark.svg` (`.brand-wordmark`, `BrandWordmark`) | chữ script "Viên Phúc / Y Đường" | 3.8 KB | sidebar mở rộng (70×36), drawer |
| `emblem.svg` (`.brand-emblem`, `BrandEmblem`) | toàn bộ logo + tagline cong | 11.8 KB | **chỉ** đăng nhập / MFA (không nằm trên đường nóng `/quay`) |
| `public/favicon.svg`, `favicon-32.png`, `apple-touch-icon.png`, `icons/icon-{192,512,maskable-512}.png` | mark vàng `#E3B455` trên nâu đỏ `#5A1D18` | 0.6–9.8 KB | tab trình duyệt, màn hình chính, PWA manifest |

- Tên spa luôn có dạng chữ thật cho screen reader (logo là `aria-hidden`).
- **Tagline** "Hồi sinh sức khoẻ và sắc đẹp" (đúng chính tả trong logo): chữ hoa giãn 0.18em ở trang đăng nhập, chữ vàng ở khung chào.
- **AuthFrame**: desktop chia 2 cột (trái: panel nâu đỏ có emblem 224px + hoa văn góc; phải: form), điện thoại: emblem 112px trên đầu card. h1 "Viên Phúc Y Đường" sr-only trên desktop (emblem đã chứa tên).
- **Câu hỏi cho EM:** có file gốc vector (AI/SVG/PDF) từ người thiết kế không? Bản trace từ PNG 1024² đủ nét tới ~600px; in ấn khổ lớn nên dùng file gốc.

## App shell (2026-11-05)
- **Desktop/tablet (≥ 768px):** sidebar nâu đỏ (`.sidebar.chrome`) sticky cao 100dvh: logo (mark + wordmark) → nav nhóm theo sổ giấy: **Quầy** (Tổng quan, Ghi lượt khách) · **Khách hàng** (Hồ sơ khách, Lâu chưa quay lại) · **Gói liệu trình** (Theo dõi gói) · **Báo cáo** (Hôm nay) · **Danh mục** (Dịch vụ, Gói bán) · **Cài đặt** (Giao diện & tài khoản). Mục đang chọn: nền gradient vàng nhạt + **thanh vàng 3px bên trái** mọc từ giữa (`scaleY`) + icon vàng. ≥ 1024px rộng 248px, thu gọn được (nút "Thu gọn", nhớ theo thiết bị `localStorage vpyd.sidebar`); 768–1023px luôn là rail icon 76px (nhãn vẫn có cho screen reader, `title` làm tooltip).
- **Top bar** (sticky, 64px, nền `surface` 88% + blur, đường kẻ dưới gradient vàng): ngày (md+), ô **Tìm khách, mở màn hình… Ctrl K** (mở command palette), avatar chữ cái + tên + spa (xl+), nút **Khóa nhanh** (icon + chữ; điện thoại chỉ icon, `aria-label` giữ tên).
- **Điện thoại (< 768px):** top bar có mark; **tab bar** nâu đỏ cố định đáy: Trang chủ · Quầy · Khách · Hôm nay (nếu có quyền) · Thêm (mở **drawer** `<dialog>` trượt từ trái với toàn bộ nav). Nội dung chừa `--tabbar-h` (68px) ở đáy; toast quầy nổi trên tab bar.
- **Command palette** (`app/CommandPalette.tsx`, chunk lazy chỉ tải khi mở): `<dialog>` modal (focus trap, Esc, backdrop do trình duyệt lo). Gõ tên/SĐT → khách (dùng `GET /customers?q=`, chỉ khi có `customer.read`) + màn hình (so khớp không dấu: "goi ban" → "Gói bán"). ↑↓ Enter. Không đụng phím tắt quầy (`/`, `Ctrl+Enter`, `Esc`).
- Nav lấy từ một model (`app/nav.ts` `navFor(me)`) cho sidebar, drawer, tab bar, palette, lối tắt trang chủ. Lọc theo permission (UX; server quyết định).
- **Không còn "sổ cũ" trong UI** (EM 2026-11-05): không mục nav, không ô trang chủ, không badge "Nhập từ sổ cũ"; gói chuyển từ sổ giấy hiển thị như mọi gói. Màn `/nhap-so-cu` đã xóa; dữ liệu sổ giấy nhập bằng CLI `app import-paper-book`.

## Quy tắc layout (audit 2026-11-05)
EM: "mấy ô cách sắp xếp nó bị khó nhìn không đồng đều chỗ to, chỗ nhỏ, chỗ thì bị dính vào nhau chỗ thì khi bấm ra hiệu ứng ảnh hưởng luôn các phần khác".
- **Một thang khoảng cách** (4px: 8/12/16/20/24) và **`gap` thay margin**: mọi khối xếp dọc là `grid gap-*` (không còn `space-y-*` ở container). `.grid` mặc định 1 cột `minmax(0,1fr)` để chữ không xuống dòng (truncate) không đẩy trang rộng ra; nút/link/segmented/badge trong grid giữ bề rộng của nó (`justify-self: start`), muốn full thì `w-full`.
- **Ô cùng hàng cao bằng nhau:** grid `align-items: stretch`; StatCard `min-h-[152px]` có dòng chân dành sẵn (`mt-auto min-h-5`) dù có hay không có ghi chú; ô lối tắt `min-h-[88px]`.
- **Hiệu ứng hover/nhấn chỉ dùng `transform` / `opacity` / `box-shadow` / `outline` / màu.** Không đổi width, height, padding, margin, border-width hay **font-weight** khi tương tác (đã bỏ chữ đậm ở nút segmented và nút Nữ/Nam đang chọn: thay bằng gạch chân vàng `inset box-shadow`).
- Badge luôn `white-space: nowrap` + cách chữ bên cạnh `ml-2`; bảng cuộn trong card (`.table-wrap`, header dính mép card).
- Panel mở ra (form hủy, thao tác gói) nằm ngay dưới dòng đã bấm: chỉ đẩy phần bên dưới, không đẩy phần ngang hàng.
- **Kiểm tự động:** `e2e/specs/layout.spec.ts` (+ `e2e/support/layout.ts`): mỗi màn, lấy mẫu ≤ 12 control, đo box mọi phần tử trước/sau hover và nhấn (mousedown, kéo ra ngoài, nhả), lỗi nếu phần tử nào ngoài control đổi vị trí/kích thước; và không cuộn ngang ở 360/768/1280/1440, đính kèm ảnh từng màn × bề rộng vào report.

## Element polish toàn app (`apps/web/src/styles.css`)
Một lớp CSS **không nằm trong layer** (thắng utility) cho vài thuộc tính, để mọi màn hình (kể cả màn chỉ dùng utility như `/nhap-so-cu`) cùng một kiểu:
- input/select/textarea: viền `border-strong`, hover `text-muted`, focus viền `primary` + quầng 3px `primary` 14%, `aria-invalid` → viền `danger`, disabled → `surface-2`. Checkbox/radio `accent-color: primary`.
- button/a: transition màu/viền/bóng/transform 120ms; nhấn → dịch 1px.
- `button.bg-primary` / `a.bg-primary`: "ánh sơn mài" (gradient trắng 10% → 0) + `shadow-primary`.
- Utility mới: `shadow-card` (= `shadow-1`), `shadow-float` (= `shadow-2`), màu `chrome`, `chrome-text(-muted)`, `chrome-accent`, `accent-soft`, `info(-soft)`, `border-strong`.


## Trước / sau
Ảnh 1280×800 và 390×844, light + dark, dữ liệu `app seed-demo`: thư mục scratchpad `ui2-shots/before` và `ui2-shots/after` (đính kèm PR #28).

**UI overhaul (2026-11-05, sau PR #27):**
| Màn | Trước (#27) | Sau |
|---|---|---|
| Khung app | header nâu đỏ 1 hàng, nav chữ cuộn ngang, logo sen tự vẽ | sidebar nhóm + icon + thanh vàng mục chọn, top bar có tìm nhanh Ctrl K, tab bar + drawer trên điện thoại, logo thật |
| Đăng nhập | card hẹp, dải nâu đỏ + sen tự vẽ | 2 cột: emblem thật trên panel nâu đỏ có hoa văn góc, form rộng |
| Trang chủ | khung chào + 4 ô chữ | khung chào hoa văn góc + watermark mark, "Hôm nay" (thực thu, khách, buổi) đếm số, lối tắt có icon + nâng nhẹ khi hover |
| Mới | — | `/bao-cao`, `/goi-lieu-trinh`, `/khach-lau-khong-quay-lai`, lịch sử trên hồ sơ khách, `/cai-dat` |
| Tải dữ liệu | chữ "Đang tải…" / nhấp nháy | skeleton có vệt sáng chạy (transform) |

**Brand redesign (PR #27):**
| Màn | Trước | Sau |
|---|---|---|
| `/dang-nhap`, `/xac-thuc` | card trắng trơn giữa nền kem, tên spa chữ đỏ | dải nâu đỏ gradient, wordmark vàng, tagline; đường kẻ vàng; nút chính ánh sơn mài |
| `/quay` | tràn ngang 48px khi có giỏ, card phẳng | hết tràn; card có bóng ấm; gói có viền trái vàng + thanh số buổi (đỏ khi ≤ 2); badge trạng thái theo màu; toast nâu đỏ viền vàng |
| Input mọi màn | viền `#E4DAD0` (~1.3:1) | viền `border-strong` 3.47:1, focus có quầng |
| Dark | nâu tím lạnh | nâu trà ấm, khung nâu đỏ sậm, vàng giữ nguyên |

## Chữ
- **UI:** *Be Vietnam Pro* (400/500/600/700). Font này thiết kế cho tiếng Việt nên dấu chồng (ệ, ở, ữ) không bị cắt. Dự phòng: `system-ui, Segoe UI, Roboto`.
- **Thương hiệu:** *Lora* 600, chỉ dùng cho logo chữ và tiêu đề trang lớn; có subset Vietnamese.
- Tải font với `subset=vietnamese`, `display=swap`; self-host qua `@fontsource` khi build. Hiện app nội bộ chỉ tải Be Vietnam Pro 400/600 + Lora 600, subset latin + vietnamese, và lần tải đầu tải font sau màn hình đầu tiên (quy tắc ở [frontend § Perf budget](../architecture/frontend.md)). Muốn thêm weight 500/700 thì đo lại LCP trước.
- Thang cỡ chữ (px / line-height): `xs 12/16` · `sm 13/18` · `base 15/22` · `md 17/24` · `lg 20/28` · `xl 24/32` · `2xl 30/38`. Số tiền dùng `font-variant-numeric: tabular-nums`.
- Ô tìm khách ở quầy dùng 18px để đọc từ xa.

## Spacing, radius, elevation, z-index
- Spacing (bước 4px): `1=4 · 2=8 · 3=12 · 4=16 · 5=20 · 6=24 · 8=32 · 10=40 · 12=48`.
- Radius: `sm 6` (input) · `md 10` (nút, dòng) · `lg 14` (card) · `full` (badge).
- Elevation: `shadow-1` (card) · `shadow-2` (toast, popover, dialog). Không dùng nhiều lớp đổ bóng.
- Z-index: `base 0 · sticky 10 · dropdown 20 · overlay 30 · dialog 40 · toast 50`.
- **Vùng chạm ≥ 44×44px** (WCAG 2.5.8 + dùng trên tablet).

## Breakpoints
`sm 640` (điện thoại ngang) · `md 768` (tablet dọc) · `lg 1024` (tablet ngang, laptop) · `xl 1280` (desktop).
- Màn hình quầy: **2 cột từ ≥ 900px** (trái: tìm + thẻ khách; phải: lượt đang ghi + thanh toán + lượt hôm nay), 1 cột khi nhỏ hơn.
- Giao diện KTV (R2/R3): thiết kế mobile trước.

## Motion
| Token | Giá trị | Dùng cho |
|---|---|---|
| `dur-fast` | 120ms | hover, nhấn nút |
| `dur-base` | 200ms | toast, mở/đóng dropdown |
| `dur-slow` | 320ms | dialog, chuyển trang |
| `ease-out` | `cubic-bezier(.2,.8,.2,1)` | phần lớn chuyển động |
| `ease-in-out` | `cubic-bezier(.4,0,.2,1)` | thay đổi layout |
- Motion để **giải thích**: toast trượt lên khi lưu, số buổi còn lại đổi có hiệu ứng nhẹ. App nội bộ không dùng hiệu ứng trang trí; 3D dành cho site công khai.
- **Chỉ transform/opacity** (chạy trên compositor). Danh sách hiệu ứng (`apps/web/src/components.css`):

| Hiệu ứng | Chi tiết | Ngân sách |
|---|---|---|
| Chuyển trang | `.page-enter`: opacity 0→1 + translateY 6px, `dur-slow` | **không chạy ở lần vẽ đầu** (không trễ LCP), chỉ sau điều hướng |
| Thanh mục nav | `scaleY` 0→1, `dur-base` | |
| Card bấm được | `.card-link`: nhấc 2px + `shadow-2` khi hover | không đổi layout |
| Đếm số | `CountUp` 700 ms ease-out (rAF), số `tabular-nums` nên không xô chữ | screen reader đọc giá trị cuối |
| Biểu đồ | cột mọc `scaleY` lệch nhau 40 ms; split bar `scaleX` | |
| Skeleton | `.skeleton::after` vệt sáng `translateX` 1.4 s | thay spinner và `animate-pulse` |
| Drawer / palette | trượt từ trái / pop-in 8px + scale .98 | |
- `prefers-reduced-motion: reduce` → tắt mọi transition/animation (tokens.css), `CountUp` hiện ngay giá trị cuối.
- `prefers-reduced-motion: reduce` → tắt mọi transition/animation, chỉ đổi trạng thái tức thì.

## Định dạng
- Tiền: `Intl.NumberFormat('vi-VN')` + `đ`, không khoảng trắng → `1.500.000đ`. Ô nhập tiền nhận số nguyên, bước 1.000.
- Ngày `dd/mm/yyyy`, giờ `HH:mm` (24h), múi giờ `Asia/Ho_Chi_Minh`. "Hôm nay"/"Hôm qua" cho ngày gần.
- SĐT hiển thị `0905 123 456`; lưu chuẩn E.164 ở backend.
- Giới tính: `Nữ` / `Nam` / `Chưa rõ`.

## Component inventory
Primitives dựng trên shadcn/ui (Radix) trong `packages/ui`, theme bằng tokens trên. Mọi component có đủ các state: **default · hover · focus-visible · active · disabled · loading · error** (thêm **empty** cho component chứa dữ liệu).

**Class dùng chung** (`apps/web/src/components.css`, layer `components` nên utility trên cùng phần tử vẫn thắng; viết CSS thay vì chuỗi utility dài vì class trong JSX nằm trong bundle JS):
| Class | Dùng cho |
|---|---|
| `.card`, `.card-link` | card trắng viền + `shadow-1`; bản bấm được nâng nhẹ |
| `.ornament` | hoa văn góc vàng (trên trái + dưới phải), lấy cảm hứng góc trang sổ giấy; `--ornament` đổi màu |
| `.btn` + `.btn-primary` / `.btn-secondary` | nút 44px; nút outline cũ (`border-border`) tự có nền `surface` + hover viền đậm |
| `.badge` + `-ok` / `-warn` / `-danger` / `-muted` / `-gold` | badge có chấm màu; chữ luôn đi kèm màu |
| `.data-table` | header dính dưới top bar, kẻ mảnh, hover dòng, `.num` canh phải + `tabular-nums` |
| `.segmented` | bộ lọc / chọn theme (`aria-pressed`) |
| `.skeleton` | khối chờ có vệt sáng |
| `.page-title`, `.eyebrow`, `.gold-rule` | tiêu đề Lora nâu đỏ 26/34, nhãn nhóm chữ hoa vàng, đường kẻ vàng |
| `.icon` | icon nét 24×24 từ sprite `assets/icons.svg` (`<Icon name>`; `<use href>` tới 1 file hash, 1.3 KB gzip, không thư viện icon) |

| Component | Ghi chú riêng |
|---|---|
| Button (`primary`, `secondary`, `ghost`, `danger`; `sm`/`md`/`lg`) | loading: spinner + giữ nguyên chiều rộng, không bấm được lần 2 |
| Toggle group (hình thức thanh toán, Nữ/Nam) | `aria-pressed`; tối thiểu 44px |
| Input, NumberInput (tiền), PhoneInput, Textarea, Select, Combobox (chọn KTV, dịch vụ) | error: viền `danger` + thông báo bên dưới, `aria-invalid`, `aria-describedby` |
| SearchBox + Listbox (tìm khách) | điều hướng ↑/↓/Enter, `role=listbox/option`, debounce 150ms, state đang tìm / không thấy / lỗi |
| Card, Badge (`debt`, `ok`, `muted`, `star`) | |
| **HealthNote** callout | luôn dùng `warning-*`, icon ⚠, tiêu đề "Lưu ý sức khỏe"; không render gì khi API không trả field |
| **PackageRow** | tên, `còn X/Y buổi`, HSD; states: active, sắp hết (≤ 2 buổi: thanh đỏ), hết buổi, hết hạn, đã hủy. Gói chuyển từ sổ giấy **không** có nhãn riêng |
| **CartLine** | dịch vụ, nguồn (trừ liệu trình / dịch vụ lẻ), KTV (bắt buộc), giá, bỏ dòng |
| **MoneySummary** | phải trả, đã trả; số `tabular-nums` |
| **StatCard** (`features/reports/ui.tsx`) | nhãn + icon tròn vàng, số 28px đếm lên, chân: `Delta` (▲/▼ % so với hôm qua, màu + chữ); bản `accent` nâu đỏ cho "Thực thu"; loading: `StatSkeleton` |
| **BarChart / SplitBar** (`features/reports/charts.tsx`) | cột HTML theo %, 1 màu nâu đỏ, cột được chọn màu vàng; nhãn giá trị khi hover/focus (cột cuối luôn hiện); là `<ol>` có `aria-label` từng cột; không thư viện chart |
| **Timeline** (lịch sử khách) | đường dọc gradient vàng, chấm theo loại (lượt nâu đỏ, mua gói vàng, hủy xám/đỏ), nhóm theo tháng |
| EmptyState (mark logo mờ + câu hướng dẫn), ErrorState ("Thử lại") | |
| Dialog / Sheet (điện thoại) | `<dialog>` + `showModal()` (`Modal` trong AppShell): focus trap, Esc, backdrop |
| ReasonDialog | bắt buộc nhập lý do (hủy, giảm giá, cho dùng gói hết hạn) |
| Toast | `role=status`; lỗi dùng `role=alert`; 3–4 giây; không che nút Lưu; nổi trên tab bar điện thoại |
| AppShell, CommandPalette | xem § App shell |
