---
title: ADR-0023 Giờ của dòng audit = lúc transaction ghi, một giờ cho cả transaction
status: accepted
date: 2026-11-12
deciders: LEAD + agent (EM ủy quyền "làm tiếp cho xong hết", không hỏi lại)
---

# ADR-0023: Giờ của dòng audit là lúc transaction ghi nó (sau mọi khóa dòng), một giờ cho cả transaction

> **Sửa một phần [ADR-0021](ADR-0021-activity-log-for-the-owner.md) D8:** `*_as_of` không còn đóng dấu giờ audit bằng `now` của nó; chỉ `Actor::at_clock` (công cụ back-fill) đặt giờ. Quyết định khi build: K60–K64 ở [spec record-history](../product/specs/record-history.md) Review log.

## Context
- Lịch sử trên từng bản ghi (ADR-0022) và "Nhật ký thao tác" (ADR-0021) xếp dòng theo `(at, id)`. Chuỗi sửa đọc "cũ → mới" theo thứ tự đó (nhóm dịch vụ lấy giá trị cũ từ dòng trước, K40; "dòng đầu = tạo, mọi dòng sau = sửa" của ô ngày / cài đặt; `summary.created` / `last`).
- Trước đây `at` là một trong hai giờ, **đều lấy trước khi thay đổi xảy ra**:
  - `COALESCE($n, now())`: `now()` của Postgres là giờ **bắt đầu transaction**;
  - các route gọi `*_as_of(…, Utc::now())` (ghi lượt, hủy, Sửa lượt, bán gói, hủy gói, đặt / đóng / hoàn tất lịch hẹn): giờ của API khi handler bắt đầu, trước cả `BEGIN`, và có thể lệch đồng hồ DB vài ms (test `record_history.rs` phải so tập action thay vì thứ tự vì lẽ đó).
- Hậu quả: request 1 bắt đầu trước nhưng chờ khóa dòng mà request 2 đang giữ; request 2 commit; request 1 ghi sau. Dòng của request 1 vẫn xếp **trước**: chuỗi đổi tên A → B (2) → C (1) đọc thành "A → C", "C → B", tên cuối trên lịch sử không phải tên thật.
- Các reader dựa vào việc **mọi dòng của một transaction cùng một `at`** để biết đó là một thao tác: gộp một thao tác một dòng (ADR-0021 D9, `Row::folds_into`), sửa thưởng / phạt = hủy + ghi cùng khoảnh khắc (`history::edit_link`, K21), dòng phụ của lượt (K7), phân trang console "không cắt giữa một khoảnh khắc" (`platform::audit`).
- Reply của thao tác lượt / lịch hẹn đọc `edited` ("Đã sửa") từ `audit_log` **trong cùng transaction** (K4, K28), nên dòng audit của chính request phải nhìn thấy được trước khi reply được dựng.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. `clock_timestamp()` cho từng dòng lúc insert | Đơn giản | Các dòng của một transaction khác giờ nhau → gộp D9, `edit_link`, K7 vỡ |
| B. Cột sequence đơn điệu, xếp theo nó | Thứ tự insert tuyệt đối | Dữ liệu back-fill (seed) xếp theo lúc chạy seed chứ không theo kịch bản; `at` hiển thị vẫn lệch thứ tự; cần migration + đổi mọi reader |
| C. Cột `xact` (transaction id) để gộp + `clock_timestamp()` từng dòng | Đúng về lý thuyết | Đổi mọi reader gộp (SQL nhật ký theo trang, Rust), dòng cũ không có `xact` → hai luật song song |
| D. Ghi xong rồi `UPDATE at` lúc commit | Giữ insert ngay | Phải cho `app_rw` `UPDATE` bảng audit (append-only) hoặc hàm definer dựa `xmin`, gấp đôi WAL |
| E. **Hoãn ghi đến cuối transaction, một giờ cho cả transaction** | Giờ luôn sau mọi khóa của use case; giữ "một transaction = một khoảnh khắc"; không đổi reader; không migration cho đường Rust | Reply đọc `edited` trong cùng transaction phải ghi audit trước; đọc giữa chừng không được đòi `edited` |

## Decision
Chọn **E**.
1. `kernel::audit::record` / `record_platform` / `record_system` chỉ **ghi nhận** (id UUIDv7 lấy lúc ghi nhận, nên thứ tự trong một khoảnh khắc = thứ tự ghi nhận); `TenantTx::commit` ghi tất cả. Dòng đầu tiên lấy `clock_timestamp()` lúc insert (`RETURNING at`), mọi dòng khác của transaction mang đúng giờ đó.
2. `TenantTx::write_audit()` ghi sớm cho câu đọc cần thấy audit của chính transaction: `operations::repo::visits` và `appointments::repo::appointments` khi có `with_edited` (reply mang "Đã sửa"). Chỉ gọi khi use case đã ghi xong (sau mọi khóa dòng); đọc giữa chừng (bản bị thay của Sửa lượt, chuyển sang gói) dùng `peek_one`, không có `edited`.
3. Giờ chỉ được đặt trước bởi `Actor::at_clock` (công cụ back-fill, `app seed-demo`). `now` của `*_as_of` là đồng hồ nghiệp vụ (ngày, luật ngày đã khóa), **không** đóng dấu audit; `TenantTx::for_actor_at` bị bỏ. Seed truyền actor mang giờ kịch bản cho 8 lời gọi `*_as_of`.
4. Migration 0053: default của `audit_log.at` = `clock_timestamp()` cho các writer SQL bỏ trống `at` (hàm áp mẫu vai trò 0040 / 0052, thao tác tài khoản của operator), mỗi transaction một dòng cho mỗi spa.
5. Dòng cũ giữ nguyên (không đóng dấu lại): không khôi phục được thứ tự thật của các cặp request chồng nhau đã qua.

## Consequences
- Positive: thứ tự lịch sử = thứ tự thay đổi, cả khi hai request tranh nhau một dòng (test `kernel/tests/audit_order.rs`: request chờ khóa xếp sau; trước bản sửa đọc "C, B"); mọi request trong cùng phiên test giờ xếp đúng thứ tự gửi (test lịch sử có thể so thứ tự, `in_order`); giờ audit cùng một đồng hồ (DB), không còn lệch API ↔ DB.
- Negative / accepted trade-offs:
  - giờ hiện trên "Ghi bởi … · 10:30" là lúc ghi xong (muộn hơn `created_at` của bản ghi vài ms);
  - lỗi insert audit (hiếm: chỉ lỗi DB) xuất hiện ở `commit` thay vì ở dòng gọi `record`;
  - quy tắc mới cho người viết code: câu đọc `audit_log` trong transaction ghi phải gọi `write_audit()` trước và chỉ sau lần ghi cuối (ghi ở doc của `TenantTx::write_audit`, `repo::visits`, `repo::appointments`).
- Follow-ups: không.

## Revisit when
- Có use case ghi audit rồi còn khóa dòng khác **sau** khi dựng reply có `edited` (khi đó tách reply ra sau, hoặc chuyển sang C).
- Cần thứ tự giữa các transaction không đụng nhau chính xác hơn mili giây (khi đó thêm sequence, B, chỉ làm tie-break).
