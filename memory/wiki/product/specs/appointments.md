---
title: "Spec R1-3: Lịch hẹn, giờ làm thực tế, nhập bù của chủ spa, giảm giá lượt (vòng 4 + vòng 5)"
status: draft (lead quyết mặc định; câu hỏi EM ở §9 và §10.8; §10.11 theo quyết định EM 2026-11-07)
updated: 2026-11-07
sources: [raw/2026-11-05-em-review-round4.md, raw/2026-11-05-em-simplify-owner-and-opening-balance.md, raw/2026-11-06-em-review-round5.md, raw/2026-11-07-em-appointments-live-test.md]
related: [front-desk-and-packages.md, ../../design/front-desk-flows.md, ../../architecture/backend.md, ../../security/authz-matrix.md]
---

# Spec R1-3: Lịch hẹn & quầy vòng 4

Phạm vi: mục **1, 5, 6, 8** của [EM review vòng 4](../../../raw/2026-11-05-em-review-round4.md). Spec R1-1 ([front-desk-and-packages](front-desk-and-packages.md)) vẫn đúng; spec này **mở rộng** US4 và bỏ "lịch hẹn (R3)" khỏi out-of-scope của R1-1.

## 1. Goal
- Quầy biết **trước khi khách đến** ai sẽ đến, giờ nào, KTV nào rảnh. Với 3–4 KTV, quầy không nhận quá số khách một khung giờ làm được.
- Lượt khách lưu **giờ làm thật** (không phải giờ bấm lưu). Chủ spa nhập bù được lượt của ngày đã qua.
- Giảm giá dịch vụ lẻ ngay trong lượt, cùng quy tắc với bán gói (D10).
- Danh sách lượt hôm nay rộng hơn, bấm vào xem được khách.

## 2. Vòng đời lịch hẹn
```
            ┌────────── cancelled (lý do) ──────────┐
 booked ──▶ arrived (KTV được gán) ──▶ done (= 1 lượt khách: trừ buổi + thu tiền)
   │  └──────────── done (đi thẳng) ───────────────┘
   └──▶ no_show (lý do, chỉ sau giờ hẹn)
```
- **booked (Đã đặt):** khách có hồ sơ **hoặc** khách mới (tên + SĐT, chưa tạo hồ sơ); 1–5 dịch vụ; KTV mong muốn (không bắt buộc); giờ bắt đầu; thời lượng = tổng `duration_min` của dịch vụ (trống = 60 phút), **đúng số phút, không làm tròn theo ô** (quyết định A, §10.11; trước đây làm tròn lên bội số của ô giờ), quầy sửa được (5–480 phút); ghi chú lịch hẹn (≤ 500 ký tự, **không** ghi thông tin sức khỏe: chỗ đó là ghi chú buổi / hồ sơ).
- **arrived (Đã đến / đang làm):** bắt buộc chọn KTV. Chỉ trong ngày của lịch hẹn.
- **done (Hoàn tất):** quầy mở giỏ lượt khách đã điền sẵn khách, dịch vụ, KTV → bấm Lưu. Server ghi lượt **bằng đúng use case `POST /visits`** (trừ buổi, giá, thu đủ, giảm giá, audit) trong cùng transaction với việc đổi trạng thái lịch hẹn; lịch hẹn giữ `visit_id`. Không có quy tắc nghiệp vụ nào bị chép lại.
- **no_show (Không đến)** và **cancelled (Đã hủy)**: bắt buộc lý do, giữ trong lịch (mờ), giải phóng ô giờ.
- **Khách vãng lai** (đến đột ngột): bỏ qua lịch hẹn, ghi thẳng lượt như cũ (fast path ≤ 4 thao tác, AC4.*).
- Khách mới trong lịch hẹn: khi đến, quầy bấm "Tạo hồ sơ" (form khách điền sẵn tên + SĐT) rồi mới hoàn tất được. SĐT đã có hồ sơ → `409 customer.phone_taken` kèm `existing_id`, quầy chọn hồ sơ đó.

### Sức chứa (capacity)
- **KTV:** một KTV không có hai lịch `booked`/`arrived` chồng giờ. Vi phạm → `409 appointment.staff_busy`, `existing_id` = lịch hẹn đang chiếm chỗ (web nêu giờ + tên khách).
- **Khách (từ 2026-11-07, §10.11):** một khách không có hai lịch mở chồng giờ → `409 appointment.customer_busy` + `existing_id`.
- ~~**Cả spa:** tại mọi thời điểm, số lịch `booked`/`arrived` ≤ số KTV đang làm. Vượt → `409 appointment.slot_full`.~~ **Bỏ ở vòng 5 (§10.4):** số khách tối đa mỗi khung giờ là cài đặt tùy chọn của chủ spa, chỉ cảnh báo.
- **KTV** = nhân viên đang làm có chức vụ chứa "kỹ thuật" hoặc "KTV" (không phân biệt hoa thường); spa chưa ghi chức vụ nào như vậy thì mọi nhân viên đang làm. Danh sách lấy qua service của `GET /staff` (crate `org`).
- Ô giờ **15 phút**; giờ mở cửa mặc định **09:00–21:00**, cấu hình theo spa ở `tenant.settings.opening_hours = {"open":"09:00","close":"21:00"}` và `settings.slot_minutes`. Giờ bắt đầu phải rơi đúng ô; lịch phải nằm trọn trong giờ mở cửa (`422 appointment.outside_opening_hours`, `starts_at not_on_slot`).
- Hai lần đặt cùng lúc: khóa `pg_advisory_xact_lock` theo (spa, ngày) rồi mới kiểm, nên không lọt; ràng buộc `EXCLUDE USING gist` (staff + khoảng thời gian) ở DB là lớp thứ hai.

### Ngày & quyền
- Quản lý: đặt lịch cho hôm nay → 90 ngày tới. Mọi thay đổi lịch hẹn của **ngày đã qua** theo quy tắc ngày đã chốt: Quản lý `403 day.locked`, chủ spa được (không nhập lại mật khẩu). **Từ 2026-11-07 (§10.11): không ai đặt lịch mới / dời lịch sang ngày đã qua, kể cả chủ spa** (`422 appointment.in_the_past`); việc đã làm thì nhập bù lượt ở Quầy.
- Chủ spa: được mọi thứ, kể cả hoàn tất lịch hẹn của ngày cũ thành lượt nhập bù (§3).
- Permission mới **`appointment.manage`** (đọc + ghi lịch): Owner, Manager. **KTV: không có** (chưa có liên kết tài khoản ↔ nhân viên nên "chỉ lịch của mình" chưa làm an toàn được; xem Q8).

## 3. Giờ làm thực tế & nhập bù (mục 5)
- `visit.check_in_at` **là giờ làm dịch vụ** (khác `created_at` = lúc ghi). Quầy chọn được giờ làm (mặc định = bây giờ); ngày nghiệp vụ của lượt = ngày của giờ làm theo Asia/Ho_Chi_Minh.
- Quản lý: giờ làm phải trong **hôm nay** và không quá 5 phút sau bây giờ (giữ nguyên `422 visit.check_in_not_today / check_in_in_future`).
- **Chủ spa** (permission mới **`visit.backdate`**): giờ làm có thể ở ngày đã qua, tối đa 366 ngày (`422 visit.check_in_too_old`). Lượt, hóa đơn dịch vụ lẻ và payment đều mang **ngày của giờ làm**, nên báo cáo, sổ doanh thu, lịch sử khách xếp đúng ngày làm. Hạn liệu trình được xét **tại ngày làm**. Audit `visit.create` ghi `backdated: true` + `recorded_on` (ngày nhập).
- **Đồng hồ đếm ngược** trên `/quay`: "Còn hh:mm:ss để nhập số liệu hôm nay", tính bằng đồng hồ của máy tới 0 giờ theo giờ spa (Asia/Ho_Chi_Minh). Máy đặt múi giờ khác → ghi chú "Giờ máy khác giờ spa (GMT+7), đồng hồ đếm theo giờ spa". Với chủ spa chỉ để tham khảo ("Chủ spa vẫn nhập bù được sau 0 giờ"). Server vẫn là nơi quyết định ngày.

## 4. Thanh toán ("đã thanh toán chưa")
- R1 vẫn **thu đủ khi hoàn tất** (R1 không có công nợ). Lịch `booked`/`arrived` = **Chưa thanh toán**; `done` = trạng thái của lượt: **Đã thanh toán X (TM/CK)**, **Không phát sinh tiền** (chỉ trừ buổi) hoặc **Lượt đã hủy**. Hiện ở thẻ lịch hẹn, danh sách và danh sách lượt.
- Trả sau / ghi nợ: **không làm**, là câu hỏi Q1.

## 5. Giảm giá trong lượt (mục 8)
- Tùy chọn trên **tổng các dòng dịch vụ lẻ** của lượt: theo **%** (1–100) hoặc **số tiền**; bắt buộc lý do. Dòng trừ liệu trình không giảm (đã trả khi mua gói).
- Server tính: `% → floor(tạm tính × % / 100)`; ≤ tạm tính; Quản lý ≤ `manager_discount_max_pct` (mặc định 10%, `403 discount.limit_exceeded`), chủ spa không giới hạn (`sale.discount_unlimited`); thiếu lý do → `422 discount.reason_required`; không có dòng lẻ → `422 discount.amount exceeds_subtotal`.
- Số giảm chia cho từng dòng lẻ theo tỷ lệ (`allocate_discount`, như bán gói) → `visit_line.discount_amount`; `line_value` = thành tiền − phần giảm, nên top dịch vụ, lịch sử khách và hoa hồng R2 đều theo **số tiền thật thu**. Hóa đơn: `subtotal`, `discount_amount`, `discount_reason`, `total`; sổ doanh thu hiện lý do giảm ở cột ghi chú.

## 6. Acceptance criteria
- **AC9.1** Đặt lịch cho khách có hồ sơ, 1 dịch vụ 60 phút, 10:00 → lịch `booked` 10:00–11:00, hiện ở cột KTV đã chọn (hoặc "Chưa gán KTV").
- **AC9.2 (neg)** KTV Lan đã có 10:00–11:00, đặt Lan 10:30 → `409 appointment.staff_busy` + `existing_id`; đặt 11:00 thì được.
- ~~**AC9.3 (neg)** 2 KTV, đã có 2 lịch 10:00–11:00 (chưa gán), đặt thêm lịch chồng → `409 appointment.slot_full`.~~ Thay bằng AC10.4 (vòng 5).
- **AC9.4 (neg)** 08:30 khi mở cửa 09:00, hoặc 10:07 → `422`.
- **AC9.5** Khách đến → chọn KTV → `arrived`; hoàn tất với giỏ trừ 1 buổi → lượt khách được ghi (còn 4/12), lịch `done` + `visit_id`; replay cùng `Idempotency-Key` không trừ thêm.
- **AC9.6 (neg)** Hoàn tất lịch của khách mới chưa có hồ sơ → `422 appointment.customer_required`; hoàn tất lần hai → `409 appointment.closed`.
- **AC9.7** Hủy / không đến kèm lý do → ô giờ được giải phóng (đặt lại được ngay); thiếu lý do → `422`; "không đến" trước giờ hẹn → `422 appointment.not_started`.
- **AC9.8 (neg)** Quản lý đổi / hủy / hoàn tất lịch của hôm qua → `403 day.locked`; chủ spa được.
- **AC9.9 (neg)** KTV, người ngoài, spa khác: `403` / `401` / `404` trên mọi route lịch hẹn.
- **AC5.1** Quản lý ghi lượt giờ làm 08:10 hôm nay → `check_in_at` 08:10, `created_at` lúc lưu.
- **AC5.2 (neg)** Quản lý ghi lượt cho hôm qua → `422 visit.check_in_not_today`; chủ spa → `201`, lượt + hóa đơn + payment mang ngày hôm qua, báo cáo ngày hôm qua tăng.
- **AC5.3** Đồng hồ đếm ngược đúng khi máy ở múi giờ khác (unit test).
- **AC8.1** Quản lý giảm 10% lượt có 1 dịch vụ lẻ 200.000đ → hóa đơn 180.000đ, thu 180.000đ; 11% → `403 discount.limit_exceeded`; chủ spa 50% → được.
- **AC8.2 (neg)** Giảm không lý do → `422`; % và số tiền cùng lúc → `422`.
- **AC6.1** Bấm một dòng lượt hôm nay → hiện chi tiết (dịch vụ, KTV, thanh toán, SĐT) + link "Mở hồ sơ".

## 7. UX (chi tiết ở [front-desk-flows](../../design/front-desk-flows.md#lịch-hẹn-vòng-4))
- `/quay` có 2 tab: **Ghi lượt** (mặc định, fast path giữ nguyên) và **Lịch hẹn** (`?view=lich`, chunk lazy, không có trong lần vẽ đầu).
- Lịch ngày kiểu Google Calendar: cột = KTV (+ "Chưa gán KTV"), hàng = ô 15 phút; ~~dòng đầu mỗi giờ ghi số chỗ trống~~ (vòng 5: mỗi giờ ghi số khách đang có, "n/max" khi chủ spa đặt tối đa, §10.4); bấm ô trống → đặt lịch; bấm lịch → panel: đổi trạng thái, khách đến, hoàn tất. Không kéo thả. Tuần: 7 ngày × số lịch + chỗ trống theo buổi. Điện thoại (< 768px): danh sách theo giờ thay cho lưới.
- Màu trạng thái: Đã đặt (xanh dương), Đã đến (vàng hổ phách), Hoàn tất (xanh lá), Không đến (đỏ, mờ), Đã hủy (xám, gạch ngang). Luôn kèm chữ, không chỉ màu. Vòng 5: màu và nhãn theo `phase` (Chưa đến / Đang làm / Xong…), §10.7.

## 8. Out of scope
Kéo thả · nhắc lịch Zalo/SMS · khách tự đặt online · lịch làm việc / nghỉ của từng KTV (thuộc quản lý nhân viên) · tài nguyên phòng/giường · công nợ.

## 9. Câu hỏi cho EM
| # | Câu hỏi | Mặc định đang làm |
|---|---|---|
| Q1 | Có cần cho khách **trả sau / ghi nợ** khi hoàn tất không? | Không: thu đủ khi hoàn tất (R1) |
| Q2 | Lịch `booked` đã qua ngày mà quầy quên đánh dấu: tự chuyển "Không đến" lúc 0 giờ? | Không tự động; hiện nhãn "Quá giờ" để quầy xử lý trong ngày |
| Q3 | Giờ mở cửa & ô giờ: 09:00–21:00, 15 phút đúng chưa? Ai sửa (màn cài đặt)? | Mặc định trên; sửa bằng dữ liệu, chưa có màn |
| Q4 | ~~Sức chứa = số KTV đang làm…~~ EM trả lời ở vòng 5: không hard-code, chủ spa tự nhập | Xem §10.4: cài đặt tùy chọn, chỉ cảnh báo |
| Q5 | Giảm giá trên **cả bill dịch vụ lẻ** đủ chưa, hay cần giảm từng dòng? | Cả bill, chia theo tỷ lệ cho từng dòng |
| Q6 | Nhập bù của chủ spa: tối đa 366 ngày, không bắt lý do. Có cần lý do không? | Không bắt lý do; audit đánh dấu `backdated` |
| Q7 | Một lịch hẹn có thể nhiều KTV (mỗi dịch vụ một người)? | Một KTV / lịch hẹn; lúc hoàn tất từng dòng vẫn chọn KTV riêng |
| Q8 | KTV xem lịch của chính mình? Cần nối tài khoản ↔ nhân viên (thuộc quản lý nhân viên) | Chưa: KTV không xem lịch |
## 10. Vòng 5: lịch = dòng thời gian của quầy (mục 3–7 của [EM review vòng 5](../../../raw/2026-11-06-em-review-round5.md))

### 10.1 Lịch và lượt khách là một (mục 3)
EM: lịch không chỉ để đặt; nó phải phản ánh cả lượt khách đã ghi để quầy biết **ai đang làm, ai chưa làm**.
- Lịch ngày / tuần / danh sách hiện **hai loại thẻ** trên cùng trục giờ:
  - **Lịch hẹn** (như vòng 4). Lịch `done` nối với lượt qua `appointment.visit_id`, lượt đó không hiện lần thứ hai.
  - **Lượt ghi thẳng** (khách vãng lai ghi ở `/quay`, không qua lịch hẹn): hiện tại giờ làm (`visit.check_in_at`), ở cột KTV của dòng dịch vụ đầu tiên, dài bằng tổng thời lượng dịch vụ (trống = 60 phút, đúng số phút từ §10.11, 5 phút – 8 giờ), nhãn "Vãng lai". Không cần đặt lịch riêng.
- **Trạng thái hiển thị** (`phase`, server tính mỗi lần đọc lịch):
  | phase | Nhãn | Từ đâu |
  |---|---|---|
  | `waiting` | Chưa đến | lịch `booked` |
  | `in_service` | Đang làm | lịch `arrived`; hoặc lượt (từ lịch hoặc ghi thẳng) mà **bây giờ < giờ làm + thời lượng** |
  | `done` | Xong | lượt mà bây giờ ≥ giờ làm + thời lượng |
  | `no_show` / `cancelled` | Không đến / Đã hủy | lịch đóng không có lượt |
  | `voided` | Lượt đã hủy | lượt bị hủy (void) |
- **Lead's decision — EM to review:** "Đang làm" **không thêm cột lưu trạng thái**. Dùng lại cột có sẵn: `appointment.arrived_at` (đã đến, chưa ghi lượt) và `visit.check_in_at` + thời lượng dịch vụ (đã ghi lượt). Lý do: không thêm thao tác "bấm xong" lúc cao điểm, không có trạng thái thứ hai phải đồng bộ với lượt (lượt là sự thật duy nhất về việc đã làm), không đổi máy trạng thái. Đánh đổi: quầy ghi lượt **lúc khách về** mà để giờ làm = bây giờ thì thẻ hiện "Đang làm" thêm một khoảng thời lượng; chọn "Giờ làm" đúng thì không sai. Muốn nút "Xong" thật thì thêm `visit.finished_at` sau (Q9).
- **Khách đến → ghi lượt luôn:** nút chính của panel lịch hẹn là **"Khách đến · ghi lượt"**: mở giỏ ghi lượt điền sẵn khách, dịch vụ (trừ liệu trình nếu còn buổi, không thì dịch vụ lẻ theo giá), KTV; lưu = `POST /appointments/{id}/complete` (đúng use case `POST /visits`), lịch giữ `visit_id`. Nút phụ **"Chỉ báo đã đến (ghi lượt sau)"** = `arrive` (chọn KTV) → thẻ "Đang làm", chưa thanh toán; khi đó nút chính là "Ghi lượt".
- Lịch có hôm nay tự làm mới mỗi 60 giây (phase đổi theo giờ) và ngay sau mỗi lần ghi / hủy lượt ở `/quay`.
- Quyền: lượt ghi thẳng chỉ có trong lịch khi người xem có **`visit.read`** (Owner, Manager); thiếu quyền → `visits: []`. Lịch chỉ mang tên khách, giới tính, dịch vụ, KTV, trạng thái thanh toán; **không** mang ghi chú buổi (dữ liệu sức khỏe) hay SĐT của lượt.

### 10.2 Thời lượng theo dịch vụ (mục 4)
- Lỗi vòng 4: dịch vụ chỉ vào lịch khi bấm "Thêm" sau khi chọn trong danh sách; chọn xong mà không bấm thì thời lượng đứng yên ở 60 phút (và lịch không có dịch vụ nên không đặt được, xem 10.5).
- Sửa: chọn một dịch vụ trong danh sách là **thêm ngay** (không còn nút "Thêm"). Thời lượng = tổng `duration_min` (trống = 60; đúng số phút từ §10.11, trước đây làm tròn lên ô giờ), **tự tính lại mỗi lần thêm / bỏ dịch vụ** cho tới khi quầy tự chọn thời lượng; khi đó hiện "Đã chỉnh tay · theo dịch vụ: 90 phút · Dùng lại theo dịch vụ". Dòng tóm tắt luôn hiện "10:00–11:30 (90 phút)".

### 10.3 Đặt lịch cho nhóm / gia đình (mục 5)
- Form "Đặt lịch hẹn" có **"+ Thêm người (gia đình, nhóm)"** (tối đa 6 người). Mỗi người: khách có hồ sơ **hoặc** khách mới (tên + SĐT), dịch vụ riêng (1–5), KTV riêng (không bắt buộc), thời lượng riêng (theo dịch vụ). Ngày + giờ hẹn chung, ghi chú chung. Hai người cùng một KTV → cảnh báo ngay trên form.
- 1 người → `POST /appointments` như cũ. ≥ 2 người → **`POST /appointments/group`** `{starts_at, note?, people[2–6]{customer_id | guest, service_ids, staff_id?, starts_at?, duration_min?}}` → `201 {group_id, appointments[]}`: **một transaction**, cùng khóa ngày, một `Idempotency-Key` (replay trả đúng response đầu). Một người sai → **không ai được lưu**; lỗi field trỏ đúng người (`people[1].service_ids`), `409 appointment.staff_busy` + `existing_id`, `409 customer.phone_taken` + `existing_id` (web tìm khách mới có SĐT đó và đổi sang hồ sơ có sẵn). `starts_at` riêng của một người phải cùng ngày (`people[i].starts_at not_same_day`).
- Mỗi lịch trong nhóm mang `group_id` (cột mới, migration 0019). Đổi lịch / hủy từng người như lịch lẻ (nhóm không khóa nhau).
- Trên lịch: thẻ cùng nhóm có nhãn **"Nhóm · n người"**; panel liệt kê cả nhóm (bấm tên để mở người khác).
- **Khách đến cả nhóm:** panel của một lịch trong nhóm (hôm nay, còn mở ≥ 2 người) có **"Khách đến cả nhóm · ghi n lượt"** → bảng: mỗi người một dòng (giỏ điền sẵn như 10.1, KTV, tiền mặt / chuyển khoản, số phải thu) → một nút "Ghi n lượt". Mỗi người vẫn là **một lượt + một thanh toán riêng** (`/complete` của từng lịch, mỗi người một `Idempotency-Key` giữ qua các lần bấm lại).
- **Lead's decision — EM to review:** ghi lượt cả nhóm **không gộp transaction**: người nào lỗi (hết buổi, ...) hiện lỗi ở dòng đó, người khác vẫn được ghi; bấm lại chỉ gửi người chưa ghi, không ghi trùng. Lý do: lúc cao điểm một người lỗi không được chặn cả gia đình. Khách mới chưa có hồ sơ, hoặc dịch vụ cần chọn giá theo giới tính → bảng bỏ qua người đó và nhắc ghi riêng (Q12).

### 10.4 Không hard-code sức chứa (mục 6)
- Con số "5" ở vòng 4 là **số KTV** (nhân viên đang làm có chức vụ KTV, `CalendarDto.capacity`), hiện ở **mọi ô 15 phút** ("còn 5") và trong danh sách giờ bắt đầu ("09:00 (còn 5)"), và server chặn `409 appointment.slot_full` theo nó. Bỏ cả ba.
- Lưới ngày: mỗi **giờ** (không phải mỗi ô 15 phút) hiện một con số: số khách đông nhất trong giờ đó (lịch chưa hủy + lượt ghi thẳng), chỉ khi có khách. Danh sách giờ hẹn: "10:00 · 2 khách". Thẻ trên lưới bỏ dòng giờ lặp ("10:00–11:30 · Đã đặt"), chỉ còn tên + trạng thái (trục giờ đã có giờ; giờ chính xác ở panel và nhãn đọc màn hình). Nhãn "Giờ bắt đầu" đổi thành "Giờ hẹn".
- **Số khách tối đa mỗi khung giờ** là cài đặt **tùy chọn của chủ spa**: nút "Số khách tối đa" trên lịch (chỉ chủ spa thấy) → `PUT /appointment-settings {max_per_slot: 1–200 | null}`, permission mới **`appointment.settings`** (Owner), audit `appointment.settings`. Trống = không giới hạn, không hiện gì thêm. Có giá trị → mỗi giờ hiện "n / max", giờ đầy tô cảnh báo, tuần hiện "k khung đủ khách", form đặt lịch cảnh báo "Khung 10:00 đã có 4/4 khách… Vẫn đặt được nếu khách chịu chờ".
- **Lead's decision — EM to review:** server **không chặn** theo sức chứa nữa (bỏ `409 appointment.slot_full`); chỉ chặn trùng KTV (`staff_busy`). Lý do: EM nói số tối đa "spa tự biết"; khách chịu chờ thì quầy vẫn nhận; chặn theo số KTV từng làm quầy không đặt được lịch hợp lệ (KTV nghỉ / đi làm thêm). Sau này "hôm nay có bao nhiêu KTV đi làm" (chấm công) có thể gợi ý con số này (Q10).
- Cột KTV giữ nguyên (KTV đang làm + "Chưa gán KTV"); tóm tắt ngày bỏ "· 5 KTV".

### 10.5 "Đặt lịch" không phản hồi (mục 7)
- **Nguyên nhân gốc** (tái hiện bằng Playwright trên stack dev, quản lý demo): chọn dịch vụ trong danh sách **chưa** thêm dịch vụ (phải bấm "Thêm" bên cạnh). Form coi như chưa có dịch vụ: nút "Đặt lịch" chỉ `aria-disabled` (trông vẫn bấm được), `onSubmit` thấy chưa đủ và **lặng lẽ return**. Không request nào tới server, không thông báo gì. Cùng lỗi làm thời lượng không đổi (mục 4). API không có lỗi.
- Sửa: chọn = thêm (10.2); nút "Đặt lịch" luôn bấm được (chỉ khóa khi đang gửi); bấm khi thiếu → "Còn thiếu: khách, dịch vụ." (theo từng người với nhóm), không gửi gì; mọi lỗi server thành câu tiếng Việt ở form: mã lỗi → câu, lỗi field → "Người 2 (Bé Na): SĐT — giá trị không hợp lệ.", ngoài giờ mở cửa kèm giờ mở cửa, lỗi mạng → "Không kết nối được máy chủ…".

### 10.6 Acceptance criteria vòng 5
- **AC10.1** Lượt ghi thẳng lúc này (KTV Tuấn, dịch vụ 90 phút) → `GET /appointments` hôm nay có nó trong `visits[]`, 90 phút, cột Tuấn, `phase = in_service`, `payment_state = paid`; lượt nhập bù hôm qua 10:00 → `done`; lượt hoàn tất từ lịch hẹn **không** có trong `visits[]`. *(test `walk_in_visits_share_the_timeline_with_bookings`)*
- **AC10.2 (neg)** Không có `visit.read` → `visits: []`; lượt bị hủy → `phase = voided`, `payment_state = voided`; không có ghi chú buổi / SĐT trong lượt của lịch; lượt của spa khác không xuất hiện (RLS + lọc tenant/branch). *(cùng test)*
- **AC10.3** `booked` → `waiting`; `arrive` → `in_service`; `complete` ngay khi đến (dịch vụ 30 phút) → `in_service`; sau thời lượng → `done` (unit test `phases_follow_status_and_service_time`).
- **AC10.4** Không có cài đặt: lịch chồng nhau (2 KTV, không gán, thêm cả lịch của Tuấn) đều `201`; `max_per_slot = null`, không còn `capacity`. Chủ spa đặt `2` → lịch trả `2`; lịch thứ 3 chồng giờ vẫn `201`; `null` xóa giới hạn; mỗi lần đổi có audit. *(tests `bookings_respect_technicians_and_the_grid`, `only_the_owner_sets_the_advisory_max_per_slot`)*
- **AC10.5 (neg)** Quản lý `PUT /appointment-settings` → `403`; `0`, `201`, `{}` → `422`; `"abc"` → 4xx; chưa đăng nhập → `401`; KTV → `403` (authz matrix).
- **AC10.6** Nhóm 3 người (Hà, Đức có hồ sơ, khách mới Bé Na) 10:00 → `201`, 3 lịch cùng `group_id`, ghi chú chung, thời lượng 90 / 30 / 60, KTV riêng; replay cùng key → cùng response, vẫn 3 lịch. *(test `a_family_is_booked_at_once_or_not_at_all`)*
- **AC10.7 (neg)** Một người trùng KTV với lịch có sẵn → `409 staff_busy` + `existing_id`, không lịch nào được lưu; 2 người cùng KTV cùng giờ → `409`; 1 hoặc 7 người → `422 people`; khách có hồ sơ 2 lần → `422 people[1].customer_id`; SĐT sai ở người 2 → `422 people[1].guest.phone`; người 2 không có dịch vụ → `422 people[1].service_ids`; sau tất cả vẫn đúng 3 lịch; KTV / người ngoài / spa khác → `403` / `401` / `404` (authz matrix).
- **AC10.8** Web (vitest): chọn dịch vụ là thêm; thời lượng 60 → 30 → 120 khi thêm dịch vụ 30 rồi 90 phút; chỉnh tay 45 rồi bỏ một dịch vụ → giữ 45 + "Đã chỉnh tay"; "Dùng lại theo dịch vụ" → 90.
- **AC10.9** Web (Playwright, form thật, `regression (round 5 item 7)`): bấm "Đặt lịch" khi trống → "Còn thiếu: khách, dịch vụ."; tìm khách → chọn dịch vụ (không bấm gì thêm) → thời lượng theo dịch vụ; 20:45 + 60 phút → câu "Lịch hẹn phải nằm trọn trong giờ mở cửa…"; 15:00 → panel lịch hẹn mở, "Chưa đến".
- **AC10.10** Web (Playwright): nhóm 2 người đặt bằng "+ Thêm người" → panel "Cùng nhóm (2 người)", thẻ "Nhóm · 2 người"; nhóm hôm nay → "Khách đến cả nhóm · ghi 2 lượt" → 2 dòng "Đã ghi lượt"; lượt ghi thẳng hiện "Vãng lai" trong danh sách; người của nhóm hiện "Đang làm"/"Xong".
- **AC10.11** Web (Playwright): lưới ngày không còn số "còn N" / "Kín" ở mỗi ô; chủ spa đặt tối đa 3 → nút "Số khách tối đa: 3", mỗi giờ "n/3"; để trống → về không giới hạn. Quản lý không thấy nút (UI theo `appointment.settings`; server 403 là ranh giới thật).

### 10.7 UX
- Màu theo phase: Chưa đến (xanh dương), Đang làm (vàng hổ phách), Xong (xanh lá), Không đến (đỏ), Đã hủy / Lượt đã hủy (xám, gạch ngang ở danh sách). Lượt vãng lai có viền nét đứt + chữ "Vãng lai". Luôn kèm chữ, không chỉ màu.
- Tóm tắt ngày: "Chưa đến n · Đang làm n · Xong n · Hủy/không đến n" (gồm cả lượt vãng lai).
- Lưới ngày ẩn thẻ đã hủy / không đến / lượt đã hủy (giải phóng chỗ); danh sách và tuần vẫn hiện.

### 10.8 Câu hỏi mới cho EM
| # | Câu hỏi | Mặc định đang làm |
|---|---|---|
| Q9 | "Đang làm" suy ra từ giờ làm + thời lượng (không có nút "Xong"). Đủ chưa? | Đủ; thêm `visit.finished_at` nếu cần |
| Q10 | Sức chứa chỉ cảnh báo, không chặn. Có muốn chặn cứng khi chủ spa đã đặt số tối đa? | Chỉ cảnh báo |
| Q11 | Trẻ em / người nhà dùng chung SĐT với người đã có hồ sơ: vẫn báo "SĐT đã có hồ sơ" như lịch lẻ. Có cần cho khách nhóm không có SĐT? | Giữ quy tắc cũ |
| Q12 | Ghi lượt cả nhóm: từng người riêng (một người lỗi không chặn người khác). Hay "tất cả hoặc không"? | Từng người riêng |

### 10.9 Vòng 6: lưới ngày = lớp sự kiện, dịch vụ kèm không cộng giờ, xem theo KTV (EM 2026-11-06)
EM: "khi 1 lịch được tạo thì UI lại bị lỗi… nó nên là 1 component trên cái calendar chứ sao lại phá đi mấy cái link"; "có option xem theo từng người không… nút đặt lịch đưa lên trên đầu"; "các component không thẳng hàng, sai width".
- **Nguyên nhân thẻ 30 phút cao ~2,5 giờ:** quy tắc "dịch vụ không có thời lượng = 60 phút" cộng cho **từng** dịch vụ. Menu thật chỉ để trống thời lượng ở nhóm **"Dịch vụ bổ sung"** (Mặt nạ thuốc bắc, Tẩy tế bào chết da đầu, Giác hơi + Đắp thuốc bắc, Thải độc đại tràng), làm kèm trong buổi chính. Gội đầu 30 phút + 2 dịch vụ kèm = 30 + 60 + 60 = 150 phút; lịch / lượt lưu đúng con số sai đó và lưới vẽ đúng con số đó. Lưới cũ còn làm nặng thêm: ô dưới thẻ bị bỏ (mất kẻ giờ, mất chỗ bấm), thẻ làm tròn theo ô và tràn quá giờ đóng cửa thì đẻ thêm hàng.
- **Quy tắc mới (server `domain::services_minutes`, web `autoMinutes`):** thời lượng = tổng thời lượng **các dịch vụ có thời lượng** (vòng 6 còn làm tròn lên ô giờ; bỏ từ §10.11). Dịch vụ trống thời lượng chỉ tính 60 phút khi **không** dịch vụ nào có thời lượng ("theo tư vấn" đứng một mình); cạnh dịch vụ có thời lượng thì là dịch vụ kèm, cộng 0 (chip trong form hiện "(kèm)"). Lịch đã lưu trước đây giữ thời lượng đã lưu (sửa lịch → "Dùng lại theo dịch vụ" để tính lại); lượt vãng lai tính lại mỗi lần đọc nên đúng ngay.
- **Lưới ngày = hai lớp** (kiểu Google Calendar): lớp ô (mọi ô của mọi cột luôn được vẽ, kẻ giờ liền) + mỗi cột KTV một lớp sự kiện; thẻ đặt tuyệt đối, cao đúng khoảng giờ thật (lượt vãng lai đúng phút giờ làm, cắt theo giờ mở cửa), phủ lên ô chứ không thay ô → giờ trống trước/sau thẻ vẫn bấm để đặt lịch. Lịch chồng giờ trong một cột chia làn đều (chỉ các thẻ chồng nhau mới chia; thẻ đứng một mình rộng cả cột, chừa 3px hai bên).
- Thẻ: tên · trạng thái (+ "quá giờ" khi đã đến mà quá giờ hẹn; vẫn giữ khoảng giờ đã đặt) · dịch vụ; thẻ 15 phút một dòng, 30 phút hai dòng; mỗi dòng cắt "…", đủ chữ ở tooltip và nhãn đọc màn hình (gồm giờ). Giữ quyết định 10.4: không có dòng giờ hiển thị trên thẻ.
- **Xem theo KTV:** ô chọn "Xem theo KTV" (Tất cả KTV / từng KTV / Chưa gán KTV), lưu trên URL (`ktv`); một KTV → lưới một cột rộng cả khung, tuần / danh sách chỉ lịch của người đó. Chỉ là bộ lọc hiển thị: kiểm tra trùng KTV khi đặt vẫn dùng cả lịch. Cột lấy từ danh sách KTV của server (không cố định số). "KTV đi làm theo ca" chưa có (chấm công chưa nối lịch) → mặc định tất cả KTV.
- **Nút "Đặt lịch"** lên thanh công cụ (bên phải, nút chính) từ 640px; điện thoại giữ `StickyActions` dính đáy (design-system: thao tác chính trong tầm ngón cái). Đang xem một KTV mà bấm → form điền sẵn KTV đó.
- **Canh hàng:** mọi điều khiển thanh công cụ cao 44px trên một hàng (nhãn "Ngày" chỉ cho trình đọc màn hình), cột KTV bằng nhau (tối thiểu 168px), trang cuộn một lần (không cuộn dọc lồng, quy tắc vòng 6 của layout) với hàng tên KTV dính dưới thanh trên cùng; nhiều cột hơn khung thì khung cuộn ngang, cột giờ dính trái; tên dài cắt "…" (lỗi cũ: chữ căn giữa tràn cả hai mép nên cột đầu như bị cắt; cột có lịch chồng được nhiều track hơn nên rộng không đều).
- Test: unit `layout.test.ts` (chiều cao = khoảng giờ, lượt lệch ô, cắt giờ mở cửa, làn), `api.test.ts` (`autoMinutes` kèm, `forStaff`), Rust `duration_comes_from_services_rounded_to_the_slot` / `a_visit_occupies_its_services_time_clamped_to_the_grid`; Playwright `regression (round 6)` (lịch 30 phút + dịch vụ kèm = 30 phút, cao 2 ô ±4px; lượt vãng lai cũng vậy; ô dưới thẻ vẫn còn, điểm ngay dưới thẻ là ô; bấm ô 10:30 cùng cột → "Đặt lịch hẹn" 10:30; cột bằng nhau ±1px, tên cột đầu không bị cắt, điều khiển cao 44px cùng hàng; nút Đặt lịch nằm trên lưới) và `view one technician's day and week`.

### 10.11 Quyết định EM 2026-11-07 + phát hiện khi test thật ([raw](../../../raw/2026-11-07-em-appointments-live-test.md))
- **A. Thời lượng chính xác.** Lịch hẹn, lượt vãng lai, giờ kết thúc, kiểm trùng KTV / khách và đếm khách mỗi giờ dùng **đúng số phút của dịch vụ**: dịch vụ 70 phút = 09:00–10:10 (trước: 10:15). Chỉ **giờ bắt đầu** nằm trên ô 15 phút; giờ kết thúc rơi đâu thì ở đó (≤ giờ đóng cửa). Thời lượng quầy tự chọn cũng chính xác: 5–480 phút (trước: bội số của ô). Server `domain::duration_minutes` / `visit_minutes` (bỏ `div_ceil`), web `autoMinutes` (bỏ tham số ô) + `lengthChoices` (ô "Thời lượng" có các bội số của ô **và** số phút theo dịch vụ, ví dụ 60 · 70 · 75). Lưới vẽ chiều cao lẻ ô (70/15 ô) bằng lớp sự kiện có sẵn. Lịch đã lưu trước giữ thời lượng đã lưu (75); "Dùng lại theo dịch vụ" để về 70. Không migration (cột `ends_at` vốn tự do).
- **B (không đổi).** "Đang làm" giữ tới hết khung giờ (§10.1).
- **Một khách, một lịch cùng lúc — lead's decision: server từ chối.** Một khách (cùng `customer_id`, hoặc cùng SĐT khách mới **ngoài nhóm của chính họ** — gia đình dùng chung SĐT, Q11) không có hai lịch `booked`/`arrived` chồng giờ → `409 appointment.customer_busy` + `existing_id`, kiểm dưới cùng khóa ngày với trùng KTV (`check_fits`). Lý do chọn chặn thay vì cảnh báo: một người không thể ở hai lịch cùng lúc; trường hợp thật gần như luôn là bấm hai lần / hai máy quầy cùng đặt (Lan Anh 09:00 ×2); chặn ở server đúng cả khi hai máy đặt cùng lúc; một người cần hai KTV cùng lúc vẫn là Q7 (một lịch + ghi chú). Người đi cùng trong nhóm là người khác nên không bao giờ trùng. Đổi lịch chỉ bị kiểm khi **đổi giờ** hoặc **gắn hồ sơ khách**, nên lịch trùng có từ trước vẫn đổi KTV / ghi chú / khách đến được (`arrive` không kiểm khách). Web báo trước khi gửi: form tìm lịch mở của cùng khách trong lịch đang tải (`customerClash`), dòng lỗi ngay dưới người đó + câu đầy đủ ở chân sheet ("Nguyễn Thị Lan Anh đã có lịch 09:00–10:10 ngày … (KTV …). Một người không có hai lịch trùng giờ…"), không gửi.
- **Lịch ở quá khứ — lead's decision: chặn ngày đã qua, hỏi lại giờ đã qua của hôm nay.** Ngày đã qua: server `422 appointment.in_the_past` cho **mọi người, kể cả chủ spa**, khi đặt mới hoặc dời sang ngày đã qua (việc đã làm = nhập bù lượt ở Quầy, `visit.backdate`). Lịch đã nằm ở ngày cũ vẫn theo quy tắc ngày đã chốt (chủ spa hoàn tất thành lượt nhập bù, AC9.8). Giờ đã qua **của hôm nay**: server vẫn nhận (người đi cùng thêm vào lịch đang làm, lịch ghi muộn vài phút; server cũng không nên dựa vào đồng hồ máy quầy), web hỏi một lần: giờ hẹn có "· đã qua", gợi ý dưới ô, bấm "Đặt lịch" → chân sheet "Giờ hẹn 09:00 đã qua (bây giờ 10:20). Khách đã làm xong thì ghi lượt nhập bù ở Quầy; khách đang làm hoặc sắp tới thì bấm “Vẫn đặt lịch”." → nút đổi thành "Vẫn đặt lịch". Đổi lịch mà giữ giờ thì không hỏi.
- **Lỗi form không còn nằm ngoài màn hình.** Một thông báo duy nhất **ghim ở chân sheet, ngay trên nút** (`.appt-footer-note`, `role=alert` cho lỗi, `status` cho cảnh báo), theo thứ tự: lỗi server → còn thiếu → ngày đã qua → khách trùng → câu hỏi giờ đã qua → trùng KTV trong nhóm → đủ khách. Bấm "Đặt lịch" khi thiếu → cuộn tới và focus ô thiếu đầu tiên (`firstMissing`); lỗi server về giờ / KTV → cuộn tới ô đó. Tại chỗ vẫn có dòng ngắn: dưới ô KTV ("Trùng giờ với người khác cùng KTV này."), dưới người bị trùng, dưới "Giờ hẹn" (đủ khách / đã qua).
- **"Hủy lịch" là nút thật.** Sheet lịch hẹn: khối cuối "Khách không đến hoặc hủy lịch" (tiêu đề `section-label`), hai nút cùng cỡ control 44px: "Không đến" (`btn-secondary`, chỉ khi đã tới giờ) và "Hủy lịch" (`btn-danger-outline`, kiểu mới: viền + chữ đỏ, nền surface; nút đỏ đặc vẫn chỉ cho bước xác nhận sau khi nhập lý do). Điện thoại: hai nút chia hàng, nhãn một dòng. "Thôi" của form lý do thành `btn-secondary`. "Đổi lịch" + "Khách đến · ghi lượt" giữ ở chân sheet.
- **"KTV làm" không đoán.** Sheet lịch (khách đến) và "Khách đến cả nhóm": mặc định = KTV đã gán của lịch, **không có thì trống** (trước: KTV đầu tiên của spa, nên lịch chưa gán bị ghi cho Lê Thị Hoa). Bấm "Chỉ báo đã đến" khi trống → focus ô + "Chọn KTV làm cho khách rồi bấm lại."; giỏ ghi lượt từ lịch vẫn lấy KTV của lịch hoặc trống (`prefill.ts` vốn đúng).

**AC (2026-11-07):**
- AC11.1 Dịch vụ 70 phút 09:00 → `duration_min 70`, `ends_at` 10:10; cùng KTV 10:00 → `409 staff_busy`, 10:15 → `201`; 19:45 + 70 → `201`; 20:00 + 70 → `422 outside_opening_hours`; thời lượng tự chọn 4 / 481 → `422`, 70 → `201`. *(Rust `a_booking_lasts_exactly_its_services_time`, unit `duration_is_the_services_exact_time`, `a_visit_occupies_its_services_exact_time`; vitest `autoMinutes`/`lengthChoices`, BookingSheet "70 minutes is 10:00–11:10"; Playwright "EM 2026-11-07…": thẻ 09:00–10:10 cao 70/15 ô ±4px)*
- AC11.2 (neg) Hà 10:00–11:30 → Hà 10:30 (KTV khác hoặc không KTV) → `409 appointment.customer_busy` + `existing_id`; 11:30 / ngày khác → `201`; dời lịch 11:30 sang 11:00 → `409`; nhóm có Hà 10:15 → `409`; hủy lịch đầu → đặt lại được; khách mới cùng SĐT hai lần cùng giờ → `409`; gia đình cùng SĐT trong một nhóm → `201`. *(Rust `one_customer_is_never_booked_twice_at_once`; vitest `customerClash`, BookingSheet "refuses a second booking…", "moving a booking onto…")*
- AC11.3 (neg) Quản lý và chủ spa đặt hôm qua → `422 appointment.in_the_past`; dời lịch sang ngày đã qua → `422`; lịch đã ở hôm qua: quản lý hủy / hoàn tất → `403 day.locked`, chủ spa hoàn tất → lượt mang ngày hôm qua. *(Rust `past_days_are_locked_for_managers_and_open_to_the_owner`)* Web: ngày đã qua → lỗi ở chân sheet, không gửi; 09:00 khi đã 10:00 → hỏi một lần rồi "Vẫn đặt lịch" mới gửi. *(vitest)*
- AC11.4 Bấm "Đặt lịch" khi thiếu dịch vụ → "Còn thiếu: dịch vụ." ở chân sheet + ô "Thêm dịch vụ" có focus; lỗi khách trùng nằm trong khung nhìn. *(vitest, Playwright `toBeInViewport`)*
- AC11.5 Lịch hôm nay chưa gán KTV → "KTV làm" trống; "Chỉ báo đã đến" khi trống → câu nhắc, vẫn "Chưa đến"; "Hủy lịch" cao ≥ 43px, bằng "Đổi lịch" ±1px, có viền, ở 1280 và 390px; axe sạch; hủy có lý do → "Đã hủy". *(Playwright)*
