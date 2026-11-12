---
title: Thiết kế backend — lịch sử trên từng bản ghi + cờ màn Nhật ký thao tác
updated: 2026-11-12
sources: [raw/2026-11-11-em-decision-inline-record-history.md]
related: [../product/specs/record-history.md, ../decisions/ADR-0022-inline-record-history.md, ../decisions/ADR-0021-activity-log-for-the-owner.md, platform-admin.md, backend.md, data.md, ../security/authz-matrix.md]
---

# Backend: lịch sử trên từng bản ghi

Spec: [record-history](../product/specs/record-history.md) (D1–D12, review log 4 lượt). Quyết định: [ADR-0022](../decisions/ADR-0022-inline-record-history.md). **Đợt 1a đã build** (0052, `reporting::history`, 5 route đợt 1, công tắc, cờ; quyết định khi build K1–K11 ở spec Review log). **Đợt 1b (web) đã build**: `features/record-history/` (`query.ts` + `chip.ts` + `lazy.tsx` ngoài chunk, chunk lazy `chunk.ts`), `MutationCache.onSuccess`, `[data-row-inert]`; quyết định K12–K20. **Đợt 2 đã build** (2026-11-12): route `appointment`, `staff-day`, `adjustment` (8 route), `edited` lịch hẹn, không migration; K21–K36. **Đợt 3 đã build** (2026-11-12): route `service`, `package-plan`, `service-category`, `settings/payroll-rules`, `settings/appointment-settings` (đủ 13 route), không migration (`audit_log_entity_idx` của 0001 trả lời cả hai kiểu, EXPLAIN ở §6); K37–K47. Bản sửa sau review độc lập 2026-11-11.

## 1. Tổng quan
```
web (mở một bản ghi)                     API (crate reporting, module history)                   Postgres
GET /record-history/visit/{id} ───▶ require(record.history.read)
                                     resolve neo (+ đổi neo) → require(quyền đọc loại neo cuối)  ──▶ kiểm tồn tại (tenant + RLS)
                                     chọn id dòng audit thuộc bản ghi (theo loại, §4.3)        ──▶ audit_log (index)
                                     rows_by_ids → fold (chỉ lượt/hóa đơn/gói) → lọc quyền     ──▶ join tên
                    ◀── 200 RecordHistory, Cache-Control: no-store
GET /visits, /reports/ledger, … ───▶ can(record.history.read)? EXISTS(audit_log_edit_idx…) AS edited : (không key)
```
Không bảng mới, không đổi cách ghi audit. Một permission, một cờ, một migration (0052), một chỉnh `fields()` (S4).

## 2. Permission `record.history.read` (spa-local)
- `kernel::Permission::RecordHistoryRead => "record.history.read"`.
- `identity::SystemRole::default_permissions`: owner có (mọi permission trừ `NOT_GRANTABLE`), manager không, technician không. Authz-matrix cùng PR.
- `platform::roles`:
  - không thuộc `OWNER_FLOOR`, không thuộc `OWNER_ONLY`;
  - `NOT_FOR_TECHNICIAN: &[Permission] = &[RecordHistoryRead]` → `check_grants(Technician, …)` có nó → `422 role.not_for_technician`;
  - `SPA_LOCAL: &[Permission] = &[RecordHistoryRead]`: `PermissionInfo` thêm `spa_local: bool`, `not_for_technician: bool` (console hiện ô chỉ đọc / tắt). Test so `SPA_LOCAL` Rust với hằng trong SQL (đọc `pg_get_functiondef` hoặc gọi hàm với vai trò mẫu) để hai nơi không lệch.
- **Spa-local nghĩa là** (LEAD, review lượt 3):
  - mẫu vai trò (`role_template_permission`) quyết giá trị **ban đầu** khi tạo spa (owner có, manager không);
  - `platform_apply_role_template` (0040, `CREATE OR REPLACE` trong 0052): với mỗi spa theo mẫu, tập mới **của spa đó** = (mẫu − SPA_LOCAL) ∪ (SPA_LOCAL hiện có của spa); `IS DISTINCT FROM`, dữ liệu ghi và `after` của dòng audit `role.permissions_changed` đều dùng **tập mới của spa** (bản 0040 ghi `to_jsonb(p_permissions)`, sẽ sai với spa-local); test `platform.rs` kiểm dòng audit đó;
  - `platform_set_tenant_role` (0041, `CREATE OR REPLACE`): cả sửa riêng lẫn "Về mẫu" (`p_permissions IS NULL`): SPA_LOCAL trong danh sách gửi lên bị bỏ qua, giá trị hiện tại giữ nguyên; `follows_template` tính như cũ;
  - phía Rust `set_for_tenant` / `set_template` không cần đổi ngoài rào KTV (DB là nơi giữ bất biến).
- Người đã nghỉ: `effective_permissions` đã cắt về `staff.read_own_past`.

### 2.1 Endpoint công tắc (chỉ operator)
`PUT /api/v1/admin/tenants/{id}/record-history/{role}` body `{visible: bool, reason: string}` (`deny_unknown_fields`, lý do qua `Reason::checked`), 🔐 step-up.
- `role` `owner` | `manager`; `technician` → `422 role.not_for_technician`; khác → `404`.
- Hàm mới `app.platform_set_record_history(p_token_hash bytea, p_tenant uuid, p_role text, p_visible boolean) RETURNS TABLE (role_id uuid, before text[], after text[])` (danh sách permission của vai trò trước / sau, đọc dưới khóa dòng role — audit `role.permissions_changed` cần danh sách):
  - `SECURITY DEFINER`, `SET search_path = app, pg_temp`, `app.platform_gate(p_token_hash, true)`;
  - `IF p_role NOT IN ('owner', 'manager') THEN RAISE EXCEPTION … USING ERRCODE = '22023'` (Rust đã chặn trước, đây là lớp thứ hai);
  - permission **cứng** `'record.history.read'` (không có tham số permission);
  - `SELECT id FROM app.role WHERE tenant_id = p_tenant AND key = p_role AND is_system FOR UPDATE` (không có → không dòng → `404`);
  - thêm (`ON CONFLICT DO NOTHING`) hoặc xóa đúng một dòng `role_permission`; **không đụng** `follows_template`;
  - `REVOKE ALL … FROM PUBLIC; GRANT EXECUTE … TO app_rw`.
- `before == after` → rollback, `409 role.record_history_unchanged`.
- Body request `RecordHistorySwitch {visible, reason}`; response `200` = `RoleGrants` của vai trò sau khi đổi (cùng shape với `PUT /admin/tenants/{id}/roles/{key}`).
- Cùng transaction: audit spa `role.permissions_changed` (`record_platform`, before/after = danh sách permission của vai trò, lý do) + audit nền tảng `platform.record_history.set` (`{role, visible}`); web console `AuditPage.tsx` thêm nhãn cho key mới.
- Đọc trạng thái: `GET /admin/tenants/{id}/roles` có sẵn.

## 3. Cờ `activity_log` (mặc định tắt từ đợt 1)
### 3.1 Registry
`kernel::feature::features!` thêm cột `default_enabled`; 6 cờ cũ `true`; mới:
```
ActivityLog => "activity_log", "Nhật ký thao tác", default_enabled: false, [], ["/api/v1/audit-log"],
```
Không sở hữu permission → `owning(AuditRead) == None`, `administration_is_never_a_switchable_feature` không đổi. Test mới: `the_activity_screen_switch_owns_its_route_not_audit_read`; test khóa `default_enabled` của từng cờ (đổi mặc định = đổi hành vi mọi spa chưa có dòng).

### 3.2 Một hàm tính cờ tắt
`kernel::feature::disabled(on: &[String], off: &[String]) -> BTreeSet<Feature>` = { dòng tắt } ∪ { `!default_enabled` và không có dòng bật }. **Cả hai đường** dùng nó: `identity::repo::member_access` (gọi từ `service::tenant_access`, đổi spa / `/me`) và đường `session_actor` (mỗi request). Unit test bảng chân trị.

### 3.3 SQL (0052)
- `app.session_actor`: `DROP FUNCTION` + `CREATE`, thêm cột cuối `enabled_features text[]`; mọi cột / điều kiện khác giữ; **lặp lại** `REVOKE ALL … FROM PUBLIC; GRANT EXECUTE … TO app_rw` (DROP làm mất grant).
- `member_access`: thêm `enabled_features` vào cùng câu.
- `app.platform_set_feature`: `CREATE OR REPLACE`, trả `v_previous` không `coalesce` (null = chưa có dòng); `platform::features::set` dùng `Option`, áp `default_enabled`.
- `platform::features`: `enabled = row.unwrap_or(default_enabled)`, trường `default_enabled` ở `FeatureInfo` / `TenantFeature`.
- Không chèn dòng `tenant_feature` (`updated_by` NOT NULL → operator). `app reset-demo` xóa `tenant_feature` → về mặc định (tắt).

### 3.4 Hành vi
`/api/v1/audit-log` khi tắt → `403 feature.disabled` từ guard `AuthUser` có sẵn. `/me.features` không có `activity_log`. Web: `mayOpen.activity = can(me, "audit.read") && hasFeature(me, "activity_log", { defaultOff: true })` (`hasFeature` hiện trả `true` khi `/me` thiếu `features`).

## 4. Endpoint lịch sử
### 4.1 Route và quyền
| Route | Đợt | Neo cuối (sau khi đổi) | Quyền đọc (sau `record.history.read`) |
|---|---|---|---|
| `GET /record-history/visit/{id}` | 1 | chuỗi lượt | `visit.read` |
| `GET /record-history/sale/{id}` | 1 | hóa đơn dịch vụ lẻ trong lượt → **chuỗi lượt**; hóa đơn bán gói → **gói của hóa đơn** | lượt: `visit.read`; gói: `customer.read`; luôn thêm `sale.read` (route theo hóa đơn) |
| `GET /record-history/refund/{id}` | 1 | gói (`refund.customer_package_id`) | `sale.read` + `customer.read` |
| `GET /record-history/customer-package/{id}` | 1 | gói | `customer.read` |
| `GET /record-history/customer/{id}` | 1 | khách | `customer.read` |
| `GET /record-history/appointment/{id}` | 2 | lịch hẹn | `appointment.manage` |
| `GET /record-history/staff-day/{staff_id}/{date}?book=attendance\|daily_review` | 2 | ô | `attendance.record` / `evaluation.record`; phạm vi `hr::scope::RecordScope::lists(staff_id)` + `period::employed_in(month)`, ngoài → `404` |
| `GET /record-history/adjustment/{id}` | 2 | chuỗi khoản thưởng / phạt | `evaluation.record` + `RecordScope::lists` |
| `GET /record-history/service/{id}`, `/package-plan/{id}`, `/service-category/{id}` | 3 | danh mục | `catalog.manage` |
| `GET /record-history/settings/payroll-rules` | 3 | quy tắc lương | `payroll.read` |
| `GET /record-history/settings/appointment-settings` | 3 | cài đặt lịch hẹn | `appointment.settings` |

**Cờ (S12):** `kernel::feature` thêm prefix `/api/v1/record-history/appointment` và `/api/v1/record-history/settings/appointment-settings` vào cờ `appointments`, `/api/v1/record-history/settings/payroll-rules` vào `payroll` (route bị chặn `403 feature.disabled` trước handler, không chỉ nhờ rút permission); test `routes_match_on_segment_boundaries` thêm 3 ca.

Mỗi loại một route Axum (không `{record}` tự do) → loại lạ `404` router; mỗi route một dòng `ROUTES` ở `authz_matrix.rs`. `reporting::history::require_route` (quyền của route) rồi `require_anchor(actor, Anchor)` là `match` vét cạn trên **loại neo cuối** (B1): đổi neo xong mới kiểm, trước mọi đọc lịch sử. Việc đổi neo chỉ đọc khóa trong spa của người gọi: loại hóa đơn quyết bởi `sale_line.line_type = 'package'` (vị từ của Sổ doanh thu, 0021) — hóa đơn bán gói → gói (`customer_package.sale_id` **chỉ để tìm id gói**), còn lại → lượt qua `sale.visit_id`; hoàn tiền → `refund.customer_package_id`; id không tồn tại / spa khác → `404` **sau** khi đã qua `record.history.read` + quyền của route (người không có hai quyền đó không phân biệt được). Người có quyền của route nhưng thiếu quyền loại neo cuối (vd. `sale.read` thiếu `visit.read`) phân biệt được `404` / `403` của một id hóa đơn: chấp nhận (id là UUIDv7 của chính spa, không lộ nội dung). Id sai dạng: `400`, nhưng **sau** quyền của route (handler nhận `Result<Path<Uuid>, PathRejection>`, build-time: người không quyền gửi id rác vẫn `403`, không oracle; xem spec Review log, quyết định khi build).

Thứ tự (F17): `401` → `403 auth.password_change_required` → `403 feature.disabled` → `require(RecordHistoryRead)` → quyền của route → parse (`422`) → đổi neo (`404`) → quyền của loại neo cuối (`403`) → đọc.

**Đợt 3 (đã build, K37–K47):** danh mục: `require_route` (`catalog.manage`) → path (`400`) → neo `Catalog(Service | PackagePlan | ServiceCategory, id)` theo `tenant_id` (`404`) → `require_anchor` (`catalog.manage`) → đọc. Cài đặt (không id): `reporting::history::require_settings` (`record.history.read` rồi `payroll.read` / `appointment.settings`) → neo `Settings(PayrollRules | AppointmentSettings)` → đọc; không có `400` / `404`. `record = {type}` không `id`.

**Đợt 2 (đã build, K21–K36):** ô ngày: `require_staff_book(None)` (một trong hai sổ) → query `book` (`400 query.invalid`) → `require_staff_book(Some(book))` → path (`400`) → `org::profile::profile_in` (`404`) → `RecordScope::lists` + `employed_in(Month::of(date))` (`404`) → đọc; `record.type = attendance | daily_review`, **không** `id`. Thưởng / phạt: `staff_adjustment` của spa (`404`) → chuỗi sửa hai chiều (`edit_link`: từ dòng `staff.adjustment` của khoản đi lùi, từ dòng `staff.adjustment_void` đi tới; cùng `at` + cùng người làm, dòng của khoảnh khắc đó phải **đúng một** hủy và **một** ghi, ≤ 20 mỗi chiều; `audit_log_tenant_at_id_idx` cho "cùng khoảnh khắc") → mọi người trong chuỗi `RecordScope::lists` (`404`, K24) → thành viên. Mỗi cặp (bản bị thay, bản thay) đọc thành **một** dòng "sửa khoản thưởng / phạt" có thay đổi cũ → mới (K21). Lịch hẹn: thành viên `entity_type='appointment' AND entity_id = id`; tạo = `appointment.create`, không có thì cột `created_by` / `created_at`.

### 4.2 Response
```
RecordHistory {
  record: { type: "visit" | "customer_package" | "customer" | "appointment" | "attendance" | "daily_review"
                 | "adjustment" | "service" | "package_plan" | "service_category" | "payroll_rules" | "appointment_settings",
            id?: uuid },
  summary: {
    created?: { actor?: ActivityActor, at?, day, source: "audit" | "record" | "paper_book" },  // D12; paper_book không có giờ (có thể không có actor)
    last?:    { actor: ActivityActor, at, day },     // dòng cuối, nếu khác dòng tạo
    edits: u32,                                       // §4.4, cùng định nghĩa với `edited`
    state: "active" | "voided" | "replaced" | "cancelled"
  },
  items: ActivityEntry[],   // cũ → mới, tối đa 100 dòng mới nhất
  omitted: u32
}
```
`ActivityActor` thêm `left?: true`. `ActivityEntry` dùng lại nguyên.

### 4.3 Thuật toán (crate `reporting`, module `history`)
1. **Neo + đổi neo + quyền** (§4.1).
2. **Chọn id dòng audit thuộc bản ghi** (thành viên theo id dòng, D6), một câu SQL mỗi loại:
   | Loại | Dòng thuộc lịch sử |
   |---|---|
   | lượt | `entity_type='visit' AND entity_id = ANY(chuỗi)` (chuỗi: `WITH RECURSIVE` lên `corrects_visit_id`, xuống `visit.corrects_visit_id = id`, ≤ 20) ∪ `entity_type='sale' AND entity_id = ANY(hóa đơn dịch vụ lẻ của chuỗi)` (`sale.visit_id ∈ chuỗi` và **không** có `sale_line.line_type = 'package'` — đúng vị từ của Sổ doanh thu, `0021` / `reporting::day`) ∪ `entity_type='customer_package' AND entity_id = ANY(gói dùng trong chuỗi: visit_line → customer_package_item → customer_package) AND action='customer_package.use_expired_override' AND after->>'visit_id' = ANY(chuỗi::text[])` (đi qua index entity, không quét audit của spa) |
   | gói | `entity_type='customer_package' AND entity_id = gói` ∪ `entity_type='sale' AND entity_id = customer_package.sale_id` (`sale.create`, `sale.void`, `sale.note_update`, `sale.paper_book`) ∪ `entity_type='customer' AND action='opening_balance.import' AND entity_id = khách của gói AND after->'customer_package_ids' ? gói::text` |
   | khách | `entity_type='customer' AND entity_id = khách AND action IN ('customer.create','customer.update','customer.health_note_set')` ∪ `opening_balance.import` của khách có `after->>'customer_created' = 'true'` |
   | lịch hẹn | `entity_type='appointment' AND entity_id = id` |
   | ô ngày | `entity_id = staff_id AND action = <sổ> AND coalesce(after->>'date', before->>'date') = date` (index §6) |
   | thưởng/phạt | `entity_type='staff' AND action IN ('staff.adjustment','staff.adjustment_void') AND entity_id = ANY(nhân viên của chuỗi) AND coalesce(after->>'adjustment_id', before->>'adjustment_id') = ANY(chuỗi)`; chuỗi = khoản bị thay bởi khoản này: cùng `at` + cùng người làm + **đúng một** `adjustment_void` và **một** `adjustment` (do `replace` ghi trong một transaction; khoản mới **có thể đổi sang nhân viên khác**, `hr/src/adjustment.rs`), lặp ≤ 20; phạm vi `RecordScope::lists` kiểm cho **mọi** nhân viên trong chuỗi |
   | danh mục / cài đặt | `entity_type` + `entity_id` tương ứng (cài đặt: `entity_type='tenant'` + action). **Đã build (đợt 3):** dịch vụ `entity_type='service'`, mẫu gói `'package_plan'`, nhóm `'service_category'`, mỗi loại chỉ `*_create` + `*_update` của nó; tạo = `*_create`, không có thì "Chưa rõ ai ghi" (danh mục không có `created_by`); quy tắc lương / số khách tối đa: `entity_type='tenant' AND entity_id = spa AND action = 'payroll.rules_update' \| 'appointment.settings'` (đúng cách `hr::rules::put` / `appointments::service::update_settings` ghi), lần lưu đầu là "tạo", mọi lần sau là sửa. `catalog.category_update` chỉ ghi `after` (tên + thứ tự): khi đọc lịch sử nhóm, giá trị cũ lấy từ `after` của dòng trước của chính nhóm đó (`activity::chain_category_updates`, K40); đổi nhóm của dịch vụ hiện tên nhóm cũ → mới (`rows_by_ids` join `service_category`, K41) |
3. **Dòng phụ để gộp (chỉ lượt):** để dòng `visit.create` mang "kèm bán gói X" và Thu = mọi hóa đơn, thêm các dòng `sale.create` / `customer_package.create` của **hóa đơn bán gói có `sale.visit_id` ∈ chuỗi** và **cùng `at`** với một dòng `visit.create|addition` đã chọn. Chúng chỉ được gộp vào dòng lượt, không bao giờ là dòng riêng (dòng gốc phải thuộc bước 2). Lịch sử gói **không** lấy dòng lượt (gốc của dòng mua là `sale.create` "bán gói"), nên không cần `visit.read` (B1).
4. **Làm giàu:** `activity::rows_by_ids(tx, ids)` dùng chung (S9, đợt 1a): join tên khách / giới tính / ẩn danh, NV, dịch vụ, mẫu gói, gói, KTV trước / sau, tên + `left` người làm, hạng + khóa gộp. **KTV của `visit.update`** (S2): `after->'staff'` là mảng `{line_id, from, to}` gồm cả dòng không đổi; làm giàu resolve `from` / `to` ra tên NV (join `app.staff` theo tenant) và `entry()` sinh **một** thay đổi `staff` (Text, "Thu → Hoa") cho mỗi phần tử `from <> to`, bỏ phần tử không đổi (hiện `diff()` bỏ mảng nên dòng "sửa lượt" rỗng). `activity::list` đổi thành "chọn id trang (keyset + lọc + gộp như hiện nay) → `rows_by_ids`", bỏ LATERAL lặp lại.
5. **Gộp:** chỉ lượt, gói (hạng của `fold_rule`, ADR-0021 D9) — trong **đúng tập id** của bước 2–3. Gốc tính trong Rust (màn nhật ký tính `root` trong SQL theo trang; ở đây tập nhỏ, đã đủ): `rows_by_ids` trả hạng + khóa gộp; một dòng là **gốc** nếu không có dòng nào khác trong tập mà nó `folds_into` (`Row::folds_into` có sẵn: cùng thời điểm + cùng người + hạng thấp hơn + chung khóa); rồi `activity::fold` gắn dòng con vào gốc. Các loại khác (chỉ hạng 0) bỏ gộp: mỗi dòng một mục.
6. **Lọc quyền (D7, D8)** trên mục đã gộp:
   - không `sale.read` → bỏ `amount`, `discount`;
   - không `customer.health_notes.read` → lọc theo **(action, trường)**: bỏ thay đổi `session_note` của `visit.update` (bỏ cả dòng nếu chỉ còn nó); bỏ hẳn dòng `customer.health_note_set`, `sale.note_update`. **Không** đụng `note` của `customer.update` ("Ghi chú hồ sơ", không gắn quyền sức khỏe, `crm/src/http.rs`);
   - không `audit.read` → thay đổi `phone` giữ tên trường, **bỏ `old` / `new`**;
   - khách ẩn danh: như activity.
7. **Tóm tắt:** `created` theo D12 (thao tác tạo; không có → cột `created_by`/`created_at` của lượt, hóa đơn, gói, lịch hẹn, `source = record`; gói `origin = 'opening_balance'` không có dòng → `source = paper_book`, chỉ `day` = `starts_on`, **không** `at` (ngày trên sổ, không có giờ; web chỉ hiện ngày)); `edits` theo §4.4 trên **danh sách đã lọc**; `state` từ chính bản ghi.
8. **Cắt:** `(at, id)` tăng dần, giữ 100 dòng cuối, `omitted`.

**Thứ tự dòng = thứ tự thay đổi ([ADR-0023](../decisions/ADR-0023-audit-instant-is-write-time.md), K60–K64, 2026-11-12).** Mọi reader xếp `(at, id)`; luật ghi bảo đảm thứ tự đó đúng:
- **Một transaction = một `at`**: `kernel::audit::record*` chỉ ghi nhận, `TenantTx::commit` ghi tất cả; dòng đầu lấy `clock_timestamp()` lúc insert, mọi dòng khác của transaction mang đúng giờ đó. Gộp một thao tác một dòng (D9), `edit_link` (cùng `at` + cùng người), dòng phụ của lượt (K7) vẫn dựa vào nó.
- **Giờ = lúc ghi, sau mọi khóa dòng** của use case: request bắt đầu trước nhưng chờ khóa của request khác xếp **sau** request đó (trước đây `now()` = giờ bắt đầu transaction, hoặc `Utc::now()` của handler, đều lấy trước khi chờ). Test `kernel/tests/audit_order.rs` (chờ khóa thật, đỏ trước bản sửa: "C, B"), `record_history.rs` `racing_edits_read_in_the_order_they_were_made` ("A → B", "B → C") và `a_request_reads_when_it_wrote_not_when_it_arrived`.
- **Trong một `at`**: thứ tự ghi nhận (id UUIDv7 lấy lúc `record`, đơn điệu trong process).
- **Reply có `edited`** (`repo::visits`, `appointments::repo::appointments` với `with_edited`) gọi `TenantTx::write_audit()` trước, nên "Đã sửa" tính cả request này; chỉ gọi sau lần ghi cuối của use case. Đọc giữa chừng (bản bị thay của Sửa lượt, chuyển sang gói) dùng `operations::service::peek_one` (không `edited`).
- **Giờ đặt trước** chỉ bởi `Actor::at_clock` (seed back-fill); `now` của `*_as_of` chỉ là đồng hồ nghiệp vụ. Writer SQL bỏ trống `at` (áp mẫu vai trò 0040 / 0052, tài khoản của operator) lấy default `clock_timestamp()` (0053).
- Dòng cũ không đóng dấu lại (K64).

### 4.4 "Tính là sửa" (một định nghĩa, D9 / D10)
`edits` = số **dòng đã gộp** (sau lọc D7) mà **hành động gốc** tính là sửa theo bảng dưới (hàm thuần `history::counts_as_edit(kind, &line)`), không đếm từng dòng audit. Chỉ hai loại có bản SQL cho `edited`; test `edited_agrees_with_summary_edits` chạy cả hai trên dữ liệu có **một lượt đổi KTV, một lượt chỉ gửi lại KTV cũ (from = to), một chuỗi A→B→C, một lượt chuyển sang gói, một lượt chỉ thêm dịch vụ**: `edited` ⇔ `edits ≥ 1` ở mọi bản.

**Literal trong SQL (S7):** câu dùng index một phần phải viết **chuỗi action literal** và lặp **đủ danh sách `IN` của điều kiện index** (sqlx chuẩn bị câu, plan chung không chứng minh được `action = $n` nằm trong `WHERE` của index); so ngày `coalesce(after->>'date', before->>'date') = $n::text`.
| Loại | Tính là sửa | `edited` ở danh sách |
|---|---|---|
| lượt | dòng (đã gộp) có gốc `visit.correct` hoặc `visit.convert_to_package` (một lần chuyển sang gói ghi cả hai trong một transaction → gộp thành **một** dòng gốc `convert_to_package`, đếm 1); `visit.update` có `check_in_at` khác null hoặc **một phần tử** `after->'staff'` có `from <> to` (writer ghi mọi dòng gửi lên, kể cả không đổi) | `v.corrects_visit_id IS NOT NULL OR EXISTS (SELECT 1 FROM app.visit w WHERE w.tenant_id = v.tenant_id AND w.corrects_visit_id = v.id) OR EXISTS (SELECT 1 FROM app.audit_log a WHERE a.tenant_id = v.tenant_id AND a.entity_type = 'visit' AND a.entity_id = v.id AND a.action IN ('visit.update', 'appointment.update') AND a.action = 'visit.update' AND (a.after->>'check_in_at' IS NOT NULL OR EXISTS (SELECT 1 FROM jsonb_array_elements(coalesce(a.after->'staff', '[]')) e WHERE e->>'from' IS DISTINCT FROM e->>'to')))` — chuỗi dài > 1 luôn có bản thay hoặc bản bị thay, nên mọi bản trong chuỗi đều `true`, khớp `edits` ≥ 1 |
| lịch hẹn | `appointment.update` đổi `starts_at` hoặc `staff_id` | `EXISTS (… a.action IN ('visit.update', 'appointment.update') AND a.action = 'appointment.update' AND (a.before->'starts_at' IS DISTINCT FROM a.after->'starts_at' OR a.before->'staff_id' IS DISTINCT FROM a.after->'staff_id'))`, chỉ khi `marks=true` |
| khách | `customer.update` | — (không danh sách mang dấu) |
| ô ngày | mọi dòng sau dòng đầu | — |
| thưởng/phạt | thay bản (`replace`) | — |
| danh mục | `*_update` | — |
| cài đặt / quy tắc lương | mọi dòng sau dòng đầu | — |
| gói | không gì (vòng đời) | — |

### 4.5 `edited` trong danh sách có sẵn
`edited: Option<bool>` (`skip_serializing_if = "Option::is_none"`), tính `CASE WHEN $can THEN … END` (không quyền → không dò):
- `VisitDto` (`GET /visits`, `GET /visits/{id}`, reply của thao tác lượt — để `setQueriesData` giữ đúng);
- `HistoryEvent` mục lượt (`GET /customers/{id}/history`);
- `LedgerRow` chỉ hàng `entry = 'sale'`, `kind = one_off`, chưa hủy (`voided = false`), có `visit_id` (như lượt); hàng `void` (id = id hóa đơn, `revenue_book` 0021), gói, hoàn tiền: không có `edited` (D9; tránh hóa đơn đã hủy của bản B bị thay mang "Đã sửa"). `EXISTS` chạy ở **câu ngoài, sau `LIMIT`** của trang;
- `AppointmentDto` khi `GET /appointments?…&marks=true` (web gửi ở `kieu=ds`; lịch ngày / tuần tới 14 ngày, `LIMIT 2000`, không dò); reply của thao tác lịch hẹn mang `edited` khi có quyền (`appointments/cache.ts` ghi đè cache);
- **Không** ở `DaySaleDto`, `CustomerPackageDto` (gói không có "Đã sửa").

Các endpoint danh sách mang `edited` gửi `Cache-Control: no-store` (nội dung phụ thuộc quyền người gọi) nếu chưa gửi.

## 5. Contract (OpenAPI → `packages/api-client`)
- Mới (cộng dồn theo đợt, mỗi đợt chỉ mount route của mình): 13 route `GET /record-history/...`; `RecordHistory`, `RecordRef`, `RecordSummary`, `RecordCreated`, `RecordState`; `PUT /admin/tenants/{id}/record-history/{role}` (request `RecordHistorySwitch`, response `RoleGrants`); query `marks` ở `GET /appointments`.
- Cộng thêm: `ActivityActor.left?`, `FeatureInfo.default_enabled`, `TenantFeature.default_enabled`, `PermissionInfo.not_for_technician`, `PermissionInfo.spa_local`, `edited?` ở `VisitDto`, `HistoryEvent`, `LedgerRow`, `AppointmentDto`.
- Lỗi mới: `role.not_for_technician` (422), `role.record_history_unchanged` (409).
- `fields()` của `visit.update` thêm `session_note` (Flag: writer ghi `"[redacted]"` → `true`, `"cleared"` → `false`, `null` = không đổi, không có thay đổi) và `companions` (Number). Tên trường **`session_note`**, không `note` (web `FIELD.note` = "Ghi chú hồ sơ" đã dùng cho `customer.update`); web `FIELD.session_note` = "Ghi chú buổi". Câu ở màn nhật ký cũng đổi theo.

## 6. Index (0052), perf
- `audit_log_edit_idx ON app.audit_log (tenant_id, entity_type, entity_id) WHERE action IN ('visit.update', 'appointment.update')` — đúng tập action của `edited` (§4.4); mỗi hàng danh sách một lần dò trên tập rất nhỏ.
- `audit_log_staff_day_idx ON app.audit_log (tenant_id, entity_id, (coalesce(after->>'date', before->>'date'))) WHERE action IN ('attendance.record', 'evaluation.daily_review')`; query lặp **đúng** biểu thức.
- Có sẵn: `audit_log_entity_idx (tenant_id, entity_type, entity_id, at)` (0001) cho bước 2; `audit_log_tenant_at_id_idx` (0051) cho bước 3.
- Tạo index trên bảng audit có dữ liệu: `CREATE INDEX` thường trong transaction migration (staging ~7.000 dòng; production chưa có). Khi bảng lớn: `CONCURRENTLY` ở migration riêng (quy ước 0036/0037).
- Ngân sách: `GET /record-history/*` p95 ≤ 100 ms; `/visits` ngày, ledger 92 ngày, `/appointments?marks=true` 14 ngày: p95 tăng ≤ 10 ms. **Bench `reports_bench.rs` seed audit thật**: ~60.000 dòng / năm × 3 (ước tính: ~14.700 lượt / năm × ~2,5 dòng + chấm công / khen 8 người × 365 × 2 + lịch hẹn + danh mục); số 6.787 dòng / năm của staging (log 2026-11-11) là dữ liệu seed nhỏ (1.990 lượt / năm), không dùng làm ngân sách.
- **Đo khi build đợt 3 (2026-11-12, máy dev, cùng bench 44.040 lượt + 183.508 dòng audit / 3 năm, thêm `SETTINGS_SEED`: mỗi dịch vụ sửa 2 lần / tháng, quy tắc lương + số khách tối đa lưu mỗi tháng, 3 năm):** dịch vụ (73 dòng) p95 19 ms, quy tắc lương 10 ms, số khách tối đa 10 ms (≤ 100). EXPLAIN: cả hai câu thành viên đi `audit_log_entity_idx` (0001) — cài đặt `entity_type='tenant' AND entity_id = spa AND action = …` 0,11 ms (36 dòng), dịch vụ 0,11 ms (73 dòng) → **không cần index mới**. Các số cũ cùng lần chạy: lượt 40, gói 11, khách 8, ô ngày 10, `/audit-log` 21 ms; `edited` p95 `/visits` +3, Sổ doanh thu +8, lịch sử khách +1 ms; `marks` p50 +4 ms.
- **Đo khi build đợt 2 (2026-11-12, máy dev, 44.040 lượt + 183.508 dòng audit / 3 năm; bench seed thêm lịch hẹn thật + ô ngày trên id thật, K33):** ô ngày p95 12–13 ms (≤ 50; `audit_log_staff_day_idx`, EXPLAIN 0,13 ms); `/appointments?marks=true` 14 ngày p50 với / không 468 / 467, 436 / 434, 449 / 451 ms (dò 350 lịch hẹn qua `audit_log_edit_idx` ~1 ms; p95 của màn ~450 ms dao động ±25 ms nên bench so trung vị cho `marks`). Lượt / gói / khách vẫn trong 100 ms (48–69 / 10–11 / 7). `/appointments` 14 ngày tự nó ~450 ms trên debug (lượt vãng lai + chuỗi hóa đơn mỗi lịch hẹn): có từ trước, ghi để theo dõi.
- **Đo khi build đợt 1a (2026-11-11, máy dev, `reports_bench.rs` `record_history_reads_within_budget`, 44.040 lượt + 179.466 dòng audit / 3 năm):** p95 lịch sử lượt (chuỗi 3 bản, khách bận nhất) 40 ms, gói 10 ms, khách 6 ms; `/audit-log?limit=50` sau refactor `rows_by_ids` 19 ms; `edited` (đo xen kẽ có / không quyền, 60 vòng): `/visits` 17 / 17 ms, sổ doanh thu 92 ngày 83 / 87 ms, lịch sử khách 29 / 26 ms → thêm ≤ 3 ms. Dò `edited` đi qua `audit_log_edit_idx` (EXPLAIN trong bench).
- Web: `staleTime: 0`, key `["record-history", type, id]`, `MutationCache.onSuccess` invalidate tiền tố (design §5); **một request mỗi lần mở** (prefetch của `LazyRecordTrace` + `refetchOnMount` theo `openedAt` / `isInvalidated` ở `RecordTrace`, spec K51).

## 7. Impact analysis
| Chỗ đổi | Bị ảnh hưởng | Việc |
|---|---|---|
| `Permission` +1 | `identity::roles` test, authz-matrix + `authz_matrix.rs`, `platform.rs::role_templates_start_as_the_code_defaults`, web admin nhãn permission | đợt 1a |
| `SPA_LOCAL` + 2 hàm role (0052) | `platform::roles::set_template` / `set_for_tenant` (kết quả trả về), test `platform.rs` sửa vai trò / mẫu / reset | đợt 1a |
| `features!` thêm cột + cờ | 6 cờ, `parse_keys`, `strip`, `MemberAccess::new`, `identity::service::me`, `platform::features`, `demo/reset.rs`, web `FEATURE` (`sentence.ts`), `FeaturesPanel` | đợt 1a |
| `session_actor` + `member_access` | đọc session ở `identity::repo` | đợt 1a |
| `platform_set_feature` trả null | `platform::features::set` | đợt 1a |
| `activity::list` → `rows_by_ids` | bench nhật ký | đợt 1a |
| Cờ `activity_log` **mặc định tắt** (S1) | `activity_log.rs` (mọi test gọi `/audit-log`, vd. quanh dòng 225, 749) và `demo_seed.rs` (~372) → setup gọi helper mới `common::enable_feature(h, tenant, "activity_log")` (tạo một dòng `platform_operator` cho harness rồi ghi `tenant_feature`); `authz_matrix.rs` dòng `GET /api/v1/audit-log` (owner `200`) → bật cờ trong setup của matrix; `authz_matrix.rs` (~2194) `me.features.len() == Feature::ALL.len()` → đếm cờ **mặc định bật**; e2e `activity.spec.ts` → bật cờ qua API admin trong setup (giữ spec); `demo/reset.rs` xóa `tenant_feature` (cờ operator đã bật mất sau reset, N8) | đợt 1a |
| Module khởi động web (S14) | `lib/queryClient.ts` (`onSuccess`), `features/auth/session.ts` (`hasFeature` `defaultOff`), `app/access.ts` (`mayOpen.activity`), `components/ResponsiveTable.tsx` (`[data-row-inert]`) — vài trăm byte; mã lịch sử còn lại lazy; ngân sách ≤ 125 KiB đo mỗi PR | 1a / 1b |
| `fields()` `visit.update` | câu nhật ký, unit `only_named_fields_leave_and_never_a_note` | đợt 1a |
| `edited` vào DTO | `operations` (lượt, lịch hẹn), `reporting` (ledger, history), web types, `setQueriesData` writers | 1a / 2 |
| `AMOUNT_LABEL` "Đảo" → "Trả lại" | `sentence.ts`, test Vitest nhật ký | đợt 1b |

## 8. Deploy / rollback (0052)
- Thứ tự: **migrate rồi deploy binary mới ngay** (cùng phiên). Trong khoảng hở, binary cũ trên schema 0052:
  - đăng nhập / mọi request spa vẫn chạy (`session_actor` chỉ thêm cột cuối; binary cũ chọn cột theo tên);
  - console: sửa vai trò có thể lỗi `422 unknown_permission` (danh sách đọc về có `record.history.read`, `platform/src/roles.rs` `check_grants`); bật / tắt cờ cho spa chưa có dòng → `500` (`previous!` gặp null). → **operator không ghi gì ở console** trong khoảng hở.
- Rollback binary về trước 0052: như trên (chạy được, trừ hai thao tác console đó); không rollback schema (migration chỉ thêm / thay hàm). Ghi vào runbook vercel-staging khi build.
