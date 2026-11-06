---
title: Front-desk screen flows (spec R1-1)
updated: 2026-11-06
sources: [product/specs/front-desk-and-packages.md, design/design-system.md, raw/2026-11-06-em-ui-redesign-direction.md]
---

> **Redesign pilot /quay (2026-11-06, [design-system § Quy tắc](design-system.md#quy-tắc-rulebook-2026-11-06--áp-cho-mọi-màn-làm-việc)).** EM: "quá xấu UI lỗi tùm lum và sao có cái vụ lưu như khách vãng lai là sao nhỉ". Thay đổi (hành vi, quyền, fast path giữ nguyên):
> - **Thanh trên:** Ghi lượt khách | Lịch hẹn · bên phải **một dòng** "🕒 Còn hh:mm:ss để nhập số liệu hôm nay" + nút ⓘ (toggletip: quản lý khóa sau 0 giờ; chủ spa vẫn nhập bù; giờ máy khác giờ spa).
> - **Hai panel từ 1024px:** trái = tìm khách + khách đang chọn + gói; phải = lượt đang ghi. Dưới 1024px xếp dọc. Ô tìm và **Khách mới** cùng một hàng; phím tắt nằm sau nút "?" (ẩn dưới 640px). Chọn khách xong danh sách kết quả ẩn (gõ lại thì hiện), Enter lần nữa không chọn lại.
> - **Gói: một trạng thái mỗi dòng.** Đang dùng: "Còn X/Y buổi · HSD" (+ chip "Sắp hết buổi" khi ≤ 2) + **Trừ 1 buổi**. Hết hạn còn buổi: chip "Hết hạn"; có `package.use_expired_override` → Trừ 1 buổi mở form lý do, không có → dòng chữ "Đã hết hạn: cần quản lý cho dùng tiếp." (không nút). **Đã hủy / hết buổi** (kể cả một dịch vụ của combo đã hết) → gom vào disclosure "Gói đã hết / đã hủy (n)", chip "Đã hủy"/"Hết buổi", "Đã dùng X/Y buổi", **không bao giờ có Trừ 1 buổi** (thay dòng cũ: badge "Đã hết buổi" + nút aria-disabled + toast). Không còn gói dùng được → "Gợi ý: tư vấn bán gói mới". Bỏ viền vàng trái và thanh số buổi.
> - **Giỏ:** dòng dịch vụ (✕ để bỏ, KTV mỗi dòng cùng một lưới); **chọn dịch vụ lẻ là thêm** (bỏ nút "Thêm", như sửa lỗi đặt lịch #44; bước ↑/↓ trên select đóng chờ Enter); Giờ làm / Giảm giá thành hai dòng "nhãn · giá trị · một nút ghost" ("Đổi giờ làm" / "Đổi giờ / nhập bù" cho chủ spa, "Dùng giờ hiện tại"; "Thêm giảm giá" / "Bỏ giảm giá"); thanh toán = 3 chip radio (Tiền mặt · Chuyển khoản · Tiền mặt + CK); ghi chú buổi; **checkout dính đáy**: Phải trả + nút "Lưu lượt khách" rộng hết.
> - **Giỏ từ lịch hẹn:** thanh ngữ cảnh "Đang ghi cho lịch hẹn 15:00 · Nguyễn Thị Lan Anh" (ngày chỉ hiện khi khác hôm nay). "Ghi như khách vãng lai" (gây khó hiểu) chuyển vào menu ⋯ của thanh: **"Bỏ liên kết lịch hẹn"** + giải thích "Lịch hẹn vẫn mở (chưa hoàn tất). Lượt này được ghi riêng, như khách đến không hẹn." Spec lịch hẹn không có luồng nào cần tách; giữ vì hiếm khi cần (mở nhầm lịch) và không mất gì.
> - Toast lên trên cùng giữa màn (không che checkout). Ảnh trước/sau: scratchpad của agent `redesign-quay/before|after` (1347 sáng/tối, 1024, 390 sáng/tối × trống / chọn khách / giỏ / từ lịch hẹn).

> **Vòng 6 (draft, 2026-11-06):** bán gói trong giỏ, "Dùng gói này cho buổi hôm nay", "Chuyển sang gói liệu trình", "Thêm dịch vụ vào lượt" — xem [spec R1-1 §10](../product/specs/front-desk-and-packages.md#10-vòng-6-upsell-trong-lượt-và-chuyển-sang-gói-liệu-trình). Lưu ý: dòng "giỏ chưa lưu được giữ trong sessionStorage" (bảng States chung, 401) **chưa có trong code** (`FrontDeskPage.tsx` giữ giỏ bằng `useState`); §10.4.1 đưa vào phạm vi.

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
- Thanh công cụ: ‹ trước · `DateField` · sau › · "Hôm nay" · **Ngày / Tuần / Danh sách** (điện thoại: Danh sách / Tuần) · tóm tắt "N lịch · x đã đặt · y đã đến · z hoàn tất · K KTV". Nút **Đặt lịch** (`StickyActions`, dính đáy trên điện thoại). **Vòng 6** ([spec §10.9](../product/specs/appointments.md)): từ 640px nút ở góc trên bên phải, cạnh tiêu đề; điều khiển cao 44px một hàng; ô "Xem theo KTV" (`ktv` trên URL); lưới ngày = lớp ô + lớp sự kiện (thẻ không thay ô, cao đúng giờ thật, chồng giờ chia làn đều), cột KTV bằng nhau, hàng tên KTV dính dưới thanh trên cùng khi trang cuộn, nhiều cột thì khung cuộn ngang với cột giờ dính (không cuộn dọc lồng).
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

**Redesign Lịch hẹn (2026-11-06, theo [rulebook](design-system.md), thay các điểm hình thức ở trên; hành vi + lớp sự kiện vòng 6 giữ nguyên):**
- Chip trạng thái = bộ `.badge` chung (`PHASE_BADGE` trong `features/appointments/api.ts`): Chưa đến `badge-info` · Đang làm `badge-warn` · Xong `badge-ok` · Không đến `badge-danger` · Đã hủy / Lượt đã hủy `badge-muted`. Bỏ `.appt-badge` riêng. Thẻ trên lưới giữ màu phase nhưng **viền 1px đều bốn cạnh** (bỏ sọc trái 4px), tên 13/18, dòng trạng thái + dịch vụ 12/16; lượt vãng lai viền nét đứt cả bốn cạnh.
- Lưới ngày: cột giờ 104px (chip "n khách" không chạm cột đầu), tên cột 13px 600 trên nền `surface`, giờ đã qua chỉ phủ nhẹ `surface-2` 55% (không còn mảng màu), giờ đầy (n/max) chip **hổ phách** (cần chú ý, không phải lỗi). Khung lưới có `shadow-1` như panel.
- Tuần: 7 panel, mỗi lịch một **dòng** (giờ + chip, tên dưới), kẻ mảnh giữa các dòng (không còn thẻ trong thẻ, không sọc); hôm nay: viền `primary` + "· Hôm nay" ở tiêu đề.
- Danh sách: **một panel**, dòng kẻ mảnh; lịch đã hủy / không đến / lượt đã hủy gom sau disclosure "Đã hủy / không đến (n)" (mở sẵn khi chỉ còn chúng).
- Trống: một dòng phía trên lịch "**Chưa có lịch hẹn hôm nay / ngày này / tuần này** [của KTV đang xem]. Bấm một ô giờ trống bên dưới hoặc nút bên cạnh để đặt lịch." + nút phụ **"Đặt lịch hẹn"** (không trùng tên nút chính "Đặt lịch"). Đang tải: skeleton đúng hình (lưới / 7 ngày / dòng). Lỗi: "Không tải được lịch hẹn. …" + "Thử lại".
- Sheet lịch hẹn: thông tin dạng `dt/dd` 15px, các phần (Cùng nhóm, Khách đã đến, Hủy/không đến) chia bằng kẻ mảnh, không khung; thêm dòng **"Đi cùng"** khi lịch có ghi chú người đi cùng. Nút chính "Khách đến · ghi lượt" dùng `aria-disabled` (khách mới chưa có hồ sơ); "Hủy lịch" mở ô lý do (nhãn trên, gợi ý dưới, thiếu → lỗi dưới ô + `aria-invalid`), nút xác nhận **`btn-danger`**; mọi nút gửi có `aria-busy`.
- Form đặt lịch: mẫu ô 44px chữ 16px nhãn trên + gợi ý/lỗi dưới; mỗi người là một `fieldset` chia bằng kẻ mảnh (không khung); dịch vụ đã chọn là các dòng có thời lượng ("30 phút" / "kèm") + nút ✕ 44px; **"+ Người đi cùng (ghi chú)"** (tối đa 5, 1–100 ký tự, bỏ dòng trống khi gửi; `companions` của `POST /appointments` hoặc `/appointments/group`; sửa lịch chưa sửa được danh sách này vì `PATCH /appointments` chưa nhận `companions`). Tóm tắt "dd/mm · 10:00–11:30 (90 phút) · n người · k người đi cùng · chưa thanh toán" trên nền `surface-2`.
- "Khách đến cả nhóm": mỗi người một dòng kẻ mảnh, giỏ đang chuẩn bị = skeleton, đã ghi = chip `badge-ok`, thiếu KTV = gợi ý dưới dòng.
- "Số khách tối đa": ô theo mẫu field (gợi ý "Để trống = không giới hạn…", lỗi dưới ô), nút "Lưu số khách tối đa" `aria-busy`.

## Vòng 6: giỏ bán gói, thêm dịch vụ, chuyển sang gói ([spec §10](../product/specs/front-desk-and-packages.md))
**Thứ tự build:** chờ PR #56 (redesign pilot `/quay`) merge, rồi dựng trên component và quy tắc mới của [design-system](design-system.md). Mọi UI mới của vòng này nằm trong chunk lazy (ngân sách khởi động ~124/125 KB gzip đã gần đầy): khối "Gói bán hôm nay", sheet "Bán gói cho lượt này", sheet "Thêm dịch vụ" chỉ tải khi bấm.

**Giỏ của lượt (3 phần, một tổng, một nút):**
1. *Dịch vụ hôm nay*: mỗi dòng có dịch vụ · KTV làm · nguồn (trừ gói / lẻ / **trừ gói vừa mua**) · giá. Dòng lẻ có thêm ô **"Người giới thiệu"** (select: Khách tự chọn [mặc định] · từng nhân viên kèm chức vụ). Dòng trừ gói không có ô này.
2. *Gói bán hôm nay*: "+ Bán gói liệu trình" mở khối inline gồm mẫu gói, **Người giới thiệu / chốt** (mặc định KTV của dòng hôm nay khớp gói, không có thì KTV dòng đầu; có "Khách tự chọn"), quà, giảm giá gói (quản lý ≤ 10 % của gói, lý do bắt buộc), "Mua cho người khác" (tìm khách có hồ sơ). Ô **"Dùng gói này cho buổi hôm nay"** chỉ hiện khi mẫu gói chứa dịch vụ đang ở dạng lẻ trong giỏ; bật thì dòng đó đổi sang "Trừ gói (mua hôm nay)", giá 0đ, xem trước "còn 11/12". Gói mua cho người khác thì ô này ẩn (gói không dùng chung).
3. *Người đi cùng (ghi chú)*: "+ Người đi cùng" mở ô chữ ("Chồng của chị Hà"), tối đa 5; không tính tiền, không chọn dịch vụ.
- Tổng: "Phải trả: lẻ X + gói Y = Z" · TM / CK / TM + CK. Một request `POST /visits` (hoặc `/appointments/{id}/complete`). Chỉ có gói, không có dịch vụ: nút "Lưu hóa đơn gói" gọi `POST /sales`.
- **Giỏ nháp:** lưu `sessionStorage` khóa `vpyd.cart.v1:<userId>:<customerId>` (try/catch; chỉ id, số lượng, lựa chọn; không ghi chú sức khỏe, không giá ngoài cái trang đang hiện). Xóa khi lưu thành công, bấm "Hủy giỏ", đăng xuất. Tải lại trang: banner "Đã khôi phục giỏ chưa lưu" + "Bỏ".

**Lượt khách hôm nay, menu ⋯:**
- **"Thêm dịch vụ / bán gói"**: sheet giỏ ở chế độ thêm (chỉ thêm, không lý do), gọi `POST /visits/{id}/additions`. Mặc định người giới thiệu của dòng thêm = KTV của lượt (đây là upsell).
- **"Bán gói cho lượt này"**: sheet gồm mẫu gói (ưu tiên gói chứa dịch vụ lẻ của lượt), người chốt, quà, giảm giá. Ô **"Tính buổi hôm nay vào gói"** (mặc định **tắt**, Q-U1) chỉ hiện khi gói chứa dịch vụ lẻ của lượt; bật thì hiện "Hôm nay khách đã trả 49.000đ (TM) cho Gội đầu · Khách đưa thêm 1.451.000đ" và gọi `convert-to-package` (If-Match); tắt thì bán gói thường qua `additions`, lượt lẻ giữ nguyên.
- Lượt của ngày trước: chỉ chủ spa thấy; tháng đã chốt lương thì nút tắt + "Tháng đã chốt lương" (server vẫn `409`).

**States:** đang lưu (nút khóa + spinner); lỗi tại khối gói ("Vượt 10 %, cần chủ spa", "Mẫu gói đã ngừng bán"); `409 visit.corrected`: "Lượt đã được sửa ở máy khác, mở bản mới"; `412`: tải lại lượt; thành công: toast "Đã lưu lượt + bán Gói … (còn 11/12)". Bàn phím: khối gói mở/đóng bằng Enter, focus vào mẫu gói; select người giới thiệu có nhãn.

**Đã làm trong PR phần A (không đụng file của pilot):** client API (`features/front-desk/upsell.ts`), giỏ nháp (`draftCart.ts`, có test), "Khách tự chọn" ở hộp thoại bán gói, phiếu lương tách "Hoa hồng giới thiệu: gói / dịch vụ", sổ doanh thu có cột "Giới thiệu".
