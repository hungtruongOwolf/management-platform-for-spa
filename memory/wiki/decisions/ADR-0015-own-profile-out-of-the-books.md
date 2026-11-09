---
title: ADR-0015 Hồ sơ của chính mình nằm ngoài sổ quản lý; hồ sơ "Quản lý" chưa gắn chỉ chủ spa ghi
status: accepted (EM 2026-11-08; F19-3 superseded by the update below)
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
- Quản lý dùng `staff.read_own` (role Quản lý có từ 0017, **không migration**) cho "Công và lương của tôi" (`/nhan-vien/cua-toi`) và "Khách của tôi".
- `RecordScope::load` thêm mọi hồ sơ có vị trí quản lý (`org::profile::is_manager_position`) vào `owner_only`; chủ spa thấy cảnh báo hồ sơ / tài khoản quản lý chưa gắn (`GET /staff/accounts` → `is_manager`).

## Consequences
- Positive: dữ liệu của quản lý về chính mình chỉ đi qua `/me/*` (một đường, cùng allowlist với KTV); F19 đóng ở server.
- Negative / accepted trade-offs: một KTV được đặt vị trí "Quản lý" sẽ chỉ chủ spa ghi; quản lý đặt vị trí khác (ví dụ "Thu ngân") mà chưa gắn thì vẫn như KTV — cảnh báo tài khoản quản lý chưa gắn hồ sơ giúp chủ spa thấy.
- Follow-ups: nếu sau này role tùy biến cho quản lý xem Bảng lương (`payroll.read`), cân nhắc áp cùng quy tắc `lists` cho `/payroll`.

## Revisit when
Spa có nhiều chi nhánh / nhiều tầng quản lý (quản lý ca ghi cho quản lý chi nhánh), hoặc có trường vai trò có cấu trúc thay cho vị trí chữ tự do.

## Update 2026-11-08: vai trò lấy từ tài khoản, không từ chức danh ([raw](../../raw/2026-11-08-em-decision-role-by-account.md))
EM: *"chúng ta chia theo role … có các role as account thì theo cái nào thì làm cái đó"*; quản lý giữ "Khách của tôi" (*"quản lý cũng phải làm cho khách như KTV"*).
- **Bỏ F19-3** (quy tắc chức danh "Quản lý" → chỉ chủ spa ghi, `is_manager_position`). Chức danh không quyết định gì. Ai giám sát = hồ sơ gắn tài khoản có role cấp `attendance.record` / `evaluation.record` / `staff.manage` (quyết định C, giữ nguyên). Hồ sơ chưa có tài khoản = KTV, như trước.
- **Thay bằng F19-2 hẹp:** tài khoản có quyền ghi sổ nhân viên mà không có `staff.manage` (quản lý) và **chưa gắn hồ sơ** không ghi gì (chấm công, khen/phàn nàn, thưởng/phạt — thêm, sửa, xóa —, đánh giá): `403 staff.link_required`, cả batch không ghi. Chủ spa miễn. Lý do: app không biết hồ sơ nào là của người đó nên không chặn được tự ghi cho mình. Sổ trả `link_required` để UI nói rõ "nhờ chủ spa gắn tài khoản"; chủ spa thấy cảnh báo tài khoản quản lý chưa gắn (`GET /staff/accounts` → `is_manager`, từ role) và một dòng nhắc ai còn chưa có tài khoản. Hậu quả chấp nhận: quản lý chưa gắn chỉ xem sổ cho tới khi được gắn (EM: mọi người sẽ có tài khoản).
- **Chức danh theo vai trò:** hồ sơ đã gắn tài khoản hiện tên role của tài khoản đó làm vị trí, ở mọi chỗ đọc (`org::repo`: danh sách chọn KTV, Nhân sự, sổ, phiếu lương), suy ra lúc đọc nên không bao giờ lệch; `position_from_account: true`, ô "Vị trí" chỉ đọc. Chữ tự do của chủ spa chỉ dùng khi chưa gắn và được giữ nguyên (sửa hồ sơ khi đang gắn không ghi đè) để hiện lại khi bỏ gắn. Chọn cách suy ra lúc đọc thay vì chép role vào `staff.position` khi gắn: không cần migration (số 0031 vì vậy bị bỏ trống), không cũ khi role đổi.
- Test: `staff.rs::a_manager_without_a_linked_profile_records_nothing` (mọi route ghi, chủ spa miễn, đã gắn vẫn bị chặn cho chính mình / quản lý khác, chức danh "Quản lý" chưa có tài khoản = KTV; mutation-check), `::a_linked_profile_shows_its_account_role`; thay `an_unlinked_manager_profile_is_the_owners_to_record` và unit `manager_positions_are_recognised_in_any_spelling` (đã xóa).
