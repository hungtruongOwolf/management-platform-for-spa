---
title: Thiết kế UI — lịch sử trên từng bản ghi + công tắc ở console admin
updated: 2026-11-11
sources: [raw/2026-11-11-em-decision-inline-record-history.md]
related: [../product/specs/record-history.md, ../architecture/record-history.md, design-system.md, front-desk-flows.md, ../architecture/frontend.md]
---

# UI: lịch sử trên từng bản ghi

Spec: [record-history](../product/specs/record-history.md). Backend: [architecture/record-history](../architecture/record-history.md). Theo rulebook + token của [design-system](design-system.md); không component mới ngoài `RecordTrace` và `LedgerSaleSheet`.

## 1. Thành phần dùng chung

### 1.1 `RecordTrace` (chunk lazy `record-history`)
Props: `record: {type, id}` hoặc `{type: "attendance" | "daily_review", staffId, date}` hoặc `{type: "payroll_rules" | "appointment_settings"}`; `verb` ("Ghi bởi" / "Bán bởi" / "Đặt bởi" / "Tạo bởi" / "Chấm bởi" / "Sửa lần cuối bởi"); `defaultOpen?` (chỉ Sổ doanh thu). Chỉ render khi `can(me, "record.history.read")`; không có quyền → không gì (không chừa chỗ).

Cấu trúc:
```
<div class="record-trace">
  <button type="button" class="record-trace-toggle" aria-expanded="false" aria-controls="rt-<id>">
    Ghi bởi Hà · 15:10 · đã sửa 1 lần  ▾
  </button>
  <div id="rt-<id>" role="region" aria-label="Lịch sử của lượt này" hidden>
    <ol class="record-trace-list"> … </ol>
  </div>
</div>
```
- Chữ: `--fs-sm` (13 px), màu `--text-muted`; mũi tên ▾ / ▴ (xoay 180° khi mở, `prefers-reduced-motion` thì không xoay động).
- Vùng chạm: nút cao tối thiểu `--size-control` trên điện thoại (padding dọc, chữ vẫn 13 px), rộng hết hàng ở 360 px; desktop cao tự nhiên, `:focus-visible` vòng focus chuẩn.
- Mở: danh sách ngay dưới nút (disclosure tại chỗ, **không** mở sheet thứ hai, vì nhiều chỗ đã ở trong `BottomSheet`). Focus ở lại nút; đóng bằng bấm lại nút. `Esc` không bắt (để sheet cha xử lý).
- Tải dòng meta: gọi `GET /record-history/...` khi bản ghi được mở (hàng lượt mở rộng, sheet mở…), không đợi bấm. Trong lúc tải: dòng meta giữ chiều cao, chữ "Đang tải…" (không spinner), `aria-busy="true"`.
- Lỗi: "Không tải được lịch sử. Thử lại" (nút "Thử lại" dạng link); `403` → ẩn cả khối và `invalidate(["me"])` (công tắc vừa tắt).
- Mỗi dòng trong danh sách (`<li>`):
  ```
  15:30  Lan đổi KTV
         KTV: Thu → Hoa
  16:05  Lan hủy lượt · Đảo 300.000 ₫
         lý do: khách đổi ý
  ```
  - Giờ `<time datetime>`: "15:30" nếu cùng ngày nghiệp vụ của bản ghi, "15:30 12/11" nếu khác ngày, "15:30 12/11/2025" nếu khác năm.
  - Câu: `actorName(e)` (+ " (đã nghỉ)" khi `actor.left`) + `HISTORY_VERB[action]` + phần tiền `moneyOf` / `discountOf` có sẵn; dòng phụ: `changeText` từng thay đổi, "lý do: …".
  - Không lặp chủ thể (đang đứng trong bản ghi đó); ngoại lệ: tên gói trong "bán gói X", "chuyển sang gói X".
- Cuối danh sách khi `omitted > 0`: "Còn {n} thay đổi cũ hơn không hiện ở đây."
- `source = record` / `none` khi mở: "Chưa có lịch sử chi tiết cho bản ghi này."

### 1.2 Khi nào tự tải, khi nào bấm mới tải
- **Một bản ghi đang mở** (hàng lượt mở rộng, sheet lịch hẹn / ngày công / hóa đơn, form sửa, đầu hồ sơ khách): tự tải, dòng meta hiện ngay ("Ghi bởi …"), danh sách đóng.
- **Nhiều bản ghi cùng lúc** (thẻ trong Lịch sử khách, các gói trong hồ sơ khách): mỗi thẻ chỉ có nút "Xem ai ghi, ai sửa ▾"; bấm mới tải, rồi hiện dòng meta + danh sách mở sẵn. Đúng ý EM "danh sách chỉ dấu nhỏ, tên người ở chỗ mở chi tiết" và không bắn N request.

### 1.3 Dấu ở danh sách
- `badge badge-muted` "Đã sửa" khi `edited === true`, đặt ngay sau badge trạng thái sẵn có (hoặc cuối tiêu đề hàng / thẻ). Trên thẻ điện thoại (`ResponsiveTable` card): nằm trong slot `subtitle` ("… · Đã sửa") để không thêm dòng.
- "Đã hủy" giữ nguyên badge sẵn có (`badge-danger`), không thêm gì.
- Không bao giờ có tên người ở danh sách.

### 1.4 Chữ (mọi chuỗi tiếng Việt)
| Chỗ | Chữ |
|---|---|
| Meta, đầy đủ | "{Verb} {Tên} · {giờ}[ {dd/mm}] · đã sửa {n} lần" |
| Meta, chưa sửa | "{Verb} {Tên} · {giờ}" |
| Meta, đã hủy | "{Verb} {Tên} · {giờ} · đã hủy" (lượt / hóa đơn / gói / lịch hẹn) |
| Meta, lượt cũ của Sửa lượt | "{Verb} {Tên} · {giờ} · đã sửa {n} lần" (cùng câu chuyện, xem spec §6.1) |
| Meta, sổ giấy | "Nhập từ sổ giấy · dd/mm/yyyy" |
| Meta, không có audit | "Chưa có lịch sử" |
| Verb | lượt "Ghi bởi"; hóa đơn bán gói, gói "Bán bởi"; lịch hẹn "Đặt bởi"; khách "Tạo bởi"; chấm công "Chấm bởi"; khen / phàn nàn "Ghi bởi"; danh mục, quy tắc lương, cài đặt lịch hẹn "Sửa lần cuối bởi" (dùng `summary.last ?? created`, không có "đã sửa n lần") |
| Tên người làm | `actorName` của `features/activity/sentence.ts`: tên tài khoản; "Đội vận hành"; "Hệ thống"; "Một tài khoản đã xóa"; người đã nghỉ thêm " (đã nghỉ)" |
| Vùng (aria-label) | "Lịch sử của lượt này" / "…hóa đơn này" / "…gói này" / "…lịch hẹn này" / "…hồ sơ khách" / "…ngày công này" / "…khen / phàn nàn ngày này" / "…dịch vụ này" / "…mẫu gói này" / "…quy tắc lương" / "…cài đặt lịch hẹn" |
| Đang tải | "Đang tải…" |
| Lỗi | "Không tải được lịch sử." + nút "Thử lại" |
| Trống | "Chưa có lịch sử chi tiết cho bản ghi này." |
| Bị cắt | "Còn {n} thay đổi cũ hơn không hiện ở đây." |
| Badge | "Đã sửa" |
| Nút bấm-mới-tải (§1.2) | "Xem ai ghi, ai sửa" |

`HISTORY_VERB: Record<AuditAction, string | null>` (cùng file với `PHRASES`, `null` = không hiện tại chỗ, khớp spec §4.2): ví dụ `visit.create` "ghi lượt", `visit.addition` "thêm vào lượt", `visit.update` "sửa giờ / KTV", `visit.correct` "sửa lượt (tính lại tiền)", `visit.convert_to_package` "chuyển lượt sang gói", `visit.void` "hủy lượt", `sale.create` "bán gói" (hóa đơn riêng) / "thu tiền", `sale.void` "hủy hóa đơn", `sale.note_update` "sửa ghi chú gói", `customer_package.create` "bán gói", `.extend` "gia hạn gói", `.cancel` "hủy gói", `.use_expired_override` "cho dùng gói đã hết hạn", `opening_balance.import` / `sale.paper_book` "nhập từ sổ giấy", `appointment.create` "đặt lịch", `.update` "đổi lịch", `.arrive` "đánh dấu khách đến", `.undo_arrival` "bỏ đánh dấu khách đến", `.complete` "hoàn tất lịch hẹn", `.no_show` "đánh dấu không đến", `.cancel` "hủy lịch", `appointment.settings` "sửa số khách tối đa", `customer.create` "tạo hồ sơ", `customer.update` "sửa hồ sơ", `customer.health_note_set` "sửa lưu ý sức khỏe", `attendance.record` "chấm công", `evaluation.daily_review` "ghi khen / phàn nàn", `catalog.*_create` "thêm", `catalog.*_update` "sửa", `payroll.rules_update` "sửa quy tắc lương". Mọi key ở spec §4.2 = `null`.

## 2. Theo màn (desktop ≥ 1024 px, điện thoại 360 px)

### 2.1 Quầy `/quay`, danh sách lượt hôm nay (`TodayVisits` → `VisitDetails`) — đợt 1
Desktop, hàng lượt mở rộng:
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 10:05  Chị Lê Thị Mai   Gội đầu dưỡng sinh · KTV Thu   300.000 ₫  [Đã sửa] ⋯ │
├─────────────────────────────────────────────────────────────────────────────┤
│  (VisitDetails: dịch vụ, KTV, tiền, thanh toán, gói bán kèm…)                │
│  Ghi bởi Hà · 10:07 · đã sửa 1 lần ▾                        ← RecordTrace   │
│    10:07  Hà ghi lượt · Thu 300.000 ₫                       ← khi mở        │
│    10:30  Lan sửa giờ / KTV                                                  │
│           KTV: Thu → Hoa                                                     │
└─────────────────────────────────────────────────────────────────────────────┘
```
Điện thoại 360 px (thẻ `ResponsiveTable`):
```
┌──────────────────────────────────┐
│ Chị Lê Thị Mai             ⋯     │
│ 10:05 · 300.000 ₫ · Đã sửa       │  ← badge trong subtitle
│ ─ mở thẻ ─                       │
│ (VisitDetails…)                  │
│ Ghi bởi Hà · 10:07 ·             │  ← nút rộng hết thẻ, ≥ 44 px
│ đã sửa 1 lần ▾                   │
└──────────────────────────────────┘
```
- Vị trí: **cuối** `VisitDetails`, trên các nút hành động nếu có; tách bằng `border-top` mảnh `--border` để không lẫn với ghi chú.
- Bản cũ (đã thay bằng lượt mới): giữ chữ "Đã sửa, thay bằng lượt mới." + badge "Đã sửa" sẵn có; `RecordTrace` cùng câu chuyện.
- `?sua=` (CorrectionPanel) không hiện `RecordTrace` (đang sửa, không phải xem).

### 2.2 "Bán gói hôm nay" (`DaySales`) — đợt 1
Hàng hóa đơn bán gói: badge "Đã sửa" khi `edited`; mở hàng (nếu có chi tiết) hoặc dùng `LedgerSaleSheet` (2.4) với `record = {type: "sale", id}`.

### 2.3 Lịch sử khách (`reports/CustomerHistory`, thẻ lượt / hóa đơn) — đợt 1
```
┌ 12/11 · Lượt ───────────────────── [Đã sửa] ┐
│ Gội đầu dưỡng sinh · KTV Hoa · 300.000 ₫     │
│ Xem ai ghi, ai sửa ▾                         │  ← chế độ "bấm mới tải"
│ (sau khi bấm)                                │
│ Ghi bởi Hà · 10:07 · đã sửa 1 lần ▴          │
│   10:07  Hà ghi lượt · Thu 300.000 ₫         │
└──────────────────────────────────────────────┘
```
- Đây là màn **nhiều bản ghi** (dòng thời gian), nên theo quy tắc §1.2: thẻ chỉ có badge + nút "Xem ai ghi, ai sửa ▾"; chỉ khi bấm mới gọi API và hiện dòng meta + danh sách. Không bắn hàng chục request khi mở trang.
- Thẻ "Lượt sửa lại" (badge-info sẵn có) không thêm "Đã sửa" (trùng nghĩa).

### 2.4 Sổ doanh thu (`reports/LedgerTab`) — đợt 1 (hàng Hoàn tiền: đợt 2)
Danh sách: badge "Đã sửa" cạnh "Hủy hóa đơn" / "Hoàn tiền" sẵn có. Khi có quyền, **hàng bấm được** (desktop: cả hàng là `<button>` trong ô đầu; điện thoại: cả thẻ) → mở `LedgerSaleSheet`:
```
BottomSheet (điện thoại: từ dưới lên; ≥ 640 px: hộp giữa màn)
┌──────────────────────────────────────────┐
│ Hóa đơn · chị Lê Thị Mai                 │  title
│ 12/11/2026 · Dịch vụ 1 lần               │  description
├──────────────────────────────────────────┤
│ Gội đầu dưỡng sinh             300.000 ₫ │  tóm tắt lại hàng (đã có trong LedgerRow)
│ Tiền mặt 300.000 ₫                       │
│ Ghi bởi Hà · 10:07 · đã sửa 1 lần ▴      │  RecordTrace defaultOpen
│   10:07  Hà ghi lượt · Thu 300.000 ₫     │
│   10:30  Lan sửa giờ / KTV …             │
├──────────────────────────────────────────┤
│ [Mở hồ sơ khách]              [Đóng]     │  footer
└──────────────────────────────────────────┘
```
- Hàng của lượt → server trả lịch sử lượt (`record.type = visit`), title đổi "Lượt · chị …".
- Hàng "Hoàn tiền" → `record = refund`, title "Gói · chị …" (đợt 2).
- Không quyền: hàng không bấm được (y như hiện nay).

### 2.5 Gói của khách (`sales/CustomerPackages` `PackageRow`) — đợt 2
```
┌ Liệu trình gội 10 buổi ─── [Đang dùng] [Đã sửa] ┐
│ Còn 6/10 · hết hạn 12/05/2027                    │
│ (các dòng buổi…)                       ⋯         │
│ Bán bởi Hà · 12/11 · đã sửa 1 lần ▾              │
└──────────────────────────────────────────────────┘
```
Nhiều gói cùng lúc → chế độ "bấm mới tải" (§1.2): ban đầu chỉ "Xem ai ghi, ai sửa ▾", sau khi bấm thành dòng meta như trên. Gói đã hủy: "Bán bởi Hà · 12/11 · đã hủy"; mở → dòng "Lan hủy gói · Hoàn 1.500.000 ₫ · lý do: …". Gói sổ giấy: "Nhập từ sổ giấy · 03/02/2025".

### 2.6 Hồ sơ khách (`customers/CustomerProfile`) — đợt 2
Dưới khối tên + SĐT, trên "Sửa hồ sơ khách":
```
Chị Lê Thị Mai
0912 333 444 · Nữ
Tạo bởi Hà · 03/02/2025 · đã sửa 2 lần ▾
  05/06/2025 09:12  Hà sửa hồ sơ
                    SĐT: 0905 111 222 → 0912 333 444     (chủ spa)
                    SĐT: 0905 ••• 222 → 0912 333 444     (quản lý được bật)
  10/09/2025 14:00  Lan sửa lưu ý sức khỏe               (chỉ ai có quyền ghi chú sức khỏe)
```

### 2.7 Lịch hẹn (`AppointmentSheet`) — đợt 3
Trong sheet, ngay dưới description (`dayTitle · PhaseBadge`), trên các nút hành động:
```
Đặt bởi Hà · 08:15 · đã sửa 1 lần ▾
  08:15  Hà đặt lịch
  09:40  Lan đổi lịch · Giờ: 14:00 → 15:00
```
Dạng danh sách (`kieu=ds`): badge "Đã sửa"; khối giờ ở dạng ngày / tuần: không.

### 2.8 Chấm công + khen / phàn nàn (`PersonDaySheet`) — đợt 3
Mỗi phần một `RecordTrace` ở cuối phần:
```
Chấm công
  Trạng thái: Đi làm · vào 08:02 · ra 18:00
  Chấm bởi Hà · 08:05 · đã sửa 1 lần ▾
Khen / phàn nàn
  Khen 1 · Phàn nàn 0
  Ghi bởi Hà · 18:10 ▾
```
Ô lưới tháng (`MonthSheet`): không dấu.

### 2.9 Bảng giá, mẫu gói, nhóm dịch vụ, quy tắc lương, cài đặt lịch hẹn — đợt 4
Trong form sửa (`ServiceForm`, `PlanForm`, form nhóm) / `RulesSheet` / sheet số khách tối đa, ngay trên nút "Lưu":
```
Sửa lần cuối bởi Lan · 12/11/2026 ▾
  03/02/2025 09:00  Lan thêm
  12/11/2026 10:00  Lan sửa · Giá Nữ: 150.000 ₫ → 170.000 ₫
```
Danh sách bảng giá / mẫu gói: không dấu.

## 3. Console admin `/admin/spa/<id>` — đợt 1

### 3.1 Panel mới "Lịch sử trên từng bản ghi" (giữa `OwnerMfaPanel` và `FeaturesPanel`)
```
┌ Lịch sử trên từng bản ghi ──────────────────────────────────────────┐
│ Dòng "Ghi bởi … · đã sửa … lần" và lịch sử thay đổi trên lượt,       │
│ hóa đơn, gói, lịch hẹn, hồ sơ khách, chấm công. Kỹ thuật viên        │
│ không bao giờ thấy.                                                  │
│                                                                      │
│ Chủ spa      [Đang hiện]                                    [Ẩn]     │
│ Quản lý      [Đang ẩn]                                      [Hiện]   │
│                                                                      │
│ (khi vai trò không theo mẫu) Vai trò Quản lý đang dùng quyền riêng,  │
│ không theo mẫu.                                                      │
└──────────────────────────────────────────────────────────────────────┘
```
- Badge: "Đang hiện" `badge-ok`, "Đang ẩn" `badge-muted`. Nút `btn-secondary` (không phải danger: không phá dữ liệu).
- Bấm → `ReasonSheet` có sẵn: title "Hiện lịch sử cho Quản lý?" / "Ẩn lịch sử với Chủ spa?"; description: hiện → "Từ thao tác kế tiếp, Quản lý thấy ai ghi, ai sửa trên từng bản ghi họ mở được. Số điện thoại cũ của khách được che." / ẩn → "Từ thao tác kế tiếp, {vai trò} không thấy ai ghi, ai sửa nữa. Dữ liệu giữ nguyên."; ô "Lý do" bắt buộc; nút "Hiện" / "Ẩn"; `useStepUp` có sẵn bọc request.
- Lỗi `409 role.record_history_unchanged` → tải lại panel, thông báo "Trạng thái đã đổi ở nơi khác, đã tải lại."
- Mobile 360 px: mỗi vai trò một hàng, nút xuống dòng dưới badge (cùng bố cục `FeaturesPanel`).

### 3.2 `FeaturesPanel`: cờ "Nhật ký thao tác"
Hàng mới tự có từ registry: "Nhật ký thao tác [Đang tắt] Mặc định · tắt [Bật]". Chữ "Mặc định" đổi thành "Mặc định · bật" / "Mặc định · tắt" theo `default_enabled`. Sheet bật: "Chủ spa thấy lại màn Nhật ký thao tác ở mục Báo cáo từ thao tác kế tiếp. Quản lý và kỹ thuật viên vẫn không thấy." Sheet tắt: chữ chung sẵn có.

### 3.3 Trình sửa vai trò / mẫu vai trò
Permission mới hiện như mọi permission, nhãn "Xem ai ghi, ai sửa trên từng bản ghi"; ô của Kỹ thuật viên bị tắt kèm chú thích "Không cấp cho kỹ thuật viên" (`not_for_technician`).

## 4. Spa: ẩn "Nhật ký thao tác"
`mayOpen.activity = can(me, "audit.read") && hasFeature(me, "activity_log")`: nav Báo cáo không có mục, route guard đẩy về trang chủ như các màn bị chặn khác. Không đổi gì khác trên `ActivityPage`.

## 5. A11y, perf
- Nút meta: `<button type="button">` + `aria-expanded` + `aria-controls`; vùng `role="region"` + `aria-label`; danh sách `<ol>`; giờ `<time>`. Badge "Đã sửa" là chữ thường (không chỉ màu). Hàng Sổ doanh thu bấm được: `<button>` có nhãn "Xem hóa đơn của chị …".
- Tương phản `--text-muted` trên `--surface` / `--surface-2` đã đạt AA ở design system (13 px thường); không dùng nhạt hơn.
- `RecordTrace` + `sentence.ts` + `LedgerSaleSheet` trong chunk lazy `record-history` (prefetch khi mở bản ghi đầu tiên); Quầy chỉ thêm `lazy()` wrapper (~0,2 KB) vào chunk khởi động; đo `pnpm --filter web budget` mỗi đợt, ngân sách 125 KB.
- CLS: dòng meta giữ chiều cao khi tải; danh sách chỉ thêm badge (cùng hàng).
- E2E axe ở 360 px + 1280 px, sáng / tối cho Quầy mở lượt, Sổ doanh thu sheet, panel admin.
