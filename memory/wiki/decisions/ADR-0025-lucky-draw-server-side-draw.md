---
title: ADR-0025 Vòng quay may mắn quay ở server (OS RNG, khóa dòng chương trình), kết quả bất biến trừ một lần trả giải
status: accepted
date: 2026-11-27
deciders: LEAD + agent backend (EM giao "xem thử cho tôi", spec lucky-draw §8)
---

# ADR-0025: Vòng quay may mắn quay ở server, dưới khóa dòng chương trình, bằng OS RNG; kết quả chỉ được trả giải một lần

## Context
- Spec [lucky-draw](../product/specs/lucky-draw.md) (approved 2026-11-27): chủ spa quay thưởng tri ân, mỗi lượt **tự trừ** người trúng và một suất (L2, không có bước "Xác nhận"); người vắng mặt được **trả giải** (suất quay lại, người đó bị loại, L3); nhiều chương trình lưu trên server, mở lại được ở mọi máy.
- Bản HTML EM gửi chọn người trúng trong trình duyệt (`Math.random`) và lưu ở `localStorage`. Kết quả quay là thứ người tham gia tranh luận ("có gian không?"): ai sửa được request (DevTools, curl) thì không được chọn người trúng, không được gửi số ngẫu nhiên, không được trúng hai lần.
- Hai tab / hai máy cùng quay một chương trình (laptop quầy + TV trình chiếu, AC4.5): không ai trúng hai lần, không vượt số suất. Mạng chập chờn: bấm lại không được ra hai lượt (AC4.6).
- Kết quả là "ai trúng gì" của một sự kiện có thật: sửa / xóa lặng lẽ một kết quả làm mất dấu vết; nhưng xóa cả chương trình là quyền của chủ spa (L7, đường xóa dữ liệu cá nhân).

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Quay ở trình duyệt, gửi người trúng lên server lưu | Đơn giản, vòng quay dừng đúng chỗ người trúng | Client chọn người trúng: sửa request là chọn được ai trúng; server chỉ kiểm được "người đó hợp lệ", không kiểm được "ngẫu nhiên" |
| B. Server chọn bằng PRNG thường (`rand::thread_rng` mặc định / số thời gian) | Không cần thêm crate | Khó chứng minh không đoán được; seed theo thời gian đoán được |
| C. **Server chọn bằng OS CSPRNG (`getrandom`), đều nhau bằng rejection sampling, dưới `SELECT … FOR UPDATE` dòng chương trình, `Idempotency-Key` bắt buộc** | Client không ảnh hưởng được kết quả; không lệch modulo; các lượt của một chương trình nối tiếp nhau; retry không ra lượt thứ hai | Vòng quay trên màn hình chỉ là trang trí (dừng ở cánh ngẫu nhiên, không phải "ô" người trúng); mỗi lượt khóa chương trình (chấp nhận: một chương trình một người bấm) |
| D. Như C nhưng khóa bằng advisory lock / SERIALIZABLE | Không giữ khóa dòng | Advisory lock dễ quên ở một đường ghi; SERIALIZABLE phải retry, thêm code |

## Decision
Chọn **C**:
1. **Server là nơi duy nhất chọn người trúng.** `POST /lucky-draws/{id}/spins` chỉ nhận `{prize_id}` (`deny_unknown_fields`: gửi thêm `participant_id` → `422 body invalid`). Nhóm đủ điều kiện = người chưa có kết quả nào trong chương trình (kể cả kết quả đã trả) và giới tính hợp với giải, theo thứ tự nhập; chỉ số = `uniform_index(n)` trên `RandomSource` (production: `OsRandom` = `getrandom`, test tiêm nguồn cố định). Rejection sampling: từ 64 bit, bỏ các giá trị ≥ bội lớn nhất của `n` vừa `u64`, nên `word % n` không lệch. Nguồn ngẫu nhiên lỗi → `500`, không ghi gì, không bao giờ chọn mặc định.
2. **Mọi lệnh ghi của một chương trình khóa dòng `lucky_draw` trước** (`repo::lock_draw`, `FOR UPDATE`): quay, trả giải, thêm / sửa / xóa người, giải, đổi tên, xóa. Trong khóa: đọc giải + số đã trao, nhóm đủ điều kiện, `seq = max + 1`, ghi kết quả, `version + 1`, audit, commit. Hai lượt cùng lúc chờ nhau; lượt sau thấy lượt trước.
3. **Idempotency**: `Idempotency-Key` bắt buộc cho quay (`kernel::idempotency`, hash = method + route + id chương trình + body chuẩn hóa); bản ghi idempotency chỉ giữ `{result_id}`, replay đọc lại kết quả + chương trình hiện tại (không lưu tên người trong bản ghi idempotency).
4. **Kết quả bất biến trừ một lần trả giải** (0055): `app_rw` chỉ có `INSERT` và `UPDATE (forfeited_at, forfeited_by)` trên `lucky_draw_result`, không `DELETE`; trigger `lucky_draw_result_forfeit_only` chặn mọi `UPDATE` khác (kể cả schema owner) và mọi `DELETE` không đến từ cascade của chương trình; `UNIQUE (draw, participant)` (một người một kết quả), `UNIQUE (draw, seq)`; FK ba cột giữ giải / người của kết quả thuộc **cùng** chương trình, `ON DELETE RESTRICT` (giải / người có kết quả không xóa riêng được), xóa chương trình vẫn `CASCADE` cả cây.

## Consequences
- Positive: không ai sửa được ai trúng từ trình duyệt; không trúng hai lần, không vượt suất (test tranh nhau thật `lucky_draw.rs::concurrent_spins_take_turns_and_never_over_award`); retry an toàn; kết quả không bị sửa lặng lẽ; job hằng ngày kiểm thêm `lucky_draw_awarded_within_quantity` và `lucky_draw_winner_gender` (`app.invariant_violations`, 13 phép kiểm).
- Negative / accepted trade-offs: vòng quay trên web không "dừng đúng ô người trúng" (không có ô theo người, chỉ 8 cánh trang trí); muốn sửa một kết quả quay nhầm thì chỉ có "trả giải" (người đó bị loại) hoặc xóa cả chương trình; mỗi chương trình chỉ một lượt ghi một lúc (đủ cho một sự kiện, không phải tải lớn).
- Follow-ups: không có (quay nhiều người một lần, trọng số: ngoài phạm vi spec §9).

## Revisit when
Có yêu cầu quay nhiều người một lần, quay có trọng số, hoặc chương trình công khai cho khách tự quay (khi đó cần thêm chống lạm dụng theo khách, không chỉ khóa theo chương trình).
