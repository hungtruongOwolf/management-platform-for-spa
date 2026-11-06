---
title: Front-desk screen flows (spec R1-1)
updated: 2026-11-04
sources: [product/specs/front-desk-and-packages.md, design/design-system.md]
---

> **Cập nhật 2026-11-04 (EM):** R1 **không có ghi nợ / thu nợ / công nợ** (mọi giao dịch thu đủ) và **không theo dõi giờ ra**. Bỏ qua các phần mô tả những luồng đó bên dưới; spec là nguồn đúng.

# Luồng màn hình quầy (spec R1-1)

Prototype bấm thử: `prototypes/front-desk/index.html` (dữ liệu giả). Đã cover US2, US4, một phần US5; các luồng còn lại mô tả dưới đây.

## Bố cục chung `/quay`
```
┌──────────────── Header: logo · chi nhánh · người dùng · khóa nhanh ────────────────┐
│ CỘT TRÁI                               │ CỘT PHẢI                                   │
│ [🔍 Ô tìm khách — focus sẵn]            │ Lượt đang ghi: các dòng dịch vụ + KTV       │
│ Kết quả / + Khách mới                  │ + Thêm dịch vụ lẻ                           │
│ Thẻ khách: tên, SĐT, giới tính, nợ     │ Phải trả · [TM][CK][Ghi nợ] · số tiền       │
│ ⚠ Lưu ý sức khỏe                       │ [ Lưu lượt khách ]  (Ctrl+Enter)            │
│ Liệu trình: còn X/Y · [Trừ 1 buổi]     │ Lượt khách hôm nay                          │
│ [+ Bán liệu trình]                     │                                             │
└────────────────────────────────────────┴─────────────────────────────────────────────┘
```
< 900px: một cột, thứ tự tìm → thẻ khách → lượt đang ghi → thanh toán → hôm nay; nút Lưu dính đáy màn hình.
Phím tắt: `/` về ô tìm · `↑↓ Enter` chọn khách · `Ctrl+Enter` lưu · `Esc` bỏ khách đang chọn.

## Fast path (≤ 4 thao tác): khách quen, trừ 1 buổi, không phát sinh tiền
1. Gõ SĐT/tên → `Enter` (chọn kết quả đầu). Focus tự nhảy tới nút "Trừ 1 buổi" đầu tiên.
2. Bấm **Trừ 1 buổi**. Focus tự nhảy tới ô chọn KTV của dòng mới.
3. Chọn **KTV** (KTV hay được chọn nằm trên cùng).
4. **Lưu**. Toast "Đã lưu… còn 4/12", form reset, focus về ô tìm.

## US2: Tìm & tạo khách
| Bước | Mô tả |
|---|---|
| Tìm | ≥ 3 chữ số → so SĐT (chuẩn hóa `+84`/`.`/khoảng trắng); ≥ 2 ký tự → so tên đã bỏ dấu ("ha" → "Hà"). Debounce 150ms, gọi `GET /customers?q=` |
| Chọn | Click hoặc `Enter` → thẻ khách |
| Khách mới | "+ Khách mới" mở form inline, điền sẵn SĐT hoặc tên từ ô tìm. Bắt buộc tên + SĐT; giới tính khuyến khích chọn (cần để tính giá) |
| Trùng SĐT | `409` → "SĐT đã có: <tên>" kèm link mở hồ sơ đó |
| States | đang tìm: 3 dòng skeleton · không thấy: "Không tìm thấy khách" + nút Khách mới · lỗi mạng: "Không tìm được, thử lại" + nút thử lại, giữ nguyên chữ đã gõ |
| Ghi chú sức khỏe | Callout ⚠ màu cảnh báo ngay dưới tên. Không có quyền xem → API không trả field nên callout **không render**, kể cả chỗ trống |

## US4: Ghi lượt khách
| Bước | Mô tả |
|---|---|
| Trừ buổi | "Trừ 1 buổi" trên PackageRow → thêm dòng `Trừ liệu trình`, số còn lại hiển thị trừ tạm ngay (chưa lưu) |
| Nhiều liệu trình cùng dịch vụ | Mặc định gói hết hạn sớm nhất; dòng có menu "Đổi liệu trình" |
| Dịch vụ lẻ / dùng thêm | Combobox nhóm theo category, ★ khuyên dùng lên đầu nhóm; giá theo giới tính của khách. Khách chưa rõ giới tính → toggle Nữ/Nam bắt buộc trước khi thêm |
| Không áp dụng | Dịch vụ không có giá cho giới tính đó (ví dụ Ôn ấm tử cung cho Nam) → không thêm được, báo "không áp dụng cho khách Nam" |
| KTV | Bắt buộc mỗi dòng; lưu khi thiếu → viền đỏ + "Cần chọn KTV", focus vào ô đầu tiên bị thiếu |
| Giờ vào/ra | Giờ vào = lúc lưu (sửa được trong ngày qua "Chi tiết"); "Xong" ở danh sách hôm nay ghi giờ ra |
| Gói hết buổi | Row xám, badge "Đã hết buổi", nút aria-disabled; bấm → toast lỗi + gợi ý **Bán gói mới** |
| Gói hết hạn | Badge "Hết hạn"; bấm trừ → ReasonDialog "Cho dùng tiếp (ghi lý do)" nếu có quyền, ngược lại báo lỗi |
| Lưu | Nút loading và khóa lại; gửi kèm `Idempotency-Key` sinh khi mở lượt |
| Thành công | Toast tóm tắt (số buổi còn lại, nợ phát sinh); danh sách hôm nay thêm dòng mới ở đầu; reset |
| `409` (hai máy cùng trừ buổi cuối) | Toast lỗi "Liệu trình vừa được dùng ở máy khác, đã cập nhật số buổi"; refetch khách, giữ lại các dòng khác |
| Lỗi mạng | Giữ nguyên giỏ; banner "Chưa lưu được. Thử lại" gửi lại **cùng** Idempotency-Key nên không bị ghi đôi |

## US3: Bán liệu trình (Dialog từ thẻ khách)
1. Chọn gói (combobox; hiển thị giá, số buổi + tặng, HSD).
2. Quà hiện vật (điền sẵn từ gói, sửa được) · **Người tư vấn chốt** (bắt buộc; có lựa chọn "Quản lý / Chủ spa").
3. Giảm giá (tùy chọn): số tiền hoặc %; bắt buộc lý do; vượt 10% với Quản lý → "Cần chủ spa duyệt" và không lưu được.
4. Thanh toán: TM / CK / Ghi nợ, có thể **thêm lần trả thứ 2** (ví dụ 500k TM + 1tr CK). Hiển thị "Còn nợ X".
5. Lưu → PackageRow mới xuất hiện ngay trên thẻ khách, kèm badge nợ nếu còn.
States: validation inline; thu > phải trả → lỗi tại ô tiền; lưu trùng → idempotent; lỗi mạng như US4.

## US5: Thu nợ & danh sách công nợ
- Thẻ khách có badge "Còn nợ X". Bấm → Sheet liệt kê từng hóa đơn còn nợ → "Thu" → số tiền (mặc định = còn nợ) + hình thức.
- Trang `/cong-no`: bảng khách · hóa đơn · còn nợ · số ngày nợ (giảm dần); lọc ≥ 30 ngày. Empty: "Không có công nợ 🎉".
- Thu vượt số nợ → lỗi tại ô; payment không có nút sửa/xóa (chỉ hủy theo US6).

## US6: Hủy / sửa sai
- **Sửa lượt (US6b, 2026-11-06, lead's design — EM review):** ⋯ của dòng hoặc nút "Sửa lượt" trong chi tiết lượt (và link "Sửa lượt" trong lịch sử khách → `/quay?sua=<id>`), chỉ hiện khi server cho phép (`visit.record` + `record.void_same_day`, ngày trước chỉ chủ spa, lượt mới nhất). Giỏ "Sửa lượt khách" mở với lượt cũ (dòng, KTV, giờ làm, giảm giá dạng số tiền, hình thức thanh toán, ghi chú), banner "Đang sửa lượt HH:MM · dd/mm của X" + "Bỏ sửa". Ô liệu trình tính buổi lượt cũ đang giữ là còn dùng được. Khung dưới tổng tiền nói trước điều sẽ xảy ra: "Chưa có gì thay đổi" / "Chỉ đổi KTV, giờ làm hoặc ghi chú: lưu thẳng, không đổi tiền" (PATCH) / khung cảnh báo "lượt cũ được hủy và ghi lại" + **"Thu thêm 50.000đ" / "Hoàn 50.000đ" / "Không đổi tiền"** + ô "Lý do sửa (bắt buộc)". Nút "Lưu sửa lượt"; một Idempotency-Key cho tới khi lưu xong. Sau khi lưu: toast "Đã sửa lượt của X · Thu thêm …", danh sách có lượt cũ (badge "Đã sửa", "Thay bằng lượt mới") và lượt mới. Lỗi `409 visit.corrected` (máy khác vừa sửa) → làm mới danh sách.
- Danh sách "Lượt khách hôm nay" → ⋯ của dòng → **Hủy lượt** (vòng 5; form lý do mở ngay dưới dòng) → ReasonDialog (bắt buộc lý do) → lượt hiển thị gạch ngang + nhãn "Đã hủy: <lý do>", buổi đã trừ được cộng lại, toast xác nhận.
- Hóa đơn bán gói chưa dùng buổi nào → **Hủy hóa đơn**; đã dùng → nút bị khóa, tooltip "Đã dùng buổi, dùng Hủy liệu trình (chủ spa)".
- Ngày đã chốt → nút Hủy chỉ hiện với chủ spa (không hỏi lại mật khẩu, EM 2026-11-05); Quản lý thấy "Ngày đã chốt, liên hệ chủ spa".

## US7: Vòng đời liệu trình (chủ spa)
- PackageRow → menu ⋯ → **Gia hạn** (chọn số tháng + lý do) / **Hủy & hoàn tiền** (số tiền ≤ đã trả, hình thức, lý do).
- Quản lý không thấy các mục này (UX); server vẫn trả `403` nếu gọi thẳng.

## US8: Nhập liệu trình dở dang (`/nhap-so-cu`)
- Form tối ưu nhập liên tục: khách (tìm/tạo) → gói (chọn từ bảng giá **hoặc** "Gói khác" nhập tay tên + dịch vụ) → tổng buổi gốc · **còn lại** · giá đã bán · **còn nợ** · ngày đăng ký · HSD · quà.
- "Lưu & nhập tiếp" giữ form mở, focus về ô khách. Bộ đếm "Đã nhập N liệu trình hôm nay".
- Validation: còn lại ≤ tổng; nợ ≤ giá bán. Bản ghi có badge **"Nhập từ sổ cũ"** ở mọi nơi.
- Thông báo rõ: "Không tính vào doanh thu hôm nay".

## US1 (bảng giá; chủ spa), để tham chiếu
`/bang-gia`: danh sách theo nhóm, mỗi dịch vụ hiển thị giá chung hoặc Nữ/Nam ("—" = không áp dụng), ★, thời lượng ("theo tư vấn"). Ngừng bán thay cho xóa. Tab "Gói liệu trình": nhiều dòng dịch vụ × số buổi, buổi tặng, giá, HSD tháng, quà.

## States chung
| State | Thể hiện |
|---|---|
| Loading | Skeleton đúng hình dạng nội dung; nút đang gửi hiện spinner, không bấm lại được |
| Empty | Câu hướng dẫn hành động tiếp theo (không chỉ "Không có dữ liệu") |
| Error | Thông báo tiếng Việt, không lộ mã kỹ thuật; luôn giữ dữ liệu đã nhập; có "Thử lại" |
| Success | Toast tóm tắt kết quả + cập nhật tại chỗ |
| 401 | Về màn hình đăng nhập, sau đó quay lại đúng trang (giỏ chưa lưu được giữ trong sessionStorage) |
| 403 | "Bạn không có quyền thực hiện thao tác này", không lộ dữ liệu |

## Lịch hẹn vòng 4
Nguồn: [EM review vòng 4](../../raw/2026-11-05-em-review-round4.md) mục 1, 5, 6, 8; quy tắc ở [spec R1-3](../product/specs/appointments.md).

**Bố cục `/quay` (từ vòng 4):** hàng đầu = nhóm nút **Ghi lượt khách | Lịch hẹn** (`?view=lich`, URL state, Back về tab trước) + đồng hồ "Còn hh:mm:ss để nhập số liệu hôm nay" bên phải (đồng hồ máy tới 0 giờ giờ spa; ghi chú khi máy khác múi giờ; chủ spa thấy "vẫn nhập bù được"). Tab **Ghi lượt khách** là mặc định: fast path ≤ 4 thao tác không đổi (e2e `front-desk.spec.ts`).
```
┌ [Ghi lượt khách][Lịch hẹn]                       Còn 03:12:45 để nhập số liệu hôm nay ┐
│ Tìm khách ──────────────────────────┐ │ Lượt khách đang ghi                         │
│ [🔍 SĐT / tên]                       │ │ (banner "Hoàn tất lịch hẹn 10:00" nếu có)   │
│ Nguyễn Thị Hà · Nữ      [Đổi khách] │ │ dòng dịch vụ + KTV · + dịch vụ lẻ           │
│ 0905 111 222 · 1 gói, còn 5 buổi    │ │ Giờ làm: bây giờ · Đổi giờ làm / nhập bù    │
│ ⚠ Lưu ý sức khỏe                     │ │ + Giảm giá dịch vụ lẻ (% | số tiền, lý do)  │
│ Gói liệu trình: … [Trừ 1 buổi]      │ │ Phải trả ~~399.000đ~~ 359.100đ · TM/CK      │
└──────────────────────────────────────┘ │ [ Lưu lượt khách ]                          │
┌ Lượt khách hôm nay (toàn chiều rộng) · Xem ngày [dd/mm/yyyy] ───────────────────────┐
│ 09:12 │ Nguyễn Thị Hà · Nữ │ Gội … (trừ buổi) · KTV Lan │ Trừ liệu trình, không phát sinh│
│   ▸ bấm dòng → chi tiết inline: dòng + KTV + tiền, giảm giá, SĐT, ⚠, "Mở hồ sơ khách"  │
└──────────────────────────────────────────────────────────────────────────────────────┘
```
- **Thông tin khách ngay dưới tên vừa chọn** (cùng ô tìm, không còn thẻ riêng): tên + giới tính, SĐT, "N gói liệu trình đang dùng, còn M buổi", lưu ý sức khỏe, gói + "Trừ 1 buổi", nút "Đổi khách" (= Esc).
- **Lượt khách hôm nay** rộng toàn trang. **Vòng 5 (EM mục 1: "không hàng không lối")**: bảng thật, cột cố định `Giờ 80px · Khách 24% · Dịch vụ · KTV (phần còn lại, tối đa 2 dòng + "…", KTV dòng dưới) · Thanh toán 16% (Tiền mặt / Chuyển khoản / Tiền mặt + CK / Trừ liệu trình / Đã hoàn tiền) · Số tiền 112px canh phải tabular · ⋯ 60px`. Bỏ nút "Chi tiết": **bấm vào dòng** (hoặc Enter trên tên khách, là nút `aria-expanded`) mở/đóng chi tiết ngay dưới dòng; ⋯ (`RowMenu`) gồm "Mở hồ sơ khách" và "Hủy lượt" (chỉ khi có quyền + lượt chưa hủy). Lượt đã hủy: dòng mờ, số tiền gạch ngang, badge "Đã hủy" cạnh tên, **lý do nằm trong chi tiết** (hủy xong chi tiết tự mở). Dưới 1024px: thẻ (tên, giờ · dịch vụ, số tiền, thanh toán, ⋯), bấm thẻ = mở chi tiết. "Mở hồ sơ khách" đi qua trail nên nút quay lại trên hồ sơ ghi tên màn quầy. Ô ngày dùng `DateField` (luôn dd/mm/yyyy).
- **Giờ làm:** mặc định "bây giờ" (một dòng chữ, fast path không thêm thao tác). "Đổi giờ làm" mở ô giờ; chủ spa (`visit.backdate`) có thêm ô ngày (lùi ≤ 366 ngày) + ghi chú "Nhập bù: lượt, hóa đơn và tiền thu được ghi vào ngày …"; toast sau khi lưu ghi "nhập bù ngày …".
- **Giảm giá:** chỉ hiện khi có dòng lẻ và có `sale.discount`; % hoặc số tiền + lý do; xem trước "Giảm X", tổng gạch giá cũ; quản lý vượt 10% thấy "cần chủ spa" (server quyết định).

**Tab Lịch hẹn** (chunk lazy, không có trong lần vẽ đầu):
- Thanh công cụ: ‹ trước · `DateField` · sau › · "Hôm nay" · **Ngày / Tuần / Danh sách** (điện thoại: Danh sách / Tuần) · tóm tắt "N lịch · x đã đặt · y đã đến · z hoàn tất · K KTV". Nút **Đặt lịch** (`StickyActions`, dính đáy trên điện thoại).
- **Ngày (≥ 640px):** cột = KTV (chức vụ "Kỹ thuật viên") + "Chưa gán KTV"; hàng = ô 15 phút từ giờ mở cửa; cột giờ có chip **số chỗ trống** mỗi ô ("Kín" đỏ). Bấm ô trống → form đặt lịch điền sẵn giờ + KTV của cột; bấm lịch → sheet chi tiết. Lịch chồng trong một cột xếp làn cạnh nhau; lịch hủy/không đến không vẽ trên lưới (xem ở Danh sách); vạch đỏ = bây giờ. Nhiều KTV thì lưới cuộn ngang **trong khung của nó**, trang không cuộn ngang.
- **Tuần:** 7 thẻ ngày (Thứ 2 → CN): số lịch, số chưa xong, số khung kín, chip từng lịch; bấm đầu thẻ → ngày đó.
- **Danh sách** (mặc định điện thoại): thẻ theo giờ: giờ, tên + giới tính, dịch vụ, KTV, **trạng thái thanh toán**, badge trạng thái.
- **Sheet chi tiết** (`BottomSheet`): khách (+ "Mở hồ sơ"), SĐT, dịch vụ + thời lượng, KTV, **Thanh toán** ("Chưa thanh toán" / "Đã thanh toán X" / "Trừ liệu trình, không phát sinh tiền" / "Lượt đã hủy"), ghi chú, lý do. Thao tác: **KTV làm** + "Khách đã đến" (đúng ngày), "Hoàn tất & ghi lượt" (mở tab Ghi lượt với giỏ điền sẵn: dịch vụ đã đặt → trừ gói nếu còn, không thì dịch vụ lẻ theo giá giới tính; KTV của lịch), "Đổi lịch", "Không đến" (sau giờ hẹn), "Hủy lịch" (lý do bắt buộc). Khách mới: "Tạo hồ sơ khách" (form điền sẵn tên + SĐT) rồi mới hoàn tất. Ngày đã chốt (không phải chủ spa): chỉ xem.
- **Form đặt lịch** (`BottomSheet` rộng): Khách có hồ sơ (ô tìm) | Khách mới (tên + SĐT; trùng → "Dùng hồ sơ này"); dịch vụ (≤ 5); ngày; giờ bắt đầu (mỗi giờ "(còn N)" / "(kín)"); thời lượng (mặc định theo dịch vụ); KTV (không bắt buộc); ghi chú (không ghi sức khỏe). `409 staff_busy` → "KTV Lan đã có lịch 10:00–11:30 (Nguyễn Thị Hà). Chọn giờ hoặc KTV khác."
- **Màu trạng thái** (token, đủ sáng/tối, luôn kèm chữ): Đã đặt `info` · Đã đến `warning` · Hoàn tất `success` · Không đến `danger` · Đã hủy `surface-2` (+ gạch ngang ở danh sách).

**Vòng 5: lịch = dòng thời gian của quầy** ([spec §10](../product/specs/appointments.md), thay các điểm tương ứng ở trên):
- Lưới / tuần / danh sách hiện cả **lượt vãng lai** (viền nét đứt, chữ "Vãng lai") cạnh lịch hẹn. Nhãn + màu theo `phase`: Chưa đến `info` · Đang làm `warning` · Xong `success` · Không đến `danger` · Đã hủy / Lượt đã hủy `surface-2`. Tóm tắt: "Chưa đến n · Đang làm n · Xong n · Hủy/không đến n" (bỏ "K KTV"). Hôm nay tự làm mới 60 giây.
- Cột giờ: **một con số mỗi giờ** (số khách đông nhất trong giờ, chỉ khi có khách); chủ spa đặt tối đa → "n/max", đầy tô đỏ. Không còn chip "còn N"/"Kín" ở mỗi ô 15 phút. Thẻ lịch: tên + trạng thái (+ "Nhóm · n người"), dịch vụ khi thẻ cao; bỏ dòng giờ lặp. Lưới không vẽ thẻ hủy / không đến / lượt đã hủy.
- Nút **"Số khách tối đa[: n]"** (chỉ chủ spa) → sheet nhập số (trống = không giới hạn).
- Sheet lịch: nút chính **"Khách đến · ghi lượt"** (đã đến: "Ghi lượt") → giỏ điền sẵn; phụ **"Chỉ báo đã đến (ghi lượt sau)"** + KTV làm. Nhóm: danh sách "Cùng nhóm (n người)" + **"Khách đến cả nhóm · ghi n lượt"** → sheet bảng: mỗi người giỏ điền sẵn, KTV, Tiền mặt / Chuyển khoản, số thu → "Ghi n lượt"; dòng nào lỗi hiện lỗi tại dòng, bấm lại chỉ gửi người chưa ghi.
- Sheet lượt vãng lai: khách (+ Mở hồ sơ), dịch vụ, KTV, thanh toán (không ghi chú buổi).
- Form đặt lịch: **chọn dịch vụ = thêm** (bỏ nút "Thêm"); thời lượng theo dịch vụ, tự tính lại tới khi chỉnh tay ("Đã chỉnh tay · theo dịch vụ: X phút · Dùng lại theo dịch vụ"); **"+ Thêm người (gia đình, nhóm)"** → mỗi người một khung "Người n" (khách, dịch vụ, thời lượng, KTV, "Bỏ người này"), giờ hẹn chung; "Giờ hẹn" mỗi giờ "· n khách" (hoặc "· n/max khách"); nút "Đặt lịch" / "Đặt lịch n người" luôn bấm được: thiếu → "Còn thiếu: …", lỗi server → câu tiếng Việt nêu đúng người; vượt tối đa → cảnh báo, vẫn đặt được.
