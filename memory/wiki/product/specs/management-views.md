---
title: "Spec R1-2: Xem lại & theo dõi (management views)"
status: draft
updated: 2026-11-05
sources: [raw/2026-11-05-em-management-views-logo-ui.md]
related: [../paper-books-analysis.md, ../personas.md, front-desk-and-packages.md, ../../architecture/backend.md, ../../security/authz-matrix.md]
---

# Spec R1-2: Xem lại & theo dõi (management views)

**Status: draft** (chờ EM duyệt; backend đã build theo các mặc định dưới đây, câu hỏi mở ở §9). Phần backend (API) do agent backend làm; giao diện do agent UI làm trên contract `apps/api/openapi.json`.

## 1. Goal
App hiện chỉ để **nhập**. EM (2026-11-05): "muốn xem tổng hôm nay là bao nhiêu, xem một khách lịch sử dùng thế nào… không có management". Spec này thêm phần **xem lại**: tình hình một ngày, một khoảng ngày, sổ doanh thu, lịch sử từng khách, danh sách liệu trình cần theo dõi. Mọi con số được **tính ra** từ các sự kiện đã nhập (lượt, hóa đơn, thanh toán, hoàn tiền), không ai phải cộng tay. Phục vụ thước đo R1 #1 (bỏ sổ giấy: thay luôn phần "đọc lại" của sổ doanh thu và sổ liệu trình) và #3 (không quên khách: danh sách sắp hết hạn, lâu không quay lại). Persona chủ spa "nắm tình hình trong một màn hình" ([personas](../personas.md)).

Spec này cũng ghi **quyết định EM bỏ khái niệm "sổ cũ"** khỏi sản phẩm (§7).

## 2. Personas & quyền
- **Chủ spa:** xem mọi thứ, kể cả mọi con số tiền.
- **Quản lý (kiêm thu ngân):** xem mọi số liệu vận hành. Xem **tiền** là một permission riêng (`report.revenue.read`). **Mặc định: bật** cho quản lý, vì quản lý chính là thu ngân, đã cầm tiền và chốt quỹ mỗi ngày ([personas](../personas.md) #1); giấu tổng tiền với người đang đếm tiền không bảo vệ được gì. Chủ spa tắt được khi có màn quản lý role (spec #4). → **Q1** cho EM.
- **Kỹ thuật viên:** không xem các màn này ở R1 (R2 có "số khách của tôi").

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
Áp dụng cho mọi endpoint (như [spec R1-1](front-desk-and-packages.md) G1–G3): chưa đăng nhập → `401`; thiếu permission → `403 auth.forbidden`; ID của spa khác → `404`. Mọi endpoint chỉ **đọc** (không ghi, không audit). Danh sách có thể dài thì phân trang bằng cursor (`limit` 1–100, mặc định 20; cursor hỏng → `422 cursor invalid`).

**Quy ước tiền (áp dụng US1–US3):** tính theo **tiền thực thu** (cash basis) từ các bảng chỉ-thêm `payment`, `sale_line`, `refund`:
- **Thu** = các dòng `payment` (đã trừ giảm giá, vì R1 thu đúng số phải trả).
- **Hủy** = dòng đảo (`reversal`) của hóa đơn bị hủy, ghi vào **ngày hủy**. Hóa đơn hôm qua hủy hôm nay: hôm qua vẫn có khoản thu, hôm nay có khoản hủy âm. Giống sổ giấy: không tẩy, chỉ ghi thêm.
- **Hoàn tiền** = dòng `refund` (khi chủ spa hủy liệu trình), ghi vào ngày hoàn; luôn thuộc loại "liệu trình".
- **Thực thu (net)** = Thu − Hủy − Hoàn.
- Mỗi khoản chia theo **hình thức** (tiền mặt / chuyển khoản) và **loại** (liệu trình / dịch vụ 1 lần), đúng các cột của sổ doanh thu.
- **Ngày nghiệp vụ** theo múi giờ của spa (Asia/Ho_Chi_Minh): 23:59 thuộc hôm nay, 00:00 thuộc hôm sau.
- **Số đếm** (lượt, buổi, gói bán) tính theo **trạng thái hiện tại**: lượt/hóa đơn đã hủy không được đếm (số lượt hủy hiện riêng).
- Liệu trình chuyển từ sổ giấy không có hóa đơn, nên không bao giờ là doanh thu.

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
- AC2.4: `top_services` (10): theo số buổi + số lần làm lẻ; `one_off_revenue` chỉ khi có quyền tiền. `top_packages` (10): số gói bán (hóa đơn chưa hủy); `revenue` (giá sau giảm) chỉ khi có quyền tiền.
- AC2.5 (neg): `from > to` → `422 from after_to`; khoảng > 366 ngày → `422 to range_too_long`. Thiếu `report.read` → `403`.

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
  - `visit_voided` / `purchase_voided`: lúc hủy, lý do, tiền đảo, số buổi được cộng lại;
  - `package_cancelled`: chủ spa hủy gói, lý do, khoản hoàn (nếu có).
  Lượt đã hủy vẫn nằm trong timeline (có cờ `voided`). **Không bao giờ** có ghi chú sức khỏe / ghi chú buổi.
- AC4.2 Trang liệu trình (`GET /customer-packages/{id}/usage`): thông tin gói (tổng/còn lại, hạn, quà tặng, trạng thái) + **mỗi lần dùng** (STT theo thứ tự cũ → mới: ngày, giờ, dịch vụ, số buổi, còn lại sau, KTV, đã hủy hay chưa). Đây là một trang của sổ giấy.
- AC4.3 Tóm tắt (`GET /customers/{id}/summary`): lần đến đầu, lần đến gần nhất, số ngày từ lần gần nhất, số lượt (hoàn tất / đã hủy), số buổi đã dùng, tiền đã trả / đã đảo / đã hoàn / **thực chi**, số gói theo trạng thái, số buổi còn lại của các gói đang dùng.
- AC4.4 (neg): Khách của spa khác → `404`. Thiếu một trong `customer.read`, `visit.read`, `sale.read` → `403`. Cursor sửa tay → `422`.
- AC4.5: Gói chuyển từ sổ giấy hiện như mọi gói khác (không nhãn riêng); timeline không có sự kiện "mua" cho nó vì không có hóa đơn.

### US5: Tổng quan liệu trình & khách cần chăm sóc lại
*Là quản lý, tôi muốn xem các liệu trình đang chạy, sắp hết hạn, đã hết buổi, đã hủy; và danh sách khách còn buổi mà lâu không quay lại, để gọi điện mời.*
- AC5.1 `GET /packages`: lọc `status` (active / expired / exhausted / cancelled, trạng thái suy ra như R1-1), tìm `q` theo tên (không dấu) hoặc SĐT, mới nhất trước. `counts` đếm cả spa (theo `q`), không phụ thuộc trang.
- AC5.2 Sắp hết hạn: `expiring_within_days=N` (1–365, mặc định cửa sổ đếm 30) → chỉ gói **active** có hạn ≤ hôm nay + N, hạn gần nhất trước, kèm `days_to_expiry`, `last_used_on`.
- AC5.3 `GET /customers/inactive?days=N` (mặc định 7, EM 2026-11-05): khách **còn buổi** trên gói đang dùng mà lần đến gần nhất (hoặc ngày bắt đầu gói nếu chưa đến lần nào) cách hôm nay ≥ N ngày; vắng lâu nhất trước; kèm số buổi còn lại. `scope=all`: mọi khách đã từng đến/mua.
- AC5.4 (neg): `days` hoặc `expiring_within_days` ngoài 1–365 → `422`; `q` 1 ký tự → `422 too_short`. KTV → `403`.

### Bổ sung vòng 3 (EM 2026-11-05, [raw](../../../raw/2026-11-05-em-ui-review-round3.md))

**US5 — phạm vi "Lâu không quay lại" (thay AC5.3):**
- AC5.3': `GET /customers/inactive?days=N` (mặc định 7) liệt kê **mọi khách đã từng đến hoặc trả tiền** mà "lần gần nhất" cách hôm nay ≥ N ngày (biên **tính cả** ngày thứ N). "Lần gần nhất" = ngày muộn nhất trong: lượt hoàn tất gần nhất, hóa đơn chưa hủy gần nhất, ngày bắt đầu gói gần nhất (gói sổ giấy không có hóa đơn). Khách chỉ tạo hồ sơ, chưa đến chưa mua → không có trong danh sách. Lượt / hóa đơn đã hủy không tính.
- Gồm cả **khách lẻ** (đến theo buổi, không gói) và **khách liệu trình** dù gói còn buổi, đã hết buổi hay hết hạn. Mỗi dòng có `kind`: `package` (có ít nhất một gói không thuộc hóa đơn đã hủy) / `one_off`; lọc `kind=package|one_off|all`. `scope=with_sessions` vẫn còn (chỉ khách còn buổi trên gói đang dùng); **mặc định đổi thành `all`**.
- `q`: tên (không dấu, ≥ 2 ký tự) hoặc SĐT (≥ 3 số), ≤ 100 ký tự. Mỗi dòng có `gender`.

**US5b — Theo dõi gọi lại khách (follow-up):**
*Là quản lý, tôi muốn đánh dấu đã gọi / nhắn khách nào, ngày nào, kết quả ra sao và khi nào gọi lại, để biết hôm nay cần gọi ai thay vì chỉ nhìn ai vắng lâu.*
- AC5b.1 `POST /customers/{id}/follow-ups` (Idempotency-Key, quyền mới `customer.follow_up`: chủ spa + quản lý): `contacted_on` (mặc định hôm nay theo giờ spa; không ở tương lai; lùi tối đa 30 ngày), `channel` = `phone` | `zalo` | `in_person` | `other`, `outcome` = `will_come` (liên lạc được, sẽ đến) | `not_interested` | `no_answer` | `other`, `note` (≤ 500 ký tự, ghi chú liên lạc, **không** ghi sức khỏe), `next_follow_up_on` (≥ ngày liên lạc, ≤ 365 ngày sau). Người ghi = user của session (body không gửi được). Chỉ thêm, không sửa / xóa (ghi nhầm thì ghi lần mới). Audit `customer.follow_up` (ghi `has_note`, không chép nội dung ghi chú).
- AC5b.2 **Quy tắc mặc định:** `no_answer` mà không chọn ngày gọi lại → `next_follow_up_on` = ngày liên lạc + **3 ngày**. Kết quả khác không có mặc định. Ngày nhập tay luôn thắng.
- AC5b.3 `GET /customers/{id}/follow-ups`: mới nhất trước, tối đa 100 (`has_more`). Quyền `customer.read`.
- AC5b.4 Mỗi dòng của `/customers/inactive` có `last_follow_up {on, channel, outcome, by}` và `next_follow_up_on`, **chỉ tính các lần liên lạc trong đợt vắng hiện tại** (`contacted_on` ≥ "lần gần nhất"). Khách quay lại sau khi được gọi → đợt vắng mới, sạch.
- AC5b.5 Lọc `follow_up=`: `due` = chưa liên lạc trong đợt này **hoặc** `next_follow_up_on` ≤ hôm nay; `not_contacted` = chưa liên lạc; `scheduled` = hẹn gọi sau hôm nay; `all` (mặc định). Đã liên lạc mà không hẹn ngày (ví dụ "không muốn") chỉ hiện ở `all`.
- AC5b.6 Sắp xếp `sort=due|absent`: `due` = chưa liên lạc trước (vắng lâu trước), rồi `next_follow_up_on` gần nhất, cuối cùng là đã liên lạc không hẹn ngày; `absent` = vắng lâu nhất trước. Mặc định `due` khi `follow_up=due|scheduled`, ngược lại `absent`.
- AC5b.7 (neg): KTV → `403`; chưa đăng nhập → `401`; khách spa khác → `404`; thiếu Idempotency-Key → `400`; cùng key khác body → `422 idempotency.key_reuse_mismatch`.

**US4 — tìm trong hồ sơ khách:** `GET /customers/{id}/history?q=&kind=&from=&to=`: `q` (không dấu, 2–100 ký tự) khớp lượt theo tên dịch vụ, danh mục, tên gói, KTV; hóa đơn theo dòng gói và người chốt; hủy gói theo tên gói. `kind` một loại sự kiện; `from`/`to` theo ngày nghiệp vụ của sự kiện (`from > to` → `422`).

**US5 — gói không thời hạn (EM hỏi "gói unlimited hết hạn thì sao"):** đã đúng từ R1-1 D2: `expires_on` trống = **không bao giờ hết hạn**. Đã kiểm `app.package_status` (`expires_on < today` với NULL → không phải `expired`), cửa sổ `expiring_within_days` và `counts.expiring_soon` (`expires_on <= …` với NULL → không đếm), `PackageStatus::derive`. Thêm test: gói không hạn bắt đầu 3 năm trước vẫn `active`, không bao giờ vào "sắp hết hạn" / "hết hạn". `/packages?q=` nay tìm cả **tên gói** (không dấu) ngoài tên / SĐT khách.

**Trùng SĐT khi đang gõ:** `GET /customers/phone-check?phone=` (quyền `customer.read`) → `{valid, taken, existing_id?, full_name?}`, chuẩn hóa giống hệt khi tạo khách; số chưa đủ → `valid=false` (không lỗi, gọi được mỗi lần gõ). Chỉ trong spa của người gọi. Không lộ gì hơn `GET /customers?q=<số>`. Tạo khách vẫn trả `409 customer.phone_taken`.

**Danh mục dịch vụ:** mọi DTO hiện tên dịch vụ có thêm `category_name` (danh mục **hiện tại** trong bảng giá, không phải snapshot): dòng lượt (`VisitLineDto`, lượt hôm nay), dòng lượt trong lịch sử, item gói (`CustomerPackageItemDto`), trang dùng gói, top dịch vụ trong báo cáo. Giới tính khách (`customer_gender`) có ở lượt (`VisitDto`), dòng sổ doanh thu, dòng tổng quan gói; dòng khách vắng có `gender`.

**US2 — báo cáo linh hoạt:** `preset` = `last_7_days`, `last_30_days`, `this_month`, `last_month` (theo ngày), `this_year`, `last_12_months` (theo tháng), `last_5_years` (theo năm), tính theo "hôm nay" của spa; `granularity` thêm `year`; ghi đè được granularity mặc định của preset; `preset` cùng `from`/`to` → `422 preset conflicts_with_range`. Giới hạn: theo ngày/tuần ≤ 366 ngày, theo tháng/năm ≤ 1830 ngày (5 năm). Mỗi điểm `series` có `counts.customers`, `counts.visits` và `net_revenue` (phẳng, cho biểu đồ đường; chỉ khi có quyền tiền). Thêm `technicians[]` cho cả khoảng (như dashboard ngày).

### US6 (để sau, R1-3): Sổ thu chi
Sổ thu chi giấy ghi cả **chi** (chi phí vận hành, rút/nộp quỹ) và **tồn quỹ** tiền mặt / tài khoản. Phần **thu** đã có (US3). Phần **chi** là loại dữ liệu mới (phiếu chi: ngày, diễn giải, số tiền, hình thức, người chi, chứng từ) cần spec riêng: quyền ghi/sửa, chốt quỹ cuối ngày, tồn đầu kỳ, đối soát. **Không làm ở R1-2.**

## 4. UX notes (cho agent UI)
- **Trang chủ = Dashboard hôm nay**: hàng thẻ số (khách, lượt, buổi, gói bán, thực thu) có mũi tên so với hôm qua / tuần trước; bảng tiền 2×2 (loại × hình thức) + hủy + hoàn; danh sách KTV. Chọn ngày bằng date picker; nút "Hôm qua / Hôm nay".
- **Báo cáo**: tab Ngày / Tuần / Tháng / Tùy chọn, biểu đồ cột doanh thu + đường lượt khách, top dịch vụ / gói.
- **Sổ doanh thu**: bảng đúng cột sổ giấy, mỗi ngày một nhóm có dòng cộng; dòng hủy/hoàn màu đỏ, số âm; dòng gốc đã hủy gạch ngang.
- **Hồ sơ khách**: tab Lịch sử (timeline), tab Liệu trình (mỗi gói một thẻ, bấm vào là trang sổ liệu trình), khối tóm tắt trên cùng.
- **Theo dõi**: tab Liệu trình (lọc trạng thái, sắp hết hạn), tab "Lâu không quay lại" (gọi điện / Zalo bằng một chạm `tel:`).
- Mọi màn có trạng thái loading / rỗng / lỗi / có dữ liệu. Tiền hiển thị `1.500.000đ`. Ẩn hẳn khối tiền khi response không có trường tiền (không hiện "0đ").
- Bỏ khỏi UI: mục menu và màn `/nhap-so-cu`, nhãn "Nhập từ sổ cũ", ô ở trang chủ (§7).

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
- **API:** `CustomerPackageDto` bỏ trường `origin`. Gói chuyển từ sổ giấy giống hệt gói khác; chỉ khác là `sale_id = null` (không có hóa đơn để hủy). Bảng vẫn giữ cột `origin` để báo cáo tiền không bao giờ tính nó là doanh thu.
- **Nhập sổ giấy chỉ dành cho người vận hành.** Hai lựa chọn: (a) lệnh CLI `app import-paper-book --tenant <slug> --file <csv>`; (b) giữ `POST /opening-balances` chỉ cho chủ spa.
  **Khuyến nghị và đã làm: (a) CLI.** Lý do: việc này là chuyển dữ liệu một lần của team, không phải thao tác của spa; CLI không có bề mặt HTTP để tấn công, chạy trên máy vận hành, đọc file CSV mà team đã soát lại, có `--dry-run`, chạy lại không nhập trùng. HTTP route vẫn còn **tạm thời** và chỉ chủ spa có quyền (migration 0013 bỏ quyền của quản lý), vì màn `/nhap-so-cu` của web vẫn gọi nó cho tới khi agent UI xóa màn đó; sau đó xóa route (ticket theo dõi trong plan).
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

  Lệnh đi qua đúng use case `packages::opening_balance::import` với quyền của chủ spa (validate, audit, không tạo hóa đơn/thanh toán). Gói đã có (cùng khách, cùng tên, cùng ngày bắt đầu) được bỏ qua, nên chạy lại an toàn.
- **Web:** bỏ menu/màn `/nhap-so-cu`, nhãn "Nhập từ sổ cũ", ô trang chủ (agent UI). `PackageActions` dùng `sale_id != null` thay cho `origin === "sale"`.

## 8. Out of scope
- Sổ thu chi / phiếu chi / chốt quỹ (US6, R1-3).
- Xuất Excel/PDF; gửi báo cáo qua Zalo.
- Hoa hồng, lương (R2). Báo cáo theo chi nhánh (chỉ 1 chi nhánh ở R1).
- Nhắc tự động (spec #3); US5 chỉ là danh sách để xem.

## 9. Open questions (EM, trước khi chốt R1-2)
- **Đã chốt (EM 2026-11-05, [raw](../../../raw/2026-11-05-em-management-decisions-ui-layout.md)):**
  - Q1: quản lý **được** xem tiền (`report.revenue.read`); chỉnh sau được.
  - Q2: "lâu không quay lại" mặc định **7 ngày**, kèm màn hình riêng xem cả danh sách khách quá hạn.
  - Q3: tiền thực thu, khoản hủy ghi vào **ngày hủy**.
  - Q4: tạm thời **không giới hạn** số ngày quản lý được xem.
