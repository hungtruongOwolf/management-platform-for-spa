---
title: ADR-0021 Nhật ký thao tác cho chủ spa + giữ tên / SĐT cũ của khách trong audit
status: accepted
date: 2026-11-11
deciders: EM, agent
---

# ADR-0021: "Nhật ký thao tác" cho chủ spa, audit giữ tên / SĐT cũ → mới của khách

## Context
- Mọi thao tác ghi đều đã vào `app.audit_log` (ai, lúc nào, before/after, lý do; migration 0001), nhưng màn hình chỉ hiện "ai làm" ở vài chỗ.
- EM 2026-11-11 ([raw 1](../../raw/2026-11-11-em-actor-trace-data-only.md), [raw 2](../../raw/2026-11-11-em-decision-activity-log-and-customer-old-values.md)):
  - **Không** hiện "ai làm" trên các màn hiện có.
  - Chủ spa cần **một chỗ** dễ tra: màn "Nhật ký thao tác", lọc theo người, loại việc, ngày; mỗi dòng là một câu kiểu "Hà hủy lượt của chị Mai · 15:10 · lý do: khách đổi ý".
  - Đổi tên hoặc SĐT khách thì lưu thêm **giá trị cũ** ("0905… → 0912…").
- Ràng buộc có sẵn:
  - [ADR-0019](ADR-0019-audit-decisions-2026-11-09.md) D13: audit khách không giữ PII rõ (chỉ cờ có/không + tên trường đã đổi), vì audit giữ mãi (EM Q-DB4) và sống lâu hơn hồ sơ đã ẩn danh.
  - Dữ liệu sức khỏe (ghi chú buổi, lưu ý sức khỏe, ghi chú gói) không bao giờ vào audit dạng rõ (`[redacted]` / cờ `has_*`).
  - `audit.read` đã có trong code, seed cho chủ spa, nằm trong "owner floor" (0040), nhưng chưa có endpoint spa nào dùng.
- Seed demo (`app seed-demo`) ghi 12 tháng lịch sử qua use case, nhưng `audit_log.at` = đồng hồ DB, nên mọi dòng audit lịch sử mang giờ chạy seed.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Server trả câu tiếng Việt sẵn | Web đơn giản | Chữ nằm trong Rust; khó link tên khách; đổi chữ phải deploy API |
| B. **Server trả mục có cấu trúc, web ghép câu** | Server quyết định dữ liệu nào được ra (allowlist từng action); web lo chữ, link, định dạng tiền / giờ | Hai nơi phải biết danh sách action → cần cơ chế bắt thiếu |
| C. Màn đọc thẳng `before` / `after` | Ít code | Lộ mọi thứ writer từng ghi (kể cả sau này lỡ ghi ghi chú), không kiểm soát được |
| D (tên/SĐT). Giữ D13, không lưu giá trị cũ | Không PII trong audit | EM đã chọn lưu |
| E (tên/SĐT). **Lưu cũ → mới chỉ cho tên + SĐT khi đổi** | Đúng yêu cầu, phạm vi PII nhỏ nhất | Audit có PII: ẩn danh sau này phải xóa cả ở audit |

## Decision
Chọn **B + E**.

1. **Chỉ chủ spa.** `GET /api/v1/audit-log` cần `audit.read`. `audit.read` thành permission **owner-only** (`platform::roles::OWNER_ONLY`, cạnh `staff.manage`): operator không cấp được cho quản lý / KTV (`422 role.owner_only_permission`). Migration 0051 xóa mọi dòng `audit.read` đã lưu cho role khác (mẫu + từng spa). Quản lý / KTV → `403`; guard route ở web chỉ là UX. Người đã nghỉ không đổi (chỉ `staff.read_own_past`). Không cờ tính năng: `audit.read` thuộc nhóm quản trị không tắt được (`kernel::feature`, test `administration_is_never_a_switchable_feature`).
2. **Server trả mục có cấu trúc, web ghép câu.** Mỗi mục:
   - `at`, `day` (ngày theo giờ spa);
   - người làm: tên tài khoản; `platform` → "Đội vận hành" (không bao giờ gửi tên / id operator); `system` → "Hệ thống";
   - `kind` (nhóm lọc), `action` (key);
   - chủ thể resolve lúc đọc bằng join: khách (tên hiện tại, giới tính để xưng "chị / anh", link hồ sơ), nhân viên, dịch vụ, mẫu gói, tài khoản, vai trò; khách đã ẩn danh hoặc bản ghi không còn → không tên, không link (web: "một khách (đã xóa thông tin)");
   - `changes` (trường, định dạng, cũ, mới) theo **allowlist từng action** (`reporting::activity::fields`), tiền, giảm giá, `detail`, `reason`.
   - Không bao giờ có `before` / `after` thô, ghi chú buổi, lưu ý sức khỏe, ghi chú chấm công, SĐT nhân viên.
3. **Danh sách action có kiểu.** `kernel::audit::Action` là enum (64 key), `AuditEntry.action: Action`. Nhóm lọc là `match` vét cạn (`reporting::activity::kind_of`): thêm action mà chưa xếp nhóm thì **không compile**. Contract có schema `AuditAction` (enum mọi key), web giữ `Record<AuditAction, …>`: thêm action mà chưa có câu thì **typecheck đỏ** (sau `pnpm api:gen`, CI so file). Key cũ không còn trong code → nhóm `other`, câu chung "làm một thao tác khác", không crash.
4. **8 nhóm lọc:** Quầy, Gói liệu trình, Lịch hẹn, Khách hàng, Nhân sự, Lương, Danh mục & cài đặt, Tài khoản & bảo mật (cộng "Khác" cho key cũ). Bảng action → nhóm ở [spec §11](../product/specs/management-views.md).
5. **Phân trang + giới hạn.** Keyset `(at, id)` giảm dần, cursor mờ; khoảng ngày mặc định 7 ngày, tối đa 366; trang 1–100 (web dùng 50). Lọc người: `actor` = user id \| `platform` \| `system` (chỉ id / mã trên URL, không tên, không SĐT).
6. **Index (0051).**
   - `audit_log (tenant_id, at DESC, id DESC)` thay `audit_log_tenant_at_idx` của 0038 (console dùng cùng thứ tự);
   - `audit_log (tenant_id, actor_user_id, at DESC, id DESC) WHERE actor_user_id IS NOT NULL` cho bộ lọc người.
7. **`customer.update` giữ tên + SĐT cũ → mới** (chỉ trường đã đổi; khóa `full_name`, `phone` trong `before` / `after`). Các trường khác vẫn chỉ tên trường. `customer.create` vẫn chỉ cờ; ngày sinh không bao giờ vào audit. **Sửa một phần D13 của ADR-0019.** Không backfill dòng cũ (0047 đã xóa giá trị, không khôi phục được).
8. **Giờ audit theo đồng hồ của use case.**
   - `TenantTx` mang `clock`: `TenantTx::for_actor_at(pool, actor, now)` ở mọi `*_as_of` → audit `at = now` (HTTP truyền `Utc::now()`, vẫn là giờ thật).
   - Công cụ (seed) dùng thêm `Actor::at_clock(t)` cho các use case không nhận đồng hồ: chấm công, khen/phàn nàn, thưởng/phạt, đánh giá, chốt lương, tạo khách, chăm sóc lại, hồ sơ nhân viên, nghỉ việc.
   - Actor của session không bao giờ có clock; không input nào của request chạm tới nó.
   - Chỉ giờ audit đổi: `created_at` của bảng nghiệp vụ vẫn là giờ thật.

9. **Một thao tác = một dòng** (bổ sung cùng ngày, EM: "một dòng thôi vì khi ghi là thu tiền luôn rồi", [raw 3](../../raw/2026-11-11-em-decision-activity-log-one-line-per-action.md)).
   - `audit_log` không đổi: một thao tác vẫn ghi nhiều dòng (lượt + hóa đơn + gói, hủy + đảo, sửa lượt = hủy + ghi lại + `visit.correct`).
   - Màn đọc gộp ở server: dòng có hạng thấp hơn, cùng `at` + cùng người làm, có chung một id lượt / hóa đơn (lấy từ chính dòng audit: `sale_id`, `sale_ids`, `package_sale_ids`, `corrects_visit_id`, `visit_id`) thì gộp vào dòng hạng cao hơn. Hạng: chuyển sang gói > sửa lượt > lượt (ghi / thêm / hủy) > hóa đơn > gói. Bảng ở `reporting::activity::fold_rule`; hành động không có trong bảng không bao giờ gộp (mặc định từ chối).
   - Dòng gộp mang tiền của cả thao tác (lượt: tổng mọi hóa đơn kể cả gói), tên gói (`packages`), key các dòng đã gộp (`folded`). Lọc nhóm khớp theo bất kỳ dòng nào trong thao tác. Bảng chi tiết ở [spec §11.3b](../product/specs/management-views.md).
   - Không migration: khóa gộp đọc từ cột + JSON của `audit_log`, dùng index `(tenant_id, at, id)` của 0051 cho mỗi lần dò cùng thời điểm.

## Consequences
- Positive:
  - Chủ spa tra được "ai làm gì, lúc nào" ở một màn, các màn khác giữ nguyên (đúng ý EM).
  - Dữ liệu ra màn là allowlist, không phụ thuộc writer ghi gì vào `before` / `after`.
  - Thiếu nhóm hay thiếu câu cho action mới bị compile / typecheck bắt, không cần nhớ.
  - Sau `reset-demo` + `seed-demo`, nhật ký trông như một năm thật.
- Negative / accepted trade-offs:
  - **Audit giờ chứa PII** (tên + SĐT cũ / mới của khách đã đổi). Màn đọc đã ẩn tên / SĐT cũ của khách đã ẩn danh. Nhưng **khi làm `customer.anonymize` (chưa build) phải xóa cả `full_name` / `phone` trong `before` / `after` của audit khách đó.** Đây là điều kiện của tính năng ẩn danh, ghi vào spec khi làm.
  - Đổi kiểu `AuditEntry.action` chạm mọi crate có ghi audit (mỗi chỗ một dòng).
  - Seed chia chấm công / khen-phàn nàn theo ngày (khoảng 2 transaction một ngày thay vì một batch một tháng), seed chậm hơn một chút.
- Follow-ups:
  - `customer.anonymize`: scrub audit (xem trên).
  - Nếu cần: link từ dòng nhân viên tới hồ sơ nhân viên theo id (hiện Nhân sự mở theo tab, chưa có trang theo id).

## Revisit when
EM muốn quản lý cũng xem được (một phần) nhật ký; luật dữ liệu cá nhân đòi hạn lưu cho audit; hoặc số dòng audit một spa vượt cỡ vài triệu (xem lại index / phân vùng).
