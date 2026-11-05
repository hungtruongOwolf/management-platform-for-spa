---
title: "Spec R1-3: Lịch hẹn, giờ làm thực tế, nhập bù của chủ spa, giảm giá lượt (vòng 4)"
status: draft (lead quyết mặc định; câu hỏi EM ở §9)
updated: 2026-11-05
sources: [raw/2026-11-05-em-review-round4.md, raw/2026-11-05-em-simplify-owner-and-opening-balance.md]
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
- **booked (Đã đặt):** khách có hồ sơ **hoặc** khách mới (tên + SĐT, chưa tạo hồ sơ); 1–5 dịch vụ; KTV mong muốn (không bắt buộc); giờ bắt đầu; thời lượng = tổng `duration_min` của dịch vụ (trống = 60 phút), làm tròn lên bội số của ô giờ, quầy sửa được (15–480 phút); ghi chú lịch hẹn (≤ 500 ký tự, **không** ghi thông tin sức khỏe: chỗ đó là ghi chú buổi / hồ sơ).
- **arrived (Đã đến / đang làm):** bắt buộc chọn KTV. Chỉ trong ngày của lịch hẹn.
- **done (Hoàn tất):** quầy mở giỏ lượt khách đã điền sẵn khách, dịch vụ, KTV → bấm Lưu. Server ghi lượt **bằng đúng use case `POST /visits`** (trừ buổi, giá, thu đủ, giảm giá, audit) trong cùng transaction với việc đổi trạng thái lịch hẹn; lịch hẹn giữ `visit_id`. Không có quy tắc nghiệp vụ nào bị chép lại.
- **no_show (Không đến)** và **cancelled (Đã hủy)**: bắt buộc lý do, giữ trong lịch (mờ), giải phóng ô giờ.
- **Khách vãng lai** (đến đột ngột): bỏ qua lịch hẹn, ghi thẳng lượt như cũ (fast path ≤ 4 thao tác, AC4.*).
- Khách mới trong lịch hẹn: khi đến, quầy bấm "Tạo hồ sơ" (form khách điền sẵn tên + SĐT) rồi mới hoàn tất được. SĐT đã có hồ sơ → `409 customer.phone_taken` kèm `existing_id`, quầy chọn hồ sơ đó.

### Sức chứa (capacity)
- **KTV:** một KTV không có hai lịch `booked`/`arrived` chồng giờ. Vi phạm → `409 appointment.staff_busy`, `existing_id` = lịch hẹn đang chiếm chỗ (web nêu giờ + tên khách).
- **Cả spa:** tại mọi thời điểm, số lịch `booked`/`arrived` (có hoặc chưa có KTV) ≤ số KTV đang làm. Vượt → `409 appointment.slot_full`.
- **KTV** = nhân viên đang làm có chức vụ chứa "kỹ thuật" hoặc "KTV" (không phân biệt hoa thường); spa chưa ghi chức vụ nào như vậy thì mọi nhân viên đang làm. Danh sách lấy qua service của `GET /staff` (crate `org`).
- Ô giờ **15 phút**; giờ mở cửa mặc định **09:00–21:00**, cấu hình theo spa ở `tenant.settings.opening_hours = {"open":"09:00","close":"21:00"}` và `settings.slot_minutes`. Giờ bắt đầu phải rơi đúng ô; lịch phải nằm trọn trong giờ mở cửa (`422 appointment.outside_opening_hours`, `starts_at not_on_slot`).
- Hai lần đặt cùng lúc: khóa `pg_advisory_xact_lock` theo (spa, ngày) rồi mới kiểm, nên không lọt; ràng buộc `EXCLUDE USING gist` (staff + khoảng thời gian) ở DB là lớp thứ hai.

### Ngày & quyền
- Quản lý: đặt lịch cho hôm nay → 90 ngày tới. Mọi thay đổi lịch hẹn của **ngày đã qua** theo quy tắc ngày đã chốt: Quản lý `403 day.locked`, chủ spa được (không nhập lại mật khẩu).
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
- **AC9.3 (neg)** 2 KTV, đã có 2 lịch 10:00–11:00 (chưa gán), đặt thêm lịch chồng → `409 appointment.slot_full`.
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
- Lịch ngày kiểu Google Calendar: cột = KTV (+ "Chưa gán KTV"), hàng = ô 15 phút; dòng đầu mỗi giờ ghi số chỗ trống; bấm ô trống → đặt lịch; bấm lịch → panel: đổi trạng thái, khách đến, hoàn tất. Không kéo thả. Tuần: 7 ngày × số lịch + chỗ trống theo buổi. Điện thoại (< 768px): danh sách theo giờ thay cho lưới.
- Màu trạng thái: Đã đặt (xanh dương), Đã đến (vàng hổ phách), Hoàn tất (xanh lá), Không đến (đỏ, mờ), Đã hủy (xám, gạch ngang). Luôn kèm chữ, không chỉ màu.

## 8. Out of scope
Kéo thả · nhắc lịch Zalo/SMS · khách tự đặt online · lịch làm việc / nghỉ của từng KTV (thuộc quản lý nhân viên) · tài nguyên phòng/giường · công nợ.

## 9. Câu hỏi cho EM
| # | Câu hỏi | Mặc định đang làm |
|---|---|---|
| Q1 | Có cần cho khách **trả sau / ghi nợ** khi hoàn tất không? | Không: thu đủ khi hoàn tất (R1) |
| Q2 | Lịch `booked` đã qua ngày mà quầy quên đánh dấu: tự chuyển "Không đến" lúc 0 giờ? | Không tự động; hiện nhãn "Quá giờ" để quầy xử lý trong ngày |
| Q3 | Giờ mở cửa & ô giờ: 09:00–21:00, 15 phút đúng chưa? Ai sửa (màn cài đặt)? | Mặc định trên; sửa bằng dữ liệu, chưa có màn |
| Q4 | Sức chứa = số KTV đang làm. Có giường/phòng giới hạn thêm không? Quản lý có được "nhận quá chỗ" khi khách chịu chờ? | Chặn cứng, không ngoại lệ |
| Q5 | Giảm giá trên **cả bill dịch vụ lẻ** đủ chưa, hay cần giảm từng dòng? | Cả bill, chia theo tỷ lệ cho từng dòng |
| Q6 | Nhập bù của chủ spa: tối đa 366 ngày, không bắt lý do. Có cần lý do không? | Không bắt lý do; audit đánh dấu `backdated` |
| Q7 | Một lịch hẹn có thể nhiều KTV (mỗi dịch vụ một người)? | Một KTV / lịch hẹn; lúc hoàn tất từng dòng vẫn chọn KTV riêng |
| Q8 | KTV xem lịch của chính mình? Cần nối tài khoản ↔ nhân viên (thuộc quản lý nhân viên) | Chưa: KTV không xem lịch |
