---
title: Design system — internal app (R1)
updated: 2026-11-05
sources: [raw/2026-11-05-em-ui-review-round3.md, "docs for reference/logo Viên phúc y đường transparent.png", "docs for reference/Menu mặt trước - mới.png", product/personas.md, raw/2026-11-05-em-brand-colors-and-demo-data.md, raw/2026-11-05-em-management-views-logo-ui.md]
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
- **Desktop/tablet (≥ 768px):** sidebar nâu đỏ (`.sidebar.chrome`) sticky cao 100dvh: logo nguyên vẹn → nav nhóm theo sổ giấy (tên chốt vòng 4, [glossary](../product/glossary.md#tên-màn-hình-và-điều-hướng-em-vòng-4-mục-7-2026-11-05)): **Quầy** (Tổng quan, Ghi lượt khách) · **Khách hàng** (Danh sách khách hàng, Khách lâu chưa quay lại) · **Gói liệu trình** (Gói liệu trình của khách) · **Báo cáo** (Báo cáo kinh doanh) · **Danh mục** (Bảng giá dịch vụ, Mẫu gói liệu trình) · **Cài đặt**. Mục nav = tiêu đề h1 của màn; tab bar điện thoại dùng `NavItem.short`. Mục đang chọn: nền gradient vàng nhạt + thanh vàng 3px bên trái + icon vàng. Mục được chọn theo **URL** (Link fuzzy match): trang chi tiết sáng mục của section nó thuộc về (hồ sơ khách → "Hồ sơ khách", dù mở từ "Theo dõi gói"). ≥ 1024px rộng 248px, thu gọn được (`localStorage vpyd.sidebar`); 768–1023px là rail icon 76px.
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
| Tiêu đề trang | Lora 26/34; eyebrow 12px chữ hoa giãn 0.14em; header cách nội dung 16–20px |

## Element polish toàn app (`apps/web/src/styles.css`)
Một lớp CSS **không nằm trong layer** (thắng utility) cho vài thuộc tính, để mọi màn hình (kể cả màn chỉ dùng utility như `/nhap-so-cu`) cùng một kiểu:
- input/select/textarea: viền `border-strong`, hover `text-muted`, focus viền `primary` + quầng 3px `primary` 14%, `aria-invalid` → viền `danger`, disabled → `surface-2`. Checkbox/radio `accent-color: primary`.
- button/a: transition màu/viền/bóng/transform 120ms; nhấn → dịch 1px.
- `button.bg-primary` / `a.bg-primary`: "ánh sơn mài" (gradient trắng 10% → 0) + `shadow-primary`.
- Utility mới: `shadow-card` (= `shadow-1`), `shadow-float` (= `shadow-2`), màu `chrome`, `chrome-text(-muted)`, `chrome-accent`, `accent-soft`, `info(-soft)`, `border-strong`.


## Trước / sau
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
| Đếm số | `CountUp` 700 ms ease-out (rAF), số `tabular-nums` nên không xô chữ | screen reader đọc giá trị cuối |
| Biểu đồ | đường hiện dần (opacity); split bar `scaleX` | |
| Skeleton | `.skeleton::after` vệt sáng `translateX` 1.4 s | thay spinner và `animate-pulse` |
| Drawer / palette | trượt từ trái / pop-in 8px + scale .98 | |
- `prefers-reduced-motion: reduce` → tắt mọi transition/animation (tokens.css), `CountUp` hiện ngay giá trị cuối.
- `prefers-reduced-motion: reduce` → tắt mọi transition/animation, chỉ đổi trạng thái tức thì.

## Chữ trên giao diện: viết hoa, viết thường (EM vòng 4 mục 4)
EM: "check kĩ mấy cái viết hoa viết thường định dạng mọi thứ lại cho tôi". **Một quy tắc cho mọi chuỗi giao diện:**
1. **Sentence case** (chỉ viết hoa chữ đầu + tên riêng) cho: tiêu đề trang, tiêu đề card, nhãn ô nhập, nút, tab, mục nav, tiêu đề cột bảng, badge, thông báo. Ví dụ: "Bán gói liệu trình", "Lưu ý sức khỏe", "Hẹn liên hệ lại", "Còn 2/12 buổi" (không "Bán Gói", không "còn 2/12 buổi" ở đầu câu).
2. **CHỮ HOA chỉ cho hai loại nhãn nhỏ:** eyebrow phía trên h1 (`.eyebrow`, 12px, giãn 0.14em) và nhãn nhóm của sidebar (`.nav-group-label`, 11px). Chữ hoa là do CSS (`text-transform`), chuỗi gốc vẫn sentence case (screen reader không đánh vần). Bảng **không** còn tiêu đề cột chữ hoa (trước: 12px uppercase).
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
| **HealthNote** callout (`.health-note`) | luôn dùng `warning-*`, icon ⚠, tiêu đề "Lưu ý sức khỏe" **sentence case** 13px (vòng 4, trước: chữ hoa); không render gì khi API không trả field |
| **PackageRow** (`.package-row`, hồ sơ khách) | dòng 1: tên gói ← → badge trạng thái; dòng 2: **"Còn X/Y buổi"** đậm · HSD · Quà; dòng 3 (combo): từng dịch vụ X/Y; rồi nút Gia hạn / Hủy gói và hoàn tiền. Vị trí cố định mọi gói (vòng 4: trước "còn 1/10 buổi" + badge nhảy chỗ theo độ dài tên). Gói chuyển từ sổ giấy **không** có nhãn riêng |
| **Card heading** (`.card-head` + `.card-title`) | icon vàng 20px + tiêu đề 18/26 600 bên trái, một nút bên phải; mọi card hồ sơ khách (Thông tin khách, Gói liệu trình, Chăm sóc lại, Lịch sử sử dụng) |
| **Detail list** (`.detail-list`) | cặp nhãn–giá trị 2 cột (nhãn 112px muted), thay chuỗi "0399 … · sinh năm 1968" |
| **Field** (`.field`) | nhãn 14px 500 trên, ô dưới cách 6px; `.filter-field`/`.filter-select` cùng kiểu (trước: 12px muted, lệch với form). Nhãn bọc ô nhập luôn xếp ô xuống dưới (rule chung, sửa "Nhóm dịch vụ mới") |
| **"Đã chi"** (tóm tắt lịch sử) | **một con số** `net_spent` (đã trừ hủy/hoàn, chú thích "đã hoàn …" khi có). EM chốt ([raw](../../raw/2026-11-05-em-carried-over-is-revenue.md)): tiền khách trả ở sổ giấy là doanh thu thật, backend ghi gói chuyển từ sổ giấy thành hóa đơn thường theo ngày gốc, nên nó nằm sẵn trong "Đã chi", sổ doanh thu và báo cáo. **Không** tách phần giấy/app, **không** đánh dấu gói chuyển từ sổ giấy ở đâu cả |
| **CartLine** | dịch vụ, nguồn (trừ liệu trình / dịch vụ lẻ), KTV (bắt buộc), giá, bỏ dòng |
| **MoneySummary** | phải trả, đã trả; số `tabular-nums` |
| **StatCard** (`features/reports/ui.tsx`) | nhãn + icon tròn vàng 28px, số 24px đếm lên, không min-height, chân: `Delta` (▲/▼ % so với hôm qua, màu + chữ); bản `accent` nâu đỏ cho "Thực thu"; loading: `StatSkeleton` |
| **LineChart** (`features/reports/LineChart.tsx`) | SVG tự viết (không thư viện): 1 trục, đường 2px nâu đỏ + vùng 10% cho kỳ này, đường xám `border-strong` cho kỳ trước (dạng emphasis, đã chạy validator dataviz cả light/dark); lưới hairline; nhãn giá trị chỉ ở điểm cuối; crosshair + tooltip (giá trị đậm trước, tên sau, line key) khi rê chuột và khi focus ← → Home End; nhãn trục X ≤ 7 (≤ 4 dưới 640px); `role="img"` + bảng chi tiết bên dưới là bản thay thế; đổi kỳ: khung cũ mờ (`.is-refreshing`) |
| **SplitBar** (`features/reports/charts.tsx`) | thanh chia phần (tiền mặt/CK, gói/lẻ) + chú thích có số và % |
| **FilterBar / SearchField / FilterSelect** (`features/reports/filters.tsx`) | một hàng lọc phía trên nội dung, xuống dòng trên điện thoại; ô tìm có icon, nút xóa, debounce 300 ms, ≥ 2 ký tự; select native có nhãn; mọi ô viền `border-strong` |
| **FollowUpForm** + `.form-dialog` + `.choice` | dialog giữa màn (điện thoại: gần full); kết quả là hàng radio 44px, mục chọn = nền `primary-soft` + vạch vàng trong (không đổi chữ đậm) |
| **Tabs** (`.tabs`) | `role=tablist`, tab chọn = gạch chân vàng 3px `inset box-shadow` |
| **Timeline** (lịch sử khách) | đường dọc gradient vàng, chấm theo loại (lượt nâu đỏ, mua gói vàng, hủy xám/đỏ), nhóm theo tháng |
| EmptyState (logo nguyên vẹn 56px + câu hướng dẫn), ErrorState ("Thử lại") | |
| Dialog / Sheet (điện thoại) | `<dialog>` + `showModal()` (`Modal` trong AppShell): focus trap, Esc, backdrop |
| ReasonDialog | bắt buộc nhập lý do (hủy, giảm giá, cho dùng gói hết hạn) |
| Toast | `role=status`; lỗi dùng `role=alert`; 3–4 giây; không che nút Lưu; nổi trên tab bar điện thoại |
| AppShell, CommandPalette, UserMenu | xem § App shell |
| **PageHeader** (`app/PageHeader.tsx`) | xem § Điều hướng |
| **CustomerName** (`app/CustomerName.tsx`) | tên + **chip giới tính ngay cạnh tên** (EM vòng 3: "giới tính để bên cạnh tên cho dễ phân biệt"): "Nữ" nền `primary-soft` chữ `primary`, "Nam" nền `info-soft` chữ `info`, `showUnknown` → "Chưa rõ" xám (chỉ ở tiêu đề hồ sơ). Chữ, không chỉ màu; screen reader đọc "Nguyễn Thị Hà, Nữ". Inline: dùng trong link, h1, ô bảng. Đã dùng ở: tìm khách (quầy + /khach-hang), khách gần đây, tiêu đề hồ sơ, ô khách của bảng báo cáo (khi dòng có `gender`) |
| **ThemePicker** (`app/ThemePicker.tsx`) | segmented Theo máy / Sáng / Tối; `useSyncExternalStore` trên `prefs.ts` nên mọi picker đang hiện luôn khớp |
