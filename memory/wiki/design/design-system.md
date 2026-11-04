---
title: Design system — internal app (R1)
updated: 2026-11-04
sources: ["docs for reference/logo Viên phúc y đường transparent.png", "docs for reference/Menu mặt trước - mới.png", product/personas.md]
---

# Design system: app nội bộ

## Hướng thiết kế (đề xuất; EM có thể chỉnh)
**"Trà thất yên tĩnh"** (tiệm trà tĩnh lặng): nền sáng ấm như giấy dó, chữ nâu đậm, **màu đỏ đô** của thương hiệu dùng cho hành động chính, **vàng kim** chỉ dùng làm điểm nhấn (viền focus, dấu ★, logo).
- **Lý do:** quản lý dùng app 8–10 giờ mỗi ngày, cần dịu mắt và dễ đọc số tiền. Nền đỏ đô và chữ vàng của menu in quá nặng cho màn hình dùng cả ngày, nên phong cách đó dành cho **site công khai** sau này. App nội bộ giữ nhận diện bằng màu chủ đạo, font serif ở tiêu đề thương hiệu và logo.
- Tính từ định hướng: ấm áp, điềm tĩnh, chuyên nghiệp, rõ ràng, nhanh.

## Màu (semantic tokens)
Mọi màu đều qua token; component không dùng mã hex trực tiếp. Độ tương phản đo trên `surface`/`bg` tương ứng.

| Token | Light | Dark | Dùng cho | Tương phản |
|---|---|---|---|---|
| `bg` | `#FAF7F2` | `#171112` | nền trang | |
| `surface` | `#FFFFFF` | `#221A1B` | card, input | |
| `surface-2` | `#F3EDE4` | `#2C2223` | hover, vùng phụ | |
| `border` | `#E4DAD0` | `#3D3132` | viền | (trang trí) |
| `text` | `#2A1F1D` | `#F3ECE8` | chữ chính | ≈ 15:1 / 16:1 |
| `text-muted` | `#6B5E5A` | `#B8A9A4` | chữ phụ | ≈ 6.2:1 / 8:1 |
| `primary` | `#7A1F2B` | `#E8A0A8` | nút chính, link, focus input | ≈ 10:1 / 9:1 |
| `on-primary` | `#FFFFFF` | `#2A0E12` | chữ trên nút chính | ≈ 10:1 |
| `primary-soft` | `#F6E7E9` | `#3A2427` | nền mục đang chọn | |
| `accent` | `#B8862B` | `#E2B95A` | vòng focus, ★, trang trí; **không dùng cho chữ nhỏ ở light** | 3.2:1 (chỉ UI ≥ 3:1) |
| `accent-text` | `#8A6418` | `#E2B95A` | chữ màu vàng kim | ≈ 5.4:1 / 10:1 |
| `danger` / `danger-soft` | `#B42318` / `#FDECEA` | `#FF8A80` / `#3A1E1C` | lỗi, công nợ | ≈ 6.6:1 |
| `warning-bg` / `warning-text` / `warning-border` | `#FFF4E0` / `#7A4A00` / `#E9B75A` | `#3A2A10` / `#F5D08A` / `#8A6418` | **lưu ý sức khỏe**, cảnh báo | ≈ 7:1 |
| `success` / `success-soft` | `#1E6B3A` / `#E7F4EC` | `#7FD3A0` / `#16301F` | thành công, "không nợ" | ≈ 6.5:1 |
| `focus` | `#B8862B` | `#E2B95A` | outline 3px | ≥ 3:1 với nền |

Dark mode theo `prefers-color-scheme`, có thể ghi đè bằng `data-theme="light|dark"` (người dùng tự chọn, lưu ở localStorage).

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
