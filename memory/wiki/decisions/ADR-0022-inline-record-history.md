---
title: ADR-0022 Lịch sử trên từng bản ghi (công tắc spa-local ở console admin) + tạm ẩn Nhật ký thao tác
status: accepted
date: 2026-11-11
deciders: EM, agent
---

# ADR-0022: Hiện "ai ghi, ai sửa" trên từng bản ghi, công tắc theo vai trò do operator bật / tắt; màn Nhật ký thao tác ẩn sau cờ mặc định tắt

Sửa một phần [ADR-0021](ADR-0021-activity-log-for-the-owner.md) (D1 "không cờ tính năng"; bối cảnh "không hiện ai làm trên các màn khác"), [ADR-0018](ADR-0018-platform-admin-console.md) D6 và [ADR-0003](ADR-0003-multi-tenancy.md) ("chủ spa xem được việc của operator", xem Consequences). Đảo quyết định EM sáng 2026-11-11 ([raw](../../raw/2026-11-11-em-actor-trace-data-only.md)). **Bản sửa sau review độc lập 2026-11-11** (spec [record-history](../product/specs/record-history.md), Review lượt 3); các điểm đánh dấu LEAD là quyết định của lead.

## Context
- EM 2026-11-11 chiều ([raw](../../raw/2026-11-11-em-decision-inline-record-history.md)):
  1. hiện dòng nhỏ "Ghi bởi Hà · 15:10 · đã sửa 1 lần" trên bản ghi, bấm mở lịch sử, mặc định đóng;
  2. bật / tắt được, mặc định hiện cho chủ spa; **công tắc chỉ ở admin**;
  3. màn "Nhật ký thao tác" giữ nhưng **tạm ẩn ngay** ("tạm thời ẩn … off show cái nhật kí thao tác đi");
  4. danh sách chỉ dấu nhỏ; tên + lịch sử ở chi tiết.
- Có sẵn: `audit_log` + `reporting::activity` (allowlist, gộp D9); vai trò là dữ liệu, operator sửa theo spa / theo mẫu (ADR-0018 D5); cờ tính năng theo spa, mặc định bật (ADR-0018 D4); test `administration_is_never_a_switchable_feature`.
- Ràng buộc: CLAUDE.md non-negotiable #1–#4.

## Options considered
**Công tắc lịch sử**
| Option | Pros | Cons |
|---|---|---|
| A. Cài đặt của spa do chủ spa bật | Chủ spa tự quyết | EM bác: chỉ admin |
| B. Cờ tính năng theo spa | Có sẵn | Không theo vai trò |
| C. Bảng `(tenant, role, visible)` riêng | Theo vai trò | Nguồn quyền song song với `role_permission` |
| D. Permission thường `record.history.read` qua trình sửa vai trò | Có sẵn hết | Đổi công tắc làm vai trò "lệch mẫu"; sửa mẫu sau âm thầm ghi đè công tắc của spa theo mẫu |
| **E. Permission `record.history.read` loại spa-local** (LEAD) | Như D, nhưng mẫu chỉ quyết giá trị ban đầu; áp mẫu / "Về mẫu" / so sánh theo mẫu bỏ qua nó; một hàm DB riêng chỉ đổi đúng nó | Thêm khái niệm "spa-local" vào hai hàm vai trò (0052) |

**Màn Nhật ký thao tác**
| Option | Pros | Cons |
|---|---|---|
| F. Xóa màn | Gọn | EM: "cứ giữ" |
| G. Rút `audit.read` | Không code | Phá owner floor + test quản trị |
| H. Ẩn sau cờ nhưng chỉ sau khi mọi màn có lịch sử | Không hồi quy | Trái lời EM "tạm thời ẩn" |
| **I. Cờ `activity_log` mặc định tắt từ đợt 1, chỉ sở hữu route `/audit-log`; đợt 1 gồm cả gói + hồ sơ khách** (LEAD) | Đúng lời EM; giữ code + quyền; bật lại theo spa | Cửa sổ hồi quy cho lịch hẹn, chấm công, thưởng/phạt, danh mục, quy tắc lương, việc của operator (ghi ở plan) |

**Hình dạng API**
| Option | Pros | Cons |
|---|---|---|
| J. Nhúng người ghi vào mọi DTO | Không request thêm | Nhiều chỗ kiểm quyền; danh sách mang tên |
| **K. Endpoint chung `/record-history/...` + chỉ `edited` ở danh sách** | Một chỗ kiểm quyền | Một request khi mở chi tiết |

## Decision
Chọn **E + I + K**.
1. **`record.history.read`, spa-local.** Mẫu: owner có, manager không (chỉ áp khi tạo spa). Không trong owner floor; không bao giờ cho KTV (`422 role.not_for_technician`, cả Rust lẫn hàm DB). 0052 sửa `platform_apply_role_template` / `platform_set_tenant_role` để bỏ qua và giữ quyền này. Operator đổi qua `PUT /admin/tenants/{id}/record-history/{role}` → hàm `platform_set_record_history` (permission và vai trò ∈ {owner, manager} viết cứng, khóa dòng role, không đụng `follows_template`). Audit spa `role.permissions_changed` (platform_admin) + nền tảng `platform.record_history.set`. Không có công tắc trong app của spa.
2. **Cờ `activity_log`** (registry có `default_enabled`), mặc định **tắt từ đợt 1**. Chỉ sở hữu route `/api/v1/audit-log` → `403 feature.disabled`; `audit.read` giữ nguyên. Mặc định nằm trong code; `session_actor` / `member_access` trả cờ đang bật; `platform_set_feature` trả null khi chưa có dòng.
3. **Đọc lịch sử.** `GET /record-history/*`:
   - đổi neo (hóa đơn dịch vụ lẻ → lượt, hóa đơn gói → gói, hoàn tiền → gói), rồi kiểm quyền đọc của **loại neo cuối** trước khi đọc;
   - thành viên lịch sử chọn theo **id dòng audit** cho từng loại;
   - chỉ lượt / gói mới gộp theo D9;
   - lọc theo quyền màn gốc (tiền ← `sale.read`; ghi chú ← `customer.health_notes.read`, dòng chỉ đổi ghi chú bị bỏ);
   - SĐT cũ / mới chỉ gửi cho `audit.read` (người khác: "đã đổi", không giá trị);
   - "Ghi bởi" chỉ lấy từ thao tác tạo.
4. **"Đã sửa" hẹp.** Chỉ tính khi điều đã ghi bị đổi: lượt đổi giờ / KTV, Sửa lượt, chuyển sang gói; lịch hẹn đổi giờ / KTV. Upsell, gia hạn, dùng gói hết hạn, ghi chú… là vòng đời.
   - Một định nghĩa dùng cho cả `edited` lẫn "đã sửa N lần".
   - Chip bản bị thay đổi thành "Đã thay" cho mọi vai trò. Mỗi hàng chỉ một chip, ưu tiên Đã hủy > Đã thay > Đã sửa.
5. Chỗ đã hiện "ai làm" giữ nguyên (hồ sơ NV, đánh giá tháng, chốt lương, gọi lại). Thưởng/phạt có thêm lịch sử (đợt 2).

## Consequences
- Positive:
  - Chủ spa truy "ai ghi, ai sửa" tại chỗ.
  - EM điều khiển theo từng spa qua operator. Công tắc không bị sửa mẫu ghi đè và không làm vai trò lệch mẫu.
  - Chỉ một chỗ kiểm quyền; thành viên lịch sử chọn chính xác theo id dòng audit.
  - Không bảng mới, không đổi cách ghi audit.
- Negative / accepted trade-offs:
  - **Sửa ADR-0018 D6 / ADR-0003.** Khi cờ `activity_log` tắt (mặc định), chủ spa **không** xem được trên UI việc đội vận hành làm với spa: khóa / mở tài khoản, đặt lại mật khẩu / MFA, sửa vai trò, cờ, ngưng spa. Dữ liệu vẫn ở `audit_log` (`actor_kind = platform_admin`) và nhật ký nền tảng. Operator bật cờ khi chủ spa cần. Không làm màn riêng cho việc này (LEAD).
  - **Cửa sổ hồi quy** từ đợt 1 tới đợt 2 / 3: lịch hẹn, chấm công, khen / phàn nàn, thưởng / phạt, danh mục, quy tắc lương, cài đặt lịch hẹn chưa có lịch sử tại chỗ. Giảm thiểu như trên ([plan](../plans/record-history.md)).
  - Thêm khái niệm spa-local vào hai hàm vai trò. Hằng spa-local nằm ở cả Rust và SQL; một test giữ hai nơi khớp nhau.
  - Quản lý được bật thấy tên cũ của khách, không thấy SĐT cũ.
  - Cờ đầu tiên mặc định tắt: một test khóa giá trị mặc định của từng cờ.
  - Khoảng hở deploy 0052 (binary cũ): operator không ghi ở console ([architecture §8](../architecture/record-history.md)).
- Follow-ups:
  - `customer.anonymize` (chưa build) vẫn phải xóa tên / SĐT trong audit (ADR-0021).
  - Khi bảng audit lớn: tạo index bằng `CONCURRENTLY` ở migration riêng.

## Revisit when
- EM muốn chủ spa tự bật / tắt cho quản lý trong app, hoặc cần quyền theo từng người.
- Quản lý cần thấy SĐT cũ.
- Chủ spa cần thấy việc của đội vận hành mà không bật cả màn nhật ký.
- Lịch sử một bản ghi thường vượt 100 dòng.
- Màn Nhật ký thao tác cần bật lại mặc định cho mọi spa.
