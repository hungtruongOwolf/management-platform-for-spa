---
title: "Spec R1-2: Xem lại & theo dõi (management views)"
status: draft
updated: 2026-11-09
sources: [raw/2026-11-05-em-management-views-logo-ui.md]
related: [../paper-books-analysis.md, ../personas.md, front-desk-and-packages.md, ../../architecture/backend.md, ../../security/authz-matrix.md]
---

# Spec R1-2: Xem lại & theo dõi (management views)

**Status: draft** (chờ EM duyệt; backend đã build theo các mặc định dưới đây, câu hỏi mở ở §9). *Cập nhật 2026-11-09: mọi câu hỏi ở §9 đã chốt; backend + UI đã build ([plan r1-2](../../plans/r1-2-management-views.md)).* Phần backend (API) do agent backend làm; giao diện do agent UI làm trên contract `apps/api/openapi.json`.

## 1. Goal
App hiện chỉ để **nhập**. EM (2026-11-05): "muốn xem tổng hôm nay là bao nhiêu, xem một khách lịch sử dùng thế nào… không có management". Spec này thêm phần **xem lại**: tình hình một ngày, một khoảng ngày, sổ doanh thu, lịch sử từng khách, danh sách liệu trình cần theo dõi. Mọi con số được **tính ra** từ các sự kiện đã nhập (lượt, hóa đơn, thanh toán, hoàn tiền), không ai phải cộng tay. Phục vụ thước đo R1 #1 (bỏ sổ giấy: thay luôn phần "đọc lại" của sổ doanh thu và sổ liệu trình) và #3 (không quên khách: danh sách sắp hết hạn, lâu không quay lại). Persona chủ spa "nắm tình hình trong một màn hình" ([personas](../personas.md)).

Spec này cũng ghi **quyết định EM bỏ khái niệm "sổ cũ"** khỏi sản phẩm (§7).

## 2. Personas & quyền
- **Chủ spa:** xem mọi thứ, kể cả mọi con số tiền.
- **Quản lý (kiêm thu ngân):** xem mọi số liệu vận hành. Xem **tiền** là một permission riêng (`report.revenue.read`). **Mặc định: bật** cho quản lý, vì quản lý chính là thu ngân, đã cầm tiền và chốt quỹ mỗi ngày ([personas](../personas.md) #1); giấu tổng tiền với người đang đếm tiền không bảo vệ được gì. Chủ spa tắt được khi có màn quản lý role (spec #4) *(thay bằng [ADR-0018](../../decisions/ADR-0018-platform-admin-console.md) D5: hiện chỉ operator nền tảng sửa role của spa)*. → **Q1** cho EM.
- **Kỹ thuật viên:** không xem các màn này ở R1 (R2 có "số khách của tôi"; *đã có: "Khách của tôi" `/khach-cua-toi`, [staff-management](staff-management.md)*).

| Permission | Chủ spa | Quản lý | KTV | Dùng cho |
|---|---|---|---|---|
| `report.read` (mới) | ✓ | ✓ | — | Dashboard ngày, báo cáo khoảng ngày (số đếm) |
| `report.revenue.read` (mới) | ✓ | ✓ (mặc định, Q1) | — | Mọi con số tiền trong báo cáo; sổ doanh thu |
| `customer.read` + `visit.read` + `sale.read` | ✓ | ✓ | — | Lịch sử khách, tóm tắt khách |
| `customer.read` + `visit.read` | ✓ | ✓ | — | Trang dùng liệu trình, khách lâu không quay lại |
| `customer.read` | ✓ | ✓ | — | Tổng quan liệu trình |
| `opening_balance.import` | ✓ (tạm, xem §7) | **— (bỏ, migration 0013)** | — | Nhập sổ giấy |

Thiếu `report.revenue.read` thì response **không chứa** các trường tiền (không có key, không phải `null` rồi giấu ở UI).

## 3. User stories & acceptance criteria
Áp dụng cho mọi endpoint (như [spec R1-1](front-desk-and-packages.md) G1–G3): chưa đăng nhập → `401`; thiếu permission → `403 auth.forbidden`; ID của spa khác → `404`. Mọi endpoint chỉ **đọc** (không ghi, không audit) *(trừ `POST /customers/{id}/follow-ups`, thêm ở US5b vòng 3)*. Danh sách có thể dài thì phân trang bằng cursor (`limit` 1–100, mặc định 20; cursor hỏng → `422 cursor invalid`).

**Quy ước tiền (áp dụng US1–US3):** tính theo **tiền thực thu** (cash basis) từ các bảng chỉ-thêm `payment`, `sale_line`, `refund`:
- **Thu** = các dòng `payment` (đã trừ giảm giá, vì R1 thu đúng số phải trả).
- **Hủy** = dòng đảo (`reversal`) của hóa đơn bị hủy, ghi vào **ngày hủy**. Hóa đơn hôm qua hủy hôm nay: hôm qua vẫn có khoản thu, hôm nay có khoản hủy âm. Giống sổ giấy: không tẩy, chỉ ghi thêm.
- **Hoàn tiền** = dòng `refund` (khi chủ spa hủy liệu trình), ghi vào ngày hoàn; luôn thuộc loại "liệu trình". Gói mua cho người khác: tiền hoàn thuộc **người trả tiền** ("Đã chi", lịch sử, sổ doanh thu ghi tên người trả; người dùng gói chỉ thấy gói bị hủy, không có dòng âm — ADR-0019 D6).
- **Thực thu (net)** = Thu − Hủy − Hoàn.
- Mỗi khoản chia theo **hình thức** (tiền mặt / chuyển khoản) và **loại** (liệu trình / dịch vụ 1 lần), đúng các cột của sổ doanh thu.
- **Ngày nghiệp vụ** theo múi giờ của spa (Asia/Ho_Chi_Minh): 23:59 thuộc hôm nay, 00:00 thuộc hôm sau.
- **Số đếm** (lượt, buổi, gói bán) tính theo **trạng thái hiện tại**: lượt/hóa đơn đã hủy không được đếm (số lượt hủy hiện riêng).
- Tiền khách đã trả trên sổ giấy **là doanh thu** như mọi khoản khác (EM 2026-11-05, [raw](../../../raw/2026-11-05-em-carried-over-is-revenue.md): app chỉ lưu dữ liệu tiền, không xác nhận): lúc migrate, mỗi gói có tiền đã trả được ghi thành một hóa đơn + thanh toán **vào đúng ngày trên sổ** (`paid_on`). Gói sổ giấy không có tiền thì không có hóa đơn.

### US1: Dashboard "Hôm nay" (cho một ngày bất kỳ)
*Là chủ spa / quản lý, tôi muốn mở một màn hình là thấy ngay hôm nay (hoặc một ngày tôi chọn) có bao nhiêu khách, thu bao nhiêu, ai làm bao nhiêu, so với hôm qua và cùng thứ tuần trước.*
- AC1.1: Given ngày D có 3 lượt hoàn tất của 2 khách và 1 lượt đã hủy, when mở dashboard ngày D → `customers = 2`, `visits = 3`, `voided_visits = 1`.
- AC1.2: Given lượt ngày D trừ tổng 4 buổi liệu trình và làm 2 dịch vụ lẻ → `sessions_used = 4`, `one_off_services = 2`. Lượt đã hủy không được tính.
- AC1.3: Given ngày D bán 2 gói (1 hóa đơn có giảm giá 100.000đ, trả CK) và thu dịch vụ lẻ tiền mặt → `revenue.collected` chia đúng `cash` / `bank_transfer`, `collected_by_kind` chia đúng `package` / `one_off`; số liệu trình là giá **sau** giảm.
- AC1.4: Given hóa đơn bán gói ngày D−1 bị hủy ngày D → dashboard D−1 vẫn có khoản thu đó; dashboard D có `voids` bằng số đó (cùng hình thức, cùng loại) và `net` = thu − hủy − hoàn; `packages_sold` của D−1 không còn đếm gói đó.
- AC1.5: Given chủ spa hủy gói + hoàn 300.000đ tiền mặt ngày D → `refunds.cash = 300000`, `net_by_kind.package` giảm 300.000đ.
- AC1.6: Given lượt ghi lúc 23:59 ngày D và lượt lúc 00:01 ngày D+1 (giờ VN) → mỗi lượt thuộc đúng ngày của nó.
- AC1.7: `technicians[]`: mỗi KTV có làm ngày D: số lượt (đếm một lần dù làm nhiều dòng), số buổi, số dịch vụ lẻ; xếp nhiều lượt nhất trước.
- AC1.8: `compare.yesterday` và `compare.same_weekday_last_week`: số khách, lượt, buổi, gói bán, và `net_revenue` (chỉ khi có quyền tiền).
- AC1.9: Không gửi `date` → ngày hôm nay của spa. Ngày tương lai / không có dữ liệu → mọi số bằng 0, không lỗi.
- AC1.10 (neg): Quản lý bị tắt `report.revenue.read` → response **không có** key `revenue` và `net_revenue`; số đếm vẫn đủ. KTV → `403`. Chưa đăng nhập → `401`.
- AC1.11 (đối soát): Chạy `seed-demo` rồi mở dashboard từng ngày → số lượt, số khách, tiền thu/hủy/hoàn/net khớp với tổng tính trực tiếp từ các dòng `visit`, `payment`, `refund`.

### US2: Báo cáo theo khoảng ngày (ngày / tuần / tháng / tùy chọn)
*Là chủ spa, tôi muốn xem doanh thu và lượt khách theo ngày, tuần, tháng để thấy xu hướng, dịch vụ và gói nào chạy nhất.*
- AC2.1: `from`/`to` (mặc định 7 ngày gần nhất), `granularity` = `day` | `week` (tuần ISO, bắt đầu thứ Hai) | `month`. `series` có **đủ mọi kỳ** trong khoảng, kỳ không có dữ liệu = 0 (biểu đồ liền mạch); kỳ đầu/cuối được cắt theo `from`/`to`.
- AC2.2: Tổng các kỳ = `totals` (trừ `customers` = số khách **khác nhau** trên cả khoảng, không phải tổng các kỳ).
- AC2.3: Tổng tiền của các ngày trong khoảng = `revenue` của khoảng; mỗi ngày khớp dashboard US1 của ngày đó.
- AC2.4: `top_services` (10): theo số buổi + số lần làm lẻ; `one_off_revenue` chỉ khi có quyền tiền. `top_packages` (10): số gói bán (hóa đơn chưa hủy); gói sổ cũ không có mẫu gom theo tên gói; `revenue` = **số tiền thực trả** (giá sau giảm; gói sổ cũ = số đã trả trên sổ) **trừ số đã hoàn** của các gói đó (ADR-0019 D7), chỉ khi có quyền tiền.
- AC2.5 (neg): `from > to` → `422 from after_to`; khoảng > 366 ngày → `422 to range_too_long` *(giới hạn đổi ở "US2 — báo cáo linh hoạt", vòng 3: theo tháng/năm ≤ 732 ngày, chủ spa ≤ 1830)*. Thiếu `report.read` → `403`.

### US3: Sổ doanh thu hằng ngày (bản số của sổ giấy)
*Là chủ spa / thu ngân, tôi muốn xem sổ doanh thu như sổ giấy: mỗi giao dịch một dòng, cột dịch vụ 1 lần / liệu trình / chuyển khoản / tiền mặt / chưa thanh toán / ghi chú, có cộng từng ngày.*
- AC3.1: Mỗi hóa đơn = 1 dòng `sale` (tên khách, mô tả dịch vụ lẻ hoặc gói, tiền CK, tiền mặt, chưa thanh toán = 0 ở R1, ghi chú = lý do giảm giá). Hóa đơn bị hủy: dòng gốc giữ nguyên, có cờ `voided`, và một dòng `void` **âm** ở ngày hủy (ghi chú = lý do hủy). Hoàn tiền = một dòng `refund` âm (ghi chú = lý do).
- AC3.2: Thứ tự như sổ: theo ngày rồi theo thứ tự ghi. `days[]` có cộng từng ngày (CK, tiền mặt, chưa thanh toán, tổng) cho **mọi ngày trong khoảng**, không chỉ trang hiện tại. Tổng của một ngày = `revenue.net.total` của US1.
- AC3.3: Phân trang cursor (`limit` ≤ 100); đi hết các trang thì không thiếu, không trùng dòng.
- AC3.4 (neg): Cần `report.revenue.read` (sổ này toàn là tiền): quản lý bị tắt quyền → `403`. Khoảng > 92 ngày → `422`.

### US4: Lịch sử một khách (thay sổ theo dõi liệu trình)
*Là quản lý, tôi muốn mở một khách và thấy toàn bộ: các lần đến (làm gì, KTV nào, trừ buổi gói nào), các lần mua, thanh toán, hủy, hoàn tiền; và từng liệu trình đã dùng bao nhiêu, còn bao nhiêu, dùng ngày nào, ai làm.*
- AC4.1 Timeline (`GET /customers/{id}/history`), mới nhất trước, phân trang cursor. Loại sự kiện:
  - `visit`: giờ vào, từng dòng (dịch vụ, nguồn gói/lẻ, gói nào, số buổi trừ, còn lại sau buổi, KTV, giá dòng lẻ), tiền thu của lượt;
  - `purchase`: mua gói (dòng, tổng, giảm giá + lý do, thanh toán theo hình thức, người chốt);
  - `visit_voided` / `purchase_voided`: lúc hủy, lý do, tiền đảo, số buổi được cộng lại *(từ F16, §10.7: một mục ở giờ của lượt / hóa đơn, mang cả nội dung lẫn `void`)*;
  - `package_cancelled`: chủ spa hủy gói, lý do, khoản hoàn (nếu có);
  - `package_extended` (thêm 2026-11-07, [front-desk §10.11](front-desk-and-packages.md)): gia hạn gói, hạn cũ → hạn mới, lý do, người làm.
  Lượt đã hủy vẫn nằm trong timeline (có cờ `voided`). **Không bao giờ** có ghi chú sức khỏe. Ghi chú buổi (`visit.note`) chỉ khi người xem có `customer.health_notes.read` (EM 2026-11-07, front-desk §10.12).
- AC4.2 Trang liệu trình (`GET /customer-packages/{id}/usage`): thông tin gói (tổng/còn lại, hạn, quà tặng, trạng thái) + **mỗi lần dùng** (STT theo thứ tự cũ → mới: ngày, giờ, dịch vụ, số buổi, còn lại sau, KTV, đã hủy hay chưa). Đây là một trang của sổ giấy.
- AC4.3 Tóm tắt (`GET /customers/{id}/summary`): lần đến đầu, lần đến gần nhất, số ngày từ lần gần nhất, số lượt (hoàn tất / đã hủy), số buổi đã dùng, tiền đã trả / đã đảo / đã hoàn / **thực chi**, số gói theo trạng thái, số buổi còn lại của các gói đang dùng.
- AC4.4 (neg): Khách của spa khác → `404`. Thiếu một trong `customer.read`, `visit.read`, `sale.read` → `403`. Cursor sửa tay → `422`.
- AC4.5: Gói chuyển từ sổ giấy hiện như mọi gói khác (không nhãn riêng). Nếu sổ có ghi tiền đã trả, timeline có sự kiện "mua" vào ngày trên sổ (hóa đơn của khoản tiền đó); không có tiền thì không có sự kiện "mua".

### US5: Tổng quan liệu trình & khách cần chăm sóc lại
*Là quản lý, tôi muốn xem các liệu trình đang chạy, sắp hết hạn, đã hết buổi, đã hủy; và danh sách khách còn buổi mà lâu không quay lại, để gọi điện mời.*
- AC5.1 `GET /packages`: lọc `status` (active / expired / exhausted / cancelled, trạng thái suy ra như R1-1), tìm `q` theo tên (không dấu) hoặc SĐT, mới nhất trước. `counts` đếm cả spa (theo `q`), không phụ thuộc trang.
- AC5.2 Sắp hết hạn: `expiring_within_days=N` (1–365, mặc định cửa sổ đếm 30) → chỉ gói **active** có hạn ≤ hôm nay + N, hạn gần nhất trước, kèm `days_to_expiry`, `last_used_on`.
- AC5.3 *(thay bằng AC5.3' ở "Bổ sung vòng 3")* `GET /customers/inactive?days=N` (mặc định 7, EM 2026-11-05): khách **còn buổi** trên gói đang dùng mà lần đến gần nhất (hoặc ngày bắt đầu gói nếu chưa đến lần nào) cách hôm nay ≥ N ngày; vắng lâu nhất trước; kèm số buổi còn lại. `scope=all`: mọi khách đã từng đến/mua.
- AC5.4 (neg): `days` hoặc `expiring_within_days` ngoài 1–365 → `422`; `q` 1 ký tự → `422 too_short`. KTV → `403`.

### Bổ sung vòng 3 (EM 2026-11-05, [raw](../../../raw/2026-11-05-em-ui-review-round3.md))

**US5 — phạm vi "Lâu không quay lại" (thay AC5.3):**
- AC5.3': `GET /customers/inactive?days=N` (mặc định 7) liệt kê **mọi khách đã từng đến hoặc trả tiền** mà "lần gần nhất" cách hôm nay ≥ N ngày (biên **tính cả** ngày thứ N). "Lần gần nhất" = ngày muộn nhất trong: lượt hoàn tất gần nhất, hóa đơn chưa hủy gần nhất, ngày bắt đầu gói gần nhất (gói sổ giấy không có hóa đơn). Khách chỉ tạo hồ sơ, chưa đến chưa mua → không có trong danh sách. Lượt / hóa đơn đã hủy không tính.
- Gồm cả **khách lẻ** (đến theo buổi, không gói) và **khách liệu trình** dù gói còn buổi, đã hết buổi hay hết hạn. Mỗi dòng có `kind`: `package` (có ít nhất một gói không thuộc hóa đơn đã hủy) / `one_off`; lọc `kind=package|one_off|all`. `scope=with_sessions` vẫn còn (chỉ khách còn buổi trên gói đang dùng); **mặc định đổi thành `all`**.
- `q`: tên (không dấu, ≥ 2 ký tự) hoặc SĐT (≥ 3 số), ≤ 100 ký tự. Mỗi dòng có `gender`.

**US5b — Theo dõi gọi lại khách (follow-up):**
*Là quản lý, tôi muốn đánh dấu đã gọi / nhắn khách nào, ngày nào, kết quả ra sao và khi nào gọi lại, để biết hôm nay cần gọi ai thay vì chỉ nhìn ai vắng lâu.*
- AC5b.1 `POST /customers/{id}/follow-ups` (Idempotency-Key, quyền mới `customer.follow_up`: chủ spa + quản lý): `contacted_on` (mặc định hôm nay theo giờ spa; không ở tương lai; lùi tối đa 30 ngày), `channel` = `phone` | `zalo` | `in_person` | `other`, `outcome` = `will_come` (liên lạc được, sẽ đến) | `not_interested` | `no_answer` | `other`, `note` (≤ 500 ký tự, ghi chú liên lạc, **không** ghi sức khỏe), `next_follow_up_on` (≥ ngày liên lạc, ≤ 365 ngày sau). Người ghi = user của session (body không gửi được). Chỉ thêm, không sửa / xóa (ghi nhầm thì ghi lần mới). Audit `customer.follow_up` (ghi `has_note`, không chép nội dung ghi chú).
- AC5b.2 **Quy tắc mặc định** (lead chốt, EM duyệt khi xem bản build, [raw](../../../raw/2026-11-05-em-answers-q5-q7.md)); không chọn ngày gọi lại thì:
  - `no_answer` → ngày liên lạc + **3 ngày**;
  - `will_come` → + **7 ngày** (kiểm lại nếu vẫn chưa đến; khách đến là sang đợt vắng mới nên tự hết);
  - `not_interested` → không có ngày; khách **ra khỏi danh sách "cần liên hệ" 90 ngày** kể từ lần liên lạc, sau đó hiện lại một lần;
  - `other` → không có ngày.
  - Ngày nhập tay luôn thắng.
- AC5b.3 `GET /customers/{id}/follow-ups`: mới nhất trước, tối đa 100 (`has_more`), kèm `today`, `suggested_next_on_by_outcome {will_come, not_interested, no_answer, other}` (ngày quy tắc sẽ lưu nếu liên lạc hôm nay) và `not_interested_pause_days` (90). Form điền sẵn ngày gợi ý **trước khi lưu**, người dùng sửa được. Quyền `customer.read`.
- AC5b.4 Mỗi dòng của `/customers/inactive` có `last_follow_up {on, channel, outcome, by}` và `next_follow_up_on`, **chỉ tính các lần liên lạc trong đợt vắng hiện tại** (`contacted_on` ≥ "lần gần nhất"). Khách quay lại sau khi được gọi → đợt vắng mới, sạch.
- AC5b.5 Mỗi dòng có `due_on` = ngày khách cần liên hệ lại: `next_follow_up_on`, hoặc với `not_interested` không có ngày thì ngày liên lạc + 90. Lọc `follow_up=`: `due` = chưa liên lạc trong đợt này **hoặc** `due_on` ≤ hôm nay; `not_contacted` = chưa liên lạc; `scheduled` = có `next_follow_up_on` sau hôm nay; `all` (mặc định). Đã liên lạc mà không có `due_on` (`other` không hẹn ngày) chỉ hiện ở `all`.
- AC5b.6 Sắp xếp `sort=due|absent`: `due` = chưa liên lạc trước (vắng lâu trước), rồi `due_on` gần nhất, cuối cùng là đã liên lạc không có `due_on`; `absent` = vắng lâu nhất trước. Mặc định `due` khi `follow_up=due|scheduled`, ngược lại `absent`.
- AC5b.7 (neg): KTV → `403` (EM Q7: KTV không quản lý khách nên **không bao giờ** có `customer.follow_up`); chưa đăng nhập → `401`; khách spa khác → `404`; thiếu Idempotency-Key → `400`; cùng key khác body → `422 idempotency.key_reuse_mismatch`.

**US4 — tìm trong hồ sơ khách:** `GET /customers/{id}/history?q=&kind=&from=&to=`: `q` (không dấu, 2–100 ký tự) khớp lượt theo tên dịch vụ, danh mục, tên gói, KTV; hóa đơn theo dòng gói và người chốt; hủy gói theo tên gói. `kind` một loại sự kiện; `from`/`to` theo ngày nghiệp vụ của sự kiện (`from > to` → `422`).

**US5 — gói không thời hạn (EM hỏi "gói unlimited hết hạn thì sao"):** đã đúng từ R1-1 D2: `expires_on` trống = **không bao giờ hết hạn**. Đã kiểm `app.package_status` (`expires_on < today` với NULL → không phải `expired`), cửa sổ `expiring_within_days` và `counts.expiring_soon` (`expires_on <= …` với NULL → không đếm), `PackageStatus::derive`. Thêm test: gói không hạn bắt đầu 3 năm trước vẫn `active`, không bao giờ vào "sắp hết hạn" / "hết hạn". `/packages?q=` nay tìm cả **tên gói** (không dấu) ngoài tên / SĐT khách.

**Trùng SĐT khi đang gõ:** `GET /customers/phone-check?phone=` (quyền `customer.read`) → `{valid, taken, existing_id?, full_name?}`, chuẩn hóa giống hệt khi tạo khách; số chưa đủ → `valid=false` (không lỗi, gọi được mỗi lần gõ). Chỉ trong spa của người gọi. Không lộ gì hơn `GET /customers?q=<số>`. Tạo khách vẫn trả `409 customer.phone_taken`.

**Danh mục dịch vụ:** mọi DTO hiện tên dịch vụ có thêm `category_name` (danh mục **hiện tại** trong bảng giá, không phải snapshot): dòng lượt (`VisitLineDto`, lượt hôm nay), dòng lượt trong lịch sử, item gói (`CustomerPackageItemDto`), trang dùng gói, top dịch vụ trong báo cáo. Giới tính khách (`customer_gender`) có ở lượt (`VisitDto`), dòng sổ doanh thu, dòng tổng quan gói; dòng khách vắng có `gender`.

**US2 — báo cáo linh hoạt:** `preset` = `last_7_days`, `last_30_days`, `this_month`, `last_month` (theo ngày), `this_year`, `last_12_months`, `last_2_years` (năm nay + năm trước; theo tháng), tính theo "hôm nay" của spa; `granularity` thêm `year` (so sánh các năm); ghi đè được granularity mặc định của preset; `preset` cùng `from`/`to` → `422 preset conflicts_with_range`. Giới hạn: theo ngày/tuần ≤ 366 ngày, theo tháng/năm ≤ 732 ngày (**2 năm**, cho mọi người, trong ngân sách 300 ms). **Chủ spa** (quyền mới `report.long_range`, migration 0015) xem được tới 1830 ngày (5 năm) theo tháng/năm, preset `last_3_years` (theo tháng) và `last_5_years` (theo năm); tính tại chỗ, không có bảng tổng hợp, ngân sách riêng p95 ≤ **1,5 s** trên bench 3 năm (đo: 368–478 ms). Thiếu quyền → `422 to range_too_long` như cũ (cả hai preset này luôn bị từ chối, kể cả ngày đầu năm khi khoảng chưa vượt 2 năm). Mỗi điểm `series` có `counts.customers`, `counts.visits` và `net_revenue` (phẳng, cho biểu đồ đường; chỉ khi có quyền tiền). Thêm `technicians[]` cho cả khoảng (như dashboard ngày).

### US6 (để sau, R1-3): Sổ thu chi
*(Số "R1-3" sau đó được dùng cho [spec lịch hẹn](appointments.md); sổ thu chi chưa có spec, chưa build.)*

Sổ thu chi giấy ghi cả **chi** (chi phí vận hành, rút/nộp quỹ) và **tồn quỹ** tiền mặt / tài khoản. Phần **thu** đã có (US3). Phần **chi** là loại dữ liệu mới (phiếu chi: ngày, diễn giải, số tiền, hình thức, người chi, chứng từ) cần spec riêng: quyền ghi/sửa, chốt quỹ cuối ngày, tồn đầu kỳ, đối soát. **Không làm ở R1-2.**

## 4. UX notes (cho agent UI)
*Gợi ý ban đầu. Màn đã build (nav, [frontend](../../architecture/frontend.md)): trang chủ `/` "Tổng quan"; `/bao-cao` "Báo cáo kinh doanh" (tab "Tổng quan" / "Sổ doanh thu" `?tab=ledger`, chọn kỳ bằng preset thay cho tab Ngày / Tuần / Tháng); `/goi-lieu-trinh` "Gói liệu trình của khách" và `/khach-lau-khong-quay-lai` "Khách lâu chưa quay lại" là **hai màn riêng** (thay cho một màn "Theo dõi" hai tab); hồ sơ khách `/khach-hang/$customerId`.*
- **Trang chủ = Dashboard hôm nay**: hàng thẻ số (khách, lượt, buổi, gói bán, thực thu) có mũi tên so với hôm qua / tuần trước; bảng tiền 2×2 (loại × hình thức) + hủy + hoàn; danh sách KTV. Chọn ngày bằng date picker; nút "Hôm qua / Hôm nay".
- **Báo cáo**: tab Ngày / Tuần / Tháng / Tùy chọn, biểu đồ cột doanh thu + đường lượt khách, top dịch vụ / gói.
- **Sổ doanh thu**: bảng đúng cột sổ giấy, mỗi ngày một nhóm có dòng cộng; dòng hủy/hoàn màu đỏ, số âm; dòng gốc đã hủy gạch ngang.
- **Hồ sơ khách**: tab Lịch sử (timeline), tab Liệu trình (mỗi gói một thẻ, bấm vào là trang sổ liệu trình), khối tóm tắt trên cùng.
- **Theo dõi**: tab Liệu trình (lọc trạng thái, sắp hết hạn), tab "Lâu không quay lại" (gọi điện / Zalo bằng một chạm `tel:`).
- Mọi màn có trạng thái loading / rỗng / lỗi / có dữ liệu. Tiền hiển thị `1.500.000đ`. Ẩn hẳn khối tiền khi response không có trường tiền (không hiện "0đ").
- Bỏ khỏi UI: mục menu và màn `/nhap-so-cu`, nhãn "Nhập từ sổ cũ", ô ở trang chủ (§7). *(Đã làm.)*

## 5. Data
Không có bảng mới; chỉ đọc. Migration `0013_management_views`:
- 2 permission mới cho role hệ thống owner + manager; bỏ `opening_balance.import` khỏi manager.
- Hàm `app.package_status(...)` (định nghĩa SQL duy nhất của trạng thái suy ra), view `app.customer_package_state` (tổng/còn lại theo gói), view `app.revenue_book` (sổ doanh thu suy ra từ `payment`, `sale_line`, `refund`). View là `security_invoker`, nên RLS của bảng gốc vẫn áp dụng cho `app_rw`.
- Index theo `(tenant_id, business_date)` cho `sale`, `payment`, `visit`, `refund`; `visit_line (tenant_id, customer_package_id)`; `refund (tenant_id, customer_id)`.
Dữ liệu cá nhân trong response: tên + SĐT khách (đã có ở `customer.read`). Không có dữ liệu sức khỏe.

## 6. Non-functional
- **Hiệu năng:** p95 đọc < **300 ms** với dữ liệu 3 năm (~44.000 lượt, ~30.800 hóa đơn); đo bằng test bench `app/tests/reports_bench.rs` (chạy tay, `--ignored`): p95 tệ nhất 150 ms (báo cáo 1 năm theo tháng), xem [backend §3b](../../architecture/backend.md).
- **Bảo mật:** server là ranh giới tin cậy duy nhất; quyền kiểm ở service; tenant lọc tường minh + RLS; trường tiền không bao giờ gửi cho người thiếu quyền; `Cache-Control: no-store`.
- **A11y / i18n / thiết bị:** như R1-1 (WCAG 2.2 AA, tiếng Việt, responsive mọi thiết bị).

## 7. Bỏ khái niệm "sổ cũ" (quyết định EM 2026-11-05)
EM: dữ liệu sổ giấy là **migration một lần** do team tự điền sau, không cần phân biệt sổ cũ / sổ mới.
- **API:** `CustomerPackageDto` bỏ trường `origin`. Gói chuyển từ sổ giấy giống hệt gói khác; có tiền đã trả thì `sale_id` trỏ tới hóa đơn của khoản tiền đó, không có thì `sale_id = null`. Bảng vẫn giữ cột `origin`; hóa đơn sổ giấy có `sale.source = paper_book` (không bao giờ có trong API) để audit và để chạy lại không ghi trùng.
- **Nhập sổ giấy chỉ dành cho người vận hành.** Hai lựa chọn: (a) lệnh CLI `app import-paper-book --tenant <slug> --file <csv>`; (b) giữ `POST /opening-balances` chỉ cho chủ spa.
  **Khuyến nghị và đã làm: (a) CLI.** Lý do: việc này là chuyển dữ liệu một lần của team, không phải thao tác của spa; CLI không có bề mặt HTTP để tấn công, chạy trên máy vận hành, đọc file CSV mà team đã soát lại, có `--dry-run`, chạy lại không nhập trùng. HTTP route vẫn còn **tạm thời** và chỉ chủ spa có quyền (migration 0013 bỏ quyền của quản lý), vì màn `/nhap-so-cu` của web vẫn gọi nó cho tới khi agent UI xóa màn đó; sau đó xóa route (ticket theo dõi trong plan). *Hiện trạng 2026-11-09: màn `/nhap-so-cu` đã xóa; route vẫn còn (chỉ chủ spa) và đã nhận thêm `paid_amount` / `paid_on` / `method` (0018); việc xóa route (plan M8) đã **bỏ** theo [ADR-0019](../../decisions/ADR-0019-audit-decisions-2026-11-09.md) D15: route giữ lại.*
- **CSV** (UTF-8, dòng đầu là header, mỗi dòng = một dịch vụ trong một gói; các dòng cùng `phone` + `package_name` + `starts_on` gộp thành một gói):

| Cột | Bắt buộc | Ý nghĩa |
|---|---|---|
| `phone` | ✓ | SĐT khách; khách đã có (theo SĐT) thì dùng hồ sơ đó |
| `full_name` | khi khách mới | Tên khách |
| `gender` | | `female` / `male` / trống |
| `package_name` | ✓ | Tên gói như trong sổ |
| `sold_price` | ✓ | Giá khách đã trả (đồng, số nguyên) |
| `starts_on` | ✓ | Ngày đăng ký `YYYY-MM-DD` |
| `expires_on` | | Hạn dùng `YYYY-MM-DD` |
| `gift_note` | | Quà tặng |
| `service` | ✓ | Tên dịch vụ, khớp đúng tên trong bảng giá (không phân biệt hoa thường) |
| `sessions_purchased` | ✓ | Số buổi mua |
| `sessions_bonus` | | Số buổi tặng (mặc định 0) |
| `sessions_remaining` | ✓ | Số buổi còn lại hôm nay |
| `paid_amount` | | Tiền khách đã trả cho gói trên sổ (đồng, trống = 0). > 0 → ghi thành hóa đơn + thanh toán, là doanh thu của ngày `paid_on` |
| `paid_on` | khi `paid_amount` > 0 | Ngày trả tiền trên sổ `YYYY-MM-DD` (không sau hôm nay) |
| `method` | | `cash` (mặc định) / `transfer` (= `bank_transfer`) |

  Lệnh đi qua `sales::paper::import_opening_balance` với quyền của chủ spa: use case nhập sổ (`packages::opening_balance::import_in`: validate, audit) rồi, trong cùng transaction, ghi tiền đã trả của từng gói thành hóa đơn `source = paper_book` (một dòng gói, một thanh toán, `business_date = paid_on`, audit `sale.paper_book`). Ghi lùi ngày được vì đây là việc của chủ spa (quyền `opening_balance.import`); ngày tương lai bị từ chối. Mỗi gói gắn tối đa **một** hóa đơn (trigger 0018), nên chạy lại không ghi trùng. Gói đã nhập trước đây mà chưa có tiền: chạy lại file có `paid_amount`/`paid_on` thì tiền được ghi **một lần** (đếm ở `payments_booked`). Các dòng cùng gói lặp lại cùng `paid_amount`/`paid_on`/`method` hoặc để trống ở dòng sau; khác nhau → lỗi, không ghi gì. Hủy hóa đơn sổ giấy (sửa số tiền nhập nhầm) đảo tiền vào ngày hủy như mọi hóa đơn, gói sổ giấy vẫn giữ nguyên. `POST /opening-balances` (chỉ chủ spa) nhận cùng các trường `paid_amount`, `paid_on`, `method` trên mỗi gói (một lần gửi = một lần nhập, chống trùng bằng Idempotency-Key).
- **Web:** bỏ menu/màn `/nhap-so-cu`, nhãn "Nhập từ sổ cũ", ô trang chủ (agent UI). `PackageActions` dùng `sale_id != null` thay cho `origin === "sale"`. *(Đã làm.)*

## 8. Out of scope
- Sổ thu chi / phiếu chi / chốt quỹ (US6; chưa có spec).
- Xuất Excel/PDF; gửi báo cáo qua Zalo.
- Hoa hồng, lương (R2). Báo cáo theo chi nhánh (chỉ 1 chi nhánh ở R1).
- Nhắc tự động (spec #3); US5 chỉ là danh sách để xem.

## 9. Open questions (EM, trước khi chốt R1-2)
- **Đã chốt (EM 2026-11-05, [raw](../../../raw/2026-11-05-em-management-decisions-ui-layout.md)):**
  - Q1: quản lý **được** xem tiền (`report.revenue.read`); chỉnh sau được.
  - Q2: "lâu không quay lại" mặc định **7 ngày**, kèm màn hình riêng xem cả danh sách khách quá hạn.
  - Q3: tiền thực thu, khoản hủy ghi vào **ngày hủy**.
  - Q4: tạm thời **không giới hạn** số ngày quản lý được xem.
- **Đã chốt vòng 3 (EM 2026-11-05, [raw](../../../raw/2026-11-05-em-answers-q5-q7.md)):**
  - Q5: 2 năm cho mọi người; 3–5 năm **chỉ chủ spa**, lâu lâu mới xem nên tính tại chỗ (chậm hơn được), chưa làm bảng tổng hợp. Web (2026-11-06, `redesign(quanly)`): preset "3 năm" (theo tháng) / "5 năm" (theo năm) chỉ hiện khi có `report.long_range`; link có preset này mở "12 tháng" cho người khác.
  - Q6: EM giao lead chốt và sẽ review bản build: AC5b.2 (+3 / +7 / nghỉ 90 ngày / không có).
  - Q7: KTV **không** follow-up.

## 10. Nhật ký ghi chú (EM 2026-11-07, [raw](../../../raw/2026-11-07-em-daily-book-and-notes-review.md))
EM: "mấy ghi chú các thứ sau này có quản lý hay chủ spa vào xem lại thì ở đâu?" Trước đây mỗi ghi chú chỉ đọc được ở màn nơi nó được ghi (lượt, hồ sơ, sổ ngày…), không có chỗ đọc lại tất cả. **Vì sao ở spec này:** đây là một màn *xem lại* chỉ đọc, cùng người dùng (chủ spa + quản lý, `report.read`) và cùng mẫu (khoảng ngày, bộ lọc trên URL, cursor) với báo cáo và lịch sử khách; nó gom cả ghi chú nhân viên lẫn ghi chú khách nên không thuộc riêng staff-management.

### US10: Đọc lại mọi ghi chú ở một chỗ
*Là chủ spa hoặc quản lý, tôi muốn đọc lại mọi ghi chú đã ghi trong app, mới nhất trước, để không phải mở từng lượt / hồ sơ / ngày mới biết đã có chuyện gì.*

**10.1 Chỗ đặt.** Nav **Báo cáo → "Nhật ký ghi chú"** (`/nhat-ky-ghi-chu`; không có tên ngắn, không nằm trên tab bar điện thoại), ngay dưới "Báo cáo kinh doanh". Lý do: nhóm Báo cáo là chỗ "xem lại" (đã chỉ mở cho `report.read` = chủ spa + quản lý, KTV không thấy), không nằm trong Nhân viên vì một nửa ghi chú là về khách, và không nằm trong Khách hàng vì một nửa là về nhân viên. Màn là chunk lazy riêng (ngân sách JS khởi động ≤ 125 KiB).

**10.2 Liệt kê gì, ai đọc được loại nào.** Mỗi loại giữ **đúng quy tắc đọc nó đã có** ở màn gốc (không nới quyền nào). Server bỏ cả nhánh SQL của loại người gọi không được đọc (deny by default); response có `kinds` = các loại được đọc, nên bộ lọc "Loại" chỉ hiện các loại đó.

| Loại (`kind`) | Chip | Nguồn | Quyền đọc (như màn gốc) | Thời điểm / ngày | Tác giả |
|---|---|---|---|---|---|
| `visit_note` | Ghi chú buổi | `visit.note_ct` (mã hóa) | `visit.read` + `customer.health_notes.read` | giờ làm / ngày của lượt | người ghi lượt |
| `customer_note` | Ghi chú hồ sơ | `customer.note` | `customer.read` | lần sửa hồ sơ cuối (`updated_at`) | — (hồ sơ không lưu người sửa) |
| `health_note` | Lưu ý sức khỏe | `customer.health_note_ct` (mã hóa) | `customer.read` + `customer.health_notes.read` | như trên | — |
| `attendance_note` | Chấm công | `attendance_day.note` | `attendance.record` | lần ghi / ngày công | người chấm |
| `review_note` | Khen / phàn nàn | `staff_daily_review.note` | `evaluation.record` | lần ghi / ngày | người ghi |
| `adjustment` | Thưởng / Phạt | `staff_adjustment.reason` (khoản chưa xóa) | `evaluation.record` | lúc ghi / ngày xảy ra | người ghi |
| `follow_up` | Chăm sóc lại | `customer_follow_up.note` | `customer.read` | lúc ghi / ngày liên hệ | người ghi |
| `void` | Hủy lượt / Sửa lượt / Hủy hóa đơn / Hủy gói | `visit.void_reason`, `sale.void_reason` (hóa đơn không thuộc lượt), `customer_package.cancel_reason` (gói không hủy theo hóa đơn) | `customer.read` + `visit.read` + `sale.read` (quy tắc lịch sử khách) | lúc hủy | người hủy |
| `employment_end` | Nghỉ việc | `staff_employment.end_reason` | `staff.manage` (**chỉ chủ spa**) | ngày làm cuối | người cho nghỉ |
| `appointment_note` (Q-N1) | Ghi chú lịch hẹn | `appointment.note` | `appointment.manage` (lịch hẹn của spa; từ #85 KTV thấy ghi chú lịch hẹn **của chính mình** qua `GET /me/appointments` (`staff.read_own`), nhưng nhật ký cần `report.read` nên KTV và người đã nghỉ (`staff.read_own_past`, ADR-0014) không vào được) | lúc đặt / **ngày đặt**; ngày hẹn ở `about_date` | người đặt |
| `evaluation_note` (Q-N1) | Nhận xét tháng / Kế hoạch kèm cặp | `staff_evaluation.comment`, `.coaching_plan` (hai dòng) | `evaluation.record` (`GET /evaluations`) | lần ghi cuối; tháng được đánh giá ở `about_date` | người chấm |
| `staff_note` (Q-N1) | Ghi chú nhân sự | `staff.note` (chữ tự do duy nhất của hồ sơ nhân sự ngoài lý do nghỉ; đợt làm việc chỉ có `end_reason`) | `staff.manage` (**chỉ chủ spa**: nằm trong `details`, ngoài allowlist 14 key của quản lý ở Nhân sự) | lần sửa hồ sơ cuối (`staff.updated_at`, như Q-N2) | — (hồ sơ không lưu người sửa) |
| `package_extension` (Q-N1) | Gia hạn gói | `customer_package_extension.reason` | `customer.read` + `visit.read` + `sale.read` (lịch sử khách hiện mục "Gia hạn") | lúc gia hạn / ngày gia hạn; hạn mới ở `about_date` | người gia hạn |
| `discount` (Q-N1) | Giảm giá | `sale.discount_reason` | hóa đơn của lượt (`sale.visit_id`): `visit.read` (`GET /visits` hiện lý do giảm); hóa đơn khác: `sale.read` (`GET /sales/{id}`) + `customer.read` (tên khách) | lúc lập hóa đơn / ngày hóa đơn | người lập |
| `appointment_reason` (F21, 2026-11-08) | Hủy lịch / Không đến | `appointment.status_reason` của lịch `cancelled` / `no_show` (bắt buộc khi đóng) | `appointment.manage` (như ghi chú lịch hẹn; KTV không vào nhật ký) | lúc đóng / **ngày đóng** (`closed_at`); ngày hẹn ở `about_date`; `detail` = `cancelled` \| `no_show` | người đóng (`closed_by`) |
| `package_note` (F4, 2026-11-08, §10.8) | Ghi chú gói | `sale.note_ct` (mã hóa) | `sale.read` + `customer.read` + `customer.health_notes.read` | lúc lập / ngày hóa đơn; khách = người dùng gói; `detail` = tên gói | người lập hóa đơn |

Không có nguồn mới nào thiếu quy tắc đọc, nên không loại nào phải "mặc định chỉ chủ spa". Ngoài phạm vi: lý do không đến / hủy lịch hẹn (`appointment.status_reason`) *(thay bằng §10.6: thêm loại `appointment_reason`, F21 2026-11-08)* và người đi cùng dạng ghi chú (EM chưa hỏi).

Quản lý mặc định thấy mọi loại trừ "Nghỉ việc" và "Ghi chú nhân sự". Quản lý bị bỏ `customer.health_notes.read` thì không thấy ghi chú buổi và lưu ý sức khỏe, kể cả khi lọc đúng loại đó hay tìm đúng chữ trong đó. KTV: `403` (màn trả "Không có quyền").

**10.3 Mỗi dòng.** Chip loại · giờ (hồ sơ: "Hồ sơ sửa lúc …"; nghỉ việc: "Ngày làm cuối") · tác giả; khách (link hồ sơ) hoặc nhân viên; chữ ghi chú (giữ xuống dòng); link về nguồn: ghi chú buổi → lịch sử khách lọc đúng ngày + "Lượt khách"; hủy/sửa lượt → lịch sử khách lọc ngày + "Hủy lượt"; hồ sơ / sức khỏe / chăm sóc lại / hủy hóa đơn / hủy gói → hồ sơ khách; chấm công / khen-phàn nàn / thưởng-phạt → sổ hằng ngày đúng ngày; nghỉ việc → Nhân sự ("Tất cả"). Gom theo ngày kinh doanh (tiêu đề ngày), mới nhất trước.

**10.4 Bộ lọc** (đều trên URL: `from`, `to`, `kind`, `staff`, `customer`, `q`): khoảng ngày (mặc định **30 ngày gần nhất**, tối đa 366 ngày một lần), loại, nhân viên (ghi chú *về* người đó, ghi chú *do tài khoản* của người đó viết, và ghi chú buổi / hủy lượt của lượt người đó làm), khách ("Chỉ xem khách này" trên một dòng → chip bỏ lọc), tìm chữ (không dấu, 2–100 ký tự; khớp chữ ghi chú hoặc tên khách / nhân viên). Phân trang cursor 30 dòng, "Xem thêm".

**Tìm trong ghi chú mã hóa (quyết định, lead's decision — EM to review).** Ghi chú buổi và lưu ý sức khỏe là dữ liệu sức khỏe, mã hóa AES-256-GCM ở tầng app; DB không tìm được trong ciphertext. Các cách: (a) giải mã mọi ghi chú của spa mỗi lần tìm — chi phí tăng theo cả lịch sử, bỏ; (b) chỉ tìm loại không mã hóa — người dùng tìm "đau lưng" sẽ không thấy đúng loại họ cần nhất, bỏ; (c) **chọn:** loại không mã hóa lọc bằng SQL; loại mã hóa *trong khoảng ngày đã lọc* được giải mã rồi so trên server, **mỗi request quét tối đa 500 ghi chú**. Hết 500 mà chưa đủ trang → trả số đã thấy + `scan_capped: true` + cursor đi tiếp; UI ghi "Đã tìm trong 500 ghi chú gần nhất…" và nút "Tìm tiếp". Chi phí: giải mã AES-GCM một ghi chú cỡ micro giây, 500 ghi chú < 5 ms CPU + vài lượt đọc 100 dòng; không index hay bản rõ nào mới trong DB, không blind index (blind index lộ tần suất từ và không hỗ trợ tìm một phần chữ). Chỉ người có `customer.health_notes.read` mới làm server giải mã; ghi chú đã giải mã không vào log, audit hay cache (response `private, no-store`).

**10.5 Bảo mật.** Server là ranh giới tin cậy: `report.read` mở màn, từng loại kiểm quyền riêng (10.2); tenant lọc tường minh ở mọi nhánh + RLS; giải mã bằng đúng associated data của bên ghi (`visit.note` + tenant + lượt; `customer.health_note` + tenant + khách), lỗi chỉ ghi id bản ghi; `deny_unknown_fields` trên query (`?userId=` → 400); `customer_id` / `staff_id` của spa khác → danh sách trống; đọc không ghi audit (không có ghi chú nào trong `audit_log`).

**10.6 Hiệu năng.** Migration 0027: index một phần theo ngày cho từng nguồn có ghi chú (`visit (tenant_id, business_date) WHERE note_ct IS NOT NULL`, `voided_at` của lượt / hóa đơn, `cancelled_at` của gói, `customer (tenant_id, updated_at)` khi có ghi chú, `customer_follow_up (tenant_id, contacted_on)`, `staff_employment (tenant_id, ended_on)`); sổ chấm công / khen / thưởng-phạt đã có `(tenant_id, work_date)` (0017).

**AC**
- AC10.1: chủ spa thấy cả **16** loại (thêm `appointment_reason`, F21 2026-11-08; `package_note`, F4 2026-11-08); quản lý mặc định thấy **14** (không "Nghỉ việc", không "Ghi chú nhân sự"); KTV `403`; chưa đăng nhập `401`. Quyền được kiểm **trước** khi đọc query: KTV gửi query sai vẫn `403` (F17).
- AC10.1b (Q-N1): giảm giá của hóa đơn trong lượt chỉ cần `visit.read`; hóa đơn khác cần `sale.read` + `customer.read`; chỉ `sale.read` → không thấy giảm giá nào.
- AC10.2: bỏ bất kỳ một quyền nào trong cột "Quyền đọc" của một loại → loại đó biến mất (cả khi lọc `kind` hoặc tìm chữ trong nó), `kinds` không còn nó.
- AC10.3: lọc khoảng ngày / loại / nhân viên / khách / chữ đúng như 10.4; trang 2 dòng đi hết danh sách một lần, không lặp.
- AC10.4: chủ spa khác không thấy dòng nào của spa này, kể cả khi đưa id khách / nhân viên của spa này.
- AC10.5: `Cache-Control: private, no-store`; màn ở chunk lazy; axe không lỗi serious/critical ở 1280 và 390, sáng và tối.

**Đã chốt (EM 2026-11-07, [raw](../../../raw/2026-11-07-em-decisions-notes-journal-scope.md)):**
- **Q-N1: có.** Thêm ghi chú lịch hẹn, nhận xét / kế hoạch kèm cặp của đánh giá tháng, ghi chú nhân sự, lý do gia hạn gói, lý do giảm giá (5 dòng cuối bảng 10.2), mỗi loại giữ quy tắc đọc ở nơi nó được ghi / hiện. Migration 0028: index một phần cho 5 nguồn này.
- **Q-N2: không.** Ghi chú hồ sơ khách, lưu ý sức khỏe (và ghi chú nhân sự) vẫn là một giá trị trên hồ sơ; nhật ký hiện lúc hồ sơ sửa lần cuối ("Hồ sơ sửa lúc …"), không lưu người ghi / lúc ghi riêng, không đổi schema.

### 10.6 Lý do hủy lịch / không đến (F21, test live 2026-11-08)
- Loại mới `appointment_reason` (bảng 10.2): chip "Hủy lịch" hoặc "Không đến" (theo `detail`), "Lịch hẹn dd/mm/yyyy" là ngày hẹn, link "Mở lịch hẹn" về lịch ngày đó. Khách chưa có hồ sơ: hiện tên khách đặt. Quy tắc đọc như ghi chú lịch hẹn (`appointment.manage`), mặc định từ chối. Migration **0029**: index một phần `appointment (tenant_id, closed_at) WHERE status IN ('cancelled','no_show')`, không cột mới.
- Test: `notes.rs` — chủ spa đọc 15 loại (16 từ §10.8), hai lý do (của khách có hồ sơ và của khách đặt), người đóng, ngày hẹn; `every_kind_keeps_its_own_read_rule` +1 dòng (bỏ `appointment.manage` → mất loại); lọc theo khách có `appointment_reason`. Vitest `Notes` "F21…".

- **Ghi chú về hồ sơ của chính mình (lead 2026-11-08, theo [ADR-0015](../../decisions/ADR-0015-own-profile-out-of-the-books.md)):** người không có `staff.manage` (quản lý) không thấy ghi chú chấm công, khen / phàn nàn, thưởng / phạt, đánh giá tháng **về hồ sơ gắn với tài khoản của mình** trong nhật ký (kể cả khi lọc `staff_id` = mình); xem qua `/me/*` như KTV. Chủ spa vẫn thấy mọi quản lý. Ghi chú quản lý viết về người khác vẫn hiện. Server: `org::profile::hidden_from` → điều kiện `$29` trong `GET /notes`. Test `notes.rs::a_manager_does_not_read_notes_about_their_own_profile`.

### 10.7 Lịch sử khách: một mục cho mỗi lượt / hóa đơn đã hủy (F16, 2026-11-08)
- Trước: lượt đã hủy hiện hai lần (dòng lượt có lý do + mục "Đã hủy" ở giờ hủy). Nay: **một** mục ở giờ của lượt / hóa đơn, `kind = visit_voided` / `purchase_voided`, mang cả nội dung (`visit` / `purchase`) và `void` (lý do, tiền hoàn, buổi trả lại). Chip "Đã hủy" / "Đã sửa". Bộ lọc "Lượt đã hủy" / "Hóa đơn đã hủy" vẫn đúng; "Lượt khách" chỉ còn lượt chưa hủy.


### 10.8 Ghi chú gói (F4, EM 2026-11-08, [raw](../../../raw/2026-11-08-em-decision-package-note.md), [front-desk §10.15](front-desk-and-packages.md))
- Loại thứ 16 **`package_note`**, chip "Ghi chú gói": `sale.note_ct` (mã hóa, migration 0030). Quy tắc đọc **`sale.read` + `customer.read` + `customer.health_notes.read`** (quy tắc ghi chú buổi đặt lên hóa đơn), mặc định từ chối; bỏ một quyền → mất loại (kể cả lọc `kind` hay tìm chữ). Chủ spa và quản lý mặc định đọc được (AC10.1: chủ spa **16** loại, quản lý **14**); KTV và người đã nghỉ không vào nhật ký.
- Một dòng mỗi hóa đơn có ghi chú: giờ = lúc lập hóa đơn, ngày = ngày hóa đơn, tác giả = người lập, khách = **người dùng gói** (người được mua cho khi "Mua cho người khác", không thì người trả), `detail` = tên các gói của hóa đơn (hiện cạnh tên khách), `visit_id` khi gói bán trong lượt. Link "Xem trong lịch sử khách" lọc đúng ngày + "Mua gói".
- Giải mã bằng đúng associated data của bên ghi (`sale.note` + tenant + hóa đơn, khóa của dòng = id hóa đơn); vào đường tìm chữ trong ghi chú mã hóa như ghi chú buổi (quét ≤ 500 / request, `scan_capped`). Index một phần mới của 0030: `sale (tenant_id, business_date) WHERE note_ct IS NOT NULL`.
- Test: `notes.rs` (chủ spa đọc 16 loại kèm tên gói và khách, quản lý không có quyền sức khỏe không thấy / không tìm ra, `every_kind_keeps_its_own_read_rule` +1 dòng), `package_notes.rs::a_cart_of_packages_keeps_one_note_per_package` (một dòng mỗi gói, khách = người dùng gói, tìm không dấu "huyet ap cao" chỉ ra `package_note`).
