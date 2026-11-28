---
title: Thiết kế UI — Vòng quay may mắn
updated: 2026-11-28
sources: [raw/2026-11-27-em-request-lucky-draw.md]
related: [../product/specs/lucky-draw.md, ../architecture/lucky-draw.md, design-system.md, ../architecture/frontend.md]
---

# UI: Vòng quay may mắn

Spec: [lucky-draw](../product/specs/lucky-draw.md) (US1–US7, §5). Hợp đồng API + web: [architecture/lucky-draw](../architecture/lucky-draw.md) §3–4. Code: `apps/web/src/features/lucky-draw/`. Theo rulebook + token của [design-system](design-system.md). Từ bản HTML của EM chỉ giữ **ảnh vòng quay** (mặt quay + khung vàng); mọi thứ khác là card trắng, chip, nút, bảng của app (EM: "AI quá mức … màu mè"). Không foil, ribbon, sen góc, nút vàng bóng, ảnh minh họa giải, font riêng, pháo hoa, âm thanh.

## 1. Bố cục theo bề rộng
| Màn | 1280 (sidebar 288) | 1024 (sidebar 288, nội dung ~700px) | 390 |
|---|---|---|---|
| `/vong-quay` | `PageHeader` "Vòng quay may mắn" + `btn-primary` "Tạo chương trình"; một card bảng: Tên chương trình (link `name-link`) · Ngày tạo · Người tham gia · Đã trao / tổng suất · Trạng thái | như 1280 | `ResponsiveTable` thẻ: tên (cả thẻ bấm được), ngày, chip trạng thái góc phải |
| `/vong-quay/$drawId` | header (breadcrumb, Trình chiếu, ⋯ Đổi tên / Xóa chương trình) → card **Tổng kết** khi *Đã xong* → hai cột 5 : 6: **sân khấu** \| Giải thưởng + Danh sách trúng thưởng → **Người tham gia** rộng hết | **một cột**: sân khấu → giải → trúng thưởng → người tham gia | một cột như 1024; vòng quay rộng hết card (sân khấu không đệm ngang), chip lọc cuộn ngang trong hàng của nó (`.chip-row`) |

- Hai cột theo **bề rộng nội dung** (container query `.ld-page` ≥ 880px), không theo viewport 1024 như spec §5 viết: ở 1024 sidebar còn để ~700px, hai cột làm card giải và bảng trúng thưởng gãy chữ. Lệch có chủ ý, xem §8.
- Trạng thái chương trình: `badge-info` Chưa quay · `badge-ok` Đang quay · `badge-muted` Đã xong (spec §5).

## 2. Sân khấu: ngoại lệ "chrome" duy nhất của vùng làm việc
- Card `.ld-stage.chrome`: nền `--gradient-chrome-glow` + `--gradient-chrome` (`chrome-bg` → `chrome-bg-2`), viền vàng mảnh trong (`--mix-gold-hairline`) + `shadow-2`, chữ `chrome-text` / `chrome-text-muted`, `--focus` = `chrome-accent` (từ `.chrome`). Lý do: khung ảnh có quầng sáng ấm nướng sẵn, chỉ đẹp trên nâu đỏ; trên nền kem quầng thành vệt bẩn.
- Quầng của ảnh khung là một **hình chữ nhật** không về trong suốt ở mép (alpha ~13 %), nên trên sân khấu lộ khung hộp. Sửa bằng mask `--texture-wheel-fade` (token mới, `radial-gradient(closest-side, black 93%, transparent)`, như mọi `texture-*` dùng chung hai theme): quầng mờ dần trong hình bầu dục, nét vẽ (kim, sen bốn phía) giữ nguyên.
- Trong sân khấu, theo thứ tự: "Giải đang quay" + tên giải (`--fs-lg`) + chip đối tượng + "Còn x/y suất" → vòng quay → nút **Quay** / **Quay tiếp** (`btn-primary btn-lg`) → **một dòng trạng thái** (cao giữ chỗ 44px): "Đang quay…", câu vì sao không quay được, hoặc lỗi. **Không còn thẻ kết quả dưới vòng quay** (EM 2026-11-28: "pop up thông báo người chiến thắng ấy chứ đừng có hiện người thắng ngay dưới vòng quay nữa"): người trúng chỉ hiện ở popup (§3b); trình đọc màn hình nghe "X trúng Y" từ dòng trạng thái (`aria-live`, chữ `sr-only`).
- Nút trên nền chrome: `btn-primary` thành **kem** (`chrome-text`) chữ `chrome-bg` (hover / nhấn pha `chrome-accent` 14 / 28 %), `btn-secondary` thành viền kem trong suốt. Cùng hình, cùng 7 trạng thái như mọi nơi; không vàng bóng.

## 3. Vòng quay (`Wheel.tsx`) và ngoại lệ chuyển động (rulebook §10)
- Hai ảnh WebP (mặt 376², 33 KB; khung 466 × 481, 64 KB), import qua Vite, nằm cùng chunk màn, không ảnh LCP của trang nào khác. Đặt theo tỉ lệ crop gốc: mặt `left 8.798%`, `top 13.929%`, `width 80.687%`, `height 78.170%` trong khung `aspect-ratio: 466 / 481`, rộng `min(100%, 420px)`. Khung `role="img"` "Vòng quay tám cánh sen", ảnh `alt=""`. Không phải control.
- Chuyển động (2026-11-28, **như bản HTML của EM**): bấm Quay → ngay lập tức **một transition** +6 vòng tới giữa một trong 8 cánh (`sector × 45 + 22,5°`, cánh bốc lúc bấm bằng `crypto.getRandomValues`, chỉ trang trí, không liên quan người trúng: ADR-0025) trong **4100 ms** `cubic-bezier(.14,.72,.12,1)`. Lượt **chỉ đáp** khi **cả hai**: vòng đã dừng (`transitionend`, hoặc hẹn giờ +400 ms khi tab ẩn) **và** server đã trả lời; lúc đó mới đưa kết quả vào cache (người trúng, số suất, danh sách cùng lúc) và mở popup. Server trả lời sau khi vòng dừng: nút vẫn `aria-busy`, "Đang quay…" tới khi có. Không còn vòng lặp chờ, đọc lại góc, hay "quay về chỗ cũ" (bản trước).
- **Vì sao là ngoại lệ của §10** (150–250 ms, chỉ cho đổi trạng thái): ở đây chuyển động *là nội dung* của khoảnh khắc quay thưởng trước khán giả; vẫn chỉ `transform`, không đổi layout, không lặp. Mọi phần khác của màn theo §10.
- **`prefers-reduced-motion`: vẫn quay**, nhẹ hơn: 2 vòng trong 2500 ms `cubic-bezier(.33,0,.2,1)` (EM: vòng quay phải quay; cú quay là nội dung — trên staging máy EM báo reduce nên bản trước đứng yên, EM thấy "chỉ là hình vòng quay cứng"). Không có trường hợp nào vòng đứng yên. Transition đặt inline `!important` để vượt luật toàn cục của `tokens.css` (luật đó bỏ transition `transform` khi reduce).
- Bấm hai lần: nút `aria-busy` trong suốt lượt (nhãn giữ "Quay"), lần bấm thứ hai bị bỏ qua; mất mạng / 5xx: vòng dừng ở chỗ nó dừng, "Chưa quay được, kiểm tra mạng rồi bấm “Quay” lại.", lần bấm sau gửi **cùng** `Idempotency-Key` (AC4.6); bị từ chối rõ (409…): hiện câu, bỏ key. Rời màn giữa lượt: kết quả đã lưu vẫn vào cache. Vòng của trình chiếu dùng chung góc.

## 3b. Popup người trúng (`WinnerDialog`, EM 2026-11-28)
EM: "mỗi lần 1 người trúng thưởng thì pop up lên cái thông báo chính giữa màn hình".
- Mở **khi lượt đáp** (vòng đã dừng + có câu trả lời). `<dialog>` modal **giữa màn ở mọi bề rộng** (điện thoại cũng giữa, không phải bottom sheet), rộng `min(100% − 32px, 30rem)`; trên trình chiếu mở đè lên (sau trong top layer) với chữ cỡ TV (`.is-big`, 46rem).
- Nội dung, canh giữa, trên nền chrome (`chrome-bg` → `chrome-bg-2`, viền vàng mảnh trong): "Chúc mừng" + viên "Lượt N" (viền + chữ `chrome-accent`) → đường vàng có nụ sen (`--texture-flourish`, như dưới tiêu đề trang) → **tên** rất to (`--fs-display` 36px, TV `--fs-display-tv` 56px, token mới) → chip giới tính → "trúng" → **tên giải** (`--fs-xl`, vàng `chrome-accent`, TV `--fs-display`) + mô tả → "Còn x/y suất".
- Nút: chính `btn-lg` kem **"Quay tiếp"** (đóng popup và quay ngay theo giải mặc định); lượt làm chương trình xong → **"Xem tổng kết"** thay chỗ (mở sheet tổng kết, không mở cả hai cùng lúc); "Vắng mặt, trả giải" (`btn-danger-outline`, vào sheet xác nhận cũ); "Đóng" (`btn-ghost` chữ kem). Esc và bấm nền đóng; focus vào nút chính khi mở, về nút "Quay" khi đóng.
- Vào: fade + scale 0,96 → 1 trong 220 ms (`@starting-style`); reduced motion chỉ fade. Không pháo hoa, foil, glow lặp, emoji.

## 4. Giải, trúng thưởng, người tham gia
- **Giải thưởng**: mỗi giải là một ô chọn (`aria-pressed`, hình `.choice`: viền `border-strong`, chọn = `primary-soft` + viền `primary` + ✓): tên, mô tả, chip đối tượng (Nữ / Nam / Cả nam và nữ = `gender-chip`), "Còn x/y suất"; không quay được → nền `surface-2`, chip `badge-muted` "Hết suất" / "Không còn người phù hợp", `aria-disabled`. Tự chọn giải đầu tiên quay được (`defaultPrize`); giải đang chọn hết suất thì nhảy sang giải kế. ⋯ "Sửa giải" / "Xóa giải" (không có "Xóa" khi giải đã có kết quả). Khóa chọn giải trong lúc quay.
- **Danh sách trúng thưởng** (EM 2026-11-28: "làm đẹp hơn … đừng làm kiểu cái danh sách"): **nhóm theo giải** theo thứ tự giải; đầu nhóm = tên giải + chip đối tượng + "Đã trao a/b"; mỗi người trúng là **một thẻ** (`WinRow`): số lượt trong vòng tròn viền vàng nền kem (`accent` / `accent-soft`, chữ `accent-text`), tên `--fs-md` 600, chip giới tính, "lúc 07:43" nhỏ muted, ⋯ "Vắng mặt, trả giải". Đã trả giải: thẻ nền `surface-2`, tên muted (không gạch ngang, vẫn đọc rõ) + chip "Vắng mặt", không ⋯ (ô trống 44px giữ chiều cao). Lượt vừa đáp **lóe nền vàng nhạt 1,6 s** (không có khi reduced motion). Điện thoại: cùng thẻ, một cột. Cùng `WinRow` ở sheet tổng kết và trình chiếu. Nút "Tải danh sách (CSV)" và "Xem tổng kết" giữ nguyên.
- **Người tham gia**: số đếm cạnh tiêu đề; "Thêm người", "Dán danh sách" (ẩn khi đủ 2.000); chip lọc Tất cả / Nữ / Nam / Đã trúng **kèm số** (segmented `.chip-row`) + ô tìm không dấu (`fold`), cả hai trên URL (`?loc=&q=`, parse trong chunk, không `validateSearch`); bảng # · Họ tên · Giới tính · Trạng thái (`badge-info` Chưa quay / `badge-ok` Đã trúng: giải / `badge-muted` Vắng mặt) · ⋯ Sửa / Xóa chỉ cho người chưa có kết quả. Điện thoại: thẻ hai dòng (tên + ⋯; giới tính · trạng thái). 100 dòng đầu + "Hiện thêm (n dòng nữa)".

## 5. Sheet và hộp thoại (`BottomSheet`)
| Sheet | Nội dung |
|---|---|
| Tạo chương trình / Đổi tên | một ô "Tên chương trình" (≤ 80, gợi ý "Ví dụ: Tri ân 20/11."); tạo xong mở thẳng chương trình (không GET lại); đổi tên gửi `If-Match` |
| Xóa chương trình | câu AC1.4, `btn-danger` "Xóa chương trình"; xong về danh sách |
| Thêm / sửa giải | Tên giải · **Dành cho** (segmented Cả nam và nữ / Chỉ nữ / Chỉ nam; khóa khi giải đã có kết quả, kèm câu vì sao) · Số suất (1–1.000, không nhỏ hơn số đã trao) · Mô tả (≤ 250) |
| Thêm / sửa người | Họ tên · Giới tính (segmented Nữ / Nam, **không chọn sẵn**, bắt buộc). Thêm xong giữ sheet mở, xóa ô, focus về Họ tên, "Đã thêm …"; nút trái "Xong" |
| Dán danh sách (rộng) | textarea + gợi ý định dạng; xem trước sống: "Sẽ thêm n người (x nữ, y nam)", hộp `.alert` liệt kê từng dòng lỗi (tối đa 20 + "và n dòng khác"), hộp `.callout` trùng tên (vẫn thêm); còn lỗi thì nút `aria-disabled`; > 500 người gửi tuần tự từng lô, "Đã thêm 500/1.200 người…"; lô lỗi giữa chừng → ô chỉ còn các dòng chưa gửi |
| Trả giải | tiêu đề "Trả giải “…” của …?", câu AC5.1, `btn-danger` "Vắng mặt, trả giải" |
| Tổng kết chương trình (rộng) | số suất đã trao; theo từng giải: đầu nhóm (tên, chip, "Đã trao a/b") + thẻ `WinRow` của người trúng; "Còn 2 suất “…” — không còn người phù hợp" / "Còn n suất chưa quay"; dòng "Vắng mặt, đã trả giải: …"; "Tải danh sách (CSV)". Mở từ "Xem tổng kết" của popup ở lượt làm `finished = true` (không tự mở cùng popup); chương trình *Đã xong* mở ra có card Tổng kết ở đầu trang (AC6.2) |

## 6. Trình chiếu (US7)
- `<dialog>` modal (top layer, focus trong hộp thoại) phủ cả cửa sổ + `document.documentElement.requestFullscreen()` khi được phép (bị từ chối thì vẫn phủ cửa sổ). Esc: thoát toàn màn hình → thoát trình chiếu; focus về nút "Trình chiếu". Không mở được trong lúc đang quay.
- Nền cả màn = chrome; trên cùng tên chương trình + "Thoát trình chiếu"; sân khấu bên trong có **select "Giải đang quay"** (giải không quay được: `disabled` kèm lý do). Màn ngang ≥ 1024: vòng quay trái (`min(52vw, 80dvh)`), giải / nút / trạng thái phải; dọc: xếp chồng. Người trúng: popup cỡ TV đè lên trình chiếu. Chương trình xong: tổng kết hiện ngay trên sân khấu (card trắng).

## 7. Trạng thái, a11y
- Đang tải: skeleton đúng hình (header, khối sân khấu 560px, hai card, bảng); danh sách: `RowsSkeleton`. Lỗi: `ErrorState` + "Thử lại"; chương trình không tồn tại (404), hoặc id trên URL không phải UUID (không gọi API: API trả 400 chữ thường cho id hỏng): "Không tìm thấy chương trình này (có thể đã bị xóa)." + link về danh sách. Trống: câu bước tiếp ở từng card (rulebook §9).
- Thông báo lỗi theo mã qua `addMessages` (`messages.ts`, chunk màn): `lucky_draw.*`, `idempotency.in_progress`, `precondition.if_match_required`; giới hạn danh sách (`errors[].field` = `prizes` / `participants`) có câu riêng.
- "X trúng Y" đọc qua `aria-live="polite"` của dòng trạng thái chỉ khi lượt đáp; popup có tên truy cập "Chúc mừng", mô tả = tên + giải; "Đang quay…" là `aria-hidden` (nút đã `aria-busy`). Mọi thao tác dùng được bằng bàn phím; vùng chạm 44px; số `tabular-nums`; sáng + tối.

## 8. Khi build (2026-11-28, web)
- **Lệch spec §5:** hai cột theo bề rộng nội dung ≥ 880px (container query) thay vì viewport ≥ 1024px (§1). "Đã trúng" chỉ đếm người trúng còn hiệu lực; người vắng mặt chỉ thấy ở Tất cả / Nữ / Nam (chip "Vắng mặt" trên dòng). Danh sách trúng thưởng mới nhất trước (CSV và tổng kết theo thứ tự quay).
- **Lệch task (lead):** không đăng ký trong `app/screens.ts` / `ROUTE_PRELOAD`: file đó chỉ dành cho màn đầu được preload song song khi mở app lạnh, và gate sẽ cộng chunk màn (~13 KiB) vào đường `/vong-quay` (≈ 127 KiB, vượt 125). Hai route import **một** module (`LuckyDrawPage.tsx` re-export `LuckyDrawListPage`): một chunk, một mảng deps trong startup.
- **Cache:** mọi lệnh ghi đưa `LuckyDrawDetail` trả về vào cache qua `putDraw`, **không lùi version** (câu trả lời quay được giữ 4 s tới khi vòng dừng; một người thêm trong lúc đó đã trả về version mới hơn). Lỗi quay luôn refetch chương trình (lượt đã lưu trước khi rớt mạng hiện ra).
- **Perf** (`pnpm --filter web budget`, gzip -9, cùng máy, sau khi gộp nhánh API): startup 113,67 → **113,68 KiB** (+0,01: hai route + mục nav; bù bằng cách chuyển bảng `SCREEN_LABELS` của nút quay lại từ `nav.ts` sang `app/screenLabel.ts`, chỉ `PageHeader` (chunk lazy) dùng); `/quay` 123,87 → 123,88; `/` 122,17 → 122,58 (bảng nhãn nay nằm trong chunk `PageHeader` mà trang chủ preload). Chunk màn `LuckyDrawPage` **12,5 KiB** gzip JS + CSS 1,8 KiB + ảnh 97 KB (chỉ màn chương trình).
- **Mã lỗi field theo API** (contract §5 B4): `required`, `too_long`, `out_of_range` (số suất), `invalid` (giới tính → "Chọn Nữ hoặc Nam.", đối tượng → câu riêng, `body` → "Dữ liệu gửi đi không hợp lệ…"), `participants` `count_out_of_range` / `too_many`, `prizes` `too_many`; `lucky_draw.quantity_below_awarded` là `code` 422 cấp trên, hiện dưới ô Số suất. Kiểu lấy từ `@vpyd/api-client` (`types.ts` chỉ đặt tên ngắn).
- Dòng không có ⋯ (người đã quay, kết quả đã trả) giữ ô trống 44px để mọi dòng cao bằng nhau.
- **Bảo mật phía web** (server vẫn là trust boundary, mọi route `lucky_draw.manage`): body quay chỉ `{prize_id}` (không người, không số ngẫu nhiên), sector chỉ trang trí; nav + route guard chỉ là UX; CSV sinh ở trình duyệt, ô bắt đầu `= + - @` / Tab / CR (kể cả sau khoảng trắng) thêm `'`, mọi ô trong ngoặc kép; tên người chỉ render qua React (không `dangerouslySetInnerHTML`), không lưu localStorage / sessionStorage; ô tìm (`?q=`) có thể chứa một phần tên người tham gia trên URL như `/khach-hang?q=` (chấp nhận: họ tên + giới tính, không SĐT).
