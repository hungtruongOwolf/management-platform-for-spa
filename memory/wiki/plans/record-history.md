---
title: "Plan: lịch sử trên từng bản ghi (4 đợt)"
updated: 2026-11-11
sources: [raw/2026-11-11-em-decision-inline-record-history.md]
related: [../product/specs/record-history.md, ../architecture/record-history.md, ../design/record-history.md, ../decisions/ADR-0022-inline-record-history.md]
---

# Plan: lịch sử trên từng bản ghi

Mỗi đợt là một PR (hoặc 2: API rồi web) **ship được riêng**: không đợt nào cần đợt sau để chạy đúng. Mỗi PR qua `build` → `qa` → `pr-review` → `secure` (chạm quyền + PII) → `scripts/merge-when-green.sh`. Chưa bắt đầu.

## Đợt 1 — công tắc, ẩn màn nhật ký, lượt + hóa đơn + Sổ doanh thu
**Backend**
- R1.1 Migration `0052_record_history.sql`: `record.history.read` vào `role_template_permission` (owner) + `role_permission` của vai trò owner hệ thống mọi spa; `session_actor` thêm `enabled_features`; `platform_set_feature` trả null khi chưa có dòng; hàm `platform_set_role_permission` (gate + step-up + `FOR UPDATE`).
- R1.2 `kernel`: `Permission::RecordHistoryRead`; `features!` thêm `default_enabled` + cờ `ActivityLog`; `feature::disabled(on, off)`. `identity`: đọc `enabled_features` (session + `tenant_access`), `SystemRole` mặc định. `platform::roles`: `NOT_FOR_TECHNICIAN`, `PermissionInfo.not_for_technician`; `platform::features`: `default_enabled`, `previous` theo mặc định; endpoint `PUT /admin/tenants/{id}/record-history/{role}`.
- R1.3 `reporting::history`: module, `authorize` (`match` vét cạn), neo lượt (chuỗi) / hóa đơn (route `refund` mount ở đợt 2; mỗi đợt chỉ mount route của mình, nên OpenAPI + matrix luôn khớp), `activity::rows_by_ids`, lọc quyền, tóm tắt; route `visit`, `sale`.
- R1.4 `edited` ở `VisitDto` (`operations`), `LedgerRow` (`reporting`), `DaySaleDto` (`sales`), `HistoryEvent` mục lượt (`reporting`).
- R1.5 OpenAPI + `pnpm api:gen`.

**Web**
- W1.1 `mayOpen.activity` thêm `hasFeature("activity_log")`.
- W1.2 Chunk lazy `record-history`: `RecordTrace`, `HISTORY_VERB` (`Record<AuditAction, string | null>`), `LedgerSaleSheet`.
- W1.3 Quầy (`VisitDetails` + badge), `DaySales`, Lịch sử khách (chế độ bấm-mới-tải), Sổ doanh thu (hàng bấm được khi có quyền, trừ hàng Hoàn tiền).
- W1.4 Admin: panel "Lịch sử trên từng bản ghi", `FeaturesPanel` "Mặc định · bật / tắt", nhãn permission + ô KTV tắt trong trình sửa vai trò.

**Test**
- Unit: `feature::disabled` bảng chân trị; `the_activity_screen_switch_owns_its_route_not_audit_read`; `administration_is_never_a_switchable_feature` (không đổi, phải xanh); `check_grants` KTV + `record.history.read` → `role.not_for_technician` (mẫu + spa); owner không có nó vẫn qua owner floor; `mask_phone`; tóm tắt `edits` / `state`.
- Integration (`app/tests/record_history.rs` mới): chủ spa `200` lượt + hóa đơn; quản lý mặc định `403`, bật → `200`, tắt → `403`; KTV `403`; người đã nghỉ `403`; id spa khác `404`; `no-store`; một dòng cho lượt có thu tiền; chuỗi Sửa lượt A→B→C cùng câu chuyện từ cả 3 id; chuyển sang gói một dòng; hóa đơn trong lượt → `record.type = visit`; quản lý không `sale.read` → không key `amount`; không `customer.health_notes.read` → không thay đổi `note`, không dòng `sale.note_update`; seed giờ lùi giữ đúng giờ; lượt không có audit → `source = record`; người làm đã nghỉ `left: true`; `edited` vắng khi không quyền, có khi có; `GET /audit-log` mặc định `403 feature.disabled`, bật → `200`, `/me.features`.
- `platform.rs`: endpoint công tắc (step-up, `409` không đổi, `422` KTV, `404` role / spa lạ, session spa `401`, audit hai phía, hàm DB tự từ chối khi không step-up); `platform_set_feature` previous = mặc định của cờ; cờ mặc định tắt hiện `enabled: false`.
- `authz_matrix.rs`: dòng cho 2 route lịch sử + route admin; `matrix_rows_agree_with_seeded_roles`.
- Bench `reports_bench.rs`: `record-history/visit` (chuỗi 3 bản) p95 ≤ 100 ms; `/visits` ngày + ledger 92 ngày có `edited` ≤ +10 ms.
- Web: Vitest `RecordTrace` (meta, mở / đóng, lỗi, trống, `omitted`, người đã nghỉ, "Đội vận hành"), `HISTORY_VERB` phủ mọi `AuditAction` (typecheck), nav ẩn "Nhật ký thao tác"; e2e `record-history.spec.ts` (chủ spa mở lượt thấy "Ghi bởi", sửa lượt thấy "Đã sửa", quản lý không thấy; 360 px, axe), admin e2e panel bật quản lý; `pnpm --filter web budget` ≤ 125 KB.

## Đợt 2 — gói của khách, hồ sơ khách, hàng Hoàn tiền
- Route `customer-package`, `refund`, `customer`; `edited` ở `CustomerPackageDto`; neo gói + hóa đơn bán nó; sổ giấy `source = paper_book`; che SĐT cũ (D8); dòng lưu ý sức khỏe theo quyền.
- Web: `PackageRow` (bấm-mới-tải), đầu `CustomerProfile` (tự tải), Sổ doanh thu hàng Hoàn tiền bấm được.
- Test: chủ spa thấy SĐT cũ đủ, quản lý được bật thấy che, số trùng SĐT hiện tại không che; khách ẩn danh không tên / không giá trị (giả lập `anonymized_at`); gói hủy hoàn tiền → `state = cancelled`, dòng hoàn tiền có tiền khi `sale.read`; gói sổ giấy; `edited` gói gia hạn / dùng gói hết hạn; matrix 3 dòng.

## Đợt 3 — lịch hẹn, chấm công, khen / phàn nàn
- Route `appointment`, `staff-day`; `edited` ở `AppointmentDto`; ô theo `(staff_id, date, book)`, phạm vi `hr::scope`.
- Web: `AppointmentSheet`, `PersonDaySheet` (2 khối), badge ở lịch dạng danh sách.
- Test: cờ lịch hẹn tắt → `403 feature.disabled`; quản lý xem ô của chủ spa / quản lý khác → `404` (quyết định C); không có chữ ghi chú chấm công; ô xóa trắng (`cleared`) hiện "xóa chấm công"; bench ô chấm công ≤ 50 ms (nếu vượt: index biểu thức, migration 0053).

## Đợt 4 — danh mục, quy tắc lương, cài đặt lịch hẹn
- Route `service`, `package-plan`, `service-category`, `settings/payroll-rules`, `settings/appointment-settings`; verb "Sửa lần cuối bởi".
- Web: `ServiceForm`, `PlanForm`, form nhóm, `RulesSheet`, sheet số khách tối đa.
- Test: quản lý (không `catalog.manage`) `403` dù được bật; cờ lương tắt → `403 feature.disabled`; giá Nữ / Nam cũ → mới.

## Ngoài kế hoạch (đã có, không làm)
Lịch sử hồ sơ NV, thưởng/phạt, đánh giá tháng, chốt lương, gọi lại khách: giữ cách hiện hiện tại (spec D11).
