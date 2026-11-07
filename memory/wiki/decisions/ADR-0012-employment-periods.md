---
title: ADR-0012 Employment is a list of periods; ending employment is soft
status: accepted (lead's decision — EM to review)
date: 2026-11-07
deciders: agent, EM to confirm
---

# ADR-0012: Quá trình làm việc là danh sách các đợt; nghỉ việc là hủy mềm

## Context
EM 2026-11-07 ([raw](../../raw/2026-11-07-em-flow-review-hr-latency.md) mục 9): "không có quản lý nhân sự… để sau này chúng tôi biết người đó làm bao lâu rồi cần remove hay add vào list của spa… bây giờ là như hardcode cứng".

Trước 0025, hồ sơ chỉ có một cặp `joined_on` / `left_on` + `status` (0005). Nghỉ việc = sửa ô ngày trong form; nhận lại làm thì phải xóa `left_on`, tức là mất dấu đợt trước. Tháng của đợt trước sẽ bị coi là "chưa vào làm" (`not_employed`), bảng công và bảng lương tháng cũ (chưa chốt) mất người đó. Không có lý do nghỉ, không có thâm niên. Mọi bảng lương, lượt khách, hoa hồng đều trỏ vào `staff.id`, nên xóa hồ sơ là không thể.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Giữ một cặp ngày, nhận lại = ghi đè | Không migration | Mất lịch sử đợt trước; tháng cũ sai `not_employed`; không tính được thâm niên thật |
| B. Bảng `staff_employment` (mỗi đợt một dòng), `staff.status/joined_on/left_on` là bản sao của đợt mới nhất | Lịch sử đầy đủ, khoảng nghỉ giữa hai đợt đúng là "không làm"; thâm niên = tổng các đợt; mọi bộ lọc `status = 'active'` (picker, `require_active`) giữ nguyên | Hai nơi lưu (bảng đợt + bản sao): phải ghi cùng transaction |
| C. Event log (hired/ended/rehired) rồi suy ra | Linh hoạt nhất | Phức tạp cho 3–8 người; mọi truy vấn phải fold sự kiện |
| D. Xóa cứng / ẩn bằng cờ | Đơn giản | Phá FK của lương, lượt, hoa hồng; trái nguyên tắc sổ sách chỉ thêm |

## Decision
Chọn **B**.
- `app.staff_employment(started_on, ended_on, end_reason, ended_by)`, RLS tenant, `EXCLUDE` không chồng đợt, tối đa một đợt mở, trigger: đợt đã kết thúc không bao giờ đổi, không DELETE (app_rw không có quyền). Trigger `AFTER INSERT ON app.staff` mở đợt đầu cho mọi cách tạo hồ sơ (API, CLI `add-staff`, demo, test). Backfill từ `joined_on/left_on`.
- `org::employment::Employment` là nơi duy nhất trả lời "ngày D / tháng M người này có làm không" và thâm niên (`tenure`: số ngày + số tháng tròn, cộng các đợt tới hôm nay; `None` khi thiếu ngày bắt đầu). `hr` (bảng công, đánh giá, bảng lương, `calc::summarize`, thưởng gắn bó theo tỷ lệ ngày) dùng nó thay cho `joined_on/left_on`.
- Nghỉ việc (`POST /staff/{id}/end`, chủ spa): ngày làm cuối ≤ hôm nay, ≥ ngày vào đợt, có lý do; từ chối nếu còn chấm công / lượt khách sau ngày đó (`422 has_later_records`) hoặc còn lịch hẹn mở gán cho người đó sau ngày đó (`409 staff.has_upcoming_bookings`). Tài khoản đăng nhập đã gắn bị **khóa** ở spa này (`membership.status = 'disabled'`); hồ sơ gắn tài khoản chủ spa thì phải bỏ gắn trước; không tự cho mình nghỉ.
- Nhận lại làm (`POST /staff/{id}/rehire`): đợt mới bắt đầu sau ngày nghỉ của đợt trước; mở lại tài khoản đã gắn.

## Consequences
- Positive: lịch sử đầy đủ cho báo cáo và phiếu lương cũ; người đã nghỉ tự ra khỏi mọi picker (lọc `status`) và mọi tháng sau (lọc theo đợt); thâm niên tính từ ngày thật.
- Negative / accepted trade-offs: sửa ngày kết thúc của một đợt đã đóng là không được (trigger) — ghi nhầm thì nhận lại làm rồi cho nghỉ đúng ngày, hoặc sửa tay bằng migration dữ liệu. Ngày vào của đợt hiện tại vẫn sửa được (`PUT /staff/{id}`).
- Follow-ups: chi trả thưởng gắn bó theo mốc 12/24 tháng và tăng lương 10 %/năm (spec §8, ngoài phạm vi) sẽ đọc `Employment::tenure`; chưa có quy tắc nào trong code đang hard-code thâm niên.

## Revisit when
Spa cần nhiều chi nhánh (đợt theo chi nhánh), hợp đồng có kỳ hạn (gói 12/24 tháng) cần ngày ký riêng, hoặc cần sửa ngày nghỉ của đợt đã đóng thường xuyên.
