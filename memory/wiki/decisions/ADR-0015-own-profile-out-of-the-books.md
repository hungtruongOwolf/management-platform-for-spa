---
title: ADR-0015 Hồ sơ của chính mình nằm ngoài sổ quản lý; hồ sơ "Quản lý" chưa gắn chỉ chủ spa ghi
status: accepted (EM to review)
date: 2026-11-08
deciders: EM, agent
---

# ADR-0015: Hồ sơ của chính mình nằm ngoài sổ quản lý; hồ sơ "Quản lý" chưa gắn chỉ chủ spa ghi

## Context
- EM 2026-11-08 ([raw](../../raw/2026-11-08-em-fix-all-and-manager-self-view.md)): tài khoản quản lý không cần thấy chính mình trong "Nhân viên"; muốn xem thì xem như KTV.
- Quyết định C (spec staff-management §11): chỉ chủ spa ghi chấm công / khen-phàn nàn / thưởng-phạt / đánh giá cho quản lý. Quyết định này biết "hồ sơ của mình" **chỉ** qua liên kết hồ sơ ↔ tài khoản (`staff.user_id`). Live test F19: hồ sơ quản lý chưa gắn → quản lý tự chấm "Đi làm" cho mình được, vì hồ sơ chưa gắn được coi là KTV.
- Non-negotiable 4 (entitled data): ẩn ở UI không đủ.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Ẩn dòng của mình ở UI | Không đổi API | Dữ liệu vẫn tới trình duyệt; trái non-negotiable 4 |
| B. Server bỏ hồ sơ gắn với session khỏi mọi sổ đọc của người không có `staff.manage`; xem của mình qua `/me/*` (quyền `staff.read_own` sẵn có) | Một quy tắc ở server, test được; không migration; quản lý xem lương mình đúng một đường | `GET /evaluations/history` của chính mình đổi từ `200` sang `404` (hành vi cũ có test) |
| C. Thêm permission mới "xem sổ của mình" | Tách bạch | Thêm role data, migration, không ai cần |
| F19-1. Coi hồ sơ chưa gắn là KTV (như cũ) | Không đổi | Quản lý chưa gắn tự ghi cho mình |
| F19-2. Chặn mọi ghi của quản lý chưa gắn | An toàn nhất | Khóa luôn việc chấm công KTV hằng ngày cho tới khi chủ spa gắn |
| F19-3. Hồ sơ có vị trí là quản lý luôn chỉ chủ spa ghi (gắn hay chưa) + cảnh báo chủ spa | Deny by default đúng chỗ rủi ro, quản lý vẫn chấm KTV; khớp tinh thần quyết định C | Dựa vào vị trí (chữ tự do): đặt sai vị trí thì sai; gập dấu/hoa thường để giảm sai |

## Decision
Chọn **B + F19-3**.
- `hr::scope::RecordScope::lists` + `org::profile::hidden_from`: hồ sơ gắn với tài khoản của người gọi không có trong `/attendance`, `/daily-reviews`, `/evaluations`, `/adjustments`, `/staff/profiles`; `/staff/{id}` và `/evaluations/history` của chính mình → `404`. Chủ spa (`staff.manage`) thấy mọi người.
- Quản lý dùng `staff.read_own` (role Quản lý có từ 0017, **không migration, 0030 không dùng**) cho "Công và lương của tôi" (`/nhan-vien/cua-toi`) và "Khách của tôi".
- `RecordScope::load` thêm mọi hồ sơ có vị trí quản lý (`org::profile::is_manager_position`) vào `owner_only`; chủ spa thấy cảnh báo hồ sơ / tài khoản quản lý chưa gắn (`GET /staff/accounts` → `is_manager`).

## Consequences
- Positive: dữ liệu của quản lý về chính mình chỉ đi qua `/me/*` (một đường, cùng allowlist với KTV); F19 đóng ở server.
- Negative / accepted trade-offs: một KTV được đặt vị trí "Quản lý" sẽ chỉ chủ spa ghi; quản lý đặt vị trí khác (ví dụ "Thu ngân") mà chưa gắn thì vẫn như KTV — cảnh báo tài khoản quản lý chưa gắn hồ sơ giúp chủ spa thấy.
- Follow-ups: nếu sau này role tùy biến cho quản lý xem Bảng lương (`payroll.read`), cân nhắc áp cùng quy tắc `lists` cho `/payroll`.

## Revisit when
Spa có nhiều chi nhánh / nhiều tầng quản lý (quản lý ca ghi cho quản lý chi nhánh), hoặc có trường vai trò có cấu trúc thay cho vị trí chữ tự do.
