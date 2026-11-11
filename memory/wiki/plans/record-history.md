---
title: "Plan: lịch sử trên từng bản ghi (đợt 1a + 1b, 2, 3)"
updated: 2026-11-11
sources: [raw/2026-11-11-em-decision-inline-record-history.md]
related: [../product/specs/record-history.md, ../architecture/record-history.md, ../design/record-history.md, ../decisions/ADR-0022-inline-record-history.md]
---

# Plan: lịch sử trên từng bản ghi

Mỗi PR qua `build` → `qa` → `pr-review` → `secure` (quyền + PII) → `scripts/merge-when-green.sh`. Bản sửa sau review độc lập 2026-11-11 (LEAD: gộp đợt 2 cũ vào đợt 1; ẩn màn nhật ký ngay đợt 1). Chưa bắt đầu.

## Đợt 1 = 1a + 1b — **lên staging cùng nhau**
1a merge trước nhưng **không deploy staging** cho tới khi 1b merge (1a ẩn màn nhật ký; 1b mới hiện lịch sử tại chỗ). Deploy: migrate 0052 rồi deploy binary ngay; operator không ghi ở console trong khoảng hở ([architecture §8](../architecture/record-history.md)).

### 1a — hạ tầng, công tắc, cờ, API (PR backend + console)
- Migration `0052_record_history.sql`:
  - `record.history.read` vào mẫu owner + vai trò owner hệ thống của mọi spa (`ON CONFLICT DO NOTHING`);
  - `CREATE OR REPLACE` `platform_apply_role_template`, `platform_set_tenant_role` (spa-local: bỏ qua / giữ `record.history.read`);
  - hàm `platform_set_record_history` (gate + step-up, permission + vai trò cứng, `FOR UPDATE`, không đụng `follows_template`), REVOKE / GRANT;
  - `session_actor` DROP / CREATE + `enabled_features` + **REVOKE / GRANT lại**;
  - `platform_set_feature` trả null khi chưa có dòng;
  - index `audit_log_edit_idx`, `audit_log_staff_day_idx`.
- Rust: `Permission::RecordHistoryRead`; `features!` + `default_enabled` + `ActivityLog`; `feature::disabled` ở cả `session_actor` và `identity::repo::member_access`; `platform::roles` `NOT_FOR_TECHNICIAN`, `SPA_LOCAL`, `PermissionInfo` 2 trường; `platform::features` mặc định; endpoint `PUT /admin/tenants/{id}/record-history/{role}`.
- `reporting`: `activity::list` → chọn id + `rows_by_ids` (S9); `fields()` `visit.update` thêm `note` (có / không) + `companions`; module `history` (`authorize` vét cạn theo loại neo cuối, chọn id dòng theo loại, gộp chỉ lượt / gói, lọc quyền, `counts_as_edit`, tóm tắt D12); route `visit`, `sale`, `refund`, `customer-package`, `customer`.
- `edited` ở `VisitDto` (+ reply thao tác lượt), `HistoryEvent` (lượt), `LedgerRow` (hàng dịch vụ lẻ, `EXISTS` sau `LIMIT`); `no-store` cho các danh sách này.
- OpenAPI + `pnpm api:gen`.
- Web (nhỏ, cùng PR vì phụ thuộc contract): `mayOpen.activity` + `hasFeature(…, { defaultOff })`; console: panel "Lịch sử trên từng bản ghi", `FeaturesPanel` "Mặc định · bật / tắt", trình sửa vai trò (ô spa-local chỉ đọc, ô KTV tắt), `AuditPage` `ACTIONS`.

**Test 1a**
- Unit: `feature::disabled` bảng chân trị; khóa `default_enabled` từng cờ; `the_activity_screen_switch_owns_its_route_not_audit_read`; `administration_is_never_a_switchable_feature` (xanh, không sửa); `check_grants` KTV → `role.not_for_technician` (mẫu + spa); owner thiếu `record.history.read` vẫn qua owner floor; `counts_as_edit` từng loại; tóm tắt D12 (gói sổ giấy gia hạn vẫn "Nhập từ sổ giấy"; khách tạo từ sổ giấy rồi sửa vẫn là người nhập); `SPA_LOCAL` Rust = SQL.
- `record_history.rs` (mới):
  - quyền: chủ spa `200`; quản lý mặc định `403`, bật `200`, tắt `403`; KTV, người đã nghỉ `403`; spa khác `404`; `no-store`; người không quyền gửi id rác vẫn `403` (không oracle);
  - B1: `sale.read` + `customer.read` thiếu `visit.read` → hóa đơn dịch vụ lẻ `403`, hóa đơn gói `200` không có dòng lượt; refund cần `sale.read` + `customer.read`;
  - B2: lượt bán kèm gói X, sau đó gia hạn + hủy hoàn tiền X → lịch sử lượt không có hai dòng đó, lịch sử gói có; `edited` lượt vẫn `false`;
  - gộp: một dòng cho lượt có thu tiền; chuỗi A→B→C cùng câu chuyện từ cả 3 id, "đã sửa 2 lần"; chuyển sang gói một dòng; `use_expired_override` có ở lượt và gói;
  - D9: thêm vào lượt không tính sửa; đổi KTV tính; chỉ đổi ghi chú không tính, và **không có dòng** với người thiếu `customer.health_notes.read`;
  - lọc: không `sale.read` → không `amount`; SĐT: chủ spa có giá trị, quản lý được bật không có chuỗi số nào trong body; khách ẩn danh (giả lập `anonymized_at`) không tên / giá trị;
  - seed giờ lùi giữ giờ; lượt không có audit → `source = record`; người làm đã nghỉ `left: true`;
  - `edited` vắng khi không quyền; `edited_agrees_with_summary_edits`;
  - `GET /audit-log` mặc định `403 feature.disabled`, bật `200`, `/me.features`.
- `activity_log.rs` (10 + 4 test hiện có) xanh nguyên sau refactor; thêm câu `visit.update` chỉ ghi chú / người đi cùng.
- `platform.rs`: công tắc (step-up, `409`, `422` KTV, `404`, session spa `401`, audit hai phía, hàm DB `42501` khi không step-up, `22023` khi vai trò lạ gọi thẳng); spa-local: bật cho quản lý → sửa mẫu manager → spa theo mẫu nhận quyền khác, giữ công tắc, `follows_template` không đổi; "Về mẫu" giữ công tắc; `PUT …/roles/manager` có / không `record.history.read` không đổi công tắc; cờ mặc định tắt `enabled: false`, `previous` theo mặc định.
- `authz_matrix.rs`: 5 route lịch sử + route admin; `matrix_rows_agree_with_seeded_roles`.
- Bench `reports_bench.rs`: seed audit ~60.000 / năm × 3; `record-history/visit` (chuỗi 3) + `customer-package` + `customer` p95 ≤ 100 ms; `/visits` ngày, ledger 92 ngày có `edited` ≤ +10 ms; nhật ký sau refactor không chậm hơn bench AC11 hiện có.
- Web: Vitest `hasFeature` `defaultOff`, nav ẩn "Nhật ký thao tác", panel admin (bật / tắt, `409`).

### 1b — các màn (PR web)
- `features/record-history/query.ts` + chunk lazy `RecordTrace` / `HISTORY` / `LedgerSaleSheet`; prefetch `useAfterFirstPaint` khi có quyền; `MutationCache.onSuccess`.
- `ResponsiveTable` `[data-row-inert]`.
- Chip: "Đã thay" thay "Đã sửa" cũ (`TodayVisits`, `CustomerHistory`, chữ "Đã thay bằng lượt mới."), "Đã sửa" theo `edited`, một chip.
- Quầy (`VisitDetails`, bỏ lặp "Nhập bù · Ghi lúc"), `DaySales` (⋯), Lịch sử khách (giờ là nút), Sổ doanh thu (ô giờ / thẻ → sheet, gồm hàng gói + hoàn tiền), `PackageRow` (nút "Lịch sử"), `CustomerProfile` (sau `detail-grid`).
- `AMOUNT_LABEL` "Đảo" → "Trả lại".
**Test 1b:** Vitest `RecordTrace` (meta, mở / đóng, lỗi, `404`, `403` → ẩn + focus, trống, `omitted`, "(đã nghỉ)", "Đội vận hành"), `HISTORY` phủ mọi `AuditAction` (typecheck), `data-row-inert` không gập thẻ, chip ưu tiên; e2e `record-history.spec.ts` (chủ spa mở lượt thấy "Ghi bởi", đổi KTV → "Đã sửa", Sửa lượt → "Đã thay" + "Đã sửa", quản lý mặc định không thấy, Sổ doanh thu sheet, hồ sơ khách SĐT cũ; 360 + 1280 px, axe); `pnpm --filter web budget` ≤ 125 KiB (khởi động không tăng).

## Cửa sổ hồi quy (từ khi đợt 1 lên staging tới đợt 2 / 3)
Màn nhật ký tắt, nên chủ spa **không xem được trên UI** ai làm những việc sau cho tới khi đợt tương ứng xong: đổi / hủy / không đến lịch hẹn, chấm công, khen / phàn nàn, sửa / xóa thưởng phạt (đợt 2); sửa giá, mẫu gói, nhóm dịch vụ, quy tắc lương, cài đặt lịch hẹn (đợt 3); việc của đội vận hành, tài khoản / mật khẩu, vai trò, cờ (không có đợt nào: chỉ nhật ký). Dữ liệu vẫn đủ ở `audit_log`. Giảm thiểu: chủ spa / EM cần thì operator bật cờ `activity_log` cho spa đó (một phút, có lý do + audit). Ghi vào hướng dẫn nghiệm thu khi deploy.

## Đợt 2 — lịch hẹn, chấm công, khen / phàn nàn, thưởng / phạt
- Route `appointment`, `staff-day`, `adjustment`; `edited` ở `AppointmentDto` khi `marks=true` (+ reply thao tác lịch hẹn); chuỗi thưởng / phạt.
- Web: `AppointmentSheet` (phần tử cuối), chip ở `kieu=ds`, `PersonDaySheet` (cuối mỗi `fieldset`), `AdjustmentsTab` ⋯.
- Test: cờ lịch hẹn tắt → `403 feature.disabled`; ô **của chính mình** `404`, ô của quản lý khác / chủ spa `200`, người không làm trong tháng (`employed_in`) `404`; một lần lưu sổ nhiều ô → lịch sử ô chỉ có ngày của nó; không chữ ghi chú chấm công; ô xóa trắng "xóa chấm công"; thưởng / phạt sửa = một chuỗi, "đã sửa 1 lần"; bench ô ≤ 50 ms với index 0052; `/appointments?marks=true` 14 ngày ≤ +10 ms.

## Đợt 3 — danh mục, quy tắc lương, cài đặt lịch hẹn
- Route `service`, `package-plan`, `service-category`, `settings/payroll-rules`, `settings/appointment-settings`.
- Web: phần tử cuối của `ServiceForm`, `PlanForm`, form nhóm, `RulesSheet`, sheet số khách tối đa.
- Test: quản lý được bật nhưng không `catalog.manage` → `403`; cờ lương tắt → `403 feature.disabled`; giá Nữ / Nam cũ → mới.

## Không làm (đã có cách hiện riêng)
Lịch sử hồ sơ nhân viên, đánh giá tháng, chốt lương, gọi lại khách (spec D11). Màn riêng cho việc của đội vận hành khi cờ tắt (LEAD: không làm).
