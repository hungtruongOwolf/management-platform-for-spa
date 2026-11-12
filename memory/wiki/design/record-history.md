---
title: Thiết kế UI — lịch sử trên từng bản ghi + công tắc ở console admin
updated: 2026-11-11
sources: [raw/2026-11-11-em-decision-inline-record-history.md]
related: [../product/specs/record-history.md, ../architecture/record-history.md, design-system.md, front-desk-flows.md, ../architecture/frontend.md]
---

# UI: lịch sử trên từng bản ghi

Spec: [record-history](../product/specs/record-history.md). Backend: [architecture/record-history](../architecture/record-history.md). Theo rulebook + token của [design-system](design-system.md) (chip trạng thái: quy tắc 7, một bộ `.badge`, luôn có chữ). Bản sửa sau review độc lập 2026-11-11 (spec, Review lượt 3). **Đợt 1a (§3–4) và 1b (§1, §2.1–2.6) đã build**; chỗ build chọn khi design chưa nói: spec Review log K12–K20 (focus khi `403`, prefetch chunk, tên nút giờ hàng hủy / hoàn, bố cục một dòng, CSS ở `components.css`). **Đợt 2 (§2.7–2.9) đã build** (2026-11-12): K21 (một dòng "sửa khoản thưởng"), K23 ("Chưa có ai ghi"), K27 (meta theo §1.2), K30 (ô chỉ xem qua sheet ghi được), K31 (⋯ riêng ở Sổ thưởng, phạt; focus về nút "Đóng" của sheet khi `403`).

## 1. Thành phần dùng chung

### 1.1 Chia module (ngân sách JS khởi động)
Khởi động đang **124,70 / 125 KiB** (số đo của reviewer 2026-11-11: index 124,28 + runtime 0,42). `TodayVisits` đã là chunk lazy (`FrontDeskPage.tsx` `loadTodayVisits`, prefetch sau lần vẽ đầu); `DaySales` chỉ được import từ `TodayVisits`.
- `features/record-history/query.ts` (rất nhỏ, **không** import UI / `sentence.ts`): `recordHistoryQuery(ref)` (key `["record-history", type, id]`, `staleTime: 0`) — import từ các màn để bắt đầu fetch song song với lúc tải chunk UI.
- Chunk lazy `record-history`: `RecordTrace`, `HISTORY`, `LedgerSaleSheet`; dùng `sentence.ts`.
- Kiểm `can(me, "record.history.read")` **ngoài** ranh giới lazy (không có quyền → không tải chunk). Có quyền → prefetch chunk bằng `useAfterFirstPaint` (`FrontDeskPage.tsx`), như `TodayVisits`.
- Ngân sách: khởi động **≤ 125 KiB**, mã lịch sử lazy. Module khởi động chỉ đổi vài dòng: `lib/queryClient.ts` (`onSuccess`), `features/auth/session.ts` (`hasFeature` `defaultOff`), `app/access.ts` (`mayOpen.activity`), `components/ResponsiveTable.tsx` (`[data-row-inert]`); không import gì từ `features/record-history/*` ngoài `query.ts` (không ở module khởi động). `pnpm --filter web budget` mỗi PR.

### 1.2 `RecordTrace`
Props: `record` (`{type, id}` | `{type: "attendance" | "daily_review", staffId, date}` | `{type: "payroll_rules" | "appointment_settings"}`), `noun` (để đặt tên nút: "lượt 12/11 10:05", "gói Liệu trình gội 10 buổi"…), `mode: "line" | "external"` (§1.3), `defaultOpen?`.

Chế độ `line` (một bản ghi đang mở):
```
<div class="record-trace" data-row-inert>
  <button type="button" class="disclosure record-trace-toggle" aria-expanded="false" aria-controls="rt-<id>">
    <span>Ghi bởi Hà · 10:07 · đã sửa 1 lần</span><span class="disclosure-arrow" aria-hidden="true"></span>
  </button>
  <ol id="rt-<id>" class="record-trace-list" hidden> … </ol>
</div>
```
- Chữ `--fs-sm`, màu `--text-muted`; mũi tên dùng lại `.disclosure` sẵn có (ký hiệu `aria-hidden`).
- Tên nút ổn định khi tải xong: nội dung meta; trong lúc tải "Đang tải ai ghi, ai sửa…" (`aria-busy`). Không `role="region"`.
- Vùng chạm ≥ `--size-control` trên điện thoại; `:focus-visible` chuẩn.
- **Luôn là phần tử cuối** của phần chi tiết bản ghi (một quy tắc cho mọi màn).
- `data-row-inert`: `ResponsiveTable` thêm `[data-row-inert]` vào guard `closest(…)` của `clickRow` → bấm trong khối lịch sử không gập thẻ (Vitest).
- Mở: danh sách ngay dưới (disclosure tại chỗ, không sheet thứ hai — nhiều chỗ đã ở trong `BottomSheet`). Focus ở lại nút.
- Mỗi `<li>`: giờ `<time datetime>` ("10:30"; khác ngày của bản ghi "10:30 12/11"; khác năm thêm năm) · câu `actorName(e)` (+ " (đã nghỉ)") + `HISTORY[action](e)` + tiền (`moneyOf` / `discountOf`) · dòng phụ `changeText` từng thay đổi, "lý do: …". Không lặp chủ thể.
- Trạng thái:
  - lỗi mạng / 5xx: "Không tải được lịch sử." + nút "Thử lại";
  - `404`: "Không xem được lịch sử của mục này." (không thử lại);
  - `403`: ẩn cả khối, chuyển focus về phần tử tiêu đề của bản ghi (hàng / tiêu đề sheet), `invalidate(["me"])`;
  - trống: "Chưa có lịch sử thay đổi.";
  - bị cắt: "Còn {n} thay đổi cũ hơn.".

### 1.3 Màn nhiều bản ghi: nút mở là phần tử có sẵn (chế độ `external`)
Không thêm hàng "Xem ai ghi, ai sửa" vào mỗi thẻ (rối; 40 lượt ≈ 1.700 px thêm). Nút mở là:
- **Lịch sử khách:** giờ của thẻ (`CustomerHistory` dòng giờ) thành `btn-ghost` (cao 44 px), là **nút bật / tắt** (`aria-expanded`, `aria-controls` trỏ tới danh sách), tên bắt đầu bằng chữ hiển thị: "12/11 10:05, ai ghi, ai sửa"; bấm → tải, danh sách hiện cuối thẻ, dòng đầu là meta "Ghi bởi Hà · 10:07 · đã sửa 1 lần"; bấm lại thì đóng. Theo loại thẻ (`HistoryKind`, `CustomerHistory.tsx`):

  | Loại thẻ | Nút? | `record` |
  |---|---|---|
  | `visit` "Lượt khách" | có | `{type: "visit", id: visit.id}` |
  | `visit_voided` "Lượt đã hủy" | có | `{type: "visit", id: visit.id}` (cùng chuỗi) |
  | `purchase` "Mua gói liệu trình" | có | `{type: "sale", id: sale_id}` → server trả lịch sử gói |
  | `purchase_voided` "Hóa đơn đã hủy" | có | `{type: "sale", id: sale_id}` |
  | `package_cancelled` "Hủy gói" | có | `{type: "customer_package", id}` |
  | `package_extended` "Gia hạn gói" | có | `{type: "customer_package", id}` |

  Id lấy từ trường có sẵn của `HistoryEvent` (kiểm tên trường khi build; thiếu id nào thì thêm vào DTO ở đợt 1a).
- **Gói của khách:** nút nhỏ `btn-ghost` "Lịch sử" ở `row-head` cạnh chip trạng thái, tên truy cập **bắt đầu bằng chữ hiển thị**: "Lịch sử gói {tên gói}" (WCAG 2.5.3).
- **"Bán gói không kèm lượt"** (`DaySales`): mục ⋯ (`RowMenu`) "Xem ai ghi, ai sửa" → hiện `RecordTrace` (mở sẵn) trong ô mở rộng sẵn có của hàng, không modal.
- **Sổ thưởng/phạt:** mục ⋯ "Xem ai ghi, ai sửa" (đợt 2), cùng cách.

### 1.4 Chip ở danh sách (D9)
Một chip mỗi hàng; ba điều kiện **loại trừ nhau** (code hiện kiểm `corrected_by_visit_id` trước: `TodayVisits.tsx`, `CustomerHistory.tsx`), đặt **trong cùng `span` với chip sẵn có** (`TodayVisits` cạnh tên khách; thẻ điện thoại cùng chỗ; số tiền vẫn ở slot `aside`, không đụng):
| Chip | Khi | Ai thấy |
|---|---|---|
| `badge-danger` "Đã hủy" | `status = voided` **và không** `corrected_by_visit_id` (sẵn có) | mọi người đọc được danh sách |
| `badge-muted` "Đã thay" | bản lượt bị Sửa lượt thay (`corrected_by_visit_id`); **đổi từ "Đã sửa"** ở `TodayVisits`, `CustomerHistory` (`visit_voided`), chữ chi tiết "Đã thay bằng lượt mới." | mọi người |
| `badge-muted` "Đã sửa" | `edited === true` **và** `status !== "voided"` | chỉ người có `record.history.read` |
`CustomerHistory` chip "Lượt sửa lại" (`badge-info`, bản mới của Sửa lượt, mọi vai trò) giữ nguyên; khi có quyền và `edited` thì **thay** bằng "Đã sửa" (một chip). Tên người không bao giờ ở danh sách.

### 1.5 Chữ (mọi chuỗi tiếng Việt)
| Chỗ | Chữ |
|---|---|
| Meta | "Ghi bởi {Tên} · {giờ}[ {dd/mm}][ · đã sửa {n} lần][ · đã hủy]" |
| Meta, khách | "Tạo bởi {Tên} · {dd/mm/yyyy}[ · đã sửa {n} lần]" |
| Meta, gói | "Ghi bởi {Tên} · {dd/mm/yyyy}[ · đã hủy]" (không "đã sửa": vòng đời gói, D9) |
| Ghi chú buổi (`visit.update` `session_note`) | "Ghi chú buổi: Có" / "Ghi chú buổi: Không" (web `FIELD.session_note` = "Ghi chú buổi"; `FIELD.note` vẫn "Ghi chú hồ sơ") |
| KTV đổi (`visit.update`) | "KTV: {tên cũ} → {tên mới}", một dòng cho mỗi dịch vụ đổi KTV |
| Meta, danh mục / quy tắc lương / cài đặt | "Sửa lần cuối bởi {Tên} · {dd/mm/yyyy}" |
| Meta, sổ giấy | "Nhập từ sổ giấy · {dd/mm/yyyy}" |
| Meta, không có dòng tạo | "Chưa rõ ai ghi" |
| Động từ người ghi | "Ghi bởi" ở **mọi** nơi (lượt, hóa đơn, gói, lịch hẹn, chấm công, khen / phàn nàn, thưởng / phạt) — không "Bán bởi" (lẫn với "Người chốt" / "chốt bởi") |
| Tên người làm | `actorName` (`sentence.ts`): tên tài khoản; "Đội vận hành"; "Hệ thống"; "Một tài khoản đã xóa"; " (đã nghỉ)" khi `actor.left` |
| Tiền đảo khi hủy | "Trả lại …" (đổi `AMOUNT_LABEL` "Đảo" → "Trả lại" ở cả màn nhật ký) |
| Tải / lỗi / 404 / trống / cắt | "Đang tải ai ghi, ai sửa…" / "Không tải được lịch sử." + "Thử lại" / "Không xem được lịch sử của mục này." / "Chưa có lịch sử thay đổi." / "Còn {n} thay đổi cũ hơn." |
| SĐT với người không có `audit.read` | "SĐT (đã đổi)" |
| Chip | "Đã sửa", "Đã thay", "Đã hủy" |

`HISTORY: Record<AuditAction, ((e: ActivityEntry) => string) | null>` (cùng file `sentence.ts`; `null` = không hiện tại chỗ, khớp spec §4.2), theo ngữ cảnh:
- `visit.create`: `detail === "backdated"` ? "nhập bù lượt" : "ghi lượt" (+ " kèm bán gói X" khi `packages`); `visit.addition` "thêm vào lượt"; `visit.update` "sửa lượt"; `visit.correct` "sửa lượt (tính lại tiền)"; `visit.convert_to_package` "chuyển lượt sang gói X"; `visit.void` "hủy lượt";
- `sale.create`: có `packages` ? "bán gói X" : "thu tiền"; `sale.void` "hủy hóa đơn"; `sale.note_update` "sửa ghi chú gói";
- `customer_package.create` "bán gói X"; `.extend` "gia hạn gói"; `.cancel` "hủy gói"; `.use_expired_override` "cho dùng gói đã hết hạn"; `opening_balance.import` / `sale.paper_book` "nhập từ sổ giấy";
- `customer.create` "thêm khách"; `customer.update` "sửa hồ sơ"; `customer.health_note_set` "sửa lưu ý sức khỏe";
- `appointment.create` "đặt lịch"; `.update` "đổi lịch"; `.arrive` "đánh dấu khách đến"; `.undo_arrival` "bỏ đánh dấu khách đến"; `.complete` "hoàn tất lịch hẹn"; `.no_show` "đánh dấu không đến"; `.cancel` "hủy lịch"; `appointment.settings` "sửa số khách tối đa";
- `attendance.record`: cleared ? "xóa chấm công" : "chấm công"; `evaluation.daily_review` "ghi khen / phàn nàn"; `staff.adjustment` "ghi thưởng" / "ghi phạt" (theo `detail`); `staff.adjustment_void` "xóa khoản thưởng / phạt";
- `catalog.service_create` "thêm dịch vụ", `.service_update` "sửa dịch vụ", `.plan_create` "thêm mẫu gói", `.plan_update` "sửa mẫu gói", `.category_create` "thêm nhóm dịch vụ", `.category_update` "sửa nhóm dịch vụ"; `payroll.rules_update` "sửa quy tắc lương";
- mọi key ở spec §4.2: `null`.

## 2. Theo màn (desktop ≥ 1024 px, điện thoại 360 px)

### 2.1 Quầy, lượt hôm nay (`TodayVisits` → `VisitDetails`) — đợt 1b
Desktop, hàng mở rộng:
```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 10:05  Chị Lê Thị Mai [Đã sửa]  Gội đầu dưỡng sinh · KTV Hoa      300.000 ₫ ⋯│
├─────────────────────────────────────────────────────────────────────────────┤
│  (VisitDetails: dịch vụ, KTV, tiền, thanh toán, gói bán kèm…)                │
│  Ghi bởi Hà · 10:07 · đã sửa 1 lần ▾                    ← phần tử cuối      │
│    10:07  Hà ghi lượt · Thu 300.000 ₫                    (khi mở)           │
│    10:30  Lan sửa lượt                                                       │
│           KTV: Thu → Hoa                                                     │
└─────────────────────────────────────────────────────────────────────────────┘
```
Điện thoại 360 px (thẻ, `onRowClick` gập / mở):
```
┌──────────────────────────────────┐
│ Chị Lê Thị Mai [Đã sửa]   300.000│  ← chip cùng span tên; tiền ở aside
│ 10:05 · Gội đầu dưỡng sinh    ⋯  │
│ (VisitDetails…)                  │
│ Ghi bởi Hà · 10:07 ·             │  ← data-row-inert, ≥ 44 px
│ đã sửa 1 lần ▾                   │
└──────────────────────────────────┘
```
- Lượt nhập bù: khi `RecordTrace` có mặt thì bỏ chữ "Nhập bù · Ghi lúc {giờ}" sẵn có (meta đã nói "Ghi bởi … · {giờ} {ngày}" + dòng "nhập bù lượt").
- Bản bị thay: chip "Đã thay", chữ "Đã thay bằng lượt mới.", cùng `RecordTrace` (cùng câu chuyện).
- `?sua=` (CorrectionPanel): không có `RecordTrace`.

### 2.2 "Bán gói không kèm lượt" (`DaySales`) — đợt 1b
Không chip "Đã sửa" (gói, D9). ⋯ → "Xem ai ghi, ai sửa" → `RecordTrace` `{type: "sale", id}` (server trả lịch sử gói) trong ô mở rộng sẵn có.

### 2.3 Lịch sử khách (`reports/CustomerHistory`) — đợt 1b
```
┌ Lượt khách ─────────────── [Đã sửa] ┐
│ [12/11 10:05]  ← btn-ghost, nút mở   │
│ Gội đầu dưỡng sinh · KTV Hoa         │
│ 300.000 ₫                            │
│ (sau khi bấm, cuối thẻ)              │
│ Ghi bởi Hà · 10:07 · đã sửa 1 lần    │
│   10:07  Hà ghi lượt · Thu 300.000 ₫ │
│   10:30  Lan sửa lượt · KTV: Thu → Hoa│
└──────────────────────────────────────┘
```
Thẻ hóa đơn bán gói: nút giờ mở lịch sử gói. Không quyền: giờ là chữ thường như hiện nay.

### 2.4 Sổ doanh thu (`reports/LedgerTab`) — đợt 1b
- Chip "Đã sửa" chỉ khi `entry === "sale"` && `!voided` && `kind === "one_off"` && `edited` (hàng `void` có id = id hóa đơn, `revenue_book`; hóa đơn đã hủy của bản B bị thay không bao giờ "Đã sửa"). Hàng gói / hoàn tiền / hủy: không chip mới.
- Có quyền: desktop, ô "Giờ" (cột 64 px) chứa `<button>` chữ giờ, tên đầy đủ "10:07, xem hóa đơn của chị Lê Thị Mai" (WCAG 2.5.3: tên chứa chữ hiển thị); điện thoại, giờ trong `rt-card-sub` của `LedgerCards` thành **`<button>` thật** (bàn phím dùng được, cùng tên), thẻ còn bấm được bằng `onClick` có guard `closest("a, button")` (Link tên khách vẫn mở hồ sơ), **không** bọc thẻ trong `<button>`.
- Chỉ hiện nút khi có `record.history.read` **và** quyền của loại neo cuối: hàng dịch vụ lẻ cần `visit.read`; hàng gói / hủy hóa đơn gói / hoàn tiền cần `customer.read` (+ `sale.read`, vốn có vì đang xem sổ). Sheet nhận `403` → đóng sheet, focus về nút giờ, `invalidate(["me"])`.
- Bấm → `LedgerSaleSheet` (`BottomSheet`):
```
┌──────────────────────────────────────────┐
│ Hóa đơn dịch vụ · chị Lê Thị Mai         │  title theo LedgerRow.kind / entry:
│ 12/11/2026                               │  "Hóa đơn dịch vụ" / "Hóa đơn bán gói" / "Hủy hóa đơn" / "Hoàn tiền gói"
├──────────────────────────────────────────┤
│ Gội đầu dưỡng sinh             300.000 ₫ │  tóm tắt hàng (dữ liệu LedgerRow)
│ Tiền mặt 300.000 ₫                       │
│ Ghi bởi Hà · 10:07 · đã sửa 1 lần ▴      │  RecordTrace mở sẵn
│   10:07  Hà ghi lượt · Thu 300.000 ₫     │
│   10:30  Lan sửa lượt · KTV: Thu → Hoa   │
├──────────────────────────────────────────┤
│                                [Đóng]    │
└──────────────────────────────────────────┘
```
- **Mở sẵn có lý do:** sheet chỉ tồn tại để xem lịch sử (mọi thông tin khác đã có trên hàng), nên mở sheet = đã yêu cầu lịch sử; bắt bấm thêm lần nữa là thừa.
- Bỏ nút "Mở hồ sơ khách" (tên khách trên hàng đã là link).
- Hàng hoàn tiền: `{type: "refund", id}`, title "Hoàn tiền gói · chị …". Hàng hủy (`entry = void`): `{type: "sale", id}` (id = id hóa đơn), title "Hủy hóa đơn · chị …".

### 2.5 Gói của khách (`sales/CustomerPackages` `PackageRow`) — đợt 1b
```
┌ Liệu trình gội 10 buổi ─── [Đang dùng] [Lịch sử ▾] ┐
│ Còn 6/10 · hết hạn 12/05/2027                      │
│ (các dòng buổi…)                         ⋯         │
│ (sau khi bấm, cuối thẻ)                            │
│ Ghi bởi Hà · 12/11/2026                            │
│   12/11 10:07  Hà bán gói Liệu trình gội 10 buổi · Thu 1.500.000 ₫ │
│   20/12 09:00  Lan gia hạn gói · Hết hạn: 12/02 → 12/05            │
└────────────────────────────────────────────────────┘
```
Gói hủy: "… · đã hủy"; dòng "Lan hủy gói · Hoàn 1.500.000 ₫ · lý do: …". Gói sổ giấy: "Nhập từ sổ giấy · 03/02/2025". Không chip "Đã sửa".

### 2.6 Hồ sơ khách (`customers/CustomerProfile`) — đợt 1b
"Sửa hồ sơ khách" nằm ở đầu panel; `RecordTrace` là phần tử cuối của khối thông tin, **sau** `detail-grid`, **trước** `HealthSection`:
```
Chị Lê Thị Mai                         [Sửa hồ sơ khách]
SĐT 0912 333 444 · Nữ · …              (detail-grid)
Tạo bởi Hà · 03/02/2025 · đã sửa 2 lần ▾
  05/06/2025 09:12  Hà sửa hồ sơ
                    SĐT: 0905 111 222 → 0912 333 444   (chủ spa)
                    SĐT (đã đổi)                       (quản lý được bật)
(HealthSection…)
```

### 2.7 Lịch hẹn (`AppointmentSheet`) — đợt 2
`RecordTrace` là phần tử cuối trong thân sheet (sau thông tin, trước footer nút):
```
Ghi bởi Hà · 08:15 · đã sửa 1 lần ▾
  08:15  Hà đặt lịch
  09:40  Lan đổi lịch · Giờ: 14:00 → 15:00
```
Chip "Đã sửa" chỉ ở dạng danh sách (`kieu=ds`, `AppointmentsCalendar` gọi `marks=true`); lịch ngày / tuần không.

### 2.8 Chấm công + khen / phàn nàn (`PersonDaySheet`) — đợt 2
Sheet là một form có các `fieldset`: `RecordTrace` (`type="button"`, không submit) là phần tử cuối **trong mỗi** `fieldset`:
```
[fieldset Chấm công]
  Trạng thái · Vào · Ra …
  Ghi bởi Hà · 08:05 · đã sửa 1 lần ▾
[fieldset Khen / phàn nàn]
  Khen 1 · Phàn nàn 0 …
  Ghi bởi Hà · 18:10 ▾
```
Khả năng tìm thấy: ở `DailyBook`, sheet mở qua nút "Giờ & ghi chú" (`BookCard`); ở Bảng công tháng, bấm một ô (chỉ ô người gọi ghi được, tháng mở; K30). Ô lưới tháng: không chip. Ô chưa ai ghi: "Chưa có ai ghi" (K23); chỉ đổi chữ ghi chú: "Ghi chú (đã đổi)" (K25).

### 2.9 Sổ thưởng / phạt (`AdjustmentsTab`) — đợt 2
⋯ "Xem ai ghi, ai sửa" → `RecordTrace` mở sẵn dưới hàng: "Ghi bởi Hà · 03/11 · đã sửa 1 lần" + "Lan sửa: Số tiền 200.000 ₫ → 300.000 ₫" (thay bản) / "Lan xóa khoản thưởng". Đã build: dòng sửa đọc "Lan sửa khoản thưởng" + dòng phụ "Số tiền: 200.000đ → 300.000đ" (chuyển người: "Nhân viên: KTV Lan → KTV Mai", K21); meta theo §1.2: "Ghi bởi Hà · 10:05[ 03/11]" (K27); ⋯ là cột riêng, "Sửa" / "Xóa" giữ trên hàng (K31).

### 2.10 Bảng giá, mẫu gói, nhóm dịch vụ, quy tắc lương, cài đặt lịch hẹn — đợt 3
Phần tử cuối của form sửa / `RulesSheet` / sheet số khách tối đa (trên footer nút): "Sửa lần cuối bởi Lan · 12/11/2026 ▾" → "Lan sửa dịch vụ · Giá Nữ: 150.000 ₫ → 170.000 ₫". Danh sách không chip.

## 3. Console admin `/admin/spa/<id>` — đợt 1a

### 3.1 Panel "Lịch sử trên từng bản ghi" (giữa `OwnerMfaPanel` và `FeaturesPanel`)
```
┌ Lịch sử trên từng bản ghi ──────────────────────────────────────────┐
│ Dòng "Ghi bởi … · đã sửa … lần" và lịch sử thay đổi trên lượt,       │
│ hóa đơn, gói, hồ sơ khách… Kỹ thuật viên không bao giờ thấy.         │
│ Công tắc riêng của spa này: sửa mẫu vai trò không đổi nó.            │
│                                                                      │
│ Chủ spa      [Đang hiện]                       [Ẩn với Chủ spa]      │
│ Quản lý      [Đang ẩn]                         [Hiện cho Quản lý]    │
└──────────────────────────────────────────────────────────────────────┘
```
- Badge "Đang hiện" (`badge-ok`) / "Đang ẩn" (`badge-muted`) — chọn có chủ ý: công tắc nói về **hiển thị**, khác "Đang bật / Đang tắt" của tính năng.
- Nút `btn-secondary` chữ "Hiện cho Quản lý" / "Ẩn với Chủ spa"; `aria-label` **bắt đầu bằng chữ hiển thị** (WCAG 2.5.3): "Hiện cho Quản lý: lịch sử trên từng bản ghi".
- `ReasonSheet` có sẵn: title "Hiện lịch sử cho Quản lý?" / "Ẩn lịch sử với Chủ spa?"; description hiện → "Từ thao tác kế tiếp, Quản lý thấy ai ghi, ai sửa trên những bản ghi họ mở được. Số điện thoại cũ của khách không hiện." / ẩn → "Từ thao tác kế tiếp, {vai trò} không thấy ai ghi, ai sửa nữa. Dữ liệu giữ nguyên."; "Lý do" bắt buộc; `useStepUp`.
- `409 role.record_history_unchanged` → tải lại, "Trạng thái đã đổi ở nơi khác, đã tải lại."
- 360 px: mỗi vai trò một hàng, nút xuống dòng dưới badge.

### 3.2 `FeaturesPanel`
Hàng mới từ registry: "Nhật ký thao tác [Đang tắt] Mặc định · tắt [Bật]"; chữ "Mặc định" → "Mặc định · bật / tắt" theo `default_enabled`. Sheet bật: "Chủ spa thấy lại màn Nhật ký thao tác ở mục Báo cáo từ thao tác kế tiếp. Quản lý và kỹ thuật viên vẫn không thấy."

### 3.3 Trình sửa vai trò / mẫu vai trò, nhật ký nền tảng
- Permission mới nhãn "Xem ai ghi, ai sửa trên từng bản ghi". Ở **vai trò của spa**: ô chỉ đọc, chú thích "Đổi ở panel Lịch sử trên từng bản ghi" (`spa_local`). Ở **mẫu**: sửa được, chú thích "Chỉ áp cho spa tạo mới". Ô Kỹ thuật viên tắt: "Không cấp cho kỹ thuật viên".
- `AuditPage.tsx` `ACTIONS` thêm `platform.record_history.set` "Đổi hiển thị lịch sử bản ghi".

## 4. Spa: ẩn "Nhật ký thao tác"
`hasFeature(me, feature, { defaultOff })`: `defaultOff: true` → thiếu `features` thì `false`. `mayOpen.activity = can(me, "audit.read") && hasFeature(me, "activity_log", { defaultOff: true })`.

## 5. Dữ liệu web
- `lib/queryClient.ts`: `MutationCache({ onError, onSuccess: () => queryClient.invalidateQueries({ queryKey: ["record-history"] }) })` — một chỗ, mọi mutation.
- Các chỗ ghi cache trực tiếp (`appointments/cache.ts`, `staff/cache.ts` `setQueriesData`) nhận reply có `edited` (backend §4.5), nên chip không mất sau khi sửa.

## 6. A11y, perf
- Nút: `<button type="button">` + `aria-expanded` + `aria-controls`; ký hiệu `aria-hidden`; giờ `<time>`; chip có chữ. Hàng Sổ doanh thu: nút ở ô giờ, tên chứa chữ giờ.
- Focus: mở / đóng giữ focus ở nút; bị ẩn do `403` → focus về tiêu đề bản ghi.
- `--text-muted` 13 px trên `--surface` / `--surface-2` đạt AA (design system).
- CLS: dòng meta giữ chiều cao khi tải; chip cùng hàng.
- E2E axe 360 px + 1280 px, sáng / tối: Quầy mở lượt, Lịch sử khách mở thẻ, Sổ doanh thu sheet, hồ sơ khách, panel admin.
