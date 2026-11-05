---
title: Design system — internal app (R1)
updated: 2026-11-05
sources: ["docs for reference/logo Viên phúc y đường transparent.png", "docs for reference/Menu mặt trước - mới.png", product/personas.md, raw/2026-11-05-em-brand-colors-and-demo-data.md]
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
| `accent-text` | `#8A6114` | `#E6C06A` | **chữ màu vàng** (badge "Nhập từ sổ cũ", "★ Khuyên dùng") | 4.9 trên `accent-soft` / 8.0 |
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

## Thương hiệu trong app
- **BrandMark** (`apps/web/src/app/BrandMark.tsx`): SVG inline, vòng tròn đôi + hoa sen 5 cánh + đế, vẽ bằng `currentColor` (thường là `chrome-accent`), `aria-hidden`. Không request ngoài (CSP `'self'`). Dùng ở header (32px), dải login/MFA (56px), watermark khung chào trang chủ (opacity 20%), empty state quầy.
- **Wordmark** "Viên Phúc Y Đường": Lora 600 vàng kim trên nâu đỏ. Trên điện thoại header chỉ hiện logo, tên vẫn có cho screen reader.
- **Tagline** "Hồi sinh sức khỏe và sắc đẹp" (từ logo): chữ hoa giãn 0.18em ở dải login, chữ thường ở khung chào.
- **AuthFrame** (`features/auth/AuthFrame.tsx`): khung chung cho đăng nhập và MFA, dải nâu đỏ + logo + đường kẻ vàng gradient.

## Element polish toàn app (`apps/web/src/styles.css`)
Một lớp CSS **không nằm trong layer** (thắng utility) cho vài thuộc tính, để mọi màn hình (kể cả màn chỉ dùng utility như `/nhap-so-cu`) cùng một kiểu:
- input/select/textarea: viền `border-strong`, hover `text-muted`, focus viền `primary` + quầng 3px `primary` 14%, `aria-invalid` → viền `danger`, disabled → `surface-2`. Checkbox/radio `accent-color: primary`.
- button/a: transition màu/viền/bóng/transform 120ms; nhấn → dịch 1px.
- `button.bg-primary` / `a.bg-primary`: "ánh sơn mài" (gradient trắng 10% → 0) + `shadow-primary`.
- Utility mới: `shadow-card` (= `shadow-1`), `shadow-float` (= `shadow-2`), màu `chrome`, `chrome-text(-muted)`, `chrome-accent`, `accent-soft`, `info(-soft)`, `border-strong`.


## Trước / sau (ảnh chụp 1280×800 và 390×844, light + dark)
| Màn | Trước | Sau |
|---|---|---|
| `/dang-nhap`, `/xac-thuc` | card trắng trơn giữa nền kem, tên spa chữ đỏ | dải nâu đỏ gradient với logo sen vàng, wordmark vàng, tagline; đường kẻ vàng; nút chính ánh sơn mài |
| Header | nền trắng, tên spa đỏ, mục chọn nền hồng nhạt | nền nâu đỏ gradient, viền dưới vàng, logo + wordmark vàng, mục chọn gạch chân vàng, "Khóa nhanh" viền kem |
| Trang chủ | 1 dòng "Xin chào" + câu "Các màn hình nghiệp vụ sẽ xuất hiện ở đây." | khung chào nâu đỏ (ngày, tên, spa, watermark sen) + 4 ô lối tắt theo quyền (Ghi lượt khách, Hồ sơ khách, Chuyển sổ giấy, Dịch vụ & gói) |
| `/quay` | **tràn ngang 48px** khi có giỏ (select dịch vụ lẻ rộng theo option dài nhất), card phẳng | hết tràn; card có bóng ấm; gói có viền trái vàng + thanh số buổi (đỏ khi ≤ 2); tên khách Lora nâu đỏ; badge trạng thái theo màu; toast nâu đỏ viền vàng trượt lên |
| Khách hàng | danh sách chữ trơn | avatar chữ cái (chữ cái tên, nền vàng nhạt), dòng cao 56px, card bo 14px có bóng |
| Hồ sơ khách | tên đậm thường, trạng thái chữ xám | tên Lora nâu đỏ; "Đang dùng" badge xanh, "Hết hạn" vàng, "Đã hủy" đỏ; "Nhập từ sổ cũ" badge vàng |
| Input mọi màn | viền `#E4DAD0` (~1.3:1, khó thấy) | viền `border-strong` 3.47:1, focus có quầng |
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
- `prefers-reduced-motion: reduce` → tắt mọi transition/animation, chỉ đổi trạng thái tức thì.

## Định dạng
- Tiền: `Intl.NumberFormat('vi-VN')` + `đ`, không khoảng trắng → `1.500.000đ`. Ô nhập tiền nhận số nguyên, bước 1.000.
- Ngày `dd/mm/yyyy`, giờ `HH:mm` (24h), múi giờ `Asia/Ho_Chi_Minh`. "Hôm nay"/"Hôm qua" cho ngày gần.
- SĐT hiển thị `0905 123 456`; lưu chuẩn E.164 ở backend.
- Giới tính: `Nữ` / `Nam` / `Chưa rõ`.

## Component inventory
Primitives dựng trên shadcn/ui (Radix) trong `packages/ui`, theme bằng tokens trên. Mọi component có đủ các state: **default · hover · focus-visible · active · disabled · loading · error** (thêm **empty** cho component chứa dữ liệu).

| Component | Ghi chú riêng |
|---|---|
| Button (`primary`, `secondary`, `ghost`, `danger`; `sm`/`md`/`lg`) | loading: spinner + giữ nguyên chiều rộng, không bấm được lần 2 |
| Toggle group (hình thức thanh toán, Nữ/Nam) | `aria-pressed`; tối thiểu 44px |
| Input, NumberInput (tiền), PhoneInput, Textarea, Select, Combobox (chọn KTV, dịch vụ) | error: viền `danger` + thông báo bên dưới, `aria-invalid`, `aria-describedby` |
| SearchBox + Listbox (tìm khách) | điều hướng ↑/↓/Enter, `role=listbox/option`, debounce 150ms, state đang tìm / không thấy / lỗi |
| Card, Badge (`debt`, `ok`, `muted`, `star`) | |
| **HealthNote** callout | luôn dùng `warning-*`, icon ⚠, tiêu đề "Lưu ý sức khỏe"; không render gì khi API không trả field |
| **PackageRow** | tên, `còn X/Y buổi`, HSD, nợ; states: active, sắp hết (≤ 2 buổi: badge vàng), hết buổi, hết hạn, đã hủy, "Nhập từ sổ cũ" |
| **CartLine** | dịch vụ, nguồn (trừ liệu trình / dịch vụ lẻ), KTV (bắt buộc), giá, bỏ dòng |
| **MoneySummary** | phải trả, đã trả, còn nợ; số `tabular-nums` |
| Dialog / Sheet (điện thoại) | dùng cho bán gói, hủy có lý do, nhập sổ cũ; focus trap, Esc đóng |
| ReasonDialog | bắt buộc nhập lý do (hủy, giảm giá, cho dùng gói hết hạn) |
| Toast | `role=status`; lỗi dùng `role=alert`; 3–4 giây; không che nút Lưu |
| Table / List (lượt hôm nay, công nợ) | sắp xếp, phân trang con trỏ; skeleton khi tải; empty state có câu hướng dẫn |
| Skeleton, EmptyState, ErrorState (có nút "Thử lại") | |
| AppShell (header, nav, nút khóa nhanh) | nav theo permission (UX), không phải bảo mật |
