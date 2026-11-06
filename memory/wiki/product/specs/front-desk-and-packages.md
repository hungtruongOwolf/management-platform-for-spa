---
title: "Spec R1-1: Màn hình quầy & liệu trình khách hàng"
status: approved
updated: 2026-11-06
sources: [raw/2026-11-04-idea-internal-platform.md, raw/2026-11-06-em-upsell-and-convert-to-package.md, raw/2026-11-06-em-answers-round6-upsell.md]
related: [../vision.md, ../personas.md, ../glossary.md, ../paper-books-analysis.md]
---

# Spec R1-1: Màn hình quầy & liệu trình khách hàng

**Status: approved** (§1–§9; **§10 Vòng 6 approved 2026-11-06** theo câu trả lời Q-U1…Q-U9 của EM, §10.8) (EM 2026-11-04; rev. 2026-11-04: bỏ công nợ và giờ ra). Các mặc định **[D1–D10]** đã được EM xác nhận. Bảng giá thật: [service-catalog](../service-catalog.md).

## 1. Goal
Thay **sổ dịch vụ**, **sổ theo dõi liệu trình khách hàng** và phần bán hàng của **sổ doanh thu** bằng một màn hình quầy. Quản lý ghi một lượt khách kèm thanh toán trong **< 30 giây**, không ghi lại ở nơi khác. Phục vụ thước đo thành công R1 #1 (bỏ sổ giấy) và #3 (không quên khách hết liệu trình, vì dữ liệu buổi còn lại được tạo ra ở đây). Spec này cũng **ghi nhận người làm và người chốt** để R2 tính hoa hồng.

## 2. Personas & roles
- **Quản lý (kiêm thu ngân):** thực hiện mọi thao tác trong spec.
- **Chủ spa:** mọi quyền của Quản lý, cộng thêm hủy/sửa sau khi đã chốt ngày, hoàn tiền, cấu hình bảng giá.
- **Kỹ thuật viên:** không thao tác trong spec này. Chỉ được gắn tên vào buổi làm và người chốt gói.
- Quyền được kiểm tra bằng permission (xem [principles](../../architecture/principles.md) #2). Bảng dưới là **mặc định ban đầu**; chủ spa đổi được ở spec #4.

| Permission | Quản lý | Chủ spa |
|---|---|---|
| `catalog.read` | ✓ | ✓ |
| `catalog.manage` | — | ✓ |
| `customer.read`, `customer.write` | ✓ | ✓ |
| `customer.health_notes.read` / `.write` | ✓ | ✓ |
| `package.sell`, `visit.record`, `payment.collect` | ✓ | ✓ |
| `sale.discount` | ✓ (≤ ngưỡng **[D10]**) | ✓ |
| `record.void_same_day` | ✓ | ✓ |
| `record.void_closed_day`, `package.cancel_refund`, `package.extend_expiry` | — | ✓ |
| `opening_balance.import` | ✓ | ✓ |

## 3. User stories & acceptance criteria
Áp dụng cho **mọi** endpoint trong spec:
- **G1:** Chưa đăng nhập → `401`, không lộ dữ liệu.
- **G2:** Đã đăng nhập nhưng thiếu permission → `403`.
- **G3:** Truy cập tài nguyên của spa (tenant) khác bằng ID hợp lệ → `404`, như thể tài nguyên không tồn tại.
- **G4:** Mọi thao tác tạo/sửa liên quan đến tiền và buổi liệu trình đều nhận `Idempotency-Key`. Gửi lại cùng key → trả lại đúng kết quả cũ, không tạo bản ghi thứ hai.
- **G5:** Mọi thao tác ghi đều để lại audit log: ai, lúc nào, thay đổi gì.
- **G6:** Tiền là số nguyên VND ≥ 0. Số âm, số lẻ hoặc vượt giới hạn → `422`.

### US1: Bảng giá dịch vụ & gói liệu trình
*Là chủ spa, tôi muốn quản lý bảng giá dịch vụ và các gói liệu trình, để quầy chọn nhanh và giá luôn đúng.*
- **Nhóm dịch vụ (ServiceCategory):** tên, thứ tự hiển thị (ví dụ: Gội đầu dưỡng sinh, Chăm sóc cổ vai gáy, …, Dịch vụ bổ sung).
- **Dịch vụ:** nhóm; tên; mô tả / phần kèm theo; thời lượng (phút, **có thể để trống = "theo tư vấn"**); cờ **★ khuyên dùng**; trạng thái đang bán/ngừng bán.
- **Giá dịch vụ theo phương án (ServicePrice):** một dịch vụ có **một giá chung** hoặc **giá theo giới tính** (Nữ / Nam). Một phương án có thể **không áp dụng** (ví dụ Ôn ấm tử cung không có giá Nam). Cấu trúc này dùng chung cho các loại phương án khác về sau (ví dụ theo thời lượng).
- **Gói liệu trình (PackagePlan):** tên; một hoặc nhiều dòng `dịch vụ × số buổi` **[D1]**; số **buổi tặng** **[D3]**; giá gói; **hạn dùng tính theo tháng, có thể để trống** **[D2]**; ghi chú quà tặng hiện vật (tự do); trạng thái.
- AC1.1: Given chủ spa, when tạo gói "Gội đầu dưỡng sinh 10 buổi + tặng 2" giá 1.500.000đ, hạn 6 tháng → gói xuất hiện trong danh sách chọn ở quầy.
- AC1.2: Given gói đã bán cho khách, when chủ spa đổi giá hoặc số buổi của gói → các liệu trình **đã bán giữ nguyên** giá và số buổi lúc bán (snapshot).
- AC1.3: Given dịch vụ/gói **ngừng bán** → không còn trong danh sách bán mới, nhưng liệu trình đã bán vẫn dùng được.
- AC1.4 (neg): Given Quản lý (không có `catalog.manage`), when sửa giá → `403`.
- AC1.5 (neg): Gói không có dòng dịch vụ nào, số buổi ≤ 0, hoặc trùng tên với gói đang bán → `422`.
- AC1.6 (neg): Xóa dịch vụ đã từng được dùng → không cho xóa, chỉ cho ngừng bán.
- AC1.7: Given dịch vụ "Thư Giãn" giá Nữ 79.000đ / Nam 99.000đ → bảng giá hiển thị cả hai giá; dịch vụ giá chung chỉ hiển thị một giá.
- AC1.8 (neg): Dịch vụ không có giá nào áp dụng, hoặc giá theo giới tính mà thiếu cả Nữ lẫn Nam → `422`.
- AC1.9: Seed ban đầu nạp đủ 35 dịch vụ / 10 nhóm từ [service-catalog](../service-catalog.md).

### US2: Hồ sơ khách hàng
*Là quản lý, tôi muốn tìm và tạo hồ sơ khách trong vài giây, thấy ngay liệu trình còn lại, công nợ và lưu ý sức khỏe.*
- **Trường:** họ tên (bắt buộc); **SĐT (bắt buộc, duy nhất trong một spa)** **[D6]**; giới tính; năm sinh; địa chỉ; nguồn biết đến spa; ghi chú chung; **ghi chú sức khỏe** (chống chỉ định, tình trạng).
- **Màn hình hồ sơ:** liệu trình đang có (còn X/Y buổi, hạn dùng), lịch sử buổi làm (mới nhất trước), ghi chú sức khỏe hiển thị nổi bật.
- AC2.1: Given gõ ≥ 3 chữ số của SĐT hoặc ≥ 2 ký tự tên (có dấu hoặc không dấu, ví dụ "hà" tìm được "Hà") → gợi ý khách trong < 300 ms.
- AC2.2: Given khách mới, when nhập tên + SĐT và lưu → hồ sơ được tạo và chọn luôn cho lượt đang ghi.
- AC2.3: SĐT được chuẩn hóa trước khi lưu và so trùng: `+84 905 123 456`, `0905.123.456` và `0905123456` là cùng một số.
- AC2.4 (neg): SĐT đã có trong spa → `409` kèm link tới hồ sơ hiện có, không tạo bản trùng.
- AC2.5 (neg): SĐT sai định dạng → `422`.
- AC2.6 (neg): Người không có `customer.health_notes.read` → API **không trả về** trường ghi chú sức khỏe. Ẩn trên giao diện là chưa đủ.
- AC2.7: Tên được chuẩn hóa Unicode NFC, nên cùng một tên gõ ở hai bộ gõ khác nhau vẫn tìm ra.

### US3: Bán liệu trình mới
*Là quản lý, tôi muốn bán gói liệu trình cho khách, ghi quà tặng, người tư vấn chốt và số tiền khách trả, để liệu trình được theo dõi tự động.*
- **Hóa đơn (Sale)** gồm dòng gói liệu trình. Chọn **người tư vấn chốt** (một nhân viên, bắt buộc **[D9]**). Giảm giá tùy chọn **[D10]**. **Thu đủ ngay khi bán**: 1 hoặc 2 hình thức (TM + CK), tổng bằng đúng số phải trả.
- Tạo **CustomerPackage**: số buổi = buổi mua + buổi tặng; ngày bắt đầu = ngày bán; hạn dùng = ngày bán + số tháng của gói (nếu có); **giá trị mỗi buổi = giá sau giảm ÷ số buổi mua**, buổi tặng có giá trị 0 **[D4]**.
- AC3.1: Given gói 10 buổi + tặng 2 giá 1.500.000đ, khách trả đủ 1.500.000đ → liệu trình "còn 12/12 buổi"; giá trị mỗi buổi mua = 150.000đ.
- AC3.2: Given trả kết hợp (500.000đ TM + 1.000.000đ CK) → hai Payment, tổng 1.500.000đ.
- AC3.3 (neg): Tổng thanh toán **khác** số phải trả (thiếu hoặc thừa) → `422 payment.amount_mismatch`.
- AC3.4: Tiền thừa khách đưa không ghi vào hệ thống; quầy tự trả lại ngoài đời.
- AC3.5 (neg): Giảm giá không ghi lý do, hoặc vượt ngưỡng của Quản lý → `422` / `403`.
- AC3.6 (neg): Người chốt không phải nhân viên đang làm việc của spa → `422`.
- AC3.7 (neg): Bấm "Lưu" hai lần hoặc mạng chập chờn rồi gửi lại → chỉ một hóa đơn và một liệu trình được tạo (G4).

### US4: Ghi lượt khách (buổi làm)
*Là quản lý, tôi muốn ghi một lượt khách gồm giờ vào, các dịch vụ (trừ buổi liệu trình hoặc dịch vụ lẻ), KTV làm và thanh toán, trong < 30 giây.*
- **Lượt (Visit):** khách, giờ vào (mặc định = bây giờ), ghi chú buổi (gồm phản ứng sau buổi; là dữ liệu sức khỏe, cùng quyền với AC2.6).
- **Mỗi dòng dịch vụ:** dịch vụ; **nguồn** = `trừ liệu trình <CustomerPackage>` hoặc `dịch vụ lẻ` (giá niêm yết, sửa được nếu có quyền giảm giá); **KTV thực hiện (bắt buộc, 1 người/dòng)**; số lượng buổi (mặc định 1).
- Dòng **dịch vụ lẻ / dùng thêm** sinh tiền phải trả trong lượt; **thu đủ ngay** (TM, CK hoặc kết hợp).
- AC4.1: Given khách có liệu trình gội đầu còn 5/12, when ghi lượt trừ 1 buổi, KTV Lan → liệu trình còn **4/12**, lịch sử ghi ngày giờ, KTV Lan, giá trị buổi.
- AC4.2: Given cùng lượt thêm "massage chân" dịch vụ lẻ 100.000đ trả CK → tạo Sale 100.000đ + Payment CK, gắn với lượt.
- AC4.2b: Given khách giới tính Nam chọn dịch vụ lẻ "Đại Thông Lạc" → giá mặc định **399.000đ** (giá Nam). Khách chưa ghi giới tính → quầy bắt chọn Nữ/Nam cho dòng đó.
- AC4.2c (neg): Chọn phương án giá không áp dụng (Ôn ấm tử cung cho khách Nam) → `422`, quầy không cho chọn.
- AC4.3: Given khách có nhiều liệu trình chứa cùng dịch vụ → mặc định chọn liệu trình **hết hạn sớm nhất**, rồi đến mua sớm nhất; quản lý đổi được.
- AC4.4: Lượt khách hoàn tất phải tăng "số khách" của KTV trong ngày (dữ liệu cho chấm công R2): một lượt có KTV đó = 1 khách.
- AC4.5 (neg): Liệu trình còn 0 buổi → không cho trừ (`409`), quầy gợi ý bán gói mới.
- AC4.6 (neg): Liệu trình **đã hết hạn** → mặc định chặn. Quản lý có thể **cho dùng tiếp kèm lý do**, được ghi audit **[D2]**.
- AC4.7 (neg, đồng thời): Liệu trình còn 1 buổi, hai thiết bị cùng trừ lúc đó → **đúng một** request thành công, request còn lại nhận `409`. Số buổi không bao giờ âm.
- AC4.8 (neg): Dòng dịch vụ thiếu KTV, KTV đã nghỉ việc, hoặc liệu trình của khách khác → `422`.
- AC4.10 (neg): Liệu trình đã bị hủy → không cho trừ (`409`).

### US5: ~~Thanh toán & công nợ~~ (hoãn, EM 2026-11-04)
R1 coi mọi giao dịch là **thu đủ ngay**: không có ghi nợ, thu nợ hay danh sách công nợ. Schema giữ `sale.amount_paid` để bật lại tính năng công nợ sau này mà không phải migrate lớn. Payment vẫn append-only; sửa sai qua US6.

### US6: Hủy / sửa sai
*Là quản lý, tôi muốn hủy lượt hoặc hóa đơn ghi nhầm, có lý do, để số liệu đúng mà vẫn truy được dấu vết.*
- **Hủy lượt:** buổi đã trừ được **cộng trả lại**; các Sale/Payment của lượt được ghi bút toán đảo (reversal), không xóa.
- **Hủy hóa đơn bán gói:** chỉ khi liệu trình **chưa dùng buổi nào**; liệu trình chuyển sang `cancelled`.
- AC6.1: Given lượt trừ 1 buổi ghi nhầm hôm nay, when quản lý hủy với lý do → liệu trình được cộng lại 1 buổi; lượt vẫn hiển thị với trạng thái "đã hủy" và lý do.
- AC6.2 (neg): Hủy không có lý do → `422`.
- AC6.3 (neg): Quản lý hủy bản ghi của **ngày đã chốt quỹ** → `403`; chủ spa thì được (`record.void_closed_day`). Ngày "đã chốt" do spec #2 định nghĩa; cho tới khi spec #2 xong, coi các ngày trước hôm nay là đã chốt.
- AC6.4 (neg): Hủy hóa đơn gói đã dùng ≥ 1 buổi → `409`; phải dùng "hủy liệu trình kèm hoàn tiền" (US7).

### US6b: Sửa lượt (EM 2026-11-06, [raw](../../../raw/2026-11-06-em-edit-visit.md))
*Là quản lý, khi ghi lượt bị sai trong ngày (sai KTV, sai giờ, sai dịch vụ, sai tiền, nhầm khách), tôi muốn sửa ngay trên lượt đó thay vì hủy rồi ghi lại bằng tay.*

**Lead's decision — EM to review.** Một nút "Sửa lượt", hai cách lưu do hệ thống tự chọn:
- **Sửa trực tiếp (không đụng tiền / buổi):** KTV từng dòng, giờ làm (trong cùng ngày nghiệp vụ), ghi chú buổi. Lượt giữ nguyên, `version` tăng, audit `visit.update`.
- **Sửa có hủy-ghi lại (đụng tiền hoặc buổi):** dịch vụ / dòng, nguồn (liệu trình ↔ dịch vụ lẻ), giảm giá (mức và lý do), hình thức / số tiền thanh toán, khách. Một thao tác nguyên tử trên server: lượt cũ bị hủy đúng như US6 (lý do "Sửa lượt: …", buổi cộng lại, tiền đảo bút toán) **và** lượt mới được ghi đúng như US4, nối với lượt cũ (`corrects_visit_id`). Một transaction, một `Idempotency-Key`; không có logic tiền riêng.
- Quầy mở giỏ "Lượt khách đang ghi" đã điền sẵn lượt cũ, chỉ đổi chỗ sai; bắt buộc lý do ngắn khi đụng tiền/buổi; trước khi lưu thấy chênh lệch: "Thu thêm 50.000đ" / "Hoàn 50.000đ" / "Không đổi tiền".
- Lượt cũ vẫn trong lịch sử, trạng thái đã hủy, kèm "Đã sửa → lượt mới"; lượt mới bình thường. Lịch hẹn đã hoàn tất bởi lượt cũ chuyển sang lượt mới.

Quyền và ngày (theo `kernel::corrections`, giống AC6.3):
| | Sửa trực tiếp | Sửa có hủy-ghi lại |
|---|---|---|
| Quyền gốc | `visit.record` (+ `customer.health_notes.write` khi sửa ghi chú) | `visit.record` + `record.void_same_day` (+ quyền của lượt mới như US4: `payment.collect`, `sale.discount*`, …) |
| Hôm nay | Quản lý, Chủ spa | Quản lý, Chủ spa |
| Ngày trước hôm nay | Chủ spa (`record.void_closed_day`); Quản lý → `403 day.locked` | như cột bên |
| KTV | `403` | `403` |

- AC6b.1: Given lượt hôm nay KTV Lan, when quản lý đổi sang KTV Mai (If-Match đúng) → `200`, lượt giữ id, `version` + 1, tiền và buổi không đổi, audit `visit.update` ghi KTV cũ → mới.
- AC6b.2: Given lượt hôm nay giờ làm 09:00, when đổi giờ làm 10:30 cùng ngày → `200`; giờ sang ngày khác → `422 check_in_at not_visit_day`; giờ trong tương lai quá 5 phút → `422 visit.check_in_in_future`.
- AC6b.3 (neg): Sửa với `version` cũ (máy khác vừa sửa) → `412 conflict.stale_version`, không đổi gì.
- AC6b.4: Given lượt dịch vụ lẻ 100.000đ TM, when sửa thành dịch vụ 150.000đ (lý do "chọn nhầm dịch vụ") → lượt cũ `voided` (lý do "Sửa lượt: chọn nhầm dịch vụ"), payment cũ có reversal −100.000đ, lượt mới 150.000đ TM nối `corrects_visit_id`; phản hồi `difference = +50.000`. Báo cáo ngày: thu 250.000, đảo 100.000, **thực thu 150.000**.
- AC6b.5: Sửa giảm tiền → `difference < 0` ("Hoàn …"); sửa chỉ đổi hình thức thanh toán → `difference = 0` ("Không đổi tiền") nhưng vẫn qua hủy-ghi lại (payment append-only).
- AC6b.6: Given lượt trừ 1 buổi liệu trình (còn 11/12), when sửa thành dịch vụ lẻ → liệu trình về 12/12, lượt mới có tiền phải thu; ngược lại dịch vụ lẻ → trừ buổi: tiền cũ đảo, buổi bị trừ.
- AC6b.7 (neg, đồng thời): Hai máy cùng sửa một lượt → **đúng một** thành công; máy còn lại `409 visit.corrected` (hoặc `412` nếu tới sau khi lượt đã đổi `version`). Buổi chỉ cộng/trừ đúng một lần.
- AC6b.8: Gửi lại cùng `Idempotency-Key` + cùng body → trả lại đúng kết quả cũ, không tạo lượt thứ ba (G4).
- AC6b.9 (neg): Sửa lượt đã hủy → `409 visit.voided`; sửa lượt đã được sửa → `409 visit.corrected` (giao diện chỉ tới lượt mới nhất).
- AC6b.10 (neg): Quản lý sửa lượt ngày đã qua → `403 day.locked` (cả sửa trực tiếp lẫn hủy-ghi lại); chủ spa được, lượt mới giữ đúng ngày nghiệp vụ của lượt cũ.
- AC6b.11 (neg): KTV gọi sửa → `403`; id lượt của spa khác → `404`; thiếu lý do khi hủy-ghi lại → `422 reason required`; thiếu `If-Match` → `400`.
- AC6b.12: Audit: sửa trực tiếp → `visit.update`; hủy-ghi lại → `visit.void` + `sale.void` (nếu có tiền) + `visit.create` (lượt mới, có `corrects_visit_id`) + `visit.correct` (lượt cũ → mới, chênh lệch, lý do).
- AC6b.13: Lượt ngày đã qua do chủ spa sửa: bút toán đảo ghi vào **ngày sửa** (như mọi lần hủy, US6), lượt mới và tiền mới ghi vào **ngày của lượt cũ** (như nhập bù). Xem câu hỏi Q-E1.
- Câu hỏi cho EM (mặc định đang dùng): **Q-E1** sửa lượt ngày đã qua: đảo tiền ghi ngày sửa, tiền mới ghi ngày cũ (mặc định, ghép từ hai quy tắc đã có) hay cả hai cùng ngày cũ? **Q-E2** đổi riêng lý do giảm giá (không đổi mức) cũng đi qua hủy-ghi lại (mặc định; sổ doanh thu đọc lý do từ hóa đơn, hóa đơn chỉ thêm) — có cần sửa trực tiếp không? ~~**Q-E3**~~ (lead chốt 2026-11-06, thống nhất với spec §10 "chuyển sang gói"): lượt đã được sửa **không** tính vào "lượt hủy" (`voided_visits`, `voided_visit_count`); báo cáo ngày có `corrected_visits` hiện "(n đã sửa)".
- AC6b.14: Báo cáo ngày sau một lần sửa: `visits` = 1, `voided_visits` = 0, `corrected_visits` = 1; tóm tắt khách `voided_visit_count` = 0; lịch không hiện lượt cũ như lượt vãng lai.

### US7: Vòng đời liệu trình
*Là chủ spa, tôi muốn gia hạn hoặc hủy liệu trình kèm hoàn tiền, để xử lý các trường hợp đặc biệt mà không phải sửa dữ liệu bằng tay.*
- Trạng thái: `active` → `exhausted` (hết buổi) / `expired` (quá hạn) / `cancelled`.
- AC7.1: Given liệu trình hết hạn còn 3 buổi, when chủ spa gia hạn thêm 2 tháng với lý do → trạng thái `active`, hạn mới được ghi audit.
- AC7.2: Given hủy liệu trình kèm hoàn tiền 300.000đ TM → tạo Refund (khoản chi gắn với khách và liệu trình), số buổi còn lại về 0, trạng thái `cancelled`.
- AC7.3 (neg): Quản lý gia hạn hoặc hoàn tiền → `403`.
- AC7.4 (neg): Hoàn nhiều hơn số khách đã trả cho liệu trình đó → `422`.

### US8: Nhập liệu trình đang dùng dở (chuyển đổi từ sổ giấy)
*Là quản lý, tôi muốn nhập các liệu trình khách đang dùng dở từ sổ giấy trong 1–2 phút mỗi khách, để bắt đầu dùng platform mà không mất buổi nào khách đã trả.*
- **Nhập:** khách (tạo mới hoặc chọn), gói (chọn từ bảng giá hoặc nhập tay tên + dịch vụ), **số buổi còn lại**, tổng số buổi gốc, **giá đã bán**, ngày đăng ký gốc, hạn dùng (nếu có), quà tặng.
- AC8.1: Given liệu trình 10 buổi đã dùng 6, giá 1.500.000đ, còn nợ 0 → tạo CustomerPackage "còn 4/10", đánh dấu `opening_balance`, **không** tạo doanh thu hay Payment trong ngày nhập.
- AC8.3: Báo cáo doanh thu và sổ quỹ **không** tính các khoản opening balance là doanh thu mới.
- AC8.4 (neg): Số buổi còn lại > tổng số buổi → `422`.
- ~~AC8.5: Bản ghi opening balance hiển thị nhãn "Nhập từ sổ cũ" ở mọi nơi.~~ **Bỏ** (EM 2026-11-05, [management-views §7](management-views.md)): gói chuyển từ sổ giấy trông như mọi gói khác; nhập sổ giấy là migration của team (`app import-paper-book`), quản lý không còn `opening_balance.import`.
- **Quyết định EM 2026-11-05 (thay bản M2 của PR #23; nguồn `raw/2026-11-05-em-opening-balance-same-day.md` và `raw/2026-11-05-em-simplify-owner-and-opening-balance.md`):** "nếu trong ngày thì cứ nhập thôi còn nếu quá ngày thì không được chỉnh sửa hay nhập thông tin của ngày đó hoặc các ngày trước nữa … cái đấy tự động"; "nếu account là chủ spa thì cứ allow hết"; trần hoàn tiền riêng cho sổ cũ "tính sau".
  - AC8.6: Không có công tắc "mở nhập sổ cũ". Người có `opening_balance.import` (chủ spa, quản lý) nhập được bất cứ lúc nào; gói nhập luôn ghi vào **ngày kinh doanh hôm nay** (múi giờ spa, Asia/Ho_Chi_Minh).
  - AC8.7: Sửa / hủy bản ghi của một ngày đã qua theo quy tắc ngày đã chốt (AC6.3): qua 0 giờ là tự khóa với quản lý (`403 day.locked`); chủ spa làm được và **không** phải nhập lại mật khẩu.
  - AC8.8: Hoàn tiền khi hủy gói sổ cũ ≤ giá ghi trong sổ (`sold_price`), giống gói bán (AC7.4: vượt → `422 refund.exceeds_paid`); chỉ chủ spa hủy + hoàn (AC7.3). Trần hoàn riêng cho gói sổ cũ do quản lý nhập: **để sau**.

## 4. UX notes
- **Màn hình quầy** (tablet/laptop ưu tiên, dùng được trên điện thoại):
  1. Ô tìm khách luôn ở trên cùng, focus sẵn. Gõ SĐT hoặc tên → chọn khách hoặc "+ Khách mới".
  2. Thẻ khách: tên, SĐT, **lưu ý sức khỏe (nổi bật màu cảnh báo)**, các liệu trình còn buổi (bấm để trừ).
  3. Giỏ của lượt: các dòng dịch vụ (nguồn, KTV, giá), tổng phải trả.
  4. Thanh toán: nút nhanh "Tiền mặt" / "Chuyển khoản" (hoặc tách 2 hình thức); luôn thu đủ.
  5. Nút "Lưu", phím Enter dùng được; sau khi lưu quay về ô tìm khách.
- **Luồng nhanh nhất** (khách quen, trừ 1 buổi, không phát sinh tiền): tìm → bấm liệu trình → chọn KTV → Lưu. **≤ 4 thao tác.**
- Trạng thái: đang tải (skeleton); trống ("Chưa có khách nào hôm nay"); lỗi (thông báo tiếng Việt dễ hiểu, giữ nguyên dữ liệu đã nhập để gửi lại); thành công (toast + số buổi còn lại).
- KTV hay được chọn được đưa lên đầu danh sách.
- **Danh sách lượt hôm nay:** xem nhanh, bấm vào để xem chi tiết hoặc hủy.
- Prototype bấm thử được cho quản lý dùng thử **trước khi code** (rủi ro #1 trong vision), làm ở `/frontend-design`.

## 5. Data
| Entity | Trường chính | Ghi chú |
|---|---|---|
| `ServiceCategory` | name, sort_order | |
| `Service` + `ServicePrice` | category_id, name, description, duration_min?, recommended, active; price: variant (`all` / `female` / `male`), amount | giá theo phương án |
| `PackagePlan` + `PackagePlanItem` | name, price, validity_months?, gift_note; item: service_id, sessions, **bonus_sessions** (buổi tặng theo từng dòng để combo rõ ràng) | snapshot khi bán; chi tiết ở [data](../../architecture/data.md) |
| `Customer` | full_name (NFC), phone_e164 (unique per tenant), gender?, birth_year?, address?, source?, note, **health_note (nhạy cảm)** | |
| `CustomerPackage` + `CustomerPackageItem` | plan snapshot, sold_price, per_session_value, sessions_total, sessions_remaining, starts_on, expires_on?, status, origin (`sale` / `opening_balance`), closer_staff_id | `version` cho optimistic lock; `CHECK sessions_remaining >= 0` |
| `Visit` + `VisitLine` | customer, check_in_at, check_out_at?, note (nhạy cảm), status; line: service, source (`package` / `one_off`), customer_package_item_id?, staff_id, qty, unit_value | |
| `Sale` + `SaleLine` | customer, lines (type: `package` / `service`; đã chừa chỗ cho `product`), discount, discount_reason, total, status | |
| `Payment` | sale_id, method (`cash` / `bank_transfer`), amount, paid_at, reference?, reversal_of? | append-only |
| `Refund` | customer_package_id, amount, method, reason | |
| `AuditLog` | actor, action, entity, before/after, at | |

- Mọi bảng có `tenant_id`, `branch_id`, `created_at`, `created_by`.
- Tiền là `BIGINT` VND. Thời gian là `timestamptz`; "ngày nghiệp vụ" theo múi giờ Asia/Ho_Chi_Minh.
- **Dữ liệu nhạy cảm:** `health_note`, `Visit.note`, chỉ trả về khi có quyền (AC2.6).
- **Lưu trữ:** giữ vô thời hạn khi spa còn hoạt động. Xóa khách = ẩn danh hóa (tên, SĐT, ghi chú) bởi chủ spa, giữ lại số liệu tài chính.

## 6. Non-functional
- **Hiệu năng:** tìm khách < 300 ms (p95); lưu lượt < 500 ms (p95); màn hình quầy tải lần đầu < 2,5 s trên 4G (LCP).
- **Bảo mật:** theo `secure`. Tenant isolation (G3), permission ở service layer, idempotency (G4), audit (G5), không có API xóa tiền/buổi. Concurrency theo `backend-design/data-integrity.md`: optimistic lock + `CHECK` constraint.
- **Accessibility:** WCAG 2.2 AA; dùng được hoàn toàn bằng bàn phím trên laptop; vùng chạm ≥ 44 px trên tablet/điện thoại.
- **i18n:** tiếng Việt; định dạng tiền `1.500.000đ`, ngày `dd/mm/yyyy`.
- **Thiết bị:** desktop, laptop, tablet, điện thoại (responsive); Chrome, Safari, Edge bản mới.

## 7. Out of scope (spec này)
Sổ quỹ & chốt quỹ (spec #2; spec này chỉ phát sinh Payment/Refund cho sổ quỹ dùng) · nhắc nhở & báo cáo (spec #3) · màn hình phân quyền & tài khoản (spec #4) · lịch hẹn (đã kéo lên vòng 4: [spec R1-3 appointments](appointments.md); cùng spec đó thêm giờ làm + nhập bù của chủ spa và giảm giá dịch vụ lẻ trong lượt) · bán sản phẩm (sau này) · in hóa đơn / gửi Zalo · thanh toán online, thẻ, ví · tích điểm, voucher · nhiều KTV cho một dòng dịch vụ · dùng chung liệu trình giữa nhiều khách **[D5]** · tính hoa hồng (R2; spec này chỉ lưu dữ liệu đầu vào).

## 8. Mặc định chờ EM xác nhận
| # | Câu hỏi | Mặc định đề xuất |
|---|---|---|
| D1 | Gói liệu trình gồm 1 dịch vụ hay có combo nhiều dịch vụ? | Hỗ trợ **combo** (nhiều dòng dịch vụ × số buổi); gói đơn là trường hợp 1 dòng |
| D2 | Liệu trình có hạn dùng không? | **Tùy gói**, để trống = không hạn. Hết hạn thì chặn trừ buổi; quản lý cho dùng tiếp có lý do; chủ spa gia hạn |
| D3 | "Quà tặng" là gì? | Hai loại: **buổi tặng** (cộng vào số buổi) và **quà hiện vật** (ghi chú chữ) |
| D4 | Giá trị một buổi (dùng cho hoa hồng R2)? | Giá sau giảm ÷ số buổi **mua**; buổi tặng có giá trị 0 |
| D5 | Người nhà dùng chung liệu trình của khách? | **Không** trong R1 |
| D6 | SĐT bắt buộc và duy nhất? | **Bắt buộc, duy nhất** trong một spa |
| D7 | ~~Chưa trả hết tiền gói có được dùng buổi không?~~ | **Bỏ** (EM 2026-11-04): R1 mọi giao dịch thu đủ ngay, không có công nợ |
| D8 | Một lượt dùng nhiều buổi của cùng dịch vụ? | Được, số lượng mặc định 1 |
| D9 | Người tư vấn chốt gói có bắt buộc không? | **Bắt buộc** (cần cho hoa hồng R2); được chọn "Quản lý / Chủ spa" nếu không có KTV chốt |
| D10 | Giảm giá: ai được giảm và giảm tối đa bao nhiêu? | Quản lý giảm tối đa **10%** mỗi hóa đơn, chủ spa không giới hạn; luôn bắt buộc ghi lý do |

## 9. Open questions
- Giá dịch vụ **Ôn ấm tử cung**: EM, trước khi seed dữ liệu.
- **Danh sách gói liệu trình** spa đang bán (menu không có): EM, trước khi seed dữ liệu. Không chặn thiết kế, vì gói được chủ spa tự tạo trong US1.
- Quy tắc hoa hồng khi khách trả góp gói (tính trên tiền đã thu hay giá bán): thuộc R2, chủ là EM, hạn trước `/spec` R2.
- Buổi tặng có tính hoa hồng "làm liệu trình" cho KTV không (giá trị 0 nghĩa là không): EM, trước R2.

## 10. Vòng 6: upsell trong lượt và chuyển sang gói liệu trình

**Status: approved (2026-11-06)** — EM trả lời Q-U1…Q-U9 ([raw/2026-11-06-em-answers-round6-upsell.md](../../../raw/2026-11-06-em-answers-round6-upsell.md)); quyết định cuối ở §10.8, thiết kế §10.4 đã sửa theo đó. Nguồn: [raw/2026-11-06-em-upsell-and-convert-to-package.md](../../../raw/2026-11-06-em-upsell-and-convert-to-package.md). EM: "khi KTV upsell bán được dịch vụ mới cho khách thì lại chẳng được flow đấy ghi vào … hôm nay đến as khách lẻ nhưng sau khi làm liệu trình thì lại chọn mua nguyên gói liệu trình thì sao".

**Goal.** Mọi thứ khách mua trong một lần đến (dịch vụ lẻ, buổi trừ gói, **gói mới mua ngay hôm nay**, dịch vụ thêm do KTV tư vấn) được ghi trên **một màn hình thanh toán ở quầy**, một lần, không phải rời `/quay` và không phải nhập lại. Khách vừa trả buổi lẻ rồi quyết định mua gói thì được **chuyển sang gói** bằng một thao tác, tiền và buổi đúng, số liệu không bị hỏng. Mục này phục vụ thước đo R1 #1 (bỏ sổ giấy: sổ giấy ghi được mọi chuyện nên app không được hẹp hơn) và hoa hồng chốt gói đúng người ([compensation-rules §3](../compensation-rules.md)).

**Personas & roles:** Quản lý (kiêm thu ngân) thao tác; Chủ spa thêm quyền với ngày đã qua; KTV tư vấn nhưng **không** thao tác (báo quầy), được gắn tên là người chốt.

### 10.1 Hành trình ở quầy hôm nay (đối chiếu code, 2026-11-06)
Ký hiệu: **[CỤT]** = bế tắc, không có đường đi; **[THIẾU]** = làm được nhưng thiếu một phần; **[NHẬP 2 LẦN]** = phải ghi lại hoặc ghi ở chỗ khác; **[OK]** = đủ dùng.

| # | Bước | App cho phép hôm nay | Bằng chứng | Đánh giá |
|---|---|---|---|---|
| 1 | Đặt lịch | Khách có hồ sơ / khách mới, ≤ 5 dịch vụ, KTV, nhóm gia đình | `POST /appointments`, `/appointments/group`; `features/appointments/BookingSheet.tsx` | [OK] |
| 2 | Khách đến (có lịch) | "Khách đến · ghi lượt" hoặc "Chỉ báo đã đến": `booked → arrived` | `POST /appointments/{id}/arrive` | [OK] |
| 2b | Khách vãng lai đến | Server không có bản ghi nào cho tới khi lưu lượt; lịch chỉ hiện lượt vãng lai **sau khi** đã ghi | `CalendarVisitDto` đọc từ `visit` ([backend §3c](../../architecture/backend.md)) | [THIẾU]: không biết khách vãng lai đang làm trước khi thanh toán |
| 3 | Đang làm, KTV tư vấn thêm dịch vụ | Chỉ thêm được vào **giỏ chưa lưu**. Giỏ là state của trình duyệt (`useState<CartLine[]>`, `FrontDeskPage.tsx:157`), không lưu vào sessionStorage (flows ghi "giỏ giữ trong sessionStorage" khi 401 nhưng code không có), mất khi tải lại hoặc rời trang | `features/front-desk/FrontDeskPage.tsx` | [THIẾU] |
| 3b | Thêm dịch vụ vào lượt **đã lưu** | `PATCH /visits/{id}` chỉ sửa ghi chú + KTV; không thêm dòng, không thu thêm tiền. Phải **hủy lượt rồi ghi lại**, và lý do "ghi nhầm" là sai sự thật trong audit | `operations/src/http.rs:31`; `visit.sale_id UNIQUE` (một hóa đơn lẻ mỗi lượt, [data](../../architecture/data.md)) | [NHẬP 2 LẦN] (Sửa lượt đang làm sẽ đỡ một phần, xem §10.4.4) |
| 4 | Thanh toán | `POST /visits`: dòng trừ gói + dòng lẻ, giảm giá dòng lẻ, TM/CK/kết hợp, thu đủ | `operations::service::record_in` | [OK] |
| 5 | Bán gói **ở quầy** | `/quay` không có nút bán gói. Thẻ khách chỉ hiện chữ "Đã hết buổi. Gợi ý: tư vấn bán gói mới." | `front-desk/CustomerCard.tsx:185`; `SellPackageDialog` chỉ gắn ở hồ sơ khách (`customers/CustomerProfile.tsx:122` → `sales/CustomerPackages.tsx`) | [CỤT] |
| 5b | Bán gói **trong lượt** rồi dùng ngay buổi hôm nay | Phải rời `/quay` sang hồ sơ khách (mất giỏ), bán gói (`POST /sales`), quay lại `/quay`, dựng lại giỏ rồi mới trừ buổi. Hai transaction, hóa đơn gói và lượt không liên kết với nhau | `sales/src/http.rs:29`; `sale` không có `visit_id` | [NHẬP 2 LẦN] |
| 5c | Hoàn tất lịch hẹn / cả nhóm | Giỏ điền sẵn (gói nếu còn buổi, không thì lẻ); sheet không bán gói được | `appointments/complete.ts`, `GroupCheckIn.tsx`, `front-desk/prefill.ts` | [CỤT] với bán gói |
| 6 | Khách trả lẻ rồi muốn mua gói | Không có đường. Cách vòng: hủy lượt (lý do giả) → bán gói ở hồ sơ → ghi lại lượt trừ gói. Quản lý không làm được với ngày trước (`403 day.locked`) | `POST /visits/{id}/void`, `POST /sales` | [CỤT] |
| 7 | Mua gói cho người nhà | `POST /sales` tạo gói cho đúng `customer_id` của hóa đơn, nên người trả tiền = người dùng gói. Mua hộ thì phải tạo hóa đơn trên hồ sơ người dùng và không ghi được ai trả | `sales/src/service.rs:240` | [THIẾU] |
| 8 | Mua lại / gia hạn | Bán gói mới từ hồ sơ (không điền sẵn gói cũ); gia hạn hạn dùng chỉ chủ spa làm được | `POST /customer-packages/{id}/extend` | [THIẾU] |
| 9 | Ai tư vấn chốt | `sale.closer_staff_id` bắt buộc khi bán gói (D9); dịch vụ lẻ thêm do KTV gợi ý không có trường người tư vấn | [data](../../architecture/data.md) `sale`; `visit_line.staff_id` = người làm | [THIẾU] (tùy Q-U3) |
| 10 | Trả trước một phần / còn nợ | Không có (US5 hoãn); sổ giấy có cột "Chưa thanh toán" | [paper-books-analysis](../paper-books-analysis.md) | hoãn |
| 11 | Báo cáo | Hóa đơn có dòng gói thì **cả hóa đơn** tính là "liệu trình" (`bool_or(line_type='package')`) | `reporting/src/day.rs:225-233`, `migrations/0013_management_views.sql:76` | Ràng buộc thiết kế: không gộp dòng lẻ và dòng gói vào **một** hóa đơn |

### 10.2 Danh sách thiếu: các đường mua gói
| | Trường hợp | Hôm nay | Đề xuất | Phạm vi |
|---|---|---|---|---|
| a | Bán gói ở quầy, không có lượt (khách ghé mua, mua tặng) | Chỉ ở hồ sơ khách | Nút "Bán gói liệu trình" ngay trên thẻ khách ở `/quay` (dùng lại `SellPackageDialog`, không mất giỏ) | **Vòng 6** |
| b | Bán gói trong lượt, buổi hôm nay trừ từ gói vừa mua | Không làm được trong một lần | Khối "Gói bán hôm nay" trong giỏ + ô "Dùng gói này cho buổi hôm nay"; **một request, một transaction** (§10.4.1) | **Vòng 6** |
| c | Đã trả lẻ xong rồi mới mua gói | Không có | "Chuyển sang gói liệu trình" trên dòng lượt hôm nay (§10.4.3), cách tính theo Q-U1 | **Vòng 6** |
| d | Mua cho người khác / người nhà | Người trả = người dùng | Hóa đơn ghi **người trả**, gói thuộc **người dùng** (Q-U5). Dùng chung một gói giữa nhiều người (D5) vẫn **không** | Vòng 6 nếu EM đồng ý Q-U5 |
| e | Mua lại / nạp thêm khi sắp hết buổi hoặc hết hạn | Bán mới, chọn lại từ đầu | "Mua lại gói này" trên dòng gói (điền sẵn mẫu gói). Luôn là **gói mới**; buổi còn của gói cũ giữ nguyên (trừ theo hạn sớm nhất, AC4.3); không gộp buổi vào gói cũ | **Vòng 6** (nút điền sẵn) |
| f | Ghi nhận KTV tư vấn (upsell) để tính hoa hồng | Gói: có `closer_staff_id`. Dịch vụ lẻ: không | Gói: người chốt mặc định = KTV của dòng hôm nay. Dịch vụ lẻ thêm: theo Q-U3 | **Vòng 6** (gói); lẻ chờ Q-U3 |
| g | Giảm giá gói | Có (D10: quản lý ≤ 10 %, bắt buộc lý do) | Giữ nguyên, áp **riêng cho từng hóa đơn gói** trong giỏ, tách khỏi giảm giá dịch vụ lẻ. Khoản "trừ tiền lượt lẻ" (nếu Q-U1 = B) **không** phải giảm giá, không tính vào trần 10 % | **Vòng 6** |
| h | Trả trước một phần / trả góp / còn nợ | Hoãn (US5) | Giữ hoãn; schema đã có `sale.amount_paid`. Chờ Q-U7 | **Sau** |

### 10.3 Các phần mềm khác làm thế nào (nghiên cứu 2026-11-06)
- **Phorest** (salon/spa, UK/IE) có sẵn "convert a treatment into a course" lúc khách thanh toán: lịch hẹn thường được đổi thành bán liệu trình, **buổi hôm nay là buổi 1** (trừ 1 buổi), hóa đơn gồm một dòng 0đ cho buổi hôm nay + giá đủ của liệu trình. Có pop-up nhắc nhân viên đề nghị chuyển. Giới hạn: không chạy khi giỏ có hai dịch vụ thuộc liệu trình. ([Phorest: convert a treatment into a course](https://phorest.helpjuice.com/305344-how-do-i-convert-a-treatment-into-a-course-gb), [prompt staff to convert](https://support.phorest.com/hc/en-us/articles/360016369439), [FAQ Courses/Series](https://support.phorest.com/hc/en-us/articles/360016348500-Frequently-Asked-questions-about-Courses-Series))
- **Zenoti** (chuỗi spa) bán membership và dùng quyền lợi **trên cùng một hóa đơn**. Gói phải được bán / thêm vào hồ sơ **trước** khi thêm dịch vụ thì mới tự trừ, nếu không phải áp tay. ([Zenoti: sell and apply benefits in one invoice](https://help.zenoti.com/en/point-of-sale/check-out/sell-and-apply-benefits-for-membership-in-one-invoice.html), [redeem package benefits](https://help.zenoti.com/en/point-of-sale/check-out/redeem-package-benefits.html), [transferred package credits](https://help.zenoti.com/en/point-of-sale/check-out/apply-transferred-package-service-credits-for-recipients.html))
- **Mangomint** bán gói ở một checkout riêng, rồi mở checkout của lịch hẹn và "Apply a package": hai bước, giống cách vòng của ta hôm nay. Dịch vụ đắt hơn gói thì khách trả phần chênh. ([Mangomint: sell and redeem same day](https://www.mangomint.com/learn/sell-and-redeem-a-package-on-the-same-day/), [redeem during checkout](https://www.mangomint.com/learn/how-to-use-redeem-a-package-during-checkout/))
- **MassageBook**: nâng cấp buổi trả trước = hủy credit, đổi giá trị thành store credit, khách trả phần chênh. Đây là mô hình "trừ tiền", khác Phorest. ([MassageBook: upgrading a prepaid series](https://support.massagebook.com/hc/en-us/articles/38968352088845-Upgrading-a-Prepaid-Service-or-Series-Appointment))
- **Fresha**: membership trả một lần hoặc định kỳ, trừ buổi lúc checkout; nhân viên gắn với từng dòng bán và đổi được người hưởng hoa hồng tới 6 tháng sau. Không tìm thấy tài liệu chính thức về bán và dùng trong cùng lượt. ([Fresha: create memberships](https://www.fresha.com/help-center/knowledge-base/memberships-packages-and-gift-cards/101261-create-memberships), [edit a sale](https://www.fresha.com/help-center/knowledge-base/sales/173-edit-a-sale), [commissions update](https://www.fresha.com/blog/fresha-commissions-update))
- **Booksy**: đặt cọc lúc đặt lịch, trừ vào tổng lúc checkout; không thấy trả sau cho gói. SalonIQ có trả sau / chia 3 kỳ qua Klarna. ([Booksy no-show protection](https://biz.booksy.com/en-us/features/no-show-protection), [SalonIQ Klarna](https://www.saloniq.com/article/introducing-our-integration-with-klarna/))
- **Phần mềm Việt Nam** (SPAGO, IDSPA, NanoSpa, MySpa) quảng cáo quản lý thẻ liệu trình + **công nợ** + hoa hồng; không có tài liệu công khai về chuyển lượt lẻ sang gói hay mua hộ (chưa xác minh chi tiết, cần demo). ([GHTK: phần mềm quản lý spa](https://ghtk.vn/blog/phan-mem-quan-ly-spa/), [FPT Shop: phần mềm quản lý salon](https://fptshop.com.vn/tin-tuc/danh-gia/phan-mem-quan-ly-salon-172916))
- Không tìm được nguồn tiếng Trung đáng tin về quy tắc "体验价抵扣疗程卡" (trừ tiền buổi trải nghiệm vào thẻ liệu trình).

**Rút ra:** (1) chuẩn ngành là **một hóa đơn / một màn hình** cho cả bán gói lẫn dùng gói; (2) với "trả lẻ rồi mua gói", phần mềm chuyên spa (Phorest) chọn **buổi hôm nay là buổi 1 của gói**, không tặng thêm buổi; (3) người hưởng hoa hồng gắn vào **từng dòng bán** và sửa được; (4) phần mềm Việt Nam có công nợ, nên Q-U7 sẽ còn quay lại.

### 10.4 Thiết kế đề xuất
#### 10.4.1 Một màn hình thanh toán (giỏ của lượt)
Giỏ ở `/quay` (và giỏ điền sẵn khi hoàn tất lịch hẹn) có ba phần, một tổng, một nút Lưu:
```
┌ Lượt khách đang ghi ─────────────────────────────────────────────────────────┐
│ Dịch vụ hôm nay                                                              │
│  Đại Thông Lạc · KTV Lan · Trừ gói Đại Thông Lạc 10+2 (mua hôm nay)   0đ  ⋯  │
│  Ngâm chân thảo dược · KTV Lan · Dịch vụ lẻ                       100.000đ  │
│ + Thêm dịch vụ lẻ / dùng thêm                                                │
│ Gói bán hôm nay                                                              │
│  Gói Đại Thông Lạc 10 buổi + tặng 2 · 3.500.000đ                             │
│   Người tư vấn chốt: [KTV Lan ▾]  Quà: …  Giảm giá gói (lý do)               │
│   [x] Dùng gói này cho buổi hôm nay (Đại Thông Lạc)                         │
│ + Bán gói liệu trình                                                         │
│ Phải trả: lẻ 100.000đ + gói 3.500.000đ = 3.600.000đ · [TM][CK][TM+CK]        │
│ [ Lưu lượt khách ]                                                           │
└──────────────────────────────────────────────────────────────────────────────┘
```
- **"+ Bán gói liệu trình"** trong giỏ mở một khối inline (mẫu gói, người chốt, quà, giảm giá gói). Tối đa 3 gói mỗi lượt. Không mất giỏ, không rời trang.
- **"Dùng gói này cho buổi hôm nay"** tự bật khi mẫu gói chứa một dịch vụ đang có trong giỏ dưới dạng dịch vụ lẻ (hoặc dịch vụ đã đặt của lịch hẹn): dòng lẻ đó đổi thành dòng trừ buổi của gói mới, giá dòng về 0đ, gói hiện xem trước "còn 11/12". Bỏ chọn → dòng trở lại dịch vụ lẻ. Một dịch vụ có nhiều dòng thì đổi lần lượt theo số buổi của gói.
- **Người tư vấn chốt** mặc định = KTV của dòng dịch vụ hôm nay khớp với gói (không có thì KTV của dòng đầu); quầy đổi được, kể cả "Khách tự chọn" (không ai được tính, §10.8 Q-U2).
- Thẻ khách (cột trái) thêm nút "Bán gói liệu trình" (= mở khối trong giỏ), và trên dòng gói hết buổi / sắp hết hạn có "Mua lại gói này" (điền sẵn mẫu gói), thay cho chữ gợi ý cụt ở `CustomerCard.tsx:185`.
- Khách chỉ mua gói, không làm dịch vụ (trường hợp a): giỏ chỉ có khối "Gói bán hôm nay", nút đổi thành "Lưu hóa đơn gói" và gọi `POST /sales` như hiện nay (không tạo lượt, không tính vào số khách của KTV).
- Giỏ chưa lưu được giữ nháp theo khách trong `sessionStorage` (try/catch), để tải lại trang hoặc 401 không mất giỏ (đúng như flows đã hứa).
- Sheet "Khách đến cả nhóm": mỗi người có thêm "+ Bán gói" (cùng khối, thu gọn). Vòng này chưa có một người trả cho cả nhóm.
- States: đang lưu (nút khóa + spinner); lỗi ở khối gói hiện ngay tại khối (ví dụ "Vượt 10 %, cần chủ spa"), giỏ giữ nguyên; thành công → toast "Đã lưu lượt + bán Gói … (còn 11/12)".

#### 10.4.2 API và dữ liệu
- **Mở rộng `POST /visits`** (và thân của `POST /appointments/{id}/complete`, check-in nhóm, vì cùng dùng `operations::service::record_in`) thêm `package_sales[]` (0–3):
  ```json
  { "customer_id": "…", "lines": [
      { "service_id": "…", "source": "package", "package_ref": "p1", "staff_id": "…", "qty": 1 },
      { "service_id": "…", "source": "one_off", "staff_id": "…", "referrer_staff_id": "…", "qty": 1 } ],
    "companions": ["Chồng của chị Hà"],
    "package_sales": [ { "ref": "p1", "package_plan_id": "…", "closer_staff_id": "…", "gift_note": "…",
                         "discount": null, "beneficiary_customer_id": null } ],
    "payments": [ { "method": "cash", "amount": 3600000 } ] }
  ```
  Không thêm endpoint `/checkouts` riêng: lượt vẫn là use case gốc, đã có sẵn idempotency, nhập bù, ngày đã chốt, giảm giá lẻ.
- **Thứ tự trong một transaction:** claim idempotency → kiểm quyền → tạo từng hóa đơn gói bằng `sales::service::sell_in(tx, …)` (tách thân của `POST /sales` ra, giống cách `record_in` đã tách) → tạo `customer_package` → ghi dòng lượt; dòng `new_package` được nối với item của gói vừa tạo (trong DB vẫn là `source = 'package'`, không thêm giá trị enum) → hóa đơn dịch vụ lẻ như cũ → chia thanh toán → audit → lưu response. Lỗi ở bất kỳ bước nào → rollback cả lượt lẫn gói.
- **Mỗi loại một hóa đơn** (ràng buộc §10.1 #11): hóa đơn dịch vụ lẻ của lượt (như hôm nay) + một hóa đơn cho mỗi gói. Báo cáo theo loại không phải đổi.
- **Chia thanh toán** (`sales::domain::allocate_payments`, hàm thuần, có unit test): `sum(payments)` phải bằng tổng mọi hóa đơn (`422 payment.amount_mismatch`). Một hình thức → mọi hóa đơn cùng hình thức. TM + CK → tiền mặt trả hóa đơn lẻ trước, rồi tới các hóa đơn gói theo thứ tự `ref`; phần còn lại là CK. Mỗi hóa đơn có Payment riêng nên hủy / đảo từng hóa đơn vẫn đúng.
- **Liên kết:** thêm `sale.visit_id uuid NULL` (FK theo tenant tới `visit`, index `(tenant_id, visit_id)`): mọi hóa đơn sinh ra trong một lượt (lẻ, gói, thêm sau) trỏ về lượt đó. `visit.sale_id` giữ cho hóa đơn lẻ ban đầu trong giai đoạn chuyển, sau đó bỏ khi `sale.visit_id` đã đủ dùng (một lượt có thể có nhiều hóa đơn, cần cho §10.4.4). `customer_package.sale_id` không đổi. Lịch sử khách nhóm hóa đơn theo `visit_id`: "Lượt 09:12 · mua Gói Đại Thông Lạc".
- **Mua cho người khác** (nếu Q-U5 = có): `sale.customer_id` = người trả tiền; `customer_package.customer_id` = `beneficiary_customer_id` (mặc định = người trả). Người dùng phải là khách có hồ sơ trong cùng spa (`404` nếu không). Hồ sơ người trả: "Đã mua tặng Gói … cho <tên>"; hồ sơ người dùng: gói hiện như mọi gói. Không dùng chung (D5). Dữ liệu: không thêm cột, chỉ bỏ giả định `sale.customer_id = customer_package.customer_id` ở code đọc (lịch sử, báo cáo khách).
- **Server quyết định mọi số tiền:** giá gói, giá dịch vụ, giá trị buổi lấy từ catalog trong transaction; `unit_price`, `total` client gửi lên bị bỏ qua như hiện nay.

#### 10.4.3 "Chuyển sang gói liệu trình" sau khi đã trả lẻ (trường hợp c)
- Ở "Lượt khách hôm nay", menu ⋯ của lượt có dòng lẻ → **"Chuyển sang gói liệu trình"**. Sheet: chọn mẫu gói (ưu tiên gói chứa dịch vụ lẻ của lượt), người chốt (mặc định KTV của dòng), quà, giảm giá gói; dòng tóm tắt "Hôm nay khách đã trả 399.000đ (Tiền mặt) cho Đại Thông Lạc".
- **Cách A (đề xuất, Q-U1): buổi hôm nay thành buổi 1 của gói.** Server làm **một lệnh sửa lượt nguyên tử**, dùng cùng cơ chế "Sửa lượt" đang build (hủy + ghi lại có lý do trong một transaction): lượt cũ → `voided` với lý do tự sinh "Chuyển sang gói <tên>"; hóa đơn lẻ của nó được đảo (reversal cùng hình thức, cùng số tiền, như hủy hiện nay); ghi lại lượt mới cùng giờ làm, cùng KTV, các dòng khớp gói thành dòng trừ buổi của gói mới; dòng lẻ không khớp vẫn là dòng lẻ (hóa đơn lẻ mới, thu lại đúng số đã trả, cùng hình thức). Hóa đơn gói thu đủ giá gói; màn hình ghi rõ **"Khách đưa thêm 3.101.000đ"** (= giá gói − tiền lẻ đã trả hôm nay). Ví dụ: Đại Thông Lạc (Nam) 399.000đ → Gói 10 + tặng 2 giá 3.500.000đ: gói còn 11/12; sổ ngày: −399.000đ dịch vụ lẻ (đảo), +3.500.000đ liệu trình; thu ròng trong ngày từ khách 3.500.000đ.
- ~~**Cách B: trừ tiền hôm nay vào giá gói.**~~ **Không làm** (Q-U1: tùy case; trường hợp gói không liên quan thì chỉ bán gói mới qua `additions`). Giữ lượt lẻ; hóa đơn gói có thêm khoản **"Trừ tiền lượt lẻ ngày dd/mm"** (`sale.credit_amount` + `credit_visit_id`, không phải giảm giá, không tính trần 10 %); gói đủ 12/12; khách đưa thêm 3.101.000đ. Spa chịu thêm một buổi. Mỗi lượt lẻ chỉ được trừ một lần (`UNIQUE credit_visit_id`).
- `POST /visits/{id}/convert-to-package` `{package_sale: {package_plan_id, closer_staff_id?, gift_note?, discount?}, payments[]}` + `Idempotency-Key` (hash gồm `{id}`) + `If-Match` (version của lượt). `201 {voided, visit, difference}` (`difference` = khách đưa thêm). Lý do hủy tự sinh: "Chuyển sang gói <tên>".
- Một lượt chỉ chuyển được một lần (lượt cũ đã `voided` → `409 visit.voided`); lượt mới mang `corrects_visit_id` (dùng chung trường của Sửa lượt). Lượt không có dòng lẻ khớp gói → `422 visit.nothing_to_convert`.
- **Thời hạn:** theo quy tắc ngày đã chốt hiện có (`kernel::corrections::authorize_void`): trong ngày → quản lý + chủ spa; ngày trước → chỉ chủ spa. Thêm (Q-U4): tháng đã chốt lương (ADR-0008) → `409 payroll.period_closed`, vì lệnh chuyển đổi hoa hồng của tháng đó.

#### 10.4.4 Thêm dịch vụ vào lượt đã lưu (upsell sau khi đã ghi)
- Quầy đã lưu lượt (ví dụ hoàn tất lịch hẹn lúc khách đến) rồi KTV bán thêm: ⋯ → **"Thêm dịch vụ / bán gói"** mở giỏ của lượt đó ở chế độ thêm: chỉ thêm dòng (lẻ, trừ gói, gói mới), không đụng dòng cũ, không cần lý do (đây không phải sửa sai). `POST /visits/{id}/additions` (thân như `POST /visits`, bỏ `customer_id` và `check_in_at`) tạo **hóa đơn mới** nối `sale.visit_id`, Payment riêng. Trong ngày: quản lý + chủ spa; ngày trước: chủ spa.
- Bớt / đổi dòng có tiền vẫn đi qua Sửa lượt (hủy + ghi lại có lý do). Thêm thì không, để audit không ghi "sửa sai" cho một lần bán hàng bình thường.

#### 10.4.5 Phân quyền, audit, báo cáo
| Thao tác | Permission (kiểm ở service, identity từ session) |
|---|---|
| `POST /visits` có `package_sales` | `visit.record` + `package.sell` + `payment.collect` (+ `sale.discount*` nếu giảm giá gói) |
| `POST /visits/{id}/additions` | `visit.record` (+ `package.sell`, `payment.collect`, `sale.discount*` theo nội dung); ngày trước: thêm `record.void_closed_day` |
| `POST /visits/{id}/convert-to-package` | `visit.record` + `package.sell` + `payment.collect` + `record.void_same_day`; ngày trước: `record.void_closed_day` |
| KTV | Không có quyền nào ở trên (KTV báo quầy). Cho KTV thấy gói mình chốt ở "Khách của tôi" (`GET /me/work`) là việc sau |

- Mỗi route mới có dòng trong `authz_matrix.rs`; tài nguyên của spa khác → `404`.
- **Audit:** `sale.create` cho mỗi hóa đơn gói (có `visit_id`, `closer_staff_id`, `beneficiary_customer_id`); `visit.addition` (hóa đơn mới, dòng mới); `visit.convert_to_package` (lượt cũ, lượt mới, gói, số tiền chuyển, cách A/B). Không ghi ghi chú buổi vào audit (như hiện nay).
- **Báo cáo (cơ sở tiền mặt; EM 2026-11-05: "cứ tính doanh thu mọi thứ 1 thể"):** mỗi đồng chỉ có một Payment nên không đếm hai lần. Chuyển cách A cùng ngày: sổ ngày có dòng đảo −399.000đ (lẻ) và +3.500.000đ (gói), doanh thu ròng của ngày đúng 3.500.000đ. Chuyển vào ngày sau (chủ spa): dòng đảo ghi vào **ngày chuyển** (như mọi lần hủy, `business_date` = ngày hủy), ngày cũ giữ nguyên. `packages_sold` +1. `voided_visits` **không** đếm lượt bị thay bởi lệnh sửa/chuyển (chỉ đếm hủy thật); cần thống nhất với Sửa lượt.
- **Hoa hồng** ([staff-management](staff-management.md) AC5.2): người chốt nhận `% × sale.total` của hóa đơn gói; KTV làm nhận `% × line_value` của dòng trừ buổi (= giá trị buổi, 350.000đ trong ví dụ) thay cho dòng lẻ 399.000đ đã đảo (cách A).
- **Hiệu năng:** `POST /visits` có gói vẫn trong ngân sách lưu lượt < 500 ms p95 (thêm tối đa 3 lần insert hóa đơn gói); thêm case vào bench.

### 10.5 User stories & acceptance criteria
G1–G6 áp dụng cho mọi endpoint dưới đây.

**US9: Bán gói trong lượt, dùng ngay buổi hôm nay.** *Là quản lý, tôi muốn bán gói ngay trong giỏ của lượt và trừ buổi hôm nay từ gói đó, để khách mua gói giữa buổi được ghi một lần.*
- AC9.1: Given khách Nam, giỏ có Đại Thông Lạc (lẻ, KTV Lan), when thêm "Gói Đại Thông Lạc 10 + tặng 2" 3.500.000đ, để bật "Dùng gói này cho buổi hôm nay", trả TM 3.500.000đ → một lượt (1 dòng trừ gói), một hóa đơn gói 3.500.000đ có `visit_id` = lượt, người chốt = KTV Lan, gói còn **11/12**, không có hóa đơn lẻ.
- AC9.2: Given giỏ có thêm Ngâm chân 100.000đ lẻ, trả TM 600.000đ + CK 3.000.000đ → hóa đơn lẻ 100.000đ trả TM; hóa đơn gói 3.500.000đ = TM 500.000đ + CK 3.000.000đ.
- AC9.3 (neg): tổng thanh toán ≠ lẻ + gói → `422 payment.amount_mismatch`, không tạo gì.
- AC9.4 (neg): người gọi có `visit.record` nhưng không có `package.sell` → `403`, không lượt nào được ghi.
- AC9.5 (neg): dòng `new_package` trỏ tới `package_ref` không có, hoặc dịch vụ không thuộc mẫu gói → `422 validation_failed` (`lines[i].package_ref`).
- AC9.6 (neg): người chốt đã nghỉ việc / thuộc spa khác → `422`; mẫu gói ngừng bán → `422 inactive`; mẫu gói của spa khác → `404`.
- AC9.7 (neg): gửi lại cùng `Idempotency-Key` → cùng response, một lượt, một gói; khác body → `422 idempotency.key_reuse_mismatch`.
- AC9.8 (neg): hóa đơn gói lỗi (ví dụ giảm giá vượt 10 % với quản lý → `403 discount.limit_exceeded`) → rollback, lượt cũng không được ghi.
- AC9.9: Hoàn tất lịch hẹn có `package_sales` → lịch `done`, lượt và gói như AC9.1 (cùng use case).
- AC9.10: Báo cáo ngày: liệu trình +3.500.000đ, dịch vụ lẻ 0đ, `packages_sold` +1, khách +1, buổi đã dùng +1.
- AC9.11 (neg, mạng): mất mạng khi lưu → giỏ giữ nguyên (cả khối gói), gửi lại cùng key → không ghi đôi.

**US10: Bán gói ở quầy không cần lượt.** *Là quản lý, tôi muốn bán gói ngay ở `/quay`, để không phải mở hồ sơ khách và mất giỏ.*
- AC10.1: Given khách đã chọn ở `/quay`, when bấm "Bán gói liệu trình" và giỏ chỉ có khối gói, lưu → `POST /sales` như US3, không tạo lượt, số khách của KTV không đổi.
- AC10.2: Given gói hết buổi trên thẻ khách, when bấm "Mua lại gói này" → khối gói điền sẵn mẫu gói đó; mẫu đã ngừng bán → "Mẫu gói đã ngừng bán, chọn gói khác".
- AC10.3 (neg): người không có `package.sell` không thấy nút; gọi thẳng API → `403`.

**US11: Chuyển lượt lẻ đã trả sang gói.** *Là quản lý, tôi muốn chuyển một lượt lẻ hôm nay sang gói khách vừa quyết định mua, để khách chỉ trả phần chênh và số liệu đúng.*
- AC11.1 (cách A): Given lượt hôm nay Đại Thông Lạc lẻ 399.000đ TM, when chuyển sang gói 3.500.000đ, khách đưa thêm 3.101.000đ TM → lượt cũ `voided`, lý do "Chuyển sang gói …", reversal −399.000đ TM; lượt mới cùng giờ làm, cùng KTV, trừ gói, `corrects_visit_id` = lượt cũ; gói **11/12**; hóa đơn gói 3.500.000đ (Payment TM 3.500.000đ); thu ròng của ngày từ khách = 3.500.000đ.
- AC11.2: lượt có thêm dòng lẻ không thuộc gói → dòng đó được ghi lại là dòng lẻ, cùng số tiền và hình thức; số tiền khách đưa thêm không đổi vì dòng này.
- AC11.3 (neg): chuyển lần hai / lượt đã hủy → `409 visit.voided`; lượt không có dòng lẻ khớp gói → `422 visit.nothing_to_convert`.
- AC11.4 (neg): quản lý chuyển lượt của ngày trước → `403 day.locked`; chủ spa → được.
- AC11.5 (neg): tháng của lượt đã chốt lương → `409 payroll.period_closed` (Q-U4).
- AC11.6 (neg): `If-Match` cũ (lượt vừa được sửa ở máy khác) → `412`; hai lệnh chuyển song song → đúng một thành công.
- AC11.7 (neg): thiếu `package.sell` hoặc `record.void_same_day` → `403`; lượt của spa khác → `404`; chưa đăng nhập → `401`.
- AC11.8: Báo cáo: `voided_visits` không tăng; hoa hồng KTV: dòng trừ buổi 350.000đ thay cho 399.000đ; người chốt: 3.500.000đ.

**US12: Thêm dịch vụ / bán gói vào lượt đã lưu.** *Là quản lý, tôi muốn thêm dịch vụ KTV vừa tư vấn vào lượt đã ghi, để không phải hủy lượt.*
- AC12.1: Given lượt đã lưu lúc 09:12 (trừ gói gội đầu), when thêm Ngâm chân 100.000đ CK → hóa đơn mới 100.000đ có `visit_id` = lượt, lượt có thêm dòng mới, dòng cũ không đổi, không có reversal.
- AC12.2 (neg): lượt đã hủy → `409 visit.voided`; quản lý thêm vào lượt của ngày trước → `403 day.locked`.
- AC12.3 (neg): gửi lại cùng key → một hóa đơn.

**US13 (nếu Q-U5 = có): Mua gói cho người khác.** *Là quản lý, tôi muốn ghi người trả tiền và người dùng gói khác nhau, để gói nằm đúng hồ sơ người đi làm.*
- AC13.1: Given mẹ (người trả) mua gói cho con (đã có hồ sơ) → `sale.customer_id` = mẹ, gói nằm trên hồ sơ con, lịch sử của mẹ có "Mua tặng Gói … cho <con>".
- AC13.2 (neg): `beneficiary_customer_id` của spa khác / không tồn tại → `404`; mẹ trừ buổi từ gói của con → `422` (gói không dùng chung, D5).

### 10.6 Ngoài phạm vi vòng 6
Trả trước một phần / còn nợ / đặt cọc (US5, Q-U7) · một người trả cho cả nhóm trong một hóa đơn · dùng chung gói giữa người nhà (D5) · gộp buổi còn lại của gói cũ vào gói mới · đổi gói (trả gói A lấy gói B khi đã dùng buổi: vẫn là hủy + hoàn của chủ spa, US7) · "mở lượt" lúc khách vãng lai vừa đến (Q-U8) · KTV tự ghi upsell trên điện thoại · hoa hồng bán dịch vụ lẻ (Q-U3).

### 10.7 Câu hỏi cho EM (Q-U1…Q-U9)
| # | Câu hỏi | Đề xuất | Vì sao |
|---|---|---|---|
| Q-U1 | Khách trả lẻ rồi mua gói: tính buổi hôm nay là **buổi 1 của gói** (A) hay **trừ tiền hôm nay vào giá gói** để gói đủ buổi (B)? | **A** | Khách trả đúng giá gói, spa không tặng thêm buổi; Phorest làm như vậy; sổ sách đơn giản (đảo lẻ, bán gói); không sinh khoản "trừ tiền" mới cạnh giảm giá |
| Q-U2 | Gói bán trong lượt: ai là người tư vấn chốt? | **Một người**, mặc định KTV đang làm cho khách, quầy đổi được | Đúng quy tắc "chốt liệu trình mới" hiện có; chia hoa hồng cho 2 người chưa có quy tắc |
| Q-U3 | KTV gợi ý thêm **dịch vụ lẻ** (không phải gói) có được hoa hồng tư vấn không? | **Không**, chỉ hoa hồng làm như hiện nay | Quy định lương chỉ có 3 loại (làm, chốt gói, bán sản phẩm); muốn thưởng upsell lẻ thì cần quy tắc mới |
| Q-U4 | Được chuyển sang gói muộn nhất khi nào? | Quản lý: trong ngày. Chủ spa: ngày sau vẫn được, **trừ khi tháng đó đã chốt lương** | Khớp quy tắc ngày đã chốt; chặn việc đổi hoa hồng của tháng đã trả lương |
| Q-U5 | Có cho **mua gói cho người khác** (mẹ mua cho con) không? | **Có**: hóa đơn ghi người trả, gói thuộc người dùng. Vẫn **không** dùng chung một gói | Hay gặp ở spa gia đình; ghi đúng người dùng thì nhắc hết buổi, lịch sử, hoa hồng làm đều đúng |
| Q-U6 | Mua lại khi gói sắp hết: có ưu đãi tự động hay gộp buổi còn lại không? | **Không** tự động; giảm giá thủ công có lý do; buổi cũ giữ ở gói cũ | Tài liệu spa không có quy tắc ưu đãi nào; gộp buổi làm sai giá trị buổi (hoa hồng) |
| Q-U7 | Trả trước một phần / còn nợ cho gói: làm bây giờ hay sau? Nếu làm thì khách còn nợ có được dùng buổi không? | **Sau** (vòng riêng); EM cho biết sổ giấy cột "Chưa thanh toán" thường ghi gì để thiết kế | US5 đã hoãn; schema sẵn `amount_paid`; còn thiếu quy tắc hoa hồng khi trả góp (đang mở ở §9) |
| Q-U8 | Khách vãng lai: có cần "mở lượt" ngay lúc khách đến (hiện "đang làm" trên lịch) rồi thêm dần, hay giữ "ghi khi thanh toán"? | **Giữ ghi khi thanh toán** + giỏ nháp không mất + "Thêm dịch vụ vào lượt" (US12) | Giữ fast path < 30 s; "mở lượt" là thay đổi lớn (lượt chưa thanh toán, quên đóng) |
| Q-U9 | Giảm giá gói trong lượt: trần 10 % của quản lý tính riêng cho từng hóa đơn gói, tách khỏi giảm giá dịch vụ lẻ? | **Có, tách riêng** | Mỗi hóa đơn một khoản giảm như hiện nay; báo cáo giảm giá rõ theo loại |

### 10.8 Quyết định cuối (EM 2026-11-06, lead's reading)
| # | EM trả lời | Áp dụng trong vòng 6 |
|---|---|---|
| Q-U1 | "tùy case chứ đâu phải cứ muốn gộp là được" | **Không tự động.** Bán gói sau khi lượt đã lưu = hóa đơn gói mới (`POST /visits/{id}/additions`), lượt lẻ giữ nguyên (**mặc định**). Chỉ khi gói chứa dịch vụ đã làm hôm nay, quầy **chọn** "Tính buổi hôm nay vào gói" → `POST /visits/{id}/convert-to-package` (cách A: đảo tiền lẻ, buổi hôm nay = buổi 1 của gói). Không có cách B (`credit_amount`). |
| Q-U2 | Một người duy nhất: chủ spa / quản lý / KTV / khách tự chọn | Mỗi **hóa đơn gói** có đúng một "người giới thiệu / chốt" (`sale.closer_staff_id`, bất kỳ nhân viên đang làm, kể cả chủ spa và quản lý vì họ là hồ sơ nhân viên), hoặc **"khách tự chọn" = null** (không ai được tính). D9 đổi: người chốt không còn bắt buộc. Mỗi **dòng dịch vụ lẻ** có đúng một người giới thiệu (`visit_line.referrer_staff_id`, null = khách tự chọn) — vì một giỏ thường có cả dịch vụ khách tự chọn lẫn dịch vụ KTV tư vấn thêm, gắn ở hóa đơn sẽ tính sai. |
| Q-U3 + Q-U4 note | "hoa hồng upsell là tính cả gói liệu trình và liệu trình lẻ"; "làm thì tính là hoa hồng làm" | Hai khoản tách biệt, có thể cho hai người: **hoa hồng làm** (§3.1) = `% × line_value` cho KTV của dòng (`visit_line.staff_id`); **hoa hồng giới thiệu / chốt (upsell)** (§3.2, cùng %) = `% × (Σ giá gói đã thu mà người đó chốt + Σ line_value dịch vụ lẻ người đó giới thiệu)`. |
| Q-U4 | "cứ ok đi" | Quản lý: trong ngày. Chủ spa: ngày trước. Tháng đã chốt lương → `409 payroll.period_closed` (cả chuyển sang gói lẫn thêm dịch vụ vào lượt của tháng đó). |
| Q-U5 | Đồng ý + người đi cùng chỉ cần ghi chú | Mua cho người khác: `sale.customer_id` = người trả, `customer_package.customer_id` = người dùng; người trả không dùng được gói (D5). **Người đi cùng dạng ghi chú** ("chồng của …", "gia đình của …"): `companions text[]` (0–5, 1–100 ký tự) trên lịch hẹn và lượt; không phải khách, không được làm / tính tiền; nhóm chỉ có 1 khách thật + ghi chú vẫn hợp lệ. Muốn phục vụ thì quầy tạo hồ sơ khách như thường. |
| Q-U6 | (không ý kiến) | Không ưu đãi tự động, "Mua lại gói này" = gói mới (điền sẵn mẫu). |
| Q-U7 | "thường là sẽ thanh toán hết"; nợ thì phải trả, nên có nhắc ai chưa thanh toán | **Ngoài phạm vi vòng 6.** Khi làm: khách còn nợ **không** được dùng buổi; có danh sách nhắc hóa đơn chưa thanh toán. Hiếm gặp. |
| Q-U8 | (không ý kiến) | Giữ "ghi khi thanh toán"; giỏ nháp giữ trong `sessionStorage` theo người dùng + khách (xóa khi lưu / hủy / đăng xuất); "Thêm dịch vụ vào lượt" (US12). |
| Q-U9 | (không ý kiến) | Trần 10 % của quản lý tính **riêng** cho từng hóa đơn gói và cho hóa đơn dịch vụ lẻ. |
| Q-E1, Q-E2 | (không ý kiến) | Như đề xuất ở US6b. Chuyển sang gói vào ngày sau: đảo tiền lẻ **và** hóa đơn gói ghi vào **ngày chuyển**; lượt mới giữ ngày cũ. |

#### AC bổ sung (chốt)
- AC9.12: dòng lẻ có `referrer_staff_id` (nhân viên đang làm, `422 staff.inactive` nếu không); dòng trừ gói không có người giới thiệu (`422 lines[i].referrer_staff_id not_allowed`).
- AC9.13: `package_sales[k].beneficiary_customer_id` ≠ khách của lượt → gói thuộc người đó, nhưng dòng `package_ref` trỏ tới gói đó → `422 lines[i].package_ref not_beneficiary`.
- AC10.4: `POST /sales` với `closer_staff_id: null` → hóa đơn "khách tự chọn", không ai được hoa hồng chốt.
- AC12.4: hủy một lượt có thêm dịch vụ → đảo **mọi** hóa đơn lẻ của lượt; hóa đơn gói bán trong lượt **giữ nguyên** (hủy riêng nếu cần).
- AC12.5: tháng đã chốt lương → `409 payroll.period_closed`.
- AC11.9: lượt cũ không có dòng lẻ khớp gói → `422 visit.nothing_to_convert`, lượt cũ **không** bị hủy (rollback).
- AC-P1 (lương): phiếu lương có `upsell_package_base`, `upsell_service_base`, `upsell_lines[]`; `closing_commission_base` = tổng hai phần; phiếu đã chốt cũ vẫn đọc được (trường mới mặc định 0 / rỗng).
- AC-P2: hủy lượt → mất hoa hồng giới thiệu dịch vụ lẻ của lượt; hoa hồng chốt gói còn cho tới khi hóa đơn gói bị hủy.
- AC-C1 (người đi cùng): `companions` 6 mục hoặc mục rỗng / > 100 ký tự → `422 companions` / `companions[i] invalid`; hoàn tất lịch hẹn không gửi `companions` → lượt lấy ghi chú của lịch hẹn; `PATCH /visits/{id}` sửa được danh sách.

**Tiếp theo:** đã chạy `/backend-design` ([backend §3e](../../architecture/backend.md), [data 0021](../../architecture/data.md)) → `/frontend-design` ([front-desk-flows vòng 6](../../design/front-desk-flows.md)) → `/breakdown` ([plan r1-6-upsell](../../plans/r1-6-upsell.md)).
