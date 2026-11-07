---
title: ADR-0011 Anonymous walk-in = a nameless customer profile; phone optional
status: accepted
date: 2026-11-07
deciders: EM (problem, decision E), agent (model)
---

# ADR-0011: Khách vãng lai không tên = hồ sơ khách không tên; SĐT không bắt buộc

## Context
EM 2026-11-07 (raw `2026-11-07-em-decisions-after-live-test.md`): "khách vãng lai mà vẫn phải nhập tên với nhập số điện thoại lỡ đâu người ta không muốn thì sao". Quyết định E: SĐT không bắt buộc trên mọi hồ sơ (kể cả người đi cùng). Thêm ngày sinh tùy chọn (ưu đãi sinh nhật sau này).

Ràng buộc hiện tại: `visit.customer_id NOT NULL`; lượt, hóa đơn, lương ("làm" theo `visit_line.staff_id`), lịch sử khách, báo cáo (số khách riêng biệt), lịch hẹn, sổ doanh thu đều nối qua `customer_id`. `customer.full_name` và `phone_e164` đều `NOT NULL`, SĐT duy nhất mỗi spa (`customer_phone_uq`). Khoảng 15 truy vấn SQL đọc `c.full_name`.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Lượt không có hồ sơ (`visit.customer_id` nullable) | Không sinh hồ sơ "rác" | Đụng mọi đường đi: lượt, hóa đơn (`sale.customer_id`), lịch sử, báo cáo số khách, Sửa lượt/Hủy lượt, chuyển sang gói, lịch hẹn → lượt. Rủi ro lớn, nhiều nhánh `NULL` |
| B. Một hồ sơ "Khách vãng lai" dùng chung mỗi spa | Một hồ sơ, không tạo thêm | Lịch sử vô nghĩa (mọi người lạ trộn lẫn), đếm khách sai (N người = 1), một người lạ sau đó cho tên thì không tách được lượt của họ |
| C. **Mỗi khách vãng lai không tên = một hồ sơ không tên** (`full_name NULL`, `phone_e164 NULL`), hiển thị "Khách vãng lai" | Mọi đường đi giữ nguyên (vẫn có `customer_id`); số khách đúng; khách cho tên lúc sau → sửa hồ sơ, giữ lịch sử | Có hồ sơ không tên trong DB; cần loại khỏi danh sách gần đây / khách lâu chưa quay lại; một nhãn hiển thị |

## Decision
Chọn **C**. Migration `0023_customer_identity`:
- `customer.full_name` và `phone_e164` bỏ `NOT NULL`; CHECK độ dài tên và định dạng SĐT giữ nguyên (chỉ áp khi có giá trị). `customer_phone_uq` là unique index thường: Postgres coi NULL là khác nhau, nên bao nhiêu hồ sơ không SĐT cũng được, còn SĐT nhập vào vẫn duy nhất. `search_text` = `''` cho hồ sơ không tên.
- `app.customer_label(full_name)` = `coalesce(full_name, 'Khách vãng lai')`: mọi read model SQL (lượt, lịch hẹn, sổ doanh thu, gói, lương "Khách của tôi") hiện nhãn này. Bản Rust: `crm::service::WALK_IN_LABEL` (test so khớp).
- `birth_date date` tùy chọn: không sau hôm nay, tối đa 120 năm (service); DB chặn < 1900 và chặn `birth_year` lệch năm của `birth_date`. Có ngày sinh thì server tự điền `birth_year`.
- `customer_package_extension` (chỉ thêm, RLS) cho mục "Gia hạn" trên dòng thời gian; copy các lần gia hạn cũ từ `audit_log`.

API: `CustomerCreate/Update.full_name`, `.phone`, `.birth_date` tùy chọn; `CustomerDto`/`CustomerSummaryDto` có `full_name` = tên hiển thị (nhãn khi không tên), `named: bool`, `phone: string | null`, `birth_date`. Web: nút "Khách vãng lai" ở ô tìm khách tại Quầy tạo hồ sơ không tên rồi chọn luôn; form hồ sơ để trống tên/SĐT được.

**Gói liệu trình cần chủ gói có tên** (`422 customer.name_required`): bán gói (`POST /sales`, gói trong giỏ của lượt, nhập sổ cũ) chặn khi người dùng gói chưa có tên; khách vãng lai không tên vẫn được *trả tiền* mua gói cho một người có tên. Không xóa được tên của hồ sơ đang có gói. SĐT không bắt buộc cho chủ gói (quyết định E).

## Consequences
- Positive: không đụng lượt / hóa đơn / lương / Sửa lượt / Hủy lượt; báo cáo đếm mỗi khách vãng lai là một khách (đúng: một người); ADR-0009 hết hạn chế "ghi lượt cần hồ sơ có SĐT" (người đi cùng chỉ có tên → "Tạo hồ sơ" không SĐT).
- Negative / accepted: hồ sơ không tên tích lũy trong DB (một mỗi lượt vãng lai không tên); chúng không có trong danh sách gần đây (`GET /customers` rỗng `q`), không tìm được theo tên, không có trong "Khách lâu chưa quay lại" (không gọi lại được); mở được từ lượt / lịch sử / sổ doanh thu. Khách có tên mà không SĐT vẫn có trong danh sách gọi lại, hiện "Chưa có SĐT".
- EM 2026-11-07 ([raw](../../raw/2026-11-07-em-walkin-package-owner.md)): quy tắc chủ gói có tên / SĐT tùy chọn — ok; gộp hồ sơ — để vòng sau.
- Follow-ups: gộp hồ sơ (người lạ quay lại và cho tên trùng một hồ sơ khác) — **mở, vòng sau** (EM đồng ý hoãn); ưu đãi sinh nhật (index theo tháng/ngày khi làm tính năng).

## Revisit when
Hồ sơ không tên chiếm phần lớn bảng `customer` (hiệu năng / dọn dẹp), hoặc EM muốn gộp hồ sơ.
