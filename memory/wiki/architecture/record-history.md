---
title: Thiết kế backend — lịch sử trên từng bản ghi + cờ màn Nhật ký thao tác
updated: 2026-11-11
sources: [raw/2026-11-11-em-decision-inline-record-history.md]
related: [../product/specs/record-history.md, ../decisions/ADR-0022-inline-record-history.md, ../decisions/ADR-0021-activity-log-for-the-owner.md, platform-admin.md, backend.md, data.md, ../security/authz-matrix.md]
---

# Backend: lịch sử trên từng bản ghi

Spec: [record-history](../product/specs/record-history.md). Quyết định: [ADR-0022](../decisions/ADR-0022-inline-record-history.md). Chưa build.

## 1. Tổng quan
```
web (chi tiết bản ghi mở)                       API (crate reporting, module history)                Postgres
  GET /record-history/visit/{id}  ──────────▶  require(record.history.read)           ──▶  resolve neo + kiểm tồn tại (RLS + tenant)
                                                match record → require(quyền đọc màn gốc)   audit_log: dòng neo (entity idx)
                                                gộp bằng activity::fold / line              audit_log: dòng cùng thời điểm (tenant,at idx)
                                                lọc trường theo quyền (D7, D8)              rows_by_ids: join tên (khách, NV, gói…)
  ◀────────────── 200 RecordHistory, Cache-Control: no-store
GET /visits, /reports/ledger, …  ──────────▶  nếu actor.can(record.history.read): EXISTS(audit…) AS edited, else không có key
```
Không bảng mới, không đổi cách ghi audit. Một permission mới, một cờ tính năng mới, một migration (0052).

## 2. Permission `record.history.read` (công tắc)
- `kernel::Permission::RecordHistoryRead => "record.history.read"` (nhóm `record`, cạnh `record.void_*`).
- `identity::SystemRole::default_permissions`: chủ spa có (= mọi permission trừ `NOT_GRANTABLE`), quản lý **không**, KTV không. Authz-matrix cập nhật cùng PR (test `matrix_rows_agree_with_seeded_roles`).
- Migration 0052 chèn `record.history.read` vào `role_template_permission` (owner) **và** `role_permission` của vai trò `owner` hệ thống ở **mọi** spa (kể cả spa có vai trò owner riêng: EM muốn mặc định hiện cho chủ spa), `ON CONFLICT DO NOTHING`, **không đổi** `follows_template` (mẫu cũng được thêm nên spa theo mẫu vẫn khớp mẫu).
- `platform::roles`:
  - **không** thêm vào `OWNER_FLOOR` (operator tắt được cho chủ spa), **không** thêm vào `OWNER_ONLY` (cấp được cho quản lý);
  - thêm `NOT_FOR_TECHNICIAN: &[Permission] = &[RecordHistoryRead]`; `check_grants(SystemRole::Technician, …)` có nó → `422 role.not_for_technician` (mẫu và spa). `PermissionInfo` thêm `not_for_technician: bool` để console tắt ô KTV.
- Người đã nghỉ: `effective_permissions` đã cắt về `staff.read_own_past` → không cần gì thêm.

### 2.1 Endpoint console (chỉ operator)
`PUT /api/v1/admin/tenants/{id}/record-history/{role}` body `{visible: bool, reason: string}` (`deny_unknown_fields`, lý do 3–500 như `Reason::checked`), 🔐 step-up.
- `role`: `owner` | `manager` → tiếp; `technician` → `422 role.not_for_technician`; khác → `404`.
- Gọi hàm mới (0052) `app.platform_set_role_permission(p_token_hash, p_tenant, p_role, p_permission, p_granted) RETURNS TABLE (role_id uuid, before text[], after text[])`:
  - `SECURITY DEFINER`, `app.platform_gate(p_token_hash, true)` (step-up kiểm trong DB như 0041);
  - `SELECT … FROM app.role WHERE tenant_id = p_tenant AND key = p_role AND is_system FOR UPDATE` (không có → không trả dòng → `404`);
  - `INSERT … ON CONFLICT DO NOTHING` hoặc `DELETE` đúng một dòng `role_permission`; `follows_template = false` (cùng nghĩa với sửa vai trò riêng, AC7.2); trả before / after đã sắp xếp;
  - không đổi gì (đã đúng giá trị) → vẫn trả dòng, Rust thấy `before == after` → rollback + `409 role.record_history_unchanged`.
- Rust kiểm `check_grants(role, after)` **trước** khi commit (rào chung), rồi ghi audit hai phía trong cùng transaction: spa `role.permissions_changed` (`record_platform`, before/after, lý do — action có sẵn, câu có sẵn ở web), nền tảng `platform.record_history.set` (`{role, visible, before, after}`).
- `REVOKE ALL … FROM PUBLIC; GRANT EXECUTE … TO app_rw` như các hàm `platform_*`.
- Đọc trạng thái: dùng `GET /admin/tenants/{id}/roles` có sẵn (web xem `permissions` có `record.history.read` không). Không endpoint đọc mới.

## 3. Cờ `activity_log` (ẩn màn Nhật ký thao tác), mặc định tắt
### 3.1 Registry
`kernel::feature::features!` thêm cột `default_enabled`:
```
ActivityLog => "activity_log", "Nhật ký thao tác", default off, [], ["/api/v1/audit-log"],
```
6 cờ cũ `default on`. Không sở hữu permission nào → `owning(AuditRead) == None`, test `administration_is_never_a_switchable_feature` không đổi. Test mới `the_activity_screen_switch_owns_its_route_not_audit_read` (route `/api/v1/audit-log` → `ActivityLog`, `owning(AuditRead) == None`, `default_enabled == false`).

### 3.2 Tính danh sách cờ tắt (một chỗ, thuần)
`kernel::feature::disabled(switched_on: &[String], switched_off: &[String]) -> BTreeSet<Feature>` = { f | dòng của f là tắt } ∪ { f | `!f.default_enabled()` và không có dòng bật }. Key lạ bỏ qua (như `parse_keys`). Unit test bảng chân trị 4 trường hợp × 2 mặc định.

### 3.3 SQL (0052)
- `app.session_actor` (bản 0039): `DROP FUNCTION` + `CREATE` lại, thêm cột cuối `enabled_features text[]` (cùng subquery, `WHERE f.enabled`). Mọi cột / điều kiện khác giữ nguyên (ADR-0017: một câu cho mỗi request).
- `identity::repo::tenant_access` (đường không qua `session_actor`): thêm `enabled_features` cùng câu.
- `app.platform_set_feature`: `CREATE OR REPLACE`, trả `v_previous` **không** `coalesce(…, true)` (null = chưa có dòng); `platform::features::set` tính `previous = row.unwrap_or(feature.default_enabled())` cho audit before/after. Cùng chữ ký nên không cần đổi quyền.
- `platform::features` (`GET /admin/features`, `GET /admin/tenants/{id}/features`): `enabled = row.unwrap_or(default_enabled)`, thêm trường `default_enabled` vào `FeatureInfo` / `TenantFeature`.
- Không chèn dòng `tenant_feature` nào (cột `updated_by` NOT NULL tham chiếu operator; mặc định nằm ở code). `app reset-demo` xóa `tenant_feature` → sau reset, màn nhật ký về tắt (đúng mặc định).

### 3.4 Hành vi
Route `/api/v1/audit-log` khi tắt → `403 feature.disabled` từ guard `AuthUser` có sẵn (`kernel::feature::for_route`), trước handler, cho mọi vai trò. `/me.features` không có `activity_log` → web `mayOpen.activity = can("audit.read") && hasFeature("activity_log")`.

## 4. Endpoint lịch sử
### 4.1 Hình dạng
| Route | `record` | Quyền đọc màn gốc (sau `record.history.read`) | Neo |
|---|---|---|---|
| `GET /record-history/visit/{id}` | lượt | `visit.read` | chuỗi `corrects_visit_id` (≤ 20 bản) + hóa đơn của chuỗi (`sale.visit_id`) + gói của các hóa đơn đó |
| `GET /record-history/sale/{id}` | hóa đơn | `sale.read` + `customer.read` | hóa đơn thuộc lượt → **đổi neo sang lượt** (như trên); không → hóa đơn + gói của nó |
| `GET /record-history/refund/{id}` | dòng hoàn tiền | `sale.read` + `customer.read` | **đổi neo sang gói** `refund.customer_package_id` |
| `GET /record-history/customer-package/{id}` | gói | `customer.read` | gói + hóa đơn bán nó (`sale_id` trong dòng audit) |
| `GET /record-history/customer/{id}` | hồ sơ khách | `customer.read` | `entity_type = 'customer'` trừ `customer.follow_up`, `opening_balance.import` (§4.2 spec) |
| `GET /record-history/appointment/{id}` | lịch hẹn | `appointment.manage` | `entity_type = 'appointment'` |
| `GET /record-history/staff-day/{staff_id}/{date}?book=attendance\|review` | ô chấm công / khen-phàn nàn | `attendance.record` / `evaluation.record` + phạm vi của sổ (`hr::scope::Scope::lists(staff_id)`, quyết định C, ADR-0015; ngoài phạm vi → `404`) | `entity_type = 'staff'`, `entity_id = staff_id`, action đúng sổ, `coalesce(after->>'date', before->>'date') = date` |
| `GET /record-history/service/{id}`, `/package-plan/{id}`, `/service-category/{id}` | danh mục | `catalog.manage` | `entity_type` tương ứng |
| `GET /record-history/settings/payroll-rules` | quy tắc lương | `payroll.read` | `payroll.rules_update` của spa |
| `GET /record-history/settings/appointment-settings` | cài đặt lịch hẹn | `appointment.settings` | `appointment.settings` của spa |

`record` là enum đóng ở path (Axum route riêng cho từng loại, không route `{record}` tự do) → loại lạ = `404` của router; mỗi route có dòng `ROUTES` trong `authz_matrix.rs`. Bảng `record → quyền` là `match` vét cạn trong `reporting::history::authorize` (thêm loại chưa có quyền → không compile).

Thứ tự kiểm (F17): `401` → `403 auth.password_change_required` → `403 feature.disabled` (route / permission của cờ) → `require(RecordHistoryRead)` → `require(<quyền màn gốc>)` → parse id / date (`422`) → tồn tại trong spa (`404`) → đọc.

### 4.2 Response
```
RecordHistory {
  record: { type: "visit" | "sale" | "customer_package" | "customer" | "appointment" | "attendance" | "daily_review"
                 | "service" | "package_plan" | "service_category" | "payroll_rules" | "appointment_settings",
            id?: uuid },            // neo sau khi đổi (sale trong lượt → visit; refund → customer_package)
  summary: {
    source: "audit" | "record" | "paper_book" | "none",
    created?: { actor: ActivityActor, at, day },   // dòng đầu (hoặc cột created_by/created_at khi source = record)
    last?:    { actor: ActivityActor, at, day },   // dòng cuối nếu khác dòng đầu
    edits: u32,                                    // số dòng sau dòng đầu, trừ dòng hủy / hủy gói / không đến
    state: "active" | "voided" | "replaced" | "cancelled"
  },
  items: ActivityEntry[],   // DTO có sẵn của Nhật ký thao tác, cũ → mới, tối đa 100 (mới nhất)
  omitted: u32
}
```
- `ActivityActor` thêm trường tùy chọn `left: true` (người làm đã nghỉ; vắng = không). Thay đổi cộng thêm, cũng dùng được cho Nhật ký thao tác sau này.
- `ActivityEntry` dùng lại nguyên (web dùng lại `sentence.ts` cho giá trị đổi, tiền, lý do).

### 4.3 Thuật toán (crate `reporting`, module mới `history`)
1. **Neo.** Một câu SQL theo loại: kiểm tồn tại trong spa (`tenant_id = $1` tường minh + RLS) và trả tập neo `(entity_type, entity_id[])`; lượt: `WITH RECURSIVE` lên theo `corrects_visit_id` và xuống theo `visit.corrects_visit_id = id`, `LIMIT 20`.
2. **Dòng neo.** `audit_log` theo `(tenant_id, entity_type, entity_id = ANY(...))` → index `audit_log_entity_idx (tenant_id, entity_type, entity_id, at)` (0001). Ô chấm công lọc `date` sau khi dò index (~1.000 dòng / nhân viên / năm, đủ nhanh; nếu bench > 50 ms thì thêm index biểu thức `((after->>'date'))` một phần cho 2 action, ghi ở plan).
3. **Dòng cùng thao tác.** `audit_log` theo `(tenant_id, at = ANY(<các at của dòng neo>))` → index `audit_log_tenant_at_id_idx` (0051). Cần để `fold` ghép đủ một thao tác (lượt + hóa đơn + gói), giống cách màn nhật ký lấy "rows of the same instant".
4. **Làm giàu.** `activity::rows_by_ids(tx, ids)` (mới, `pub(crate)`): cùng các join tên (khách, giới tính, ẩn danh, NV, dịch vụ, mẫu gói, gói, KTV trước / sau, tên người làm) như `activity::list`, cộng `left` của người làm (cùng điều kiện với `activity::people`). Test `rows_by_ids_matches_the_list_enrichment` so mục của hai đường trên cùng dữ liệu (tránh hai bản lệch nhau).
5. **Gộp.** `activity::fold` + `activity::line` (đổi `pub(crate)`), rồi **giữ dòng nào có (gốc hoặc dòng gộp) thuộc tập neo**. Dòng hạng 0 (gia hạn, dùng gói hết hạn, `visit.update`) là dòng riêng.
6. **Lọc theo quyền (D7, D8)** trên `ActivityEntry` đã gộp:
   - không `sale.read` → `amount = None`, `discount = None`;
   - không `customer.health_notes.read` → bỏ thay đổi trường `note` / `health_note` của lượt; bỏ hẳn dòng `customer.health_note_set`, `sale.note_update`;
   - không `audit.read` → mọi `ChangeFormat::Phone` có giá trị khác SĐT hiện tại của khách được che `+84905•••222` (web hiện "0905 ••• 222"); hàm thuần `mask_phone` + unit test;
   - giữ nguyên: không bao giờ `before/after` thô (allowlist `fields()` có sẵn).
7. **Tóm tắt.** Từ danh sách đã lọc: `created` = dòng đầu; `edits` = số dòng sau đó có action không thuộc {`visit.void`, `sale.void`, `customer_package.cancel`, `appointment.cancel`, `appointment.no_show`, `appointment.complete`, `appointment.arrive`}; `state` từ chính bản ghi (lượt: `status`, `corrected_by_visit_id`; gói: `cancelled_at`; …). Không có dòng audit → `source = paper_book` (gói `customer_package.origin = 'opening_balance'`, hoặc hóa đơn có dòng `sale.paper_book`) / `record` (cột `created_by`, `created_at`: lượt, hóa đơn, gói, lịch hẹn) / `none`.
8. **Cắt.** Sắp `(at, id)` tăng dần, giữ 100 dòng cuối, `omitted` = phần bỏ.

Một transaction đọc (`TenantTx::for_actor`), 3–4 câu SQL tuần tự (neo, dòng neo + cùng thời điểm có thể gộp thành một câu CTE, làm giàu). API ↔ DB cùng vùng nên mỗi câu ~1–3 ms.

### 4.4 Dấu `edited` trong danh sách có sẵn
Trường `edited: Option<bool>` (`#[serde(skip_serializing_if = "Option::is_none")]`), tính trong **cùng câu SQL** của danh sách bằng `CASE WHEN $can_history THEN EXISTS(...) END` (tắt → không dò, `None`):

| DTO / endpoint | `edited = true` khi |
|---|---|
| `VisitDto` (`GET /visits`, `GET /visits/{id}`, trả về của các thao tác lượt) | `corrects_visit_id IS NOT NULL` hoặc có audit `visit.update` / `visit.addition` trên lượt |
| `LedgerRow` (`GET /reports/ledger`) | hàng có `visit_id` → như lượt; hóa đơn bán gói → có `sale.note_update`; hàng hoàn tiền → không (đã là sự kiện hủy) |
| `DaySaleDto` (`GET /sales?date=`) | có `sale.note_update` |
| `CustomerPackageDto` (`GET /customers/{id}/packages`, `/customer-packages/{id}`) | có `customer_package.extend` / `.use_expired_override` hoặc `sale.note_update` trên hóa đơn bán nó |
| `AppointmentDto` (`GET /appointments`) | có `appointment.update` / `.undo_arrival` |
| `HistoryEvent` (`GET /customers/{id}/history`, mục lượt) | như lượt |

`EXISTS` dò `audit_log_entity_idx` một lần mỗi hàng (≤ 100 hàng / trang). "Đã hủy" không thêm trường: dùng `status` / `voided` sẵn có.

`edited` **không** chứa tên người hay giờ (D9 spec). Response từ idempotent replay (`idempotency::complete` lưu body): body lưu lúc ghi đã tính theo quyền của người ghi; replay chỉ trả cho cùng người dùng cùng key, nên không lộ cho người khác (ghi trong threat model).

## 5. Contract (OpenAPI → `packages/api-client`)
- Mới (cộng dồn qua 4 đợt, mỗi đợt chỉ mount route của mình — [plan](../plans/record-history.md)): 12 route `GET /record-history/...`, schema `RecordHistory`, `RecordRef`, `RecordSummary`, `RecordHistorySource`, `RecordState`; `PUT /admin/tenants/{id}/record-history/{role}` + body `RecordHistorySwitch`.
- Đổi (cộng thêm): `ActivityActor.left?`, `FeatureInfo.default_enabled`, `TenantFeature.default_enabled`, `PermissionInfo.not_for_technician`, `edited?` ở 6 DTO trên.
- Mã lỗi mới: `role.not_for_technician` (422), `role.record_history_unchanged` (409).
- `pnpm api:gen` + CI so file như mọi lần.

## 6. Cache, perf
- Mọi `GET /record-history/*`: `Cache-Control: no-store` (giống `/audit-log`). Web: query key `["record-history", type, id]` (`staleTime: 0`), invalidate tiền tố `["record-history"]` sau mọi mutation thành công của lượt / hóa đơn / gói / lịch hẹn / khách / chấm công / danh mục / quy tắc lương.
- Ngân sách: `GET /record-history/*` p95 ≤ 100 ms ở `reports_bench` (3 năm ~44.000 lượt, audit ~6.800 dòng / năm × 3 + một lượt sửa 3 lần); danh sách có `edited` p95 tăng ≤ 10 ms (`/visits` ngày, ledger 92 ngày). Bench thêm vào `app/tests/reports_bench.rs`.
- Không N+1: một lượt gọi mỗi lần mở chi tiết; danh sách một câu.

## 7. Impact analysis (theo `.claude/skills/build/impact-analysis.md`)
| Chỗ đổi | Ai bị ảnh hưởng | Việc |
|---|---|---|
| `Permission` thêm 1 | `identity::roles` test số lượng owner, `authz-matrix.md`, `authz_matrix.rs` seed, `platform.rs::role_templates_start_as_the_code_defaults`, web `admin` nhãn permission | cập nhật cùng PR |
| `features!` thêm cột | 6 cờ cũ, `parse_keys`, `strip`, `MemberAccess::new`, `identity::service::me` (`features`), `platform::features`, `demo/reset.rs`, test `keys_are_unique…`, web `FEATURE` map trong `sentence.ts` (câu `feature.changed`), admin `FeaturesPanel` | cập nhật cùng PR |
| `session_actor` đổi cột | `identity::repo` (đọc session), test `session_actor` | cùng PR (0052) |
| `platform_set_feature` trả null | `platform::features::set` (`previous!` → `Option`) | cùng PR |
| `activity::fold/line/Row` thành `pub(crate)` | chỉ `reporting` | — |
| `edited` vào 6 DTO | handler + repo từng crate (`operations`, `reporting`, `sales`, `packages`, appointments), snapshot test DTO, web type | theo đợt |
