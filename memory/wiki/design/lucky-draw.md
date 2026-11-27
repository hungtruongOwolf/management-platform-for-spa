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
- Trong sân khấu, theo thứ tự: "Giải đang quay" + tên giải (`--fs-lg`) + chip đối tượng + "Còn x/y suất" → vòng quay → nút **Quay** / **Quay tiếp** (`btn-primary btn-lg`) + câu bước tiếp khi không quay được → thẻ kết quả.
- Nút trên nền chrome: `btn-primary` thành **kem** (`chrome-text`) chữ `chrome-bg` (hover / nhấn pha `chrome-accent` 14 / 28 %), `btn-secondary` thành viền kem trong suốt. Cùng hình, cùng 7 trạng thái như mọi nơi; không vàng bóng.
- Thẻ kết quả (cao tối thiểu 136px để không xô layout): "Người trúng · lượt n", **tên** `--fs-xl` màu `chrome-accent` (điểm nhấn vàng duy nhất), tên giải, nút "Vắng mặt, trả giải". Kết quả đã trả: "Vắng mặt, đã trả giải", không còn nút. Hiện bằng fade 200 ms.

## 3. Vòng quay (`Wheel.tsx`) và ngoại lệ chuyển động (rulebook §10)
- Hai ảnh WebP (mặt 376², 33 KB; khung 466 × 481, 64 KB), import qua Vite, nằm cùng chunk màn, không ảnh LCP của trang nào khác. Đặt theo tỉ lệ crop gốc: mặt `left 8.798%`, `top 13.929%`, `width 80.687%`, `height 78.170%` trong khung `aspect-ratio: 466 / 481`, rộng `min(100%, 420px)`. Khung `role="img"` "Vòng quay tám cánh sen", ảnh `alt=""`. Không phải control.
- Chuyển động: bấm Quay → **quay ngay** (đà 0,5 s `cubic-bezier(.55,0,.75,.5)` rồi vòng đều 0,25 s/vòng, keyframe hằng 0 → 360° trên ảnh bên trong để trình duyệt chạy ngoài main thread) → khi server trả `SpinOutcome`: đọc góc đang quay, **giảm tốc 4 s** `cubic-bezier(.14,.72,.12,1)` thêm 3 vòng tới giữa một trong 8 cánh (`crypto.getRandomValues`, chỉ trang trí) → `transitionend` (hoặc hẹn giờ 4,4 s dự phòng khi tab ẩn) → **mới** đưa kết quả vào cache: người trúng, số suất, bảng, trạng thái người hiện cùng lúc. Lỗi: về chỗ cũ 0,7 s.
- **Vì sao là ngoại lệ của §10** (150–250 ms, chỉ cho đổi trạng thái): ở đây chuyển động *là nội dung* của khoảnh khắc quay thưởng trước khán giả, và nó che đúng khoảng chờ server; vẫn chỉ `transform`, không đổi layout, không lặp vô hạn ngoài lúc chờ. Mọi phần khác của màn theo §10.
- `prefers-reduced-motion`: không xoay; kết quả hiện ngay khi server trả lời (`useMediaQuery`, CSS toàn cục cũng tắt animation).
- Bấm hai lần: nút `aria-busy` trong suốt lượt (nhãn giữ "Quay"), lần bấm thứ hai bị bỏ qua; mất mạng / 5xx: "Chưa quay được, kiểm tra mạng rồi bấm “Quay” lại.", lần bấm sau gửi **cùng** `Idempotency-Key` (AC4.6).

## 4. Giải, trúng thưởng, người tham gia
- **Giải thưởng**: mỗi giải là một ô chọn (`aria-pressed`, hình `.choice`: viền `border-strong`, chọn = `primary-soft` + viền `primary` + ✓): tên, mô tả, chip đối tượng (Nữ / Nam / Cả nam và nữ = `gender-chip`), "Còn x/y suất"; không quay được → nền `surface-2`, chip `badge-muted` "Hết suất" / "Không còn người phù hợp", `aria-disabled`. Tự chọn giải đầu tiên quay được (`defaultPrize`); giải đang chọn hết suất thì nhảy sang giải kế. ⋯ "Sửa giải" / "Xóa giải" (không có "Xóa" khi giải đã có kết quả). Khóa chọn giải trong lúc quay.
- **Danh sách trúng thưởng**: lượt mới nhất trên cùng; Lượt · Người trúng · Giải · Lúc (giờ spa); dòng đã trả mờ (`text-muted`) + chip "Vắng mặt", không ⋯; dòng còn lại ⋯ "Vắng mặt, trả giải". Nút "Tải danh sách (CSV)" và "Xem tổng kết" khi có kết quả. 100 dòng đầu + "Hiện thêm".
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
| Tổng kết chương trình (rộng) | số suất đã trao; theo từng giải: tên + "x/y suất", danh sách "Lượt n Tên (Nữ)"; "Còn 2 suất “…” — không còn người phù hợp" / "Còn n suất chưa quay"; dòng "Vắng mặt, đã trả giải: …"; "Tải danh sách (CSV)". Tự mở **một lần** sau khi vòng dừng ở lượt làm `finished = true`; chương trình *Đã xong* mở ra có card Tổng kết ở đầu trang (AC6.2) |

## 6. Trình chiếu (US7)
- `<dialog>` modal (top layer, focus trong hộp thoại) phủ cả cửa sổ + `document.documentElement.requestFullscreen()` khi được phép (bị từ chối thì vẫn phủ cửa sổ). Esc: thoát toàn màn hình → thoát trình chiếu; focus về nút "Trình chiếu". Không mở được trong lúc đang quay.
- Nền cả màn = chrome; trên cùng tên chương trình + "Thoát trình chiếu"; sân khấu bên trong có **select "Giải đang quay"** (giải không quay được: `disabled` kèm lý do). Màn ngang ≥ 1024: vòng quay trái (`min(52vw, 80dvh)`), giải / nút / kết quả phải; dọc: xếp chồng. Chương trình xong: tổng kết hiện ngay trên sân khấu (card trắng).

## 7. Trạng thái, a11y
- Đang tải: skeleton đúng hình (header, khối sân khấu 560px, hai card, bảng); danh sách: `RowsSkeleton`. Lỗi: `ErrorState` + "Thử lại"; chương trình không tồn tại (404), hoặc id trên URL không phải UUID (không gọi API: API trả 400 chữ thường cho id hỏng): "Không tìm thấy chương trình này (có thể đã bị xóa)." + link về danh sách. Trống: câu bước tiếp ở từng card (rulebook §9).
- Thông báo lỗi theo mã qua `addMessages` (`messages.ts`, chunk màn): `lucky_draw.*`, `idempotency.in_progress`, `precondition.if_match_required`; giới hạn danh sách (`errors[].field` = `prizes` / `participants`) có câu riêng.
- Kết quả đọc qua `aria-live="polite"` chỉ sau khi vòng dừng; "Đang quay…" là `aria-hidden` (nút đã `aria-busy`). Mọi thao tác dùng được bằng bàn phím; vùng chạm 44px; số `tabular-nums`; sáng + tối.

## 8. Khi build (2026-11-28, web)
- **Lệch spec §5:** hai cột theo bề rộng nội dung ≥ 880px (container query) thay vì viewport ≥ 1024px (§1). "Đã trúng" chỉ đếm người trúng còn hiệu lực; người vắng mặt chỉ thấy ở Tất cả / Nữ / Nam (chip "Vắng mặt" trên dòng). Danh sách trúng thưởng mới nhất trước (CSV và tổng kết theo thứ tự quay).
- **Lệch task (lead):** không đăng ký trong `app/screens.ts` / `ROUTE_PRELOAD`: file đó chỉ dành cho màn đầu được preload song song khi mở app lạnh, và gate sẽ cộng chunk màn (~13 KiB) vào đường `/vong-quay` (≈ 127 KiB, vượt 125). Hai route import **một** module (`LuckyDrawPage.tsx` re-export `LuckyDrawListPage`): một chunk, một mảng deps trong startup.
- **Cache:** mọi lệnh ghi đưa `LuckyDrawDetail` trả về vào cache qua `putDraw`, **không lùi version** (câu trả lời quay được giữ 4 s tới khi vòng dừng; một người thêm trong lúc đó đã trả về version mới hơn). Lỗi quay luôn refetch chương trình (lượt đã lưu trước khi rớt mạng hiện ra).
- **Perf** (`pnpm --filter web budget`, gzip -9, cùng máy): startup 113,67 → **113,70 KiB** (+0,03: hai route + mục nav; bù bằng cách chuyển bảng `SCREEN_LABELS` của nút quay lại từ `nav.ts` sang `app/screenLabel.ts`, chỉ `PageHeader` (chunk lazy) dùng); `/quay` 123,87 → 123,89; `/` 122,17 → 122,60 (bảng nhãn nay nằm trong chunk `PageHeader` mà trang chủ preload). Chunk màn `LuckyDrawPage` **12,7 KiB** gzip JS + CSS 1,9 KiB + ảnh 97 KB (chỉ màn chương trình).
- **Bảo mật phía web** (server vẫn là trust boundary, mọi route `lucky_draw.manage`): body quay chỉ `{prize_id}` (không người, không số ngẫu nhiên), sector chỉ trang trí; nav + route guard chỉ là UX; CSV sinh ở trình duyệt, ô bắt đầu `= + - @` / Tab / CR (kể cả sau khoảng trắng) thêm `'`, mọi ô trong ngoặc kép; tên người chỉ render qua React (không `dangerouslySetInnerHTML`), không lưu localStorage / sessionStorage; ô tìm (`?q=`) có thể chứa một phần tên người tham gia trên URL như `/khach-hang?q=` (chấp nhận: họ tên + giới tính, không SĐT).
