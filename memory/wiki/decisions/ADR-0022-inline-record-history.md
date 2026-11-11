---
title: ADR-0022 Lịch sử trên từng bản ghi (công tắc ở console admin) + tạm ẩn Nhật ký thao tác
status: accepted
date: 2026-11-11
deciders: EM, agent
---

# ADR-0022: Hiện "ai ghi, ai sửa" trên từng bản ghi, công tắc theo vai trò do operator bật / tắt; màn Nhật ký thao tác ẩn sau cờ mặc định tắt

Sửa một phần [ADR-0021](ADR-0021-activity-log-for-the-owner.md) (D1 "không cờ tính năng"; bối cảnh "không hiện ai làm trên các màn khác"). Đảo quyết định EM sáng 2026-11-11 ([raw](../../raw/2026-11-11-em-actor-trace-data-only.md)).

## Context
- EM 2026-11-11 chiều ([raw](../../raw/2026-11-11-em-decision-inline-record-history.md)):
  1. hiện dòng nhỏ "Ghi bởi Hà · 15:10 · đã sửa 1 lần" trên bản ghi, bấm mở lịch sử, mặc định đóng;
  2. bật / tắt được, mặc định hiện cho chủ spa; **công tắc chỉ ở admin**, không ở tài khoản spa / quản lý;
  3. màn "Nhật ký thao tác" giữ nhưng tạm ẩn, tắt từ admin;
  4. danh sách chỉ dấu nhỏ; tên + lịch sử ở chi tiết.
- Có sẵn: `audit_log` + read model `reporting::activity` (allowlist trường, gộp một thao tác một dòng, ADR-0021 D9); vai trò là dữ liệu, operator sửa vai trò theo spa có step-up + audit hai phía (ADR-0018 D5); cờ tính năng theo spa, mặc định bật, chỉ operator ghi (ADR-0018 D4); test `administration_is_never_a_switchable_feature` cấm cờ sở hữu `audit.read`.
- Ràng buộc: server là ranh giới tin cậy duy nhất; deny by default; chỉ gửi dữ liệu người gọi được xem (CLAUDE.md #1–#4).

## Options considered
**Công tắc lịch sử**
| Option | Pros | Cons |
|---|---|---|
| A. Cài đặt của spa (`tenant.settings`) do chủ spa bật ở Cài đặt | Chủ spa tự quyết | EM bác: "không phải ở account của spa" |
| B. Cờ tính năng theo spa | Có sẵn UI + audit | Theo spa, không theo vai trò: không thể "chủ spa bật, quản lý tắt" |
| C. Bảng cài đặt mới `(tenant, role, visible)` đọc trong `session_actor` | Theo vai trò | Thêm một nguồn quyền song song với role_permission; phải tự map vai trò → quyền |
| D. **Permission mới `record.history.read`**, operator bật / tắt theo vai trò (panel riêng + trình sửa vai trò) | Đúng "permission là dữ liệu"; kiểm bằng `Actor::require`; step-up, audit, mẫu vai trò cho spa mới có sẵn; KTV chặn bằng rào chung | Đổi một quyền làm vai trò của spa thành "riêng" (`follows_template = false`) như mọi lần sửa vai trò riêng |

**Màn Nhật ký thao tác**
| Option | Pros | Cons |
|---|---|---|
| E. Xóa màn | Gọn | EM: "cứ giữ" |
| F. Rút `audit.read` khỏi chủ spa | Không code mới | Phá owner floor + test quản trị; `audit.read` còn là quyền đọc việc operator làm với spa |
| G. **Cờ `activity_log` mặc định tắt, chỉ sở hữu route `/audit-log`** | Giữ code + quyền; bật lại cho từng spa trong 1 phút; dùng guard route có sẵn | Cơ chế cờ cần thêm "mặc định tắt" |

**Hình dạng API**
| Option | Pros | Cons |
|---|---|---|
| H. Nhúng `{created_by, created_at, edits}` vào mọi DTO danh sách + chi tiết | Không request thêm | Đổi ~10 DTO, mỗi chỗ một lần kiểm quyền (dễ sót), danh sách mang tên người (trái câu 3 của EM) |
| I. **Endpoint chung `GET /record-history/{loại}/{id}` + chỉ `edited` trong danh sách** | Một chỗ kiểm quyền + lọc trường; dùng lại `ActivityEntry` + `fold`; danh sách không mang tên | Một request khi mở chi tiết (song song với request chi tiết) |

## Decision
Chọn **D + G + I**.
1. **`record.history.read`**: chủ spa có trong mẫu (mặc định hiện), quản lý không (mặc định ẩn); không trong owner floor (tắt được cho chủ spa); không cấp được cho KTV (`422 role.not_for_technician`); không có công tắc nào trong app của spa. Operator đổi bằng panel "Lịch sử trên từng bản ghi" ở trang spa (endpoint `PUT /admin/tenants/{id}/record-history/{role}`, hàm DB `platform_set_role_permission` khóa dòng role, đổi đúng một permission) hoặc trình sửa vai trò / mẫu vai trò. Audit spa `role.permissions_changed` (platform_admin) + nền tảng `platform.record_history.set`.
2. **Cờ `activity_log`**: registry có thêm `default_enabled`; cờ này mặc định **tắt**, chỉ sở hữu route `/api/v1/audit-log` → `403 feature.disabled`; `audit.read` giữ nguyên (owner-only + owner floor). Mặc định nằm ở code (không chèn dòng `tenant_feature`); `session_actor` trả thêm danh sách cờ bật; `platform_set_feature` trả null khi chưa có dòng.
3. **Đọc**: `GET /record-history/*` dùng `audit_log`, neo theo loại (chuỗi Sửa lượt, hóa đơn → lượt, hoàn tiền → gói, ô chấm công theo nhân viên + ngày), gộp bằng `fold_rule` của ADR-0021 D9, lọc theo quyền đọc của màn gốc (tiền ← `sale.read`, ghi chú ← `customer.health_notes.read`, lương ← `payroll.read`), SĐT cũ của khách che giữa cho người không có `audit.read`. Danh sách chỉ thêm `edited` (vắng mặt khi không có quyền).
4. Chỗ đã hiện "ai làm" từ trước (hồ sơ NV, thưởng/phạt, đánh giá tháng, chốt lương, gọi lại) giữ nguyên.

## Consequences
- Positive:
  - Chủ spa truy "ai ghi, ai sửa" tại chỗ; EM điều khiển theo từng spa qua operator mà không deploy.
  - Một endpoint, một bảng `match` quyền → dễ review, test matrix phủ từng loại × vai trò.
  - Không bảng mới, không đổi cách ghi audit; màn nhật ký bật lại được bất cứ lúc nào.
- Negative / accepted trade-offs:
  - Bật / tắt cho một spa làm vai trò đó thành "quyền riêng" (mẫu đổi sau không tự áp; console nói rõ, "Về mẫu" có sẵn).
  - Quản lý được bật thấy tên cũ của khách và các thay đổi trong phạm vi màn họ đã mở được; SĐT cũ chỉ che giữa (rủi ro chấp nhận, xem threat model).
  - Khi màn nhật ký tắt, chủ spa không còn chỗ xem việc đội vận hành làm với spa (vẫn có trong `audit_log` + nhật ký nền tảng); bật lại cờ khi cần.
  - Cờ đầu tiên mặc định tắt: ai đổi `default_enabled` sau này sẽ đổi hành vi của mọi spa chưa có dòng → test khóa giá trị mặc định của từng cờ.
- Follow-ups:
  - Gộp phần làm giàu tên của `activity::list` và `rows_by_ids` thành một (test đối chiếu tạm thời giữ hai bản khớp).
  - `customer.anonymize` (chưa build) vẫn phải xóa tên / SĐT trong audit (ADR-0021).

## Revisit when
EM muốn chủ spa tự bật / tắt cho quản lý trong app; cần quyền theo từng người chứ không theo vai trò; quản lý cần thấy SĐT cũ đầy đủ; số dòng audit của một bản ghi thường vượt 100; màn Nhật ký thao tác cần bật lại mặc định cho mọi spa.
