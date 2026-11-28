---
title: "Spec: Vòng quay may mắn (chương trình quay thưởng của chủ spa)"
status: approved (EM 2026-11-27 giao làm; lead chốt mặc định ở §8, không hỏi lại)
updated: 2026-11-28
sources: [raw/2026-11-27-em-request-lucky-draw.md, raw/2026-11-28-em-lucky-draw-layout-feedback.md, raw/2026-11-28-em-spin-6s.md]
related: [../../security/authz-matrix.md, ../../design/design-system.md, ../../architecture/backend.md, ../../architecture/data.md]
---

# Spec: Vòng quay may mắn

## 1. Mục tiêu
Chủ spa tổ chức quay thưởng tri ân (ví dụ "Tri ân 20/11") ngay trong app: nhập **chương trình**, **người tham gia** (họ tên + giới tính), **giải thưởng** (dành cho Nam / Nữ / cả hai, số suất), rồi quay. Mỗi lần quay **tự trừ** người trúng và một suất của giải, lưu vào **danh sách trúng thưởng**; hết giải thì hiện **tổng kết** ai trúng gì. Nhiều chương trình được lưu trên server (không còn localStorage của bản HTML), mở lại xem bất cứ lúc nào, trên máy nào cũng được.

Nguồn UI: bản HTML EM gửi (`vien-phuc-wheel.zip`, raw 2026-11-27). **Giữ ảnh vòng quay** (mặt quay tám cánh sen + khung vàng, EM: "vòng quay tôi thấy mọi thứ khá ổn"), **bỏ phần trang trí còn lại** (nền đỏ toàn trang, khung viền foil, ribbon tiêu đề, hoa sen góc, ảnh minh họa giải, nút vàng bóng, font riêng) và dựng lại bằng design system của app ("Trà thất Đông y", [design-system](../../design/design-system.md)); EM: "mọi thứ bây giờ đang bị AI quá mức … màu mè".

## 2. Vai trò
| Vai trò | Quyền |
|---|---|
| Chủ spa | Toàn quyền: `lucky_draw.manage` (mới, chỉ role owner + mẫu vai trò owner) |
| Quản lý, KTV, người đã nghỉ | Không thấy mục nav, mọi route `403` |
| Operator nền tảng | Không đọc dữ liệu chương trình; chỉ thấy permission trong danh mục mẫu vai trò |

Không có cờ tính năng riêng: permission là đủ (owner-only).

## 3. Khái niệm
- **Chương trình** (`lucky_draw`): tên (1–80 ký tự), người tạo, lúc tạo. Trạng thái **suy ra**, không lưu: *Chưa quay* (0 kết quả) · *Đang quay* (có kết quả, còn cặp người–giải hợp lệ) · *Đã xong* (có kết quả, không còn cặp hợp lệ).
- **Người tham gia** (`lucky_draw_participant`): họ tên (1–120), giới tính **Nam / Nữ** (bắt buộc), thứ tự nhập. Tối đa 2.000 người / chương trình.
- **Giải thưởng** (`lucky_draw_prize`): tên (1–140), mô tả (≤ 250, tùy chọn), dành cho `all` / `female` / `male`, số suất 1–1.000, thứ tự. Tối đa 50 giải / chương trình. *Còn lại* = số suất − số kết quả **chưa** bị trả của giải đó.
- **Kết quả** (`lucky_draw_result`): lượt quay số `seq` (1, 2, 3… theo chương trình), giải, người trúng, lúc quay, người quay. Có thể **trả giải** (người trúng vắng mặt / không nhận): suất quay lại cho giải, người đó **vẫn bị loại** khỏi các lượt sau.
- **Đủ điều kiện** cho giải G: người chưa có kết quả nào trong chương trình (kể cả kết quả đã trả) **và** (`G.eligible = all` hoặc giới tính khớp).

## 4. User stories + tiêu chí chấp nhận
Mọi AC có test tự động (API: `apps/api/crates/app/tests/lucky_draw.rs` + `authz_matrix.rs`; web: Vitest; e2e: `lucky-draw.spec.ts`).

### US1 — Danh sách chương trình
*Là chủ spa, tôi muốn thấy mọi chương trình đã tạo và tạo chương trình mới, để mở lại chương trình cũ xem kết quả.*
- AC1.1: `GET /lucky-draws` trả mọi chương trình của spa, mới nhất trước, mỗi dòng: tên, lúc tạo, số người, tổng suất, số suất đã trao, trạng thái suy ra.
- AC1.2: `POST /lucky-draws {name}` → `201`, mở màn chương trình. Tên rỗng / > 80 → `422` (`errors[].field = name`).
- AC1.3 (neg): quản lý / KTV → `403`; không phiên → `401`; id chương trình của spa khác ở mọi route `/lucky-draws/{id}/…` → `404`.
- AC1.4: Đổi tên (`PATCH`, `If-Match`; version cũ → `412`). Xóa chương trình (`DELETE`) xóa luôn người tham gia, giải, kết quả; hỏi xác nhận trên UI ("Xóa chương trình “…”? Danh sách người tham gia, giải và kết quả sẽ bị xóa, không khôi phục được."). Audit giữ tên + số đếm, không giữ danh sách tên người.

### US2 — Người tham gia
*Là chủ spa, tôi muốn nhập danh sách người tham gia kèm giới tính, từng người hoặc dán cả danh sách từ Excel.*
- AC2.1: Thêm một người: họ tên + chọn **Nữ / Nam** (segmented, mặc định chưa chọn → phải chọn). `POST /lucky-draws/{id}/participants {participants:[{name, gender}]}` (1–500 mục mỗi lần).
- AC2.2: Dán danh sách: mỗi dòng `Họ tên, Nữ` / `Họ tên<Tab>Nam` / `Họ tên; Nam` (hai cột Excel dán ra Tab). Giới tính không phân biệt hoa thường / dấu (`nu`, `Nữ`, `NAM`). Dòng trống bỏ qua; dòng đầu đúng bằng tiêu đề ("Họ tên, Giới tính") bỏ qua. Xem trước: số người sẽ thêm, **từng dòng lỗi** ("Dòng 4: thiếu giới tính — ví dụ: Trần Văn Minh, Nam"); còn dòng lỗi thì không gửi. Trùng tên với người đã có → chỉ cảnh báo, vẫn thêm.
- AC2.3 (neg): tên rỗng / > 120, giới tính khác `male`/`female`, > 500 mục một lần, tổng > 2.000 → `422` và **không thêm ai** (một transaction).
- AC2.4: Sửa / xóa người **chưa có kết quả** (`PATCH` có `If-Match`, `DELETE`). Người đã có kết quả (kể cả đã trả giải) → `409 lucky_draw.participant_drawn`; UI không hiện thao tác sửa / xóa cho họ.
- AC2.5: Danh sách có lọc Tất cả / Nữ / Nam / Đã trúng (kèm số đếm), tìm theo tên không dấu; trạng thái mỗi người: *Chưa quay* · *Đã trúng: <giải>* · *Vắng mặt*.

### US3 — Giải thưởng
*Là chủ spa, tôi muốn nhập giải, mỗi giải dành cho Nam, Nữ hay cả hai, kèm số suất.*
- AC3.1: Thêm giải: tên, "Dành cho" (Cả nam và nữ / Chỉ nữ / Chỉ nam), số suất (1–1.000), mô tả tùy chọn. `POST /lucky-draws/{id}/prizes`.
- AC3.2: Sửa giải (`PATCH`, `If-Match`): tên, mô tả luôn sửa được; số suất không được nhỏ hơn số đã trao (`422 lucky_draw.quantity_below_awarded`); **đối tượng** không đổi được khi giải đã có kết quả (`409 lucky_draw.prize_drawn`).
- AC3.3: Xóa giải chưa có kết quả nào (kể cả đã trả); có → `409 lucky_draw.prize_drawn`.
- AC3.4 (neg): tên rỗng / > 140, mô tả > 250, `eligible` lạ, số suất ngoài 1–1.000, > 50 giải → `422`.

### US4 — Quay
*Là chủ spa, tôi muốn chọn giải rồi bấm quay, hệ thống tự chọn ngẫu nhiên một người phù hợp giới tính, tự trừ người và suất, lưu vào danh sách trúng thưởng.*
- AC4.1: Màn chương trình **tự chọn sẵn** giải đầu tiên (theo thứ tự) còn suất và còn người đủ điều kiện; chủ spa bấm thẻ giải khác để đổi. Giải hết suất / không còn người phù hợp: thẻ mờ, không chọn được, ghi rõ lý do ("Hết suất" / "Không còn người phù hợp").
- AC4.2: Bấm **Quay** → `POST /lucky-draws/{id}/spins {prize_id}` + `Idempotency-Key`. **Server** chọn người, đều nhau trong nhóm đủ điều kiện, bằng CSPRNG (OS RNG); ghi kết quả `seq = lượt kế tiếp`; trả `201 {result, prize_remaining, finished}`. Không có bước "Xác nhận": lưu ngay (EM: "mỗi lần quay sẽ tự trừ"). Vị trí dừng của vòng quay chỉ là trang trí.
- AC4.3: Vòng quay chạy **6 s** rồi dừng (EM 2026-11-28: "tầm 6s thôi", thay 4,1 s; ~9 vòng, mở nhanh như cũ, đuôi chậm dần dài; một hằng số `SPIN_MS` để chỉnh); **chỉ sau khi dừng** (và đã có câu trả lời của server) mới hiện người trúng — **popup giữa màn** "Chúc mừng · Lượt N", tên, giải, "Còn x/y suất", "Quay tiếp" / "Xem tổng kết", "Vắng mặt, trả giải", "Đóng" (EM 2026-11-28; không hiện người trúng dưới vòng quay) — và cập nhật danh sách, số suất, trạng thái người. `prefers-reduced-motion`: **vẫn quay**, nhẹ hơn (~3 vòng, cùng 6 s) (EM 2026-11-28: vòng quay phải quay, cú quay là nội dung).
- AC4.4 (neg): giải hết suất → `409 lucky_draw.prize_exhausted`; không còn người phù hợp → `409 lucky_draw.no_eligible`; giải của chương trình khác / spa khác → `404`; cùng `Idempotency-Key` gửi lại → cùng kết quả, không quay thêm; bấm hai lần → một lượt (nút `aria-busy` trong lúc quay).
- AC4.5: Hai tab cùng quay một chương trình: các lượt nối tiếp nhau (khóa dòng chương trình), không ai trúng hai lần, không vượt số suất (test tranh nhau thật).
- AC4.6: Lỗi mạng: thẻ kết quả nói "Chưa quay được, kiểm tra mạng rồi bấm Quay lại" và vòng quay dừng về chỗ cũ; bấm lại dùng **cùng** `Idempotency-Key` nên không ra hai lượt nếu lượt trước thực ra đã lưu.

### US5 — Trả giải (người trúng vắng mặt)
- AC5.1: Ở dòng kết quả (danh sách trúng thưởng, thẻ kết quả vừa quay): **"Vắng mặt, trả giải"** → xác nhận ("Trả giải “…” của Nguyễn Văn A? Suất này quay lại cho lượt sau; Nguyễn Văn A không được quay nữa.") → `POST /lucky-draws/{id}/results/{rid}/forfeit`. Kết quả còn trong danh sách, mờ, chip "Vắng mặt"; suất của giải +1.
- AC5.2 (neg): trả lần hai → `409 lucky_draw.result_forfeited`; kết quả của chương trình khác → `404`.

### US6 — Tổng kết
*Là chủ spa, tôi muốn khi hết giải thì thấy lại ai trúng gì.*
- AC6.1: Lượt quay làm `finished = true` → sau khi vòng dừng, mở **Tổng kết chương trình**: số lượt đã trao, danh sách trúng thưởng **theo từng giải** (thứ tự giải, trong giải theo thứ tự quay), giải còn suất mà không còn ai phù hợp ("Còn 2 suất “Cổ vai gáy” — không còn người phù hợp"), người vắng mặt riêng một dòng.
- AC6.2: Mở lại bất cứ lúc nào bằng nút "Xem tổng kết" (tab Trúng thưởng) hoặc nút của popup ở lượt cuối; chương trình *Đã xong* ở danh sách mở thẳng vào màn chương trình. **Không** còn card tổng kết ở đầu trang (EM 2026-11-28: người trúng hiện hai lần, bỏ cái ở trên).
- AC6.3: **Tải danh sách (CSV)**: UTF-8 có BOM, cột Lượt, Họ tên, Giới tính, Giải thưởng, Lúc quay, Trạng thái; ô bắt đầu bằng `= + - @` / Tab được thêm `'` (chống CSV injection). Sinh ở trình duyệt từ dữ liệu đã có.

### US7 — Trình chiếu
- AC7.1: Nút "Trình chiếu" mở lớp toàn màn hình (thử `requestFullscreen`; không được thì vẫn phủ cửa sổ): tên chương trình, giải đang chọn (đổi được), vòng quay **vừa khít màn hình, không bao giờ cuộn**, nút Quay, danh sách trúng thưởng trong một khung cao cố định cuộn bên trong; lề đều 24px bốn phía (EM 2026-11-28). Người trúng: popup cỡ TV. Esc / "Thoát trình chiếu" đóng, focus về nút mở.

## 5. UX notes
- **Nav:** nhóm **Khách hàng** → "Vòng quay may mắn" (route `/vong-quay`, chương trình `/vong-quay/$drawId`), chỉ hiện khi có `lucky_draw.manage`. Điện thoại: trong drawer "Thêm". Chunk lazy, không thêm byte vào startup (ADR-0024).
- **Danh sách:** `PageHeader` "Vòng quay may mắn" + nút chính "Tạo chương trình". Bảng: Tên · Ngày tạo · Người tham gia · Đã trao / tổng suất · Trạng thái (`badge-info` Chưa quay, `badge-ok` Đang quay, `badge-muted` Đã xong). Trống: dạy bước ("Chưa có chương trình nào. Bấm “Tạo chương trình”, đặt tên như “Tri ân 20/11”, rồi thêm người tham gia và giải thưởng."). Tạo = sheet một ô tên.
- **Màn chương trình, máy tính (bề rộng nội dung ≥ 880px; EM 2026-11-28: "vòng quay to hơn, chiếm phần lớn chủ đạo của trang … điều phối sắp xếp lại trang cho rõ ràng"):** trang dùng **hết bề rộng nội dung** (không cột hẹp giữa màn) và **vừa khít chiều cao màn hình: trang không cuộn** (1536 × 730, 1920 × 1080; 1366 × 657 vẫn dùng tốt). Header (tên, breadcrumb "Vòng quay may mắn", nút Trình chiếu, menu ⋯ Đổi tên / Xóa chương trình). Hai cột: trái, phần lớn = **sân khấu** (card nền `chrome-bg`→`chrome-bg-2` như khung app, vì ảnh vòng quay đỏ–vàng có quầng sáng chỉ đẹp trên nền nâu đỏ): **vòng quay cao hết sân khấu** (đường kính theo chiều cao còn lại, ≥ 60 % chiều cao màn ở 1536 × 730), bên cạnh là giải đang chọn, nút **Quay** (`btn-primary btn-lg`, trên nền chrome dùng biến thể sáng giống khung chào) và một dòng trạng thái; phải, rộng cố định (25–28rem, đủ cho ba tab một dòng) = một card **ba tab**: **Giải thưởng** (thẻ chọn được, `aria-pressed`, chip đối tượng, "Còn 2/3 suất", ⋯ Sửa / Xóa, "Thêm giải thưởng") · **Trúng thưởng** (thẻ theo giải, ⋯ Vắng mặt, trả giải; "Tải danh sách (CSV)", "Xem tổng kết") · **Người tham gia** ("Thêm người", "Dán danh sách", lọc, tìm, danh sách). Mỗi tab: nút / lọc ở trên đứng yên, **danh sách cuộn bên trong khung** (EM: "1 component kích thước cứng rồi nếu list dài thì scroll ở trong đó"); không danh sách nào kéo dài trang, không còn "Hiện thêm". Tab đang mở nằm trên URL (`?tab=giai|trung|nguoi`).
- **Điện thoại / bề rộng nội dung < 880px:** một cột: sân khấu (vòng quay rộng hết card) → card ba tab, khung cao cố định (`min(40rem, 82svh)`) cuộn bên trong; trang cuộn bình thường.
- **Ô chọn giải của trình chiếu:** `<select>` gốc của app (giữ native cho form + test); giao diện danh sách mở ra được làm lại toàn app ở PR riêng (`appearance: base-select`), màn này chỉ đặt kích thước và vị trí ô.
- **Trạng thái nút Quay:** chưa có người / giải → disabled + câu bước tiếp ngay dưới ("Thêm người tham gia và giải thưởng để bắt đầu quay."); đang quay → `aria-busy`, nhãn giữ "Quay"; sau một lượt → nhãn "Quay tiếp".
- **Không** dùng: ảnh minh họa giải, font riêng của bản HTML, nút vàng bóng, viền foil, hoa sen góc, chữ hoa giãn chữ, pháo hoa / confetti, âm thanh. Người trúng hiện bằng popup (fade + scale 220 ms). Ảnh vòng quay: 2 file WebP (mặt quay ~33 KB, khung ~64 KB, nén lại từ bản gốc 161 + 268 KB), tải cùng chunk màn, `alt=""` + `role="img" aria-label="Vòng quay tám cánh sen"` ở khung.
- Chữ giao diện tiếng Việt, sentence case; số `tabular-nums`; vùng chạm 44 px; sáng + tối; axe sạch.

## 6. Dữ liệu
- Bảng mới (migration **0055**), mọi bảng có `tenant_id` + RLS `tenant_isolation`, FK kèm tenant theo [data](../../architecture/data.md): `lucky_draw`, `lucky_draw_participant`, `lucky_draw_prize`, `lucky_draw_result`. Xóa chương trình = `ON DELETE CASCADE` xuống ba bảng con. Kết quả: `UNIQUE (tenant_id, draw_id, participant_id)` (một người một kết quả, kể cả đã trả), `UNIQUE (tenant_id, draw_id, seq)`; chỉ được `UPDATE` một lần để trả giải (`forfeited_at`, `forfeited_by`), mọi cột khác bất biến (trigger).
- Dữ liệu cá nhân: họ tên + giới tính người tham gia (ít nhạy cảm, không SĐT). Không vào log. Audit: số đếm, tên chương trình / giải; tên người chỉ ở dòng quay / trả giải / sửa / xóa một người (để truy vết "ai trúng gì"), giống cách audit đã giữ tên khách.
- Lưu tới khi chủ spa xóa chương trình.

## 7. Non-functional
- **Bảo mật:** server là trust boundary: `lucky_draw.manage` kiểm trên mọi route; người trúng chọn ở server (client không gửi người, không gửi số ngẫu nhiên); `Idempotency-Key` cho quay; khóa dòng chương trình cho mọi thao tác ghi của chương trình đó; id spa khác `404`; CSV chống injection; giới hạn kích thước request.
- **Hiệu năng:** `GET /lucky-draws/{id}` 2.000 người + 50 giải + 1.000 kết quả trả trong p95 < 150 ms (local); quay p95 < 100 ms (local). Chunk màn ≤ 25 KiB gzip JS (không tính ảnh); startup không đổi.
- **A11y:** WCAG 2.2 AA; kết quả đọc qua `aria-live="polite"` sau khi vòng dừng; vòng quay không phải control; mọi thao tác dùng được bằng bàn phím.
- **Thiết bị:** laptop quầy (trình chiếu qua TV / máy chiếu), tablet, điện thoại.

## 8. Mặc định lead chốt (EM giao "xem thử cho tôi", không hỏi lại)
| # | Câu hỏi | Mặc định | Lý do |
|---|---|---|---|
| L1 | Quay theo giải hay theo người? | Chọn giải (tự chọn sẵn) rồi quay ra người, như bản HTML | EM khen bản HTML; quay từng giải là cách quay thưởng quen thuộc |
| L2 | Có bước "Xác nhận người trúng" như bản HTML? | Không; lưu ngay, có "Vắng mặt, trả giải" | EM: "mỗi lần quay sẽ tự trừ"; ít thao tác ([[fewer-operator-steps]]) |
| L3 | Người vắng mặt có được quay lại? | Không, bị loại khỏi chương trình | Quy ước quay thưởng thường gặp; tránh một người trúng hai lần |
| L4 | Giới tính "Chưa rõ" như bản HTML? | Không; bắt buộc Nam / Nữ | EM: "list danh sách … cùng với giới tính"; giải theo giới cần giới rõ |
| L5 | Người tham gia lấy từ hồ sơ khách? | Không (gõ / dán tên) ở bản này | Người tham gia có thể không phải khách; giữ phạm vi nhỏ |
| L6 | Ai dùng? | Chỉ chủ spa | EM: "page của chủ spa" |
| L7 | Xóa chương trình đã quay? | Được, hỏi xác nhận, audit giữ tên + số đếm | Dữ liệu của chủ spa, không phải tiền; có đường xóa dữ liệu cá nhân |
| L8 | Ảnh minh họa từng giải? | Bỏ | EM chê "AI quá mức"; tên + chip đối tượng đủ đọc |

## 9. Ngoài phạm vi
Chọn người tham gia từ danh sách khách · gửi kết quả qua Zalo / SMS · trạng thái "Đã trao quà" · quay nhiều người một lần · trọng số / tỉ lệ khác nhau · âm thanh, hiệu ứng pháo hoa · quản lý / KTV quay · in phiếu.

## 10. Câu hỏi mở
Không có câu chặn. EM xem trên staging rồi góp ý.
