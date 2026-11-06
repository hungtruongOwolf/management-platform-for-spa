---
title: Design system — internal app (R1)
updated: 2026-11-06
sources: [raw/2026-11-06-em-ui-redesign-direction.md, raw/2026-11-06-research-ui-skills-taste-impeccable.md, raw/2026-11-06-em-review-round5.md, raw/2026-11-05-em-ui-review-round3.md, "docs for reference/logo Viên phúc y đường transparent.png", "docs for reference/Menu mặt trước - mới.png", product/personas.md, raw/2026-11-05-em-brand-colors-and-demo-data.md, raw/2026-11-05-em-management-views-logo-ui.md]
---

# Design system: app nội bộ

## Quy tắc (rulebook, 2026-11-06 — áp cho mọi màn làm việc)
EM chốt hướng redesign ([raw](../../raw/2026-11-06-em-ui-redesign-direction.md)): app nội bộ là **chế độ "Vận hành"**, không phải trang quảng bá. Người dùng ngồi cả ngày, nên mẫu quen thuộc + phân cấp rõ thắng mọi trang trí. Thương hiệu (nâu đỏ, vàng, logo, Lora) sống ở **khung app** (sidebar, tab bar, đăng nhập, khung chào); vùng làm việc yên tĩnh. Màn mẫu đầu tiên: `/quay` "Ghi lượt khách" (PR redesign pilot). Màn khác chuyển dần theo đúng các quy tắc này.

**1. Chữ.** Một họ chữ cho UI: *Be Vietnam Pro*. *Lora* chỉ ở chỗ thương hiệu (logo chữ, đăng nhập/MFA, lời chào ở khung chào trang chủ); tiêu đề màn làm việc (`.page-title`) dùng Be Vietnam Pro 600 24/32 màu `text`. Thang cố định (không `clamp`, tỉ lệ ≈ 1,13–1,2), token trong `tokens.css`:

| Token | px / line-height | Dùng cho |
|---|---|---|
| `--fs-xs` | 12 / 16 | chip trạng thái, kbd |
| `--fs-sm` | 13 / 18 | nhãn nhóm, chữ phụ, meta dòng |
| `--fs-base` | 15 / 22 | chữ thường, nút |
| `--fs-field` | 16 / 24 | **chữ trong ô nhập/select/textarea** (điện thoại không zoom khi focus) |
| `--fs-md` | 17 / 24 | tiêu đề panel, nút lớn |
| `--fs-lg` | 20 / 28 | tên khách đang chọn |
| `--fs-xl` | 24 / 32 | tiêu đề trang, số tiền phải trả |

Không chữ hoa giãn chữ (eyebrow) ở bất kỳ đâu; nhãn nhóm là sentence case 13px 600 `text-muted` (`.section-label`, `.eyebrow` của palette). Số luôn `tabular-nums`.

**2. Khoảng cách.** Bước 4px; chỉ dùng **4 · 8 · 12 · 16 · 20 · 24 · 32**. Trong panel: 16 giữa các khối, 12 trong một khối, 8 giữa control cạnh nhau. Panel padding 16 (điện thoại) / 20. `gap`, không margin lẻ.

**3. Màu theo vai trò.** Màu nhấn (`primary` nâu đỏ) chỉ cho **hành động, mục đang chọn, trạng thái**; không tô tiêu đề hay icon trang trí bằng nâu đỏ/vàng. Vàng (`accent`) chỉ ở khung app, focus ring, ★. Nền trang `bg` phẳng (**bỏ chấm vàng `--texture`**), panel `surface`, vùng phụ `surface-2`. Đỏ = lỗi/hủy, hổ phách = cần chú ý, xanh lá = đang dùng/đã thu, xanh dương = thông tin trung tính. Tương phản: chữ ≥ 4.5:1, viền control ≥ 3:1, kiểm tự động trong `contrast.test.ts` (thêm `on-primary` trên `primary-pressed` và `danger`).

**4. Nút** (`.btn` + kiểu, `components.css`). Bốn kiểu, **phẳng** (không gradient, không bóng màu):

| Kiểu | Dùng khi | Mặc định | Hover | Nhấn | 
|---|---|---|---|---|
| `btn-primary` | **một** hành động chính mỗi vùng tác vụ (Lưu lượt khách) | nền `primary`, chữ `on-primary` | `primary-hover` | `primary-pressed` |
| `btn-secondary` | hành động thường (Trừ 1 buổi, Đổi khách, Khách mới) | viền `border-strong`, nền `surface` | viền + chữ `primary` | nền `primary-soft` |
| `btn-ghost` | hành động phụ trong dòng (Đổi giờ, Thêm giảm giá, Bỏ sửa, ✕) | trong suốt, chữ `primary` | nền `surface-2` | nền `primary-soft` |
| `btn-danger` | hành động phá (Hủy lượt) | nền `danger` | tối hơn 14% | |

Đủ 7 trạng thái: mặc định · hover · focus-visible (vòng vàng toàn cục) · nhấn (đổi nền, dịch 1px) · disabled / `aria-disabled` (mờ 50%, `not-allowed`; ưu tiên `aria-disabled` để không mất focus) · đang tải `aria-busy="true"` (spinner trước nhãn, giữ nguyên bề rộng, không bấm lần 2) · lỗi (thông báo cạnh nút/ô, không đổi màu nút). Cao 44px; `btn-lg` 48px; `btn-block` rộng 100%; `btn-icon` vuông 44px luôn có `aria-label`. Nhãn = **động từ + đối tượng** ("Lưu lượt khách", "Thêm giảm giá", "Bỏ liên kết lịch hẹn"), không "OK", không "Thêm" trơn.

**5. Ô nhập** (`.input`, `.field`). Nhãn **ở trên** (14px 500), ô cao 44px, chữ 16px, viền `border-strong`, bo 6px; dưới ô là gợi ý `.field-hint` (13px muted) hoặc lỗi `.field-error` (13px `danger`, có `aria-describedby`, ô `aria-invalid` viền đỏ). Placeholder không thay nhãn. Select chọn-là-thêm (dịch vụ lẻ, dịch vụ đặt lịch): chọn bằng chuột thêm ngay; bước qua bằng ↑/↓ khi danh sách đóng chỉ **chọn tạm** và hiện "Nhấn Enter để thêm …".

**6. Card / panel.** Một kiểu: viền `border`, bo 14, `shadow-1`. **Không card trong card**: các dòng trong panel cách nhau bằng đường kẻ mảnh (`border-bottom`), không khung riêng. **Không viền sọc một bên** (border-left màu) ở nội dung: trạng thái nói bằng chip + chữ. Ngoại lệ duy nhất là thanh vàng mục nav đang chọn (chỉ báo điều hướng). Hoa văn góc (`.ornament`) chỉ ở panel đăng nhập.

**7. Chip trạng thái** (`.badge`, một bộ cho cả app, luôn có chữ): `badge-ok` đang dùng / đã thu · `badge-warn` cần chú ý (hết hạn, sắp hết buổi) · `badge-danger` đã hủy / lỗi · `badge-muted` đã xong / hết buổi · `badge-info` trung tính (đã đặt) · `badge-gold` chỉ cho ★ thương hiệu. Mỗi đối tượng **một** trạng thái hiển thị; không đặt hai chip mâu thuẫn ("Đã hủy" + "Đã hết buổi") và không cho hành động mà trạng thái đã cấm.

**8. Danh sách / bảng.** Dòng ≥ 44px (bảng ~48px), số canh phải `tabular-nums`, cột cố định khi nhiều dòng (`ResponsiveTable` `width`), ≥ 2 thao tác mỗi dòng → menu ⋯. Mục đã đóng (gói hết buổi/đã hủy, lượt đã hủy) gom sau một disclosure có số đếm hoặc làm mờ, không trộn với mục đang dùng.

**9. Trống / đang tải / lỗi.** Trống **dạy bước tiếp theo** ("Chưa có dịch vụ. Bấm “Trừ 1 buổi” … hoặc chọn dịch vụ lẻ bên dưới"; "Không tìm thấy khách nào. Bấm “Khách mới” …"). Đang tải = skeleton đúng hình (không spinner, trừ nút `aria-busy`). Lỗi = câu tiếng Việt nói cái gì hỏng + cách sửa + "Thử lại", giữ dữ liệu đã nhập. Thông tin phụ (phím tắt, ghi chú giờ khóa sổ) nằm sau một toggletip `Tip` (nút tròn "?"/"i", bấm/chạm mở, Esc đóng, nội dung là mô tả của nút).

**10. Chuyển động.** 150–250 ms, **chỉ cho thay đổi trạng thái** (hover, mở/đóng, toast); chỉ `transform`/`opacity`/màu. **Bỏ `CountUp`** (số hiện ngay), không hiệu ứng tải trang, không lấp lánh trang trí (trừ skeleton). `prefers-reduced-motion`: tắt animation và chuyển động, **giữ** đổi màu/opacity để vẫn thấy phản hồi. Modal là lựa chọn cuối: ưu tiên mở ngay tại chỗ (inline, disclosure); toast nằm **trên cùng** giữa màn (không che thanh lưu dính đáy).

**Kiểm tra mỗi màn trước khi xong:** 1280 / 1024 / 390, sáng + tối, các trạng thái (hover/focus/nhấn/disabled/đang tải/lỗi/trống), axe sạch, vùng chạm 44px, không chữ bị cắt, không cuộn ngang.

## Hướng thiết kế: "Trà thất Đông y" (2026-11-05, thay "Trà thất yên tĩnh")
EM thấy UI cũ "hơi xấu" và giao việc phối 4 màu đặc trưng của spa: **nâu đỏ, vàng, trắng, kem**, sao cho "sinh động và tinh tế" ([raw](../../raw/2026-11-05-em-brand-colors-and-demo-data.md)). Hướng mới lấy cảm hứng trực tiếp từ menu in: **khung app (header, dải đầu trang đăng nhập, khung chào ở trang chủ) dùng nền nâu đỏ đậm + chữ kem + vàng kim**, còn **vùng làm việc** giữ nền kem + card trắng để đọc số tiền cả ngày không mỏi mắt.
- **Quy tắc 70 / 20 / 10:** kem + trắng ~70% (nền, card), nâu đỏ ~20% (header, nút chính, tiêu đề), vàng ~10% (viền focus, gạch chân mục nav đang chọn, thanh số buổi, ★, logo, đường kẻ). Vàng **không** dùng làm nền lớn, cũng không dùng cho chữ nhỏ trên nền sáng (chỉ dùng `accent-text`).
- Tính từ định hướng: ấm áp, sang, điềm tĩnh, rõ ràng, nhanh. "Sinh động" đến từ khung nâu đỏ và logo vàng; vùng làm việc phẳng và yên tĩnh (2026-11-06: bỏ ánh sơn mài trên nút chính, chấm vàng nền trang, hoa văn góc ở màn làm việc).
- Lý do giữ vùng làm việc sáng: quản lý dùng app 8–10 giờ/ngày; nền đỏ đô toàn trang như menu in quá nặng (giữ cho site công khai sau này).

## Màu (semantic tokens)
Mọi màu đều qua token (`packages/ui/src/tokens/tokens.css`), component không dùng hex trực tiếp. **`packages/ui/src/tokens/contrast.test.ts` đọc chính file tokens và kiểm mọi cặp chữ/UI theo WCAG AA (chữ 4.5:1, UI 3:1) ở cả light và dark**, đồng thời kiểm 2 khối dark (media query và `data-theme="dark"`) giống hệt nhau. Sửa token mà vỡ tương phản thì CI fail.

| Token | Light | Dark | Dùng cho | Tương phản (light / dark) |
|---|---|---|---|---|
| `bg` | `#FAF4E8` kem | `#18110F` | nền trang (phẳng; `--texture` đã bỏ 2026-11-06) | |
| `surface` | `#FFFFFF` trắng | `#221815` | card, input | |
| `surface-2` | `#F5EBDA` kem đậm | `#2D211D` | hover, vùng phụ, skeleton | |
| `border` | `#E6D6BD` | `#453530` | viền card (trang trí) | |
| `border-strong` | `#9C8670` | `#7D675C` | **viền input/select/textarea** (WCAG 1.4.11) | 3.47 / 3.28 |
| `text` | `#2B1B17` | `#F6EDE1` | chữ chính | 15.1 / 16.1 |
| `text-muted` | `#6D5A50` | `#C4B2A3` | chữ phụ (kể cả trên `surface-2`) | 5.5 / 7.6 (tệ nhất) |
| `primary` | `#7A2620` nâu đỏ | `#E8A592` | nút chính, link, tiêu đề trang | 9.0 trên kem / 9.1 |
| `primary-hover` | `#5F1C17` | `#F2BFAE` | hover nút chính | |
| `primary-pressed` | `#4A1511` | `#F8D6CA` | nút chính đang nhấn | on-primary trên nó ≥ 4.5 (test) |
| `on-primary` | `#FFFFFF` | `#2A0F0B` | chữ trên nút chính | 9.9 / 8.7 |
| `primary-soft` | `#F6E4DC` | `#3B2420` | nền mục chọn, gợi ý tìm kiếm | primary trên nó 8.0 / 7.0 |
| `accent` | `#B07D24` vàng kim | `#E0B456` | vòng focus, viền trái gói, thanh buổi, ★ | 3.3 trên kem (chỉ UI) / 9.6 |
| `accent-text` | `#8A6114` | `#E6C06A` | **chữ màu vàng** ("★ Khuyên dùng", badge vàng) | 4.9 trên `accent-soft` / 8.0 |
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

## Thương hiệu trong app (logo thật, nguyên vẹn, 2026-11-05 vòng 3)
EM vòng 3 ([raw](../../raw/2026-11-05-em-ui-review-round3.md)): "logo bị sai trong trang chính … cứ để logo nguyên … đừng có bỏ ra". PR #29 đã tách logo thành mark (vòng + sen + lá + âm dương) và wordmark ghép lại: **bỏ hẳn**. Quy tắc mới:
- **Luôn dùng TOÀN BỘ logo gốc, không sửa, không tách, không ghép lại:** sen, vòng tròn, chữ script "Viên Phúc / Y Đường", âm dương có lá, tagline cong "Hồi sinh sức khoẻ và sắc đẹp". Ở cỡ nhỏ tagline rất nhỏ: EM chấp nhận, vẫn giữ nguyên. **Ngoại lệ duy nhất (EM 2026-11-05, [raw](../../raw/2026-11-05-em-answers-round4-polish.md)):** icon tab trình duyệt 16–48px dùng mark rút gọn vòng + sen (xem bảng cỡ bên dưới); PWA, apple-touch và mọi chỗ trong app vẫn logo đầy đủ.
- Chữ "Viên Phúc Y Đường" đặt cạnh logo là tùy chọn; hiện app **không** lặp tên/tagline dạng chữ cạnh logo (đăng nhập đã bỏ dòng "HỒI SINH SỨC KHOẺ VÀ SẮC ĐẸP", khung chào trang chủ bỏ tagline chữ). Tên spa luôn có dạng chữ cho screen reader (h1 sr-only ở đăng nhập, `aria-label` của link logo).
- **Nguồn:** `docs for reference/logo Viên phúc y đường transparent.png` (1024², chỉ có raster; nét nằm ở kênh alpha). `scripts/brand/build_logo_assets.py` trace kênh alpha bằng potrace ra **một** file `apps/web/src/assets/brand/emblem.svg` (11.3 KB gzip) và sinh favicon/PWA. `mark.svg`, `wordmark.svg` đã xóa.
- **Component:** `BrandLogo` (`app/BrandLogo.tsx`, class `.brand-logo`): CSS mask, đặt cỡ bằng **chiều rộng** (tỉ lệ 923:943 của logo). Màu: token `--brand-ink` (sáng: nâu đỏ `#7A2620`, tối: vàng `#E0B456`; kiểm ≥ 4.5:1 trên `bg`/`surface`/`surface-2` trong `contrast.test.ts` vì logo có chữ); trong khung nâu đỏ (`.chrome`) tự đổi sang `chrome-accent` `#E3B455` (5.3:1).

| Chỗ | Cỡ (rộng) |
|---|---|
| Sidebar mở (≥ 1024px) | 104px, giữa, trên cùng |
| Sidebar rail (768–1023px, hoặc thu gọn) | 52px |
| Top bar điện thoại | 48px |
| Drawer điện thoại | 96px |
| Khung chào trang chủ | 64px (điện thoại) / 88px |
| Empty state, quầy chưa chọn khách | 56px |
| Đăng nhập / MFA | 144px (điện thoại) / 240px |
| apple-touch, PWA 192/512/maskable | toàn bộ logo vàng trên nền nâu đỏ `#5A1D18` (maskable: logo trong 62% giữa) |
| Tab trình duyệt (`favicon.ico` 16/32/48, `favicon.svg`, `favicon-32.png`, `favicon-16.png`) | **mark rút gọn: vòng tròn + hoa sen** (EM trả lời vòng 4, [raw](../../raw/2026-11-05-em-answers-round4-polish.md): "bản rút gọn chỉ cho icon tab, mọi chỗ khác giữ logo đầy đủ"). Hai phần là chính component trace của logo (lớn nhất = vòng, kế tiếp = sen), sen phóng 1,55× đặt giữa vòng, không vẽ lại. Nét làm dày trước khi thu nhỏ (max filter 31/21/15 px nguồn cho 16/32/48; SVG stroke 14), ô bo 18%, lề 6%. Link + manifest có `?v=ICON_VERSION` (hiện 5; trình duyệt cache favicon rất lâu), `app/icons.test.ts` kiểm mọi link cùng version với script |

- **Câu hỏi cho EM (vẫn mở):** file gốc vector (AI/SVG/PDF)? Bản trace từ PNG 1024² nét tới ~600px.

## App shell (2026-11-05, vòng 3)
- **Desktop/tablet (≥ 768px):** sidebar nâu đỏ (`.sidebar.chrome`) sticky cao 100dvh: logo nguyên vẹn → nav nhóm theo sổ giấy (tên chốt vòng 4, [glossary](../product/glossary.md#tên-màn-hình-và-điều-hướng-em-vòng-4-mục-7-2026-11-05)): **Quầy** (Tổng quan, Ghi lượt khách) · **Khách hàng** (Danh sách khách hàng, Khách lâu chưa quay lại) · **Gói liệu trình** (Gói liệu trình của khách) · **Báo cáo** (Báo cáo kinh doanh) · **Danh mục** (Bảng giá dịch vụ, Mẫu gói liệu trình) · **Cài đặt**. Mục nav = tiêu đề h1 của màn; tab bar điện thoại dùng `NavItem.short`. Mục đang chọn: nền gradient vàng nhạt + thanh vàng 3px bên trái + icon vàng. Mục được chọn theo **URL** (Link fuzzy match): trang chi tiết sáng mục của section nó thuộc về (hồ sơ khách → "Hồ sơ khách", dù mở từ "Theo dõi gói"). ≥ 1024px rộng **288px** (vòng 5: 248 → 264; vòng 6: 264 → 288), thu gọn được (`localStorage vpyd.sidebar`); 768–1023px là rail icon 76px. **Nhãn nav không bao giờ bị cắt và luôn một dòng** (EM vòng 5 mục 2, vòng 6): `white-space: nowrap`, bề rộng tính từ nhãn dài nhất đo trong trình duyệt — "Khách lâu chưa quay lại" (Be Vietnam Pro 500 16px) 184,3px + padding mục 12 + icon 20 + gap 12 + padding 12 = 240,3 + padding sidebar 2×14 + viền 1 = 269,3 + thanh cuộn mảnh ≈ 11 (màn thấp) = 280,3 → **288** (dư ≈ 8px cho khác biệt render font). Thêm/đổi tên mục nav dài hơn → đo lại và chỉnh `--sidebar-w`. Sidebar `overflow: hidden auto` (chỉ cuộn dọc khi nội dung của chính nó cao hơn viewport; `overflow-y: auto` một mình làm trục x cũng cuộn). Lỗi cũ: lớp ẩn nhãn của rail (`nowrap` + `overflow: hidden`) không được gỡ khi sidebar mở, nên nhãn bị cắt. e2e `layout.spec.ts` kiểm 1024/1280/1440 (một dòng, không cắt), thu gọn và rail tablet.
- **Cả trang cuộn như một** (EM vòng 6): không có vùng cuộn dọc lồng trong card trên desktop. `.table-wrap` chỉ `overflow-x: auto` (bảng thật sự rộng hơn card mới cuộn ngang trong card), không `max-height`; header bảng không còn sticky. `.table-wrap` có `position: relative` để mọi phần tử `absolute` bên trong (`.sr-only` ", Nữ" của `CustomerName`) lấy nó làm containing block. **Lỗi vòng 6:** bảng cuộn trong card (`max-height: min(70vh, 720px)`) + `.sr-only` absolute không có tổ tiên positioned → các span đó thoát khỏi vùng cuộn, vị trí tĩnh sâu trong bảng kéo dài document quá `.shell` (đo: /quay 1280×800 dư 427px, 1920×965 dư 92px), sidebar sticky hết containing block nên dừng giữa màn, lộ nền trang bên dưới. e2e `layout.spec.ts` "the page scrolls as one" (1280×800, 1920×965, mọi màn, /quay ≥ 12 lượt): cuộn tới đáy → sidebar phủ đúng viewport, `scrollHeight` document ≤ `.shell` + 2px, không phần tử nào trong `main` cuộn dọc. Quy tắc: phần tử `absolute` đặt trong vùng cuộn phải có containing block nằm trong vùng cuộn đó.
- **Top bar** (sticky 64px): ngày (md+), ô **Tìm khách, mở màn hình… Ctrl K** (command palette), **menu tài khoản**.
- **Menu tài khoản** (`app/UserMenu.tsx`, thay chip avatar + tên cũ mà EM chê "chưa ổn"): nút tròn chữ cái đầu tên (điện thoại/tablet chỉ avatar 44px; ≥ 1024px thêm tên + vai trò, rộng tối đa 160px, cắt "…", mũi tên ▾). Bấm mở panel 320px: avatar + tên + **vai trò · spa**; **Giao diện** (Theo máy / Sáng / Tối, dùng chung một store với `/cai-dat`); **Khóa nhanh** (đăng xuất, đăng nhập lại quay về đúng màn đang mở: `/dang-nhap?next=…`); **Đăng xuất** (đăng xuất, xóa trail, về trang chủ sau khi đăng nhập). Đóng bằng Esc (focus về nút), bấm ra ngoài, Tab ra ngoài. Vai trò ("Chủ spa / Quản lý / Kỹ thuật viên") suy từ permission chỉ để **hiển thị**; `/me` chưa có trường role (câu hỏi cho backend).
- **Điện thoại (< 768px):** top bar có logo; **tab bar** nâu đỏ cố định đáy: Trang chủ · Quầy · Khách · Hôm nay · Thêm (drawer `<dialog>` với toàn bộ nav). Nội dung chừa `--tabbar-h` (68px).
- **Command palette** (`app/CommandPalette.tsx`, chunk lazy): như cũ.
- Nav lấy từ một model (`app/nav.ts` `navFor(me)`); `screenLabel(pathname)` cho tên ngắn của màn không có PageHeader (dùng bởi nút quay lại).

## Điều hướng (2026-11-05 vòng 3)
EM: "bấm vào để xem infor chi tiết rồi nhảy qua trang khác muốn quay về xem thì không biết làm thế nào". Mô hình chọn theo các hệ thống lớn ([ADR-0007](../decisions/ADR-0007-navigation-and-url-state.md)):
- **Breadcrumb = vị trí trong cây** (GOV.UK, Polaris): `Khách hàng › Nguyễn Thị Hà`. Chỉ hiện từ 640px; điện thoại chỉ có nút quay lại (Material "Up", GOV.UK mobile).
- **Nút quay lại = lịch sử thật** (Apple HIG: nhãn là tên màn trước): "‹ Theo dõi gói của khách". Có màn trước trong app → `history.back()` (giữ nguyên bộ lọc + vị trí cuộn). Mở thẳng bằng link/reload không có màn trước → về **cha logic** (crumb cuối). Không bao giờ quay về `/dang-nhap`/`/xac-thuc`. Khi đích quay lại chính là cha, từ 640px chỉ hiện breadcrumb (không lặp hai thứ giống nhau).
- **Trail** (`app/trail.ts`): mỗi entry lịch sử (theo `history.state.__TSR_index`) lưu URL + tiêu đề màn (PageHeader ghi). `sessionStorage` (`vpyd.trail`, ≤ 60 entry, mỗi tab một trail, sống qua reload). Crumb tới một danh sách mở lại **URL cuối cùng** của danh sách đó trong tab (giữ bộ lọc).
- **Trạng thái danh sách nằm trên URL** (tìm, lọc, tab, ngày, trang): `useSearchState` (xem [frontend § State](../architecture/frontend.md#state-management)). Lọc dùng `replace` (không lấp lịch sử), giá trị mặc định không lên URL. Router bật `scrollRestoration`.
- Liên kết chéo (khách → gói → lượt) giữ trail vì mỗi bước là một entry: quay lại từng bước, mỗi bước nói rõ về đâu.
- **`PageHeader`** (`app/PageHeader.tsx`): `title` (chuỗi hoặc node, ví dụ `CustomerName`), `label` (tên dạng chữ cho trail khi title là node), `eyebrow`, `breadcrumbs: {label,to}[]` (không gồm trang hiện tại), `back` (mặc định: có khi có breadcrumbs), `titleId`, `children` = nút thao tác bên phải (xuống dòng khi hẹp). Mọi màn dùng nó (một h1 mỗi màn). Hồ sơ khách: header giữ nguyên lúc đang tải/lỗi để luôn có đường về.

## Mobile patterns (EM vòng 4 mục 3, 2026-11-05)
EM: "UI của mobile đâu thể y như web được … nghiên cứu lại vụ UI/UX cho mobile" ([raw](../../raw/2026-11-05-em-review-round4.md)). Điện thoại (< 640px) có **hình dạng riêng**, không thu nhỏ bản desktop. Nguồn (đã đọc; trang M3/HIG/Polaris render bằng JS nên một số số liệu lấy từ docs thư viện chính thức):
- **M3** navigation bar 3–5 đích, mỗi đích icon + nhãn một dòng, cao 80dp ([Android MDC docs](https://github.com/material-components/material-components-android/blob/master/docs/components/BottomNavigation.md), [m3 specs](https://m3.material.io/components/navigation-bar/specs)); top app bar 64dp, ≤ 2–3 action ([m3](https://m3.material.io/components/top-app-bar/specs)); modal bottom sheet cho tác vụ tập trung (form, bộ lọc), rộng tối đa 640dp, có drag handle ([m3](https://m3.material.io/components/bottom-sheets/guidelines)); vùng chạm 48dp.
- **Apple HIG**: tab bar chỉ cho section cấp cao, luôn hiện, 3–5 tab ([tab bars](https://developer.apple.com/design/human-interface-guidelines/tab-bars)); nút back mang tên màn trước ([navigation bars](https://developer.apple.com/design/human-interface-guidelines/navigation-bars)); sheet medium/large có grabber, luôn có Xong/Hủy rõ ([sheets](https://developer.apple.com/design/human-interface-guidelines/sheets)); vùng chạm 44×44pt.
- **Shopify Polaris**: IndexTable `condensed` thành danh sách xếp chồng trên màn nhỏ; ResourceList = mỗi dòng là thẻ 2–3 dòng có số chính; DataTable cuộn ngang nên tránh trên điện thoại ([index table](https://polaris-react.shopify.com/components/tables/index-table), [data table](https://polaris-react.shopify.com/components/tables/data-table)).
- **Baymard**: bộ lọc trên mobile mở thành lớp phủ có nút Áp dụng ([#1125](https://baymard.com/guidelines/1125-apply-buttons-in-mobile-filtering-interfaces)), hiện số kết quả/bộ lọc đang chọn ([#467](https://baymard.com/guidelines/467-indicating-the-number-of-matches-for-filtering-options)).
- **WCAG 2.2** 2.5.8 (AA) tối thiểu 24px, 2.5.5 (AAA) 44px → app giữ 44px ([w3.org](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)). Thanh cố định đáy cộng `env(safe-area-inset-bottom)` (cần `viewport-fit=cover`, [MDN](https://developer.mozilla.org/en-US/docs/Web/HTML/Viewport_meta_tag)).

**Quy tắc cho mọi màn ở < 640px:**
1. Khung: **top app bar** 56px (logo · tìm nhanh · tài khoản) + **tab bar** đáy 5 đích (Trang chủ · Quầy · Khách · Báo cáo · Thêm), nhãn ngắn một dòng ([glossary](../product/glossary.md#tên-màn-hình-và-điều-hướng-em-vòng-4-mục-7-2026-11-05)). Tiêu đề trang là "large title" ngay dưới top bar, nút quay lại có tên đích ở trên tiêu đề (HIG).
2. **Danh sách, không bảng rộng:** mọi bảng dùng `ResponsiveTable` → thẻ có tên (link mở chi tiết, cả thẻ bấm được), dòng phụ, **một số chính** góc phải (còn buổi, tiền, số ngày vắng, trạng thái), còn lại là cặp nhãn–giá trị. Không cuộn ngang.
3. **Bộ lọc vào sheet:** ô tìm giữ nguyên trên trang (thứ dùng đầu tiên), còn select/ngày/phạm vi vào `BottomSheet` sau nút "Bộ lọc" có số đang chọn (`ResponsiveFilters`). Lọc vẫn áp ngay (URL), nút "Xem kết quả" đóng sheet, "Xóa bộ lọc" đặt lại.
4. **Form mở trong sheet** (`BottomSheet`): ghi liên hệ, bán gói, sửa hồ sơ… Nút chính ghim ở `footer` của sheet, trên vùng home indicator.
5. **Thao tác chính dính đáy** (`StickyActions`) khi form dài trên trang: nằm ngay trên tab bar, nút giãn đều, nút chính ở bên phải.
6. Segmented nhiều lựa chọn (kỳ báo cáo, trạng thái gói) → **một hàng chip cuộn ngang** (`.chip-row`), không xếp 3 hàng.
7. Vùng chạm ≥ 44px; control quan trọng ở nửa dưới màn (tầm ngón cái); không hover-only.

**Component API** (`apps/web/src/components/`, CSS ở `src/mobile.css`; dùng chung cho mọi agent):
| Component | API | Ghi chú |
|---|---|---|
| `usePhone()` / `useMediaQuery(q)` | `boolean`, theo `matchMedia` (live) | jsdom không có `matchMedia` → `false` (test thấy bản desktop) |
| `ResponsiveTable<T>` | `caption`, `rows`, `rowKey`, `columns: Column<T>[]`, `rowClassName?`, `cardsBelow? = 640 | 768 | 1024`\| 768`, `stretchLink?`, `footer?: { table, cards }` | `Column`: `id`, `header`, `cell(row)`, `numeric?` (canh phải, tabular), `card?: "title" \| "subtitle" \| "aside" \| "field" \| "hidden"` (mặc định `field`), `cardLabel?`, `className?`. Desktop: `.data-table` trong `.table-wrap` + `<caption>` ẩn. Điện thoại: `<ul aria-label={caption}>` thẻ `.rt-card` (từ 640px: lưới 2 cột thẻ); chỉ một bản trong DOM. Bảng 6+ cột (gói của khách, khách vắng, chi tiết theo ngày) dùng `cardsBelow={1024}`: trên tablet có rail icon, bảng ép mọi ô xuống 2 dòng |
| `ResponsiveTable` (vòng 5) | thêm `onRowClick?(row)`, `expanded?(row)`, `Column.width?` | `onRowClick`: bấm vào dòng/thẻ (trừ control bên trong) → mở chi tiết; dòng phải có một nút thật làm cùng việc cho bàn phím. `expanded`: nội dung dưới dòng (một `<tr>` colSpan trên desktop, chân thẻ trên điện thoại). Có `width` ở bất kỳ cột nào → `table-layout: fixed` + `<colgroup>`: mọi dòng cùng lưới, chữ dài xuống dòng trong cột của nó |
| `RowMenu` (`components/RowMenu.tsx`, vòng 5) | `label`, `items: { label, onSelect, danger? }[]` | nút "⋯" (icon `dots`, `aria-haspopup=menu`) mở `role=menu`: Enter/Space/↓ mở và focus mục đầu, ↑/↓/Home/End, Esc đóng và trả focus về nút, Tab rời menu, bấm ra ngoài / cuộn / đổi cỡ đóng. Panel là popover thủ công (top layer, không bị `.table-wrap` cắt); không có Popover API thì là panel `fixed`. Quy tắc: một dòng có từ 2 thao tác trở lên → gom vào ⋯, không xếp hàng nút trên mỗi dòng |
| `BottomSheet` | `open`, `onClose`, `title`, `description?`, `children`, `footer?`, `wide?`, `className?` | `<dialog>` + `showModal()` (focus trap, Esc, backdrop); điện thoại: trượt từ đáy, grabber, kéo xuống > 96px để đóng, thân cuộn riêng, footer ghim + safe-area; ≥ 640px: dialog giữa màn 560px (`wide`: 760px). Hook `useModalDialog(open)` dùng lại được |
| `StickyActions` | `children` (nút), `summary?`, `label? = "Thao tác"`, `always?` | `role=group`; < 640px dính trên tab bar, nút giãn đều; ≥ 640px hàng nút canh phải (dính nếu `always`) |
| `ResponsiveFilters` (`features/reports/filters.tsx`) | `label`, `search?`, `children`, `activeCount`, `onReset?` | ≥ 640px = `FilterBar` một hàng; < 640px = ô tìm + nút "Bộ lọc" (badge số) mở `BottomSheet` |
| `DateField` (`components/DateField.tsx`) | `label`, `value` (ISO `yyyy-mm-dd` hoặc `""`), `onChange(iso)`, `min?`, `max?` | luôn hiện `dd/mm/yyyy` bất kể ngôn ngữ trình duyệt (input date gốc hiện `mm/dd/yyyy` trên Chrome tiếng Anh); nút lịch mở picker gốc |

Ví dụ:
```tsx
<ResponsiveTable caption="Gói liệu trình của khách" rows={items} rowKey={(p) => p.id} stretchLink
  columns={[
    { id: "customer", header: "Khách", card: "title", cell: (p) => <Link to=…>{p.customer_name}</Link> },
    { id: "plan", header: "Gói", card: "subtitle", cell: (p) => p.name },
    { id: "left", header: "Còn", numeric: true, card: "aside", cell: (p) => `${p.left}/${p.total}` },
    { id: "expiry", header: "Hạn dùng", cell: (p) => expiryText(p.expires_on, formatDate) },
  ]} />
```

## Quy tắc layout (audit 2026-11-05, vòng 2 + 3)
EM vòng 2: "mấy ô … chỗ to, chỗ nhỏ, chỗ thì bị dính vào nhau"; vòng 3: "responsive … đừng có bị lỗi … check kĩ lại", "đừng có làm thừa thãi … điều chỉnh cho đúng kích thước".
- **Một thang khoảng cách** (4px: 8/12/16/20/24) và **`gap` thay margin**. `.grid` mặc định 1 cột `minmax(0,1fr)`; nút/link/segmented/badge trong grid giữ bề rộng của nó.
- **Ô cùng hàng cao bằng nhau nhờ grid stretch**, không nhờ `min-height` cố định (StatCard 152px cũ gây thừa trống: đã bỏ). Dòng chân chỉ chiếm chỗ khi có nội dung.
- **Hover/nhấn chỉ dùng `transform` / `opacity` / `box-shadow` / `outline` / màu.** Không đổi kích thước/khung/font-weight khi tương tác.
- Badge `nowrap`; **bảng trên điện thoại (< 640px) giữ mỗi ô một dòng và cuộn ngang trong card** (`.table-wrap`), không bẻ tên gói thành từng chữ.
- Panel mở ra nằm ngay dưới dòng đã bấm, chỉ đẩy phần bên dưới.
- **Focus ring không bao giờ che nội dung bên cạnh** (EM vòng 3: vòng focus ô tìm khách che chữ nhãn phía trên). Ba loại, đều vàng `--focus` (≥ 3:1 trên `surface`, `bg`, `surface-2`):
  - mặc định (nút, link): 3px, cách 2px ra ngoài (tổng 5px) — các control cách nhau ≥ 8px;
  - **ô nhập**: 2px sát viền (offset 0), bỏ quầng `box-shadow`; ô nằm **cách nhãn 6px** (rule chung trong `components.css`, áp cho cả nhãn bọc ô và nhãn đứng trước ô; nhãn `flex` và `sr-only` không bị đẩy) → còn 4px trống;
  - control xếp sát nhau (nav, tab bar, segmented, menu, option listbox, cột biểu đồ): vòng vẽ **bên trong** control (offset −3px); link chữ trong dòng (tên khách trên SĐT): 2px, offset 1px.
- **Kiểm tự động** (`e2e/specs/layout.spec.ts`, helper `e2e/support/layout.ts` + `responsive.ts`):
  - hover/nhấn không xê dịch phần tử nào khác (như vòng 2);
  - mọi màn (+ hồ sơ khách tên dài) **quét 320 → 1920px mỗi 80px** (21 bề rộng, resize liên tục như kéo cửa sổ) **và 1280×800 zoom 200 %** (= viewport 640×400): không cuộn ngang, **không phần tử anh em nào chồng lên nhau** (so bounding box từng cặp con in-flow của mọi phần tử; bỏ qua absolute/fixed, inline, sr-only), **không chữ bị cắt** (phần tử clip overflow có chữ trực tiếp mà nội dung lớn hơn hộp, trừ `text-overflow: ellipsis`);
  - **vòng focus không phủ chữ của phần tử khác** ở 1280 và 360px (focus bằng bàn phím mẫu ≤ 16 control đang nhìn thấy mỗi màn). Đã thử ngược: chạy trên bản trước sửa thì bắt đúng lỗi ô tìm khách của EM.
  - Ảnh 360/768/1280/1440 mỗi màn đính kèm vào report.

## Kích thước (size scale, vòng 3)
Audit lại từng component; số trong ngoặc là trước → sau khi đổi.
| Thành phần | Kích thước |
|---|---|
| Control (nút, input, select, segmented ngoài) | cao **44px** (vùng chạm WCAG 2.5.8); segmented: nút 40 trong khung 48; ô tìm khách quầy/khách 52px, chữ 18px |
| Nút quay lại (PageHeader) | cao 36px, vùng chạm mở rộng 44px bằng `::after` (không đẩy tiêu đề) |
| Icon | 16 (trong chữ, breadcrumb) · 18 (trong nút, menu) · 20 (nav, tiêu đề card, lối tắt) · 24 (chỉ sprite gốc) |
| Card | padding 16px (điện thoại) / 16–20px; radius 14; bóng `shadow-1` |
| **Ô số (StatCard)** | padding 14×16, nhãn 14px + icon tròn 28px, số 24/32px; **không min-height** (152px → ~92px), lưới `stat-grid` `auto-fit minmax(180px,1fr)` gap 12 |
| Khung chào trang chủ | padding 16×20 / 20×28, logo 64/88px, h1 24/28px (py 32–40 + h1 36px → gọn ~120px) |
| Ô lối tắt | min-height 72px (88 → 72), icon khung 40px (48 → 40), gap 12 |
| Bảng | ô 12×14px (dòng ~48px), header 13px sentence case (vòng 4; trước 12px chữ hoa) |
| Top app bar điện thoại | 56px + `safe-area-inset-top` (≥ 768px: 64px) |
| Bottom sheet | điện thoại: rộng 100%, cao ≤ 92dvh, bo 16 trên, grabber 36×4; ≥ 640px: 560 / 760px giữa màn |
| Empty state | padding 32px dọc (40 → 32), logo 56px |
| Dialog / panel | panel tài khoản 320px; palette 640px; drawer 320px (≤ 86vw) |
| Avatar | 36px (top bar, danh sách), 44px (panel tài khoản), 56px (cài đặt) |
| Tiêu đề trang | Be Vietnam Pro 600 24/32 màu `text` (2026-11-06; trước Lora 26/34 nâu đỏ + eyebrow chữ hoa, đã bỏ); header cách nội dung 16–20px |

## Element polish toàn app (`apps/web/src/styles.css`)
Một lớp CSS **không nằm trong layer** (thắng utility) cho vài thuộc tính, để mọi màn hình (kể cả màn chỉ dùng utility như `/nhap-so-cu`) cùng một kiểu:
- input/select/textarea: viền `border-strong`, hover `text-muted`, focus viền `primary` + quầng 3px `primary` 14%, `aria-invalid` → viền `danger`, disabled → `surface-2`. Checkbox/radio `accent-color: primary`.
- button/a: transition màu/viền/bóng/transform 120ms; nhấn → dịch 1px.
- `button.bg-primary` / `a.bg-primary`: **phẳng** (2026-11-06; trước: "ánh sơn mài" gradient + `shadow-primary`, đã bỏ cùng token), hover `primary-hover`, nhấn `primary-pressed`.
- Utility mới: `shadow-card` (= `shadow-1`), `shadow-float` (= `shadow-2`), màu `chrome`, `chrome-text(-muted)`, `chrome-accent`, `accent-soft`, `info(-soft)`, `border-strong`.


## Trước / sau
**Áp rulebook: khách hàng + gói + danh mục (`redesign/khach`, 2026-11-06, EM "ok áp dụng cho cả app"):** ảnh 8 màn × 1280/1024/390 × sáng/tối (API giả), scratchpad agent `rollout-khach/before`, `rollout-khach/after`; axe sạch 1280/390 cả hai theme, không cuộn ngang ở cả ba bề rộng. CSS chung các màn này: `features/customers/khach.css` (lazy) dùng lại `.panel`, `.input`, `.field-error`, `.section-label` của `front-desk.css`. Trạng thái gói một nguồn: `features/sales/packageState.ts` (`packageState`: đang dùng / **sắp hết** = còn ≤ 2 buổi hoặc hết hạn trong ≤ 30 ngày / hết hạn / hết buổi / đã hủy; `isClosedPackage`).
| Màn | Trước | Sau |
|---|---|---|
| Hồ sơ khách | card trong card (mỗi gói, mỗi mục lịch sử, mỗi ô tóm tắt là một hộp); icon vàng trên tiêu đề; chấm màu timeline | panel phẳng, dòng cách bằng đường kẻ; tóm tắt 1 dải số; tiêu đề không icon; mục lịch sử có **một** chip (Đã hủy / Đã sửa / Lượt sửa lại), lượt hủy mờ + gạch số tiền; "Sửa lượt" là nút ghost |
| Gói trên hồ sơ | "Đang dùng" cho gói còn 1/5 buổi; gói hết buổi vẫn có "Gia hạn", "Hủy gói và hoàn tiền"; 2–3 nút mỗi dòng | chip một trạng thái (có "Sắp hết"); gói hết buổi/đã hủy gom sau "Gói đã hết / đã hủy (N)", **không thao tác**; ≥ 2 thao tác → menu ⋯, 1 thao tác → nút ghost; form mở ngay dưới dòng; xác nhận = toast trên cùng |
| Lưu ý sức khỏe | chỉ sửa trong sheet "Sửa hồ sơ" | mục riêng trong "Thông tin khách" (chỉ khi API gửi trường), "Thêm/Sửa lưu ý sức khỏe" mở tại chỗ |
| Khách mới / Bán gói / Gia hạn / Hủy gói / Dịch vụ / Mẫu gói | ô 44px chữ 15px, viền nhạt, lỗi không nối `aria-describedby`, form trong hộp viền đứt/nền hồng | `.field` nhãn trên + `.input` 16px viền `border-strong`, gợi ý/lỗi dưới ô nối `aria-describedby` (nhãn không chứa gợi ý), radio = `.choice`, nút chính phẳng `aria-busy`; trùng SĐT báo ngay dưới ô + "Mở hồ sơ" |
| Khách lâu chưa quay lại | 6 cột, ở 1024px bảng cuộn ngang trong card (875 > 678px) | 4 cột cố định (Khách · Vắng · Liên hệ + hẹn lại · Đã liên hệ) vừa khung; chip "liệu trình" vàng → chữ phụ; hẹn = `badge-info`, đến hạn = `badge-warn` |
| Gói liệu trình của khách | chip 4 trạng thái, thanh tiến độ vàng | chip 5 trạng thái (có "Sắp hết"), cột cố định, gói đã đóng mờ |
| Bảng giá / Mẫu gói | mỗi mẫu gói một card, "Ngừng bán" lẫn với đang bán, chữ "Đang tải…" | một panel, dịch vụ/mẫu ngừng bán gom "Ngừng bán (N)", skeleton, trống chỉ bước tiếp theo, "Sửa" ghost |

Lưu ý hành vi: gói **hết buổi** không còn "Hủy gói và hoàn tiền" (trạng thái đóng, rule 7); muốn hoàn tiền gói đã dùng hết thì cần EM quyết định (server vẫn cho phép).

**Vòng 5, Sổ hằng ngày (`feat/staff-daily-book-polish`):** ảnh 1280/1024/390 × sáng/tối + hover/focus + sheet "Giờ & ghi chú" + form thưởng/phạt, thư mục scratchpad của agent `shots/before-*`, `shots/after-*`.
| Chỗ | Trước | Sau |
|---|---|---|
| Chấm công | "✓ P ĐX K" dồn trái, nửa hàng trống; mục chọn = hộp + gạch vàng cong | 4 lựa chọn bằng chữ lấp hàng (2 × 2 trên điện thoại), mục chọn tô màu trạng thái + ✓ |
| Chip trạng thái góc phải | lặp lại lựa chọn ("Đi làm", "Nghỉ có phép (P)") | bỏ; chỉ còn "Chưa chấm công", "Đi muộn n phút" |
| Khen / Phàn nàn | "− 0 +" chữ rời, nút nhỏ | stepper viền, nút 44px, cùng hàng với "+ Ghi thưởng / phạt" trong "Ghi nhận trong ngày" |
| Segmented toàn app, `.choice` | gạch/vạch vàng inset cong ở góc | ô nổi viền 1px (segmented), viền `primary` cả bốn cạnh (`.choice`) |
**Vòng 4 (`feat/ui-polish-mobile`):** ảnh 12 màn × 360/390/768/1280/1440 × sáng/tối, scratchpad agent `r4-polish/before`, `r4-polish/after`, trạng thái tương tác `r4-polish/states`.
| Màn | Trước | Sau |
|---|---|---|
| Điện thoại: bảng (gói, khách vắng, chi tiết ngày, sổ doanh thu) | bảng cuộn ngang trong card | thẻ (tên, số chính, cặp nhãn–giá trị, nút); tablet 2 cột thẻ |
| Điện thoại: bộ lọc | 3–4 select xếp chồng chiếm nửa màn | ô tìm + nút "Bộ lọc (n)" → bottom sheet |
| Kỳ báo cáo, trạng thái gói | segmented xuống 3 hàng | một hàng chip cuộn ngang |
| Hồ sơ khách | một card, "Liệu trình" nhỏ, "(sổ cũ)", "Đã chi 0đ", ô ngày mm/dd/yyyy, "THÁNG 11 NĂM 2026" | 2 card cùng kiểu tiêu đề, dòng gói cố định, "Đã chi" một con số (gói sổ giấy được ghi thành hóa đơn thường), `DateField`, "Tháng 11/2026", sửa/bán/ghi liên hệ trong sheet |
| Tooltip biểu đồ | "30 ngày qua" vỡ 3 dòng | một dòng mỗi chuỗi |
| Bảng giá | nhãn "Nhóm dịch vụ mới" lệch cạnh ô | nhãn trên, ô + nút cùng hàng |
| Tên | Gói bán, Theo dõi gói, Kinh doanh | Mẫu gói liệu trình, Gói liệu trình của khách, Báo cáo kinh doanh |
| Favicon tab | toàn bộ logo, nét mảnh tan ở 16px | mark vòng + sen, nét dày, `.ico` + cache-bust `?v=5` (PWA/apple-touch vẫn logo đầy đủ) |

**Vòng 3 (shell, `feat/ui-shell-r3`):** ảnh trước/sau 360/768/1280/1440 mọi màn + hồ sơ khách + focus ô tìm khách + menu tài khoản + dark, thư mục scratchpad của agent `ui3-shell/before`, `ui3-shell/after`.
| Màn | Trước (#29) | Sau |
|---|---|---|
| Logo | mark + wordmark ghép lại (sidebar, top bar, drawer, empty state, favicon) | logo nguyên vẹn mọi nơi; đăng nhập bỏ dòng tagline chữ lặp |
| Trang chi tiết | link "‹ Danh sách khách" cố định | breadcrumb + nút quay lại theo lịch sử, giữ bộ lọc + cuộn |
| User | avatar + tên + spa (xl) + nút "Khóa nhanh" riêng | menu tài khoản: tên, vai trò, spa, giao diện, Khóa nhanh, Đăng xuất |
| Focus | ring 3px + quầng 3px che nhãn ô tìm | ring 2px sát viền, nhãn cách 6px |
| Trang chủ | khung chào ~ 176px, ô "Hôm nay" 152px | khung chào ~ 120px có logo, ô số ~ 92px |

Ảnh 1280×800 và 390×844, light + dark, dữ liệu `app seed-demo`: thư mục scratchpad của agent `ui2-shots/before`, `ui2-shots/after` và `ui2-shots/after-widths` (360/768/1280/1440) (PR #29). CI đính kèm ảnh từng màn × bề rộng vào report của `layout.spec.ts`.

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
- **Thương hiệu:** *Lora* 600, **chỉ** ở chỗ thương hiệu: đăng nhập/MFA, lời chào ở khung chào trang chủ (2026-11-06: không còn ở tiêu đề màn, tên khách, số liệu, empty state, avatar); có subset Vietnamese.
- Tải font với `subset=vietnamese`, `display=swap`; self-host qua `@fontsource` khi build. Hiện app nội bộ chỉ tải Be Vietnam Pro 400/600 + Lora 600, subset latin + vietnamese, và lần tải đầu tải font sau màn hình đầu tiên (quy tắc ở [frontend § Perf budget](../architecture/frontend.md)). Muốn thêm weight 500/700 thì đo lại LCP trước.
- Thang cỡ chữ: xem § Quy tắc mục 1 (token `--fs-*`); ô nhập 16px. Số tiền dùng `font-variant-numeric: tabular-nums`.
- Ô tìm khách ở quầy dùng 18px để đọc từ xa.

## Spacing, radius, elevation, z-index
- Spacing (bước 4px): `1=4 · 2=8 · 3=12 · 4=16 · 5=20 · 6=24 · 8=32 · 10=40 · 12=48`.
- Radius: `sm 6` (input) · `md 10` (nút, dòng) · `lg 14` (card) · `full` (badge).
- Elevation: `shadow-1` (card) · `shadow-2` (toast, popover, dialog). Không dùng nhiều lớp đổ bóng.
- Z-index: `base 0 · sticky 10 · dropdown 20 · overlay 30 · dialog 40 · toast 50`.
- **Vùng chạm ≥ 44×44px** (WCAG 2.5.8 + dùng trên tablet).

## Breakpoints
`sm 640` (điện thoại ngang) · `md`cardsBelow? = 640 | 768 | 1024` (tablet dọc) · `lg 1024` (tablet ngang, laptop) · `xl 1280` (desktop).
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
| Biểu đồ | đường hiện dần (opacity); split bar `scaleX` | |
| Skeleton | `.skeleton::after` vệt sáng `translateX` 1.4 s | thay spinner và `animate-pulse` |
| Drawer / palette | trượt từ trái / pop-in 8px + scale .98 | |
- `prefers-reduced-motion: reduce` → tắt animation và chuyển động (transform); vẫn giữ transition màu/nền/viền/opacity (tokens.css) để còn phản hồi. `CountUp` đã bỏ (2026-11-06): số hiện ngay.

## Chữ trên giao diện: viết hoa, viết thường (EM vòng 4 mục 4)
EM: "check kĩ mấy cái viết hoa viết thường định dạng mọi thứ lại cho tôi". **Một quy tắc cho mọi chuỗi giao diện:**
1. **Sentence case** (chỉ viết hoa chữ đầu + tên riêng) cho: tiêu đề trang, tiêu đề card, nhãn ô nhập, nút, tab, mục nav, tiêu đề cột bảng, badge, thông báo. Ví dụ: "Bán gói liệu trình", "Lưu ý sức khỏe", "Hẹn liên hệ lại", "Còn 2/12 buổi" (không "Bán Gói", không "còn 2/12 buổi" ở đầu câu).
2. **CHỮ HOA chỉ còn ở nhãn nhóm của sidebar** (`.nav-group-label`, 11px, trong khung nâu đỏ = điều hướng). Eyebrow chữ hoa phía trên h1 **đã bỏ** (2026-11-06, prop `eyebrow` của `PageHeader` đã xóa); `.eyebrow` giờ là nhãn nhóm sentence case 13px muted (command palette). Bảng không có tiêu đề cột chữ hoa.
3. **Tên riêng giữ nguyên cách viết của menu in** (EM chốt 2026-11-05, [raw](../../raw/2026-11-05-em-answers-round4-polish.md): "như trên menu nhé"): tên dịch vụ ("Thư Giãn", "An Kiên Lạc", "Thân Tâm An Định"), **tên gói/mẫu gói** ("Gội đầu Thư Giãn 10 buổi + tặng 2"), tên người, tên spa. Đây là dữ liệu, không phải chữ giao diện: không tự đổi hoa/thường dữ liệu chủ spa nhập, kể cả khi nó đứng trong một câu sentence case ("Bán gói Gội đầu Thư Giãn…").
4. Viết tắt chỉ khi chật và đã quen: "SĐT", "KTV", "HSD", "CK", "DV lẻ", "Buổi LT" (tiêu đề cột bảng; thẻ điện thoại dùng tên đầy đủ qua `cardLabel`).
5. Dấu: "…" (một ký tự) cho placeholder và "Đang tải…"; " · " ngăn các mẩu thông tin cùng dòng; "–" cho khoảng ngày; "×" cho số buổi ("Thư Giãn × 10").
6. Không viết hoa sau dấu "·" trừ tên riêng; nhóm thời gian là "Tháng 11/2026" (không "THÁNG 11 NĂM 2026").

## Định dạng
- Tiền: `Intl.NumberFormat('vi-VN')` + `đ`, không khoảng trắng → `1.500.000đ`. Ô nhập tiền nhận số nguyên, bước 1.000. Trục biểu đồ rút gọn: `1,2 tr` · `850 k`.
- **Ngày luôn `dd/mm/yyyy` có số 0 đầu** (`05/11/2026`), giờ `HH:mm` (24h), múi giờ `Asia/Ho_Chi_Minh`. Thứ viết bằng số như sổ giấy: **"Thứ 5, 05/11/2026"**, **"Chủ nhật, 08/11/2026"** (`lib/format.ts` `dayLabel`, `longDate`) ở top bar, khung chào, bảng chi tiết theo ngày, sổ doanh thu, tooltip biểu đồ. "Hôm nay"/"Hôm qua" cho ngày gần. Nhóm tháng: "Tháng 11/2026" (`monthLabel`).
- **Ô nhập ngày = `DateField`**, không dùng `<input type="date">` hiển thị trực tiếp: Chrome tiếng Anh hiện `mm/dd/yyyy` (ảnh EM vòng 4). `DateField` là ô chữ `dd/mm/yyyy` (gõ "05112026" tự chèn "/"), nút lịch mở picker gốc; giá trị trong code/URL vẫn ISO `yyyy-mm-dd`. Màn `/quay` (appointments agent) còn một ô "Xem ngày" gốc: đã báo agent đó dùng `DateField`.
- SĐT hiển thị `0905 123 456`; lưu chuẩn E.164 ở backend.
- Giới tính: `Nữ` / `Nam` / `Chưa rõ`.

## Component inventory
Primitives dựng trên shadcn/ui (Radix) trong `packages/ui`, theme bằng tokens trên. Mọi component có đủ các state: **default · hover · focus-visible · active · disabled · loading · error** (thêm **empty** cho component chứa dữ liệu).

**Class dùng chung** (`apps/web/src/components.css`, layer `components` nên utility trên cùng phần tử vẫn thắng; viết CSS thay vì chuỗi utility dài vì class trong JSX nằm trong bundle JS):
| Class | Dùng cho |
|---|---|
| `.card`, `.card-link` | card trắng viền + `shadow-1`; bản bấm được nâng nhẹ |
| `.ornament` | hoa văn góc vàng; **chỉ panel đăng nhập** (2026-11-06 bỏ khỏi khung chào trang chủ và mọi màn làm việc) |
| `.btn` + `.btn-primary` / `-secondary` / `-ghost` / `-danger`, `.btn-lg`, `.btn-block`, `.btn-icon` | xem § Quy tắc mục 4 (7 trạng thái, `aria-busy` = đang tải); nút outline cũ (`border-border`) tự có nền `surface` |
| `.badge` + `-ok` / `-warn` / `-danger` / `-muted` / `-info` / `-gold` | một bộ chip trạng thái (§ Quy tắc mục 7); chấm màu + chữ |
| `.data-table` | header dính dưới top bar, kẻ mảnh, hover dòng, `.num` canh phải + `tabular-nums` |
| `.segmented` | bộ lọc / chọn theme / kiểu xem lịch (`aria-pressed`). State: idle chữ muted trên nền `surface-2`; hover nền `surface` 60 %; pressed nền `surface`; **chọn = ô nổi cả khối** (nền `surface` + viền 1px `border-strong` + `shadow-1`, chữ `primary`, cùng độ đậm); focus vòng vàng bên trong; disabled mờ 55 %. Vòng 5 (2026-11-06) **bỏ gạch chân vàng inset**: trên nút bo góc nó cong ở hai đầu, nhìn như ô vỡ. Viền trong suốt sẵn ở idle nên chọn không đổi kích thước |
| `.skeleton` | khối chờ có vệt sáng |
| `.page-title`, `.eyebrow` | tiêu đề màn Be Vietnam Pro 600 24/32; nhãn nhóm sentence case 13px muted (`.gold-rule` đã xóa, không dùng) |
| `.icon` | icon nét 24×24 từ sprite `assets/icons.svg` (`<Icon name>`; `<use href>` tới 1 file hash, 1.3 KB gzip, không thư viện icon) |

| Component | Ghi chú riêng |
|---|---|
| Button (`primary`, `secondary`, `ghost`, `danger`; `sm`/`md`/`lg`) | loading: spinner + giữ nguyên chiều rộng, không bấm được lần 2 |
| Toggle group (hình thức thanh toán, Nữ/Nam) | `aria-pressed`; tối thiểu 44px |
| Input, NumberInput (tiền), PhoneInput, Textarea, Select, Combobox (chọn KTV, dịch vụ) | error: viền `danger` + thông báo bên dưới, `aria-invalid`, `aria-describedby` |
| SearchBox + Listbox (tìm khách) | điều hướng ↑/↓/Enter, `role=listbox/option`, debounce 150ms, state đang tìm / không thấy / lỗi |
| Card, Badge (`debt`, `ok`, `muted`, `star`) | |
| **HealthNote** callout (`.health-note`) | luôn dùng `warning-*`, icon ⚠, tiêu đề "Lưu ý sức khỏe" **sentence case** 13px (vòng 4, trước: chữ hoa); không render gì khi API không trả field |
| **PackageRow** (`.package-row`, hồ sơ khách) | dòng 1: tên gói ← → badge trạng thái; dòng 2: **"Còn X/Y buổi"** đậm · HSD · Quà; dòng 3 (combo): từng dịch vụ X/Y; rồi nút Gia hạn / Hủy gói và hoàn tiền. Vị trí cố định mọi gói (vòng 4: trước "còn 1/10 buổi" + badge nhảy chỗ theo độ dài tên). Gói chuyển từ sổ giấy **không** có nhãn riêng |
| **Card heading** (`.card-head` + `.card-title`) | icon vàng 20px + tiêu đề 18/26 600 bên trái, một nút bên phải; mọi card hồ sơ khách (Thông tin khách, Gói liệu trình, Chăm sóc lại, Lịch sử sử dụng) |
| **Detail list** (`.detail-list`) | cặp nhãn–giá trị 2 cột (nhãn 112px muted), thay chuỗi "0399 … · sinh năm 1968" |
| **Field** (`.field`) | nhãn 14px 500 trên, ô dưới cách 6px; `.filter-field`/`.filter-select` cùng kiểu (trước: 12px muted, lệch với form). Nhãn bọc ô nhập luôn xếp ô xuống dưới (rule chung, sửa "Nhóm dịch vụ mới") |
| **"Đã chi"** (tóm tắt lịch sử) | **một con số** `net_spent` (đã trừ hủy/hoàn, chú thích "đã hoàn …" khi có). EM chốt ([raw](../../raw/2026-11-05-em-carried-over-is-revenue.md)): tiền khách trả ở sổ giấy là doanh thu thật, backend ghi gói chuyển từ sổ giấy thành hóa đơn thường theo ngày gốc, nên nó nằm sẵn trong "Đã chi", sổ doanh thu và báo cáo. **Không** tách phần giấy/app, **không** đánh dấu gói chuyển từ sổ giấy ở đâu cả |
| **CartLine** (`.cart-line`, /quay) | dòng 1: dịch vụ + nguồn · giá (dòng trừ buổi để trống) · ✕ `btn-ghost btn-icon` "Bỏ dòng …"; dòng 2: "KTV" + select 44px rộng hết (mọi dòng cùng lưới). Các dòng cách nhau đường kẻ, không khung |
| **Checkout** (`.checkout`, /quay) | cuối panel giỏ, **dính đáy viewport** (điện thoại: trên tab bar): "Phải trả" + số 24px tabular (giá gốc gạch khi có giảm), ghi chú khi 0đ, nút chính `btn-primary btn-lg btn-block` "Lưu lượt khách" (không bao giờ bị cắt; `aria-busy` khi lưu) |
| **MoneySummary** | phải trả, đã trả; số `tabular-nums` |
| **StatCard** (`features/reports/ui.tsx`) | nhãn + icon tròn vàng 28px, số 24px hiện ngay (Be Vietnam Pro; bỏ `CountUp` + Lora 2026-11-06), không min-height, chân: `Delta` (▲/▼ % so với hôm qua, màu + chữ); bản `accent` nâu đỏ cho "Thực thu"; loading: `StatSkeleton` |
| **LineChart** (`features/reports/LineChart.tsx`) | SVG tự viết (không thư viện): 1 trục, đường 2px nâu đỏ + vùng 10% cho kỳ này, đường xám `border-strong` cho kỳ trước (dạng emphasis, đã chạy validator dataviz cả light/dark); lưới hairline; nhãn giá trị chỉ ở điểm cuối; crosshair + tooltip (giá trị đậm trước, tên sau, line key) khi rê chuột và khi focus ← → Home End; nhãn trục X ≤ 7 (≤ 4 dưới 640px); `role="img"` + bảng chi tiết bên dưới là bản thay thế; đổi kỳ: khung cũ mờ (`.is-refreshing`) |
| **SplitBar** (`features/reports/charts.tsx`) | thanh chia phần (tiền mặt/CK, gói/lẻ) + chú thích có số và % |
| **FilterBar / SearchField / FilterSelect** (`features/reports/filters.tsx`) | một hàng lọc phía trên nội dung, xuống dòng trên điện thoại; ô tìm có icon, nút xóa, debounce 300 ms, ≥ 2 ký tự; select native có nhãn; mọi ô viền `border-strong` |
| **FollowUpForm** + `.form-dialog` + `.choice` | dialog giữa màn (điện thoại: gần full); kết quả là hàng radio 44px, mục chọn = nền `primary-soft` + **viền 1px `primary` cả bốn cạnh** (vòng 5 bỏ vạch vàng 3px bên trái, cùng lý do như segmented), không đổi chữ đậm; hover viền `border-strong`; disabled mờ |
| **AttendanceChoice** (`features/staff/AttendanceChoice.tsx`, `.att-choice`) | chấm công một người một ngày trong Sổ hằng ngày: 4 lựa chọn **bằng chữ** (Đi làm · Nghỉ có phép `P` · Nghỉ đột xuất `ĐX` · Nghỉ không phép `K`; mã là chip phụ, tooltip = quy tắc), 4 cột bằng nhau lấp hàng, **2 × 2 khi thẻ < 560px** (container query). `role=radiogroup` + `role=radio`/`aria-checked`, một tab stop (mục đang chọn), ←/→/↑/↓/Home/End **chỉ di chuyển focus**, Space/Enter mới chọn (mỗi lần chọn là một lần lưu, đi qua 3 mục không được ghi 3 lần). State: chọn = nền màu trạng thái (success/info/warning/danger soft) + viền cùng màu + icon ✓ + chữ đậm (màu không là tín hiệu duy nhất); hover viền `border-strong`; focus vòng vàng bên trong; **đang lưu** = mục vừa bấm hiện đã chọn và nhấp nháy nhẹ (reduced-motion: đứng yên 75 %), các mục khác `aria-disabled` (giữ focus) + live "Đang lưu…"; **lỗi** = quay về giá trị server + "Chưa lưu được: …" ngay trên thẻ; disabled (tháng đã chốt, ngày tương lai) = `disabled`, mục đã chọn vẫn hiện màu |
| **Counter** (stepper, `features/staff/Counter.tsx`) | nhãn + một khối viền `[− n +]`, nút 44 × 44 icon `minus`/`plus`, số `tabular-nums` giữa hai vạch; hover/pressed/focus bên trong/disabled (0 hoặc 99, tháng chốt); khi đang lưu dùng `aria-disabled` thay `disabled` để không mất focus khi bấm + liên tục |
| **Thẻ Sổ hằng ngày** (`.book-card`) | 3 dải giống nhau mọi thẻ: (1) tên (xuống dòng, `overflow-wrap:anywhere`) + vai trò · giờ đến/về, bên phải chỉ badge **có thêm thông tin** ("Chưa chấm công", "Đi muộn n phút") + nút "Giờ & ghi chú"; **không** còn chip lặp lại trạng thái đã chọn; (2) AttendanceChoice; (3) "Ghi nhận trong ngày": Khen, Phàn nàn, "+ Ghi thưởng / phạt" một hàng (nút dạt phải; điện thoại: full-width), dưới là các dòng thưởng/phạt (badge · số tiền đậm · lý do) và ghi chú. Mọi nút trong thẻ là `btn-secondary` 44px (thẻ không có hành động chính, chấm công mới là thao tác chính). Thẻ chưa chấm: viền `warning-border` 1px (bỏ vạch trái 4px). Đầu trang: ô "Ngày" và "Tiến độ" cùng dạng nhãn + khối 44px nên thẳng hàng |
| **Tabs** (`.tabs`) | `role=tablist`, tab chọn = gạch chân vàng 3px `inset box-shadow` |
| **Timeline** (lịch sử khách) | đường dọc gradient vàng, chấm theo loại (lượt nâu đỏ, mua gói vàng, hủy xám/đỏ), nhóm theo tháng |
| EmptyState (logo nguyên vẹn 56px + tiêu đề Be Vietnam Pro 600 + câu hướng dẫn bước tiếp theo), ErrorState ("Thử lại") | |
| **Tip** (`features/front-desk/Tip.tsx`, `.tip`) | toggletip: nút tròn 28px vẽ trong vùng chạm 44px, bấm/chạm/Enter mở (không mở khi hover, để không phủ nội dung), Esc đóng (không lan ra Esc của trang), nội dung là `aria-describedby` của nút. Dùng cho phím tắt quầy, ghi chú giờ khóa sổ |
| Dialog / Sheet (điện thoại) | `<dialog>` + `showModal()` (`Modal` trong AppShell): focus trap, Esc, backdrop |
| ReasonDialog | bắt buộc nhập lý do (hủy, giảm giá, cho dùng gói hết hạn) |
| Toast | `role=status`; 4 giây; phẳng (không sọc trái); **trên cùng giữa màn** (`top: 72px + safe-area`, 2026-11-06) để không che thanh lưu dính đáy; lỗi nền `danger` |
| AppShell, CommandPalette, UserMenu | xem § App shell |
| **PageHeader** (`app/PageHeader.tsx`) | xem § Điều hướng |
| **CustomerName** (`app/CustomerName.tsx`) | tên + **chip giới tính ngay cạnh tên** (EM vòng 3: "giới tính để bên cạnh tên cho dễ phân biệt"): "Nữ" nền `primary-soft` chữ `primary`, "Nam" nền `info-soft` chữ `info`, `showUnknown` → "Chưa rõ" xám (chỉ ở tiêu đề hồ sơ). Chữ, không chỉ màu; screen reader đọc "Nguyễn Thị Hà, Nữ". Inline: dùng trong link, h1, ô bảng. Đã dùng ở: tìm khách (quầy + /khach-hang), khách gần đây, tiêu đề hồ sơ, ô khách của bảng báo cáo (khi dòng có `gender`) |
| **ThemePicker** (`app/ThemePicker.tsx`) | segmented Theo máy / Sáng / Tối; `useSyncExternalStore` trên `prefs.ts` nên mọi picker đang hiện luôn khớp |
