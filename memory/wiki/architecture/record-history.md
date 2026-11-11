---
title: Thiết kế backend — lịch sử trên từng bản ghi + cờ màn Nhật ký thao tác
updated: 2026-11-11
sources: [raw/2026-11-11-em-decision-inline-record-history.md]
related: [../product/specs/record-history.md, ../decisions/ADR-0022-inline-record-history.md, ../decisions/ADR-0021-activity-log-for-the-owner.md, platform-admin.md, backend.md, data.md, ../security/authz-matrix.md]
---

# Backend: lịch sử trên từng bản ghi

Spec: [record-history](../product/specs/record-history.md) (D1–D12, review log 3 lượt). Quyết định: [ADR-0022](../decisions/ADR-0022-inline-record-history.md). Chưa build. Bản sửa sau review độc lập 2026-11-11.

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
  - `platform_apply_role_template` (0040, `CREATE OR REPLACE` trong 0052): với mỗi spa theo mẫu, tập mới = (mẫu − SPA_LOCAL) ∪ (SPA_LOCAL hiện có của spa); so sánh "đổi chưa" bỏ qua SPA_LOCAL;
  - `platform_set_tenant_role` (0041, `CREATE OR REPLACE`): cả sửa riêng lẫn "Về mẫu" (`p_permissions IS NULL`): SPA_LOCAL trong danh sách gửi lên bị bỏ qua, giá trị hiện tại giữ nguyên; `follows_template` tính như cũ;
  - phía Rust `set_for_tenant` / `set_template` không cần đổi ngoài rào KTV (DB là nơi giữ bất biến).
- Người đã nghỉ: `effective_permissions` đã cắt về `staff.read_own_past`.

### 2.1 Endpoint công tắc (chỉ operator)
`PUT /api/v1/admin/tenants/{id}/record-history/{role}` body `{visible: bool, reason: string}` (`deny_unknown_fields`, lý do qua `Reason::checked`), 🔐 step-up.
- `role` `owner` | `manager`; `technician` → `422 role.not_for_technician`; khác → `404`.
- Hàm mới `app.platform_set_record_history(p_token_hash bytea, p_tenant uuid, p_role text, p_visible boolean) RETURNS TABLE (role_id uuid, before boolean, after boolean)`:
  - `SECURITY DEFINER`, `SET search_path = app, pg_temp`, `app.platform_gate(p_token_hash, true)`;
  - `IF p_role NOT IN ('owner', 'manager') THEN RAISE EXCEPTION … USING ERRCODE = '22023'` (Rust đã chặn trước, đây là lớp thứ hai);
  - permission **cứng** `'record.history.read'` (không có tham số permission);
  - `SELECT id FROM app.role WHERE tenant_id = p_tenant AND key = p_role AND is_system FOR UPDATE` (không có → không dòng → `404`);
  - thêm (`ON CONFLICT DO NOTHING`) hoặc xóa đúng một dòng `role_permission`; **không đụng** `follows_template`;
  - `REVOKE ALL … FROM PUBLIC; GRANT EXECUTE … TO app_rw`.
- `before == after` → rollback, `409 role.record_history_unchanged`.
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
| `GET /record-history/staff-day/{staff_id}/{date}?book=attendance\|review` | 2 | ô | `attendance.record` / `evaluation.record`; phạm vi `hr::scope::RecordScope::lists(staff_id)` + `period::employed_in(month)`, ngoài → `404` |
| `GET /record-history/adjustment/{id}` | 2 | chuỗi khoản thưởng / phạt | `evaluation.record` + `RecordScope::lists` |
| `GET /record-history/service/{id}`, `/package-plan/{id}`, `/service-category/{id}` | 3 | danh mục | `catalog.manage` |
| `GET /record-history/settings/payroll-rules` | 3 | quy tắc lương | `payroll.read` |
| `GET /record-history/settings/appointment-settings` | 3 | cài đặt lịch hẹn | `appointment.settings` |

Mỗi loại một route Axum (không `{record}` tự do) → loại lạ `404` router; mỗi route một dòng `ROUTES` ở `authz_matrix.rs`. `reporting::history::authorize(actor, Anchor)` là `match` vét cạn trên **loại neo cuối** (B1): đổi neo xong mới kiểm, trước mọi đọc lịch sử. Việc đổi neo chỉ đọc khóa (`sale.visit_id`, có gói trỏ tới hóa đơn không, `refund.customer_package_id`) trong spa của người gọi; id không tồn tại / spa khác → `404` **sau** khi đã qua `record.history.read` + quyền của route (không thành oracle cho người không quyền).

Thứ tự (F17): `401` → `403 auth.password_change_required` → `403 feature.disabled` → `require(RecordHistoryRead)` → quyền của route → parse (`422`) → đổi neo (`404`) → quyền của loại neo cuối (`403`) → đọc.

### 4.2 Response
```
RecordHistory {
  record: { type: "visit" | "customer_package" | "customer" | "appointment" | "attendance" | "daily_review"
                 | "adjustment" | "service" | "package_plan" | "service_category" | "payroll_rules" | "appointment_settings",
            id?: uuid },
  summary: {
    created?: { actor: ActivityActor, at, day, source: "audit" | "record" | "paper_book" },  // D12
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
   | lượt | `entity_type='visit' AND entity_id = ANY(chuỗi)` (chuỗi: `WITH RECURSIVE` lên `corrects_visit_id`, xuống `visit.corrects_visit_id = id`, ≤ 20) ∪ `entity_type='sale' AND entity_id = ANY(hóa đơn dịch vụ lẻ của chuỗi)` (`sale.visit_id ∈ chuỗi` và không có `customer_package.sale_id` trỏ tới — cùng cách suy ra `SaleKind` của Sổ doanh thu) ∪ `action='customer_package.use_expired_override' AND after->>'visit_id' ∈ chuỗi` |
   | gói | `entity_type='customer_package' AND entity_id = gói` ∪ `entity_type='sale' AND entity_id = customer_package.sale_id` (`sale.create`, `sale.void`, `sale.note_update`, `sale.paper_book`) ∪ `action='opening_balance.import' AND entity_id = khách của gói AND after->'customer_package_ids' ? gói` |
   | khách | `entity_type='customer' AND entity_id = khách AND action IN ('customer.create','customer.update','customer.health_note_set')` ∪ `opening_balance.import` của khách có `after->>'customer_created' = 'true'` |
   | lịch hẹn | `entity_type='appointment' AND entity_id = id` |
   | ô ngày | `entity_id = staff_id AND action = <sổ> AND coalesce(after->>'date', before->>'date') = date` (index §6) |
   | thưởng/phạt | `action IN ('staff.adjustment','staff.adjustment_void') AND entity_id = staff_id AND coalesce(after->>'adjustment_id', before->>'adjustment_id') ∈ chuỗi`; chuỗi = khoản bị thay bởi khoản này (cùng `at`, cùng người, cùng nhân viên: một `adjustment_void` + một `adjustment`, do `replace` ghi trong một transaction), lặp ≤ 20 |
   | danh mục / cài đặt | `entity_type` + `entity_id` tương ứng (cài đặt: `entity_type='tenant'` + action) |
3. **Dòng phụ để gộp (chỉ lượt):** để dòng `visit.create` mang "kèm bán gói X" và Thu = mọi hóa đơn, thêm các dòng `sale.create` / `customer_package.create` của **hóa đơn bán gói có `sale.visit_id` ∈ chuỗi** và **cùng `at`** với một dòng `visit.create|addition` đã chọn. Chúng chỉ được gộp vào dòng lượt, không bao giờ là dòng riêng (dòng gốc phải thuộc bước 2). Lịch sử gói **không** lấy dòng lượt (gốc của dòng mua là `sale.create` "bán gói"), nên không cần `visit.read` (B1).
4. **Làm giàu:** `activity::rows_by_ids(tx, ids)` dùng chung (S9, đợt 1a): join tên khách / giới tính / ẩn danh, NV, dịch vụ, mẫu gói, gói, KTV trước / sau, tên + `left` người làm, hạng + khóa gộp. `activity::list` đổi thành "chọn id trang (keyset + lọc + gộp như hiện nay) → `rows_by_ids`", bỏ LATERAL lặp lại.
5. **Gộp:** chỉ lượt, gói (hạng của `fold_rule`, ADR-0021 D9) — trong **đúng tập id** của bước 2–3. Các loại khác (chỉ hạng 0) bỏ gộp: mỗi dòng một mục.
6. **Lọc quyền (D7, D8)** trên mục đã gộp:
   - không `sale.read` → bỏ `amount`, `discount`;
   - không `customer.health_notes.read` → bỏ thay đổi `note` / `health_note`; bỏ hẳn dòng `customer.health_note_set`, `sale.note_update`, và dòng `visit.update` chỉ còn thay đổi ghi chú;
   - không `audit.read` → thay đổi `phone` giữ tên trường, **bỏ `old` / `new`**;
   - khách ẩn danh: như activity.
7. **Tóm tắt:** `created` theo D12 (thao tác tạo; không có → cột `created_by`/`created_at` của lượt, hóa đơn, gói, lịch hẹn, `source = record`; gói `origin = 'opening_balance'` không có dòng → `source = paper_book`, `at` = `starts_on`); `edits` theo §4.4 trên **danh sách đã lọc**; `state` từ chính bản ghi.
8. **Cắt:** `(at, id)` tăng dần, giữ 100 dòng cuối, `omitted`.

### 4.4 "Tính là sửa" (một định nghĩa, D9 / D10)
Hàm thuần `history::counts_as_edit(kind, &row) -> bool`, và **chỉ hai** loại có bản SQL cho `edited` (test `edited_agrees_with_summary_edits` chạy cả hai trên cùng dữ liệu):
| Loại | Tính là sửa | `edited` ở danh sách |
|---|---|---|
| lượt | `visit.correct`, `visit.convert_to_package` (trong chuỗi); `visit.update` có `check_in_at` hoặc `staff` không rỗng | `corrects_visit_id IS NOT NULL OR EXISTS (audit_log_edit_idx: action='visit.update' của lượt, AND (after->>'check_in_at' IS NOT NULL OR jsonb_array_length(coalesce(after->'staff','[]')) > 0))` |
| lịch hẹn | `appointment.update` đổi `starts_at` hoặc `staff_id` | `EXISTS (audit_log_edit_idx: action='appointment.update' …, before/after khác ở 2 khóa đó)`, chỉ khi `marks=true` |
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
- `LedgerRow` hàng dịch vụ lẻ có `visit_id` (như lượt); hàng gói / hoàn tiền không có (D9). `EXISTS` chạy ở **câu ngoài, sau `LIMIT`** của trang;
- `AppointmentDto` khi `GET /appointments?…&marks=true` (web gửi ở `kieu=ds`; lịch ngày / tuần tới 14 ngày, `LIMIT 2000`, không dò); reply của thao tác lịch hẹn mang `edited` khi có quyền (`appointments/cache.ts` ghi đè cache);
- **Không** ở `DaySaleDto`, `CustomerPackageDto` (gói không có "Đã sửa").

Các endpoint danh sách mang `edited` gửi `Cache-Control: no-store` (nội dung phụ thuộc quyền người gọi) nếu chưa gửi.

## 5. Contract (OpenAPI → `packages/api-client`)
- Mới (cộng dồn theo đợt, mỗi đợt chỉ mount route của mình): 13 route `GET /record-history/...`; `RecordHistory`, `RecordRef`, `RecordSummary`, `RecordCreated`, `RecordState`; `PUT /admin/tenants/{id}/record-history/{role}` + `RecordHistorySwitch`; query `marks` ở `GET /appointments`.
- Cộng thêm: `ActivityActor.left?`, `FeatureInfo.default_enabled`, `TenantFeature.default_enabled`, `PermissionInfo.not_for_technician`, `PermissionInfo.spa_local`, `edited?` ở `VisitDto`, `HistoryEvent`, `LedgerRow`, `AppointmentDto`.
- Lỗi mới: `role.not_for_technician` (422), `role.record_history_unchanged` (409).
- `fields()` của `visit.update` thêm `note` (chỉ có / không, không giá trị) và `companions` (số) — câu ở màn nhật ký cũng đổi theo.

## 6. Index (0052), perf
- `audit_log_edit_idx ON app.audit_log (tenant_id, entity_type, entity_id) WHERE action IN ('visit.update', 'appointment.update')` — đúng tập action của `edited` (§4.4); mỗi hàng danh sách một lần dò trên tập rất nhỏ.
- `audit_log_staff_day_idx ON app.audit_log (tenant_id, entity_id, (coalesce(after->>'date', before->>'date'))) WHERE action IN ('attendance.record', 'evaluation.daily_review')`; query lặp **đúng** biểu thức.
- Có sẵn: `audit_log_entity_idx (tenant_id, entity_type, entity_id, at)` (0001) cho bước 2; `audit_log_tenant_at_id_idx` (0051) cho bước 3.
- Tạo index trên bảng audit có dữ liệu: `CREATE INDEX` thường trong transaction migration (staging ~7.000 dòng; production chưa có). Khi bảng lớn: `CONCURRENTLY` ở migration riêng (quy ước 0036/0037).
- Ngân sách: `GET /record-history/*` p95 ≤ 100 ms; `/visits` ngày, ledger 92 ngày, `/appointments?marks=true` 14 ngày: p95 tăng ≤ 10 ms. **Bench `reports_bench.rs` seed audit thật**: ~60.000 dòng / năm × 3 (ước tính: ~14.700 lượt / năm × ~2,5 dòng + chấm công / khen 8 người × 365 × 2 + lịch hẹn + danh mục); số 6.787 dòng / năm của staging (log 2026-11-11) là dữ liệu seed nhỏ (1.990 lượt / năm), không dùng làm ngân sách.
- Web: `staleTime: 0`, key `["record-history", type, id]`, `MutationCache.onSuccess` invalidate tiền tố (design §5).

## 7. Impact analysis
| Chỗ đổi | Bị ảnh hưởng | Việc |
|---|---|---|
| `Permission` +1 | `identity::roles` test, authz-matrix + `authz_matrix.rs`, `platform.rs::role_templates_start_as_the_code_defaults`, web admin nhãn permission | đợt 1a |
| `SPA_LOCAL` + 2 hàm role (0052) | `platform::roles::set_template` / `set_for_tenant` (kết quả trả về), test `platform.rs` sửa vai trò / mẫu / reset | đợt 1a |
| `features!` thêm cột + cờ | 6 cờ, `parse_keys`, `strip`, `MemberAccess::new`, `identity::service::me`, `platform::features`, `demo/reset.rs`, web `FEATURE` (`sentence.ts`), `FeaturesPanel` | đợt 1a |
| `session_actor` + `member_access` | đọc session ở `identity::repo` | đợt 1a |
| `platform_set_feature` trả null | `platform::features::set` | đợt 1a |
| `activity::list` → `rows_by_ids` | `activity_log.rs` (phải xanh nguyên), bench nhật ký | đợt 1a |
| `fields()` `visit.update` | câu nhật ký, unit `only_named_fields_leave_and_never_a_note` | đợt 1a |
| `edited` vào DTO | `operations` (lượt, lịch hẹn), `reporting` (ledger, history), web types, `setQueriesData` writers | 1a / 2 |
| `AMOUNT_LABEL` "Đảo" → "Trả lại" | `sentence.ts`, test Vitest nhật ký | đợt 1b |

## 8. Deploy / rollback (0052)
- Thứ tự: **migrate rồi deploy binary mới ngay** (cùng phiên). Trong khoảng hở, binary cũ trên schema 0052:
  - đăng nhập / mọi request spa vẫn chạy (`session_actor` chỉ thêm cột cuối; binary cũ chọn cột theo tên);
  - console: sửa vai trò có thể lỗi `422 unknown_permission` (danh sách đọc về có `record.history.read`, `platform/src/roles.rs` `check_grants`); bật / tắt cờ cho spa chưa có dòng → `500` (`previous!` gặp null). → **operator không ghi gì ở console** trong khoảng hở.
- Rollback binary về trước 0052: như trên (chạy được, trừ hai thao tác console đó); không rollback schema (migration chỉ thêm / thay hàm). Ghi vào runbook vercel-staging khi build.
