---
title: "Spec R1-1: Màn hình quầy & liệu trình khách hàng"
status: approved
updated: 2026-11-04
sources: [raw/2026-11-04-idea-internal-platform.md]
related: [../vision.md, ../personas.md, ../glossary.md, ../paper-books-analysis.md]
---

# Spec R1-1: Màn hình quầy & liệu trình khách hàng

**Status: approved** (EM 2026-11-04; rev. 2026-11-04: bỏ công nợ và giờ ra). Các mặc định **[D1–D10]** đã được EM xác nhận. Bảng giá thật: [service-catalog](../service-catalog.md).

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
- Câu hỏi cho EM (mặc định đang dùng): **Q-E1** sửa lượt ngày đã qua: đảo tiền ghi ngày sửa, tiền mới ghi ngày cũ (mặc định, ghép từ hai quy tắc đã có) hay cả hai cùng ngày cũ? **Q-E2** đổi riêng lý do giảm giá (không đổi mức) cũng đi qua hủy-ghi lại (mặc định; sổ doanh thu đọc lý do từ hóa đơn, hóa đơn chỉ thêm) — có cần sửa trực tiếp không? **Q-E3** đếm "lượt hủy" trong báo cáo ngày có tính lượt đã được sửa không (mặc định: có, vì nó đúng là một lần hủy)?

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
