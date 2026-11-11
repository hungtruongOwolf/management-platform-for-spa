---
title: "Spec: Lịch sử trên từng bản ghi (ai ghi, ai sửa) + tạm ẩn Nhật ký thao tác"
status: approved
updated: 2026-11-11
sources: [raw/2026-11-11-em-decision-inline-record-history.md, raw/2026-11-11-em-actor-trace-data-only.md, raw/2026-11-11-em-decision-activity-log-and-customer-old-values.md, raw/2026-11-11-em-decision-activity-log-one-line-per-action.md]
related: [management-views.md, front-desk-and-packages.md, appointments.md, staff-management.md, platform-admin.md, ../../decisions/ADR-0022-inline-record-history.md, ../../decisions/ADR-0021-activity-log-for-the-owner.md, ../../architecture/record-history.md, ../../design/record-history.md, ../../plans/record-history.md]
---

# Spec: Lịch sử trên từng bản ghi + tạm ẩn "Nhật ký thao tác"

**Status: approved** (EM 2026-11-11 giao lead tự phân tích, tự review, không hỏi lại; [raw](../../../raw/2026-11-11-em-decision-inline-record-history.md)). Chưa build. Thiết kế backend: [architecture/record-history](../../architecture/record-history.md). Thiết kế UI: [design/record-history](../../design/record-history.md). Kế hoạch: [plans/record-history](../../plans/record-history.md). Quyết định: [ADR-0022](../../decisions/ADR-0022-inline-record-history.md).

**Vì sao là trang spec riêng, không phải một mục của management-views:** tính năng cắt ngang 4 spec (Quầy + gói, lịch hẹn, nhân sự, danh mục) và màn console nền tảng; management-views §11 chỉ nói về màn "Nhật ký thao tác" độc lập. §11 được thêm một dòng trỏ về đây (màn đó nay ẩn mặc định).

## 1. Goal
Chủ spa muốn **truy được ai ghi, ai sửa, ai hủy** một bản ghi ngay tại chỗ đang xem (lượt, hóa đơn, gói, lịch hẹn, hồ sơ khách, ô chấm công…), không phải mở một màn nhật ký riêng rồi tìm. EM 2026-11-11 đảo quyết định sáng cùng ngày ("không cần show") và chọn:
1. Mỗi bản ghi có một **dòng nhỏ, màu nhạt**: "Ghi bởi Hà · 15:10 · đã sửa 1 lần". Bấm vào thì mở **lịch sử của chính bản ghi đó** (ai, lúc nào, đổi gì, lý do). **Mặc định đóng.**
2. **Mặc định hiện cho chủ spa.** Có **công tắc**, đặt ở **console admin nền tảng** (EM: "công tắc bật tắt là admin control nhé chơ không phải ở account của spa và quản lý đâu"). Operator bật/tắt riêng cho chủ spa và cho quản lý. KTV không bao giờ thấy.
3. **Danh sách** chỉ hiện dấu nhỏ "Đã sửa" / "Đã hủy"; **tên người và lịch sử đầy đủ** chỉ ở chỗ mở chi tiết.
4. Màn "Nhật ký thao tác" (`/nhat-ky-thao-tac`) **giữ trong code nhưng tạm ẩn**, tắt từ admin (cờ tính năng theo spa, **mặc định tắt**).

Phục vụ thước đo R1 #1 (bỏ sổ giấy): sổ giấy có chữ người ghi; app phải trả lời được "ai ghi dòng này" ngay trên dòng đó.

## 2. Personas & quyền
- **Chủ spa:** thấy dòng "ai ghi" + lịch sử ở mọi chỗ trong phạm vi (§4), mặc định bật.
- **Quản lý (kiêm thu ngân):** mặc định **không** thấy; operator bật được cho từng spa.
- **KTV:** không bao giờ (server từ chối cấp quyền cho vai trò KTV; các màn của KTV `/me/*`, "Khách của tôi" không có tính năng này).
- **Người đã nghỉ:** không (quyền suy ra chỉ còn `staff.read_own_past`, ADR-0014).
- **Operator nền tảng:** bật/tắt công tắc (cho chủ spa, cho quản lý) và cờ "Nhật ký thao tác" theo spa; **không** đọc được lịch sử bản ghi của spa (console chỉ metadata, ADR-0018).

### 2.1 Quyết định (lead, theo ủy quyền của EM; chi tiết + lý do ở [ADR-0022](../../decisions/ADR-0022-inline-record-history.md))
| # | Quyết định | Lý do ngắn |
|---|---|---|
| D1 | Công tắc = permission mới **`record.history.read`** ("Xem ai ghi, ai sửa trên từng bản ghi"). Chủ spa có trong mẫu vai trò (mặc định bật), quản lý không (mặc định tắt). | "Permission là dữ liệu" ([principles](../../architecture/principles.md)); operator đã có sẵn công cụ sửa vai trò theo spa có step-up + audit hai phía (ADR-0018 D5); server kiểm bằng `Actor::require` như mọi quyền khác (deny by default). |
| D2 | Operator bật/tắt bằng **panel riêng** "Lịch sử trên từng bản ghi" ở trang spa `/admin/spa/<id>` (hai công tắc: Chủ spa, Quản lý), gọi endpoint riêng chỉ đổi đúng một permission (hàm DB khóa dòng role). Cũng có trong trình sửa vai trò / mẫu vai trò như mọi permission. | Operator không phải dò 40 ô; đổi một quyền nguyên tử, không đè quyền khác. |
| D3 | `record.history.read` **không** nằm trong owner floor (tắt được cho chủ spa) và **không bao giờ** cấp được cho KTV (`422 role.not_for_technician`, cả mẫu lẫn spa). | EM: "bật tắt được", "default là hiện cho chủ spa"; KTV không thuộc diện. |
| D4 | Không có công tắc nào trong app của spa (Cài đặt, tài khoản chủ spa / quản lý). Route `/admin/*` chỉ nhận cookie operator: session spa gọi → `401`. | EM làm rõ: chỉ admin control. |
| D5 | Màn "Nhật ký thao tác" ẩn bằng **cờ tính năng mới `activity_log`**, **mặc định tắt** (cờ đầu tiên mặc định tắt; registry có thêm `default_enabled`). Cờ chỉ sở hữu **route** `/api/v1/audit-log`, **không** sở hữu `audit.read`. | Giữ test `administration_is_never_a_switchable_feature`: `audit.read` vẫn là quyền quản trị không tắt được (owner floor), chỉ màn bị ẩn. Route tắt → `403 feature.disabled` như 6 cờ cũ. |
| D6 | Lịch sử đọc từ `app.audit_log` (không bảng mới), **gộp một thao tác một dòng** đúng luật D9 của ADR-0021 (`reporting::activity::fold_rule`), cùng allowlist trường. | Một nguồn sự thật cho màn nhật ký và lịch sử bản ghi; không lộ `before/after` thô. |
| D7 | Dữ liệu trong lịch sử giữ **quyền đọc của màn gốc**: tiền chỉ khi có `sale.read`; dòng ghi chú buổi / ghi chú gói / lưu ý sức khỏe (chỉ "đã đổi", không bao giờ nội dung) chỉ khi có `customer.health_notes.read`; lương chỉ khi có `payroll.read`. | Entitled data only (non-negotiable #4), giống Nhật ký ghi chú. |
| D8 | SĐT cũ → mới của khách: chủ spa (`audit.read`) thấy đủ; người khác (quản lý được bật) thấy **che giữa** mọi số khác SĐT hiện tại ("0905 ••• 222"). Tên cũ → mới hiện đủ cho ai mở được hồ sơ. | Số cũ có thể đã thuộc người khác (nhà mạng cấp lại); quản lý không cần số cũ để làm việc. ADR-0021 chỉ cho chủ spa xem SĐT cũ. |
| D9 | Danh sách chỉ có cờ `edited` (đã sửa) cho người có quyền; "Đã hủy" dùng trạng thái sẵn có (không đổi, mọi người có quyền đọc danh sách vẫn thấy). Tên người chỉ ở chi tiết. | EM câu 3. |
| D10 | Dòng "ai ghi" + lịch sử tải bằng **một endpoint chung** `GET /record-history/...` khi mở chi tiết; DTO hiện có chỉ thêm `edited`. | Một chỗ kiểm quyền (dễ review, ít rủi ro rò); không đổi hình dạng mọi DTO chi tiết; danh sách không tốn thêm tên. |
| D11 | Chỗ đã hiện "ai làm" từ trước giữ nguyên, không làm thêm: lịch sử hồ sơ nhân viên (`GET /staff/{id}/history`), "Người ghi" thưởng/phạt, "Chấm bởi" đánh giá tháng, "Chốt bởi" bảng lương, người gọi lại khách. | Tránh hai cách hiện cho cùng một việc. |

## 3. User stories
- **US-H1** Là chủ spa, tôi muốn thấy ngay trên một lượt / hóa đơn / gói / lịch hẹn ai ghi, lúc nào, đã sửa mấy lần, để biết hỏi ai khi số liệu lạ.
- **US-H2** Là chủ spa, tôi muốn bấm vào dòng đó để xem từng thay đổi (ai, lúc nào, đổi gì, lý do), để truy một lần hủy / sửa giá / đổi SĐT khách.
- **US-H3** Là chủ spa, tôi muốn danh sách chỉ hiện dấu "Đã sửa" / "Đã hủy" nhỏ, để danh sách gọn mà vẫn biết dòng nào có chuyện.
- **US-H4** Là operator, tôi muốn bật / tắt lịch sử này riêng cho chủ spa và cho quản lý của một spa, có lý do và nhật ký, để làm đúng yêu cầu từng spa.
- **US-H5** Là operator, tôi muốn màn "Nhật ký thao tác" tắt mặc định và bật lại được cho từng spa, để giữ công cụ mà không làm rối menu chủ spa.
- **US-H6** Là quản lý được bật lịch sử, tôi muốn thấy cùng dòng "ai ghi" như chủ spa nhưng không thấy số điện thoại cũ của khách, để làm việc mà không lộ dữ liệu không cần.

## 4. Phạm vi: mọi action audit (64 key của `kernel::audit::Action`) → có lịch sử tại chỗ không
"Bản ghi" = thứ người dùng mở ra xem. Một dòng lịch sử = một **thao tác** (gộp theo D9 ADR-0021), nên một action có thể chỉ xuất hiện như phần gộp của dòng khác. Cột "Đợt" = wave ở [plan](../../plans/record-history.md).

### 4.1 Có lịch sử tại chỗ
| Action | Bản ghi (neo) | Màn hiện dòng "ai ghi" + lịch sử | Dấu ở danh sách | Đợt |
|---|---|---|---|---|
| `visit.create` | lượt | Quầy (hàng lượt mở rộng `VisitDetails`), Lịch sử khách (thẻ lượt), Sổ doanh thu (sheet hóa đơn của lượt) | — (dòng đầu) | 1 |
| `visit.addition` | lượt | như trên | Đã sửa | 1 |
| `visit.update` (giờ, KTV, người đi cùng, ghi chú buổi: chỉ "đã đổi") | lượt | như trên | Đã sửa | 1 |
| `visit.correct` (Sửa lượt) | chuỗi lượt (cũ → mới) | như trên, **một câu chuyện cho cả chuỗi** (§6.1) | Đã sửa (cả bản cũ lẫn bản mới) | 1 |
| `visit.convert_to_package` | chuỗi lượt | như trên | Đã sửa | 1 |
| `visit.void` | lượt | như trên | Đã hủy (trạng thái sẵn có) | 1 |
| `sale.create` | hóa đơn | gộp vào dòng lượt khi thuộc lượt; hóa đơn bán gói riêng: Sổ doanh thu, "Bán gói hôm nay" (`DaySales`), gói của khách | — | 1 (gói: 2) |
| `sale.void` | hóa đơn | như trên | Đã hủy (sẵn có) | 1 |
| `sale.note_update` (ghi chú gói: chỉ "đã đổi") | hóa đơn bán gói | Sổ doanh thu, gói của khách; chỉ khi có `customer.health_notes.read` | Đã sửa | 1 (gói: 2) |
| `customer_package.create` | gói | gói của khách (thẻ gói trong hồ sơ khách), sheet dòng "Hoàn tiền" ở Sổ doanh thu | — | 2 |
| `customer_package.extend` | gói | như trên | Đã sửa | 2 |
| `customer_package.cancel` | gói | như trên | Đã hủy (sẵn có) | 2 |
| `customer_package.use_expired_override` | gói | như trên (dòng riêng, có lý do) | Đã sửa | 2 |
| `opening_balance.import`, `sale.paper_book` | khách / hóa đơn sổ giấy | gói của khách: dòng đầu "Nhập từ sổ giấy" | — | 2 |
| `customer.create` | khách | hồ sơ khách (dưới tên) | — | 2 |
| `customer.update` (tên + SĐT cũ → mới, trường khác chỉ "đã đổi") | khách | hồ sơ khách | — (không có danh sách khách mang dấu) | 2 |
| `customer.health_note_set` (chỉ "đã đổi") | khách | hồ sơ khách, chỉ khi có `customer.health_notes.read` | — | 2 |
| `appointment.create`, `.update`, `.arrive`, `.undo_arrival`, `.complete`, `.no_show`, `.cancel` | lịch hẹn | sheet lịch hẹn (`AppointmentSheet`) | Đã sửa (`update`, `undo_arrival`) ở dạng danh sách; Đã hủy / Không đến sẵn có | 3 |
| `attendance.record` | ô chấm công (nhân viên × ngày) | `PersonDaySheet` phần chấm công (không bao giờ nội dung ghi chú) | — (ô lưới quá nhỏ, §4.3) | 3 |
| `evaluation.daily_review` | ô khen / phàn nàn (nhân viên × ngày) | `PersonDaySheet` phần khen / phàn nàn | — | 3 |
| `catalog.service_create`, `.service_update` | dịch vụ (giá Nữ / Nam cũ → mới) | form sửa dịch vụ ở Bảng giá | — (§4.3) | 4 |
| `catalog.plan_create`, `.plan_update` | mẫu gói | form sửa mẫu gói | — | 4 |
| `catalog.category_create`, `.category_update` | nhóm dịch vụ | form sửa nhóm | — | 4 |
| `payroll.rules_update` | quy tắc lương của spa | `RulesSheet` "Quy tắc lương của spa" | — | 4 |
| `appointment.settings` | cài đặt lịch hẹn | sheet số khách tối đa | — | 4 |

### 4.2 Không có lịch sử tại chỗ (và vì sao)
| Action | Vì sao không |
|---|---|
| `customer.follow_up` | Đã hiện "kênh · người gọi" trong Lịch sử khách và danh sách khách vắng (D11). |
| `evaluation.record` | Đã có "Chấm bởi … · giờ" ở đánh giá tháng (D11). |
| `staff.adjustment`, `staff.adjustment_void` | Đã có cột "Người ghi" ở sổ thưởng/phạt (D11). |
| `staff.create`, `staff.update`, `staff.end`, `staff.rehire`, `staff.pay_update`, `staff.account_link` | Đã có "Lịch sử thay đổi" của hồ sơ nhân viên (`GET /staff/{id}/history`, chỉ chủ spa) (D11). |
| `payroll.close` | Đã có "Chốt bởi" (D11). |
| `staff.login_create`, `staff.login_password_reset`, `staff.login_role` | Việc bảo mật tài khoản chỉ chủ spa làm (`staff.manage` 🔒): người làm luôn là chủ spa, dòng "ai ghi" không thêm thông tin; không phải bản ghi nghiệp vụ. |
| `account.password_change`, `account.password_reset_requested`, `account.password_reset_dismissed` | Sự kiện bảo mật cá nhân; yêu cầu quên mật khẩu đã có danh sách riêng cho chủ spa. Không có "bản ghi" để gắn dòng. |
| `account.locked_by_platform`, `account.unlocked_by_platform`, `account.mfa_reset_by_platform`, `account.sessions_revoked_by_platform`, `account.password_reset_by_platform` | Việc của đội vận hành trên tài khoản; spa không có màn bản ghi tương ứng. Vẫn ở `audit_log` (truy vết) và ở Nhật ký thao tác khi operator bật lại. |
| `role.permissions_changed`, `role.permissions_reset`, `feature.changed`, `tenant.created`, `tenant.suspended`, `tenant.unsuspended`, `tenant.owner_mfa_changed` | Cấu hình nền tảng: spa không có màn vai trò / cờ; console có nhật ký nền tảng riêng. |

Đếm: 4.1 có **35** key (9 Quầy + 6 gói + 8 lịch hẹn + 3 khách + 2 sổ nhân sự + 1 lương + 6 danh mục; `customer.follow_up` ở 4.2), 4.2 có **29** key (1 gọi lại + 1 đánh giá tháng + 2 thưởng/phạt + 6 hồ sơ NV + 1 chốt lương + 3 đăng nhập NV + 3 tài khoản + 5 việc operator trên tài khoản + 7 cấu hình nền tảng); tổng **64**. Test web giữ `HISTORY_VERB: Record<AuditAction, string | null>` (null = không hiện tại chỗ), nên action mới chưa được xếp → typecheck đỏ (giống ADR-0021 D3).

### 4.3 Chỗ cố ý không đặt dấu / dòng
- **Ô lưới tháng chấm công** (`MonthSheet`): ô 1 ký tự, thêm dấu là nhiễu; mở ô → `PersonDaySheet` có dòng "ai chấm".
- **Danh sách Bảng giá / mẫu gói:** dịch vụ nào cũng từng được sửa, dấu "Đã sửa" vô nghĩa; dòng "Sửa lần cuối bởi …" ở form sửa.
- **Danh sách khách** (`/khach-hang`): không dấu; dòng "Tạo bởi … · đã sửa N lần" ở hồ sơ.
- **Lịch dạng ngày / tuần** (khối giờ): không dấu; dạng danh sách (`kieu=ds`) có.
- **Màn của KTV, màn `/me/*`, "Khách của tôi", Tổng quan hôm nay, báo cáo số liệu, Nhật ký ghi chú:** không.

## 5. Acceptance criteria
Chung cho mọi endpoint mới: chưa đăng nhập `401`; thiếu `record.history.read` → `403 auth.forbidden` **trước khi đọc dữ liệu** (F17); thiếu quyền đọc của màn gốc → `403`; id của spa khác hoặc không tồn tại → `404`; `Cache-Control: no-store`.

**AC-H1 (chủ spa thấy dòng "ai ghi")**
- Given chủ spa (mặc định có `record.history.read`) và một lượt Hà ghi lúc 15:10, When mở hàng lượt ở Quầy, Then thấy "Ghi bởi Hà · 15:10".
- Given lượt đó Lan đổi KTV lúc 15:30, Then "Ghi bởi Hà · 15:10 · đã sửa 1 lần"; danh sách Quầy có dấu "Đã sửa" ở hàng đó.
- Given lượt ghi bù (giờ làm 10/11, ghi lúc 15:10 ngày 12/11), Then "Ghi bởi Hà · 15:10 12/11".
- Negative: KTV gọi `GET /record-history/visit/{id}` → `403`; response `GET /visits` của KTV/quản lý tắt **không có key** `edited`.

**AC-H2 (mở lịch sử)**
- When bấm dòng "Ghi bởi …", Then mở danh sách thay đổi cũ → mới; mỗi dòng: giờ, người làm, việc, tiền (nếu có `sale.read`), giá trị đổi, lý do. Mặc định đóng; bấm lần nữa thì đóng.
- Given lượt có thu tiền, Then **một** dòng "Hà ghi lượt · Thu 300.000 ₫" (không có dòng "thu tiền" riêng, D9 ADR-0021); `audit_log` vẫn đủ hai dòng.
- Given lượt bị hủy, Then dòng "Lan hủy lượt · Đảo 300.000 ₫ · lý do: khách đổi ý"; dòng meta "… · đã hủy".
- Negative: không bao giờ có nội dung ghi chú buổi, lưu ý sức khỏe, ghi chú gói, ghi chú chấm công; quản lý không có `sale.read` (nếu operator rút) → không có key `amount` / `discount`.

**AC-H3 (Sửa lượt đọc như một lượt)**
- Given lượt A sửa thành B (Sửa lượt) rồi B sửa thành C, When mở lịch sử ở A, B hoặc C, Then cùng một danh sách: "ghi lượt" (A) → "sửa lượt (tính lại tiền) · Chênh lệch …" (A→B) → "sửa lượt …" (B→C), không có dòng "hủy lượt" / "ghi lượt" lẻ của từng bản; meta của C: "Ghi bởi <người ghi A> · <giờ A> · đã sửa 2 lần".
- "Chuyển sang gói" là một dòng "chuyển lượt sang gói X".

**AC-H4 (operator bật / tắt)**
- `PUT /admin/tenants/{id}/record-history/{role} {visible, reason}` (step-up, lý do 3–500): `owner` / `manager` → `200` (vai trò sau khi đổi); `technician` → `422 role.not_for_technician`; role lạ → `404`; giá trị đang có → `409 role.record_history_unchanged`; thiếu step-up → `403 platform.step_up_required`; spa không có → `404`.
- Có hiệu lực ở request kế tiếp: bật cho quản lý → quản lý `GET /record-history/visit/{id}` `200`; tắt → `403`, `/me.permissions` không còn `record.history.read`, web ẩn mọi dòng "ai ghi" và dấu.
- Audit nền tảng `platform.record_history.set` + audit spa `role.permissions_changed` (`actor_kind = platform_admin`, before/after, lý do).
- Negative: session của chủ spa / quản lý gọi route này → `401`; `PUT /admin/role-templates/technician` hay `PUT /admin/tenants/{id}/roles/technician` có `record.history.read` → `422 role.not_for_technician`.

**AC-H5 (màn Nhật ký thao tác ẩn)**
- Given spa không có dòng `tenant_feature` cho `activity_log` (mọi spa hiện có và spa mới), When chủ spa gọi `GET /audit-log`, Then `403 feature.disabled`; `/me.features` không có `activity_log`; nav Báo cáo không có "Nhật ký thao tác"; vào thẳng `/nhat-ky-thao-tac` → về trang chủ như mọi route bị chặn.
- When operator bật (`PUT /admin/tenants/{id}/features/activity_log {enabled: true, reason}`), Then chủ spa thấy lại màn ở request kế tiếp; quản lý vẫn `403` (không có `audit.read`).
- `GET /admin/tenants/{id}/features` trả `activity_log` `enabled: false`, `default_enabled: false`; 6 cờ cũ vẫn `default_enabled: true` và hành vi không đổi.
- `administration_is_never_a_switchable_feature` vẫn xanh: `owning(audit.read) == None`.

**AC-H6 (PII)**
- Given khách đổi SĐT 0905 111 222 → 0912 333 444, When chủ spa mở lịch sử hồ sơ, Then "SĐT: 0905 111 222 → 0912 333 444"; When quản lý được bật mở, Then "SĐT: 0905 ••• 222 → 0912 333 444" (số trùng SĐT hiện tại hiện đủ).
- Given khách đã ẩn danh (khi `customer.anonymize` có), Then không tên, không SĐT cũ / mới (giữ quy tắc ADR-0021).
- Negative: response không bao giờ có SĐT nhân viên, ngày sinh khách, `before` / `after` thô, id hay tên operator.

**AC-H7 (dấu danh sách)**
- `edited` có mặt (true/false) chỉ khi người gọi có `record.history.read`; vắng mặt hoàn toàn khi không (không phải `false`).
- Bản cũ của Sửa lượt giữ badge "Đã sửa" sẵn có; bản mới (đang có hiệu lực) có `edited: true`.

**AC-H8 (UI)** 360 px không cuộn ngang; sáng + tối; axe không lỗi serious / critical; dòng meta là `<button>` có `aria-expanded`; tải / lỗi / trống có chữ (design page).

## 6. Edge cases
1. **Sửa lượt (chuỗi).** Lịch sử theo **chuỗi** `corrects_visit_id` (lên và xuống, tối đa 20 bản), mở từ bản nào cũng ra cùng câu chuyện. Dòng meta tính từ dòng đầu của chuỗi (người ghi bản gốc), số lần sửa = số dòng sau dòng đầu trừ dòng hủy. Bản cũ trong danh sách giữ chữ sẵn có "Đã sửa, thay bằng lượt mới."
2. **Chuyển sang gói.** Gộp theo hạng 5 của `fold_rule`; tên gói trong dòng.
3. **Lượt + hóa đơn + gói trong giỏ.** Gộp như D9; mở lịch sử từ dòng hóa đơn ở Sổ doanh thu mà hóa đơn thuộc lượt → server trả lịch sử **của lượt** (neo đổi thành lượt; `record.type = visit`).
4. **Dòng "Hoàn tiền" ở Sổ doanh thu** → lịch sử của gói bị hủy (neo đổi thành gói).
5. **Khách đã ẩn danh:** chủ thể không tên, không link; `customer.update` không giá trị cũ / mới (giống activity).
6. **Người làm đã nghỉ:** "Hà (đã nghỉ)" (cùng luật `left` với bộ lọc người của Nhật ký thao tác: membership không còn hoạt động hoặc hồ sơ không còn đợt mở). Tài khoản không còn → "Một tài khoản đã xóa".
7. **Đội vận hành / Hệ thống:** "Đội vận hành" (không tên, không id operator), "Hệ thống" (yêu cầu quên mật khẩu). Trong phạm vi §4.1 hiếm gặp nhưng xử lý chung.
8. **Dữ liệu seed giờ lùi:** mọi giờ lấy từ `audit_log.at` (seed ghi theo đồng hồ use case, ADR-0021 D8), không trộn `created_at` của bảng nghiệp vụ (giờ chạy seed).
9. **Bản ghi không có audit** (dữ liệu trước khi có audit, nhập SQL tay, sổ giấy qua CLI): `summary.source`:
   - `paper_book` → meta "Nhập từ sổ giấy · dd/mm/yyyy";
   - `record` (bảng có `created_by` / `created_at`: lượt, hóa đơn, gói, lịch hẹn) → meta "Ghi bởi … · giờ" từ cột, danh sách "Chưa có lịch sử chi tiết";
   - `none` (khách, dịch vụ, mẫu gói, ô chấm công không có dòng audit) → meta "Chưa có lịch sử" (không ẩn dòng, để người dùng biết là không có chứ không phải lỗi).
10. **Nhiều thay đổi:** tối đa 100 dòng gần nhất (cũ → mới), `omitted` = số dòng cũ hơn; web: "Còn N thay đổi cũ hơn không hiện ở đây."
11. **Công tắc đổi giữa phiên:** request kế tiếp `403` → web ẩn dòng, gọi lại `/me`.
12. **Tính năng tắt:** lịch hẹn tắt → `appointment.manage` bị rút → `GET /record-history/appointment/…` `403 feature.disabled`; lương tắt → quy tắc lương tương tự.
13. **Ô chấm công của chính mình / của quản lý khác:** cùng phạm vi đọc như `GET /attendance` (quyết định C, ADR-0015); ngoài phạm vi → `404`.
14. **Hai thao tác cùng giây** (seed): không gộp nhầm (khóa gộp là id trong dòng audit, đã test ở AC11.9).
15. **Key audit cũ** (không còn trong code): hiện câu chung "làm một thao tác khác" (giống activity).

## 7. Ma trận quyền
| Ai | Dòng "ai ghi" + lịch sử | Dấu "Đã sửa" | SĐT cũ của khách | Màn Nhật ký thao tác |
|---|---|---|---|---|
| Chủ spa, công tắc bật (mặc định) | ✓ (trong quyền đọc của từng màn) | ✓ | đủ | chỉ khi cờ `activity_log` bật (mặc định tắt) |
| Chủ spa, operator tắt | — (`403`, không key `edited`) | — | — | như trên |
| Quản lý, operator bật | ✓ (trong quyền đọc của từng màn: không lương, không danh mục / cài đặt nếu không có quyền sửa) | ✓ | che giữa | — (`audit.read` owner-only) |
| Quản lý, tắt (mặc định) | — | — | — | — |
| KTV | — (không cấp được) | — | — | — |
| Người đã nghỉ | — | — | — | — |
| Operator | không gọi được route spa (`401`); chỉ bật / tắt ở `/admin` | — | — | bật / tắt cờ |

Chi tiết route → permission: [authz-matrix](../../security/authz-matrix.md) § Lịch sử trên từng bản ghi.

## 8. UX notes
Chi tiết wireframe + chữ ở [design/record-history](../../design/record-history.md). Tóm tắt: dòng meta 13 px màu `--text-muted` ngay dưới phần đầu của bản ghi khi mở chi tiết; bấm → mở danh sách thay đổi ngay bên dưới (disclosure, mặc định đóng); danh sách: badge `badge-muted` "Đã sửa" cạnh trạng thái; Sổ doanh thu: bấm hàng → sheet "Hóa đơn" có dòng meta + lịch sử. Admin: panel "Lịch sử trên từng bản ghi" ở trang spa; cờ "Nhật ký thao tác" hiện trong panel "Tính năng" với nhãn "Mặc định: tắt".

## 9. Data
- Không bảng mới; đọc `app.audit_log` + join tên (như `reporting::activity`). PII: tên + SĐT cũ / mới khách (đã có từ ADR-0021), che theo D8. Không ghi gì mới vào audit ngoài thao tác của operator.
- Migration 0052: permission `record.history.read` cho mẫu chủ spa + vai trò chủ spa của mọi spa; `app.session_actor` trả thêm `enabled_features`; `app.platform_set_feature` trả giá trị trước (null khi chưa có dòng); hàm `app.platform_set_role_permission`. Chi tiết [architecture](../../architecture/record-history.md).
- Lưu giữ: theo audit (giữ mãi, EM Q-DB4); `customer.anonymize` (chưa build) phải xóa tên / SĐT trong audit như ADR-0021 đã ghi.

## 10. Non-functional
- **Perf:** `GET /record-history/*` p95 ≤ 100 ms ở bench 3 năm (ngân sách chung 300 ms của màn xem lại, [backend §3b](../../architecture/backend.md)); danh sách có `edited`: p95 tăng ≤ 10 ms khi bật, 0 khi tắt (không chạy truy vấn). Web: không tăng JS khởi động (Quầy nằm trong chunk khởi động, đang 123,8 / 125 KB): mã lịch sử ở chunk lazy `record-history`, tải khi mở chi tiết.
- **Bảo mật:** non-negotiable #1–#4; [threat-model](../../security/threat-model.md) § Lịch sử trên từng bản ghi.
- **A11y:** WCAG 2.2 AA, vùng chạm ≥ 44 px trên điện thoại, focus không mất khi mở / đóng.
- **i18n:** tiếng Việt; tên người làm giống hệt Nhật ký thao tác (`features/activity/sentence.ts` `actorName`).

## 11. Out of scope
- Công tắc trong app của spa; quyền riêng theo từng người (chỉ theo vai trò).
- Lịch sử cho hồ sơ nhân viên, thưởng/phạt, đánh giá tháng, bảng lương (đã có cách hiện riêng, D11).
- Xuất lịch sử ra file; tìm kiếm trong lịch sử.
- Đổi cách ghi audit (không thêm action, không đổi payload).

## 12. Open questions
Không (EM ủy quyền quyết). Các điểm có thể xem lại ghi ở ADR-0022 § Revisit when.

## Review log
Hai lượt tự rà soát sau bản nháp (EM yêu cầu "review đi review lại kĩ").

**Lượt 1 — đối chiếu code + non-negotiables**
| Phát hiện | Xử lý |
|---|---|
| Bản nháp định đặt cờ màn nhật ký sở hữu `audit.read` → phá test `administration_is_never_a_switchable_feature` và làm chủ spa mất `audit.read` (owner floor). | D5: cờ chỉ sở hữu route `/api/v1/audit-log`, permission giữ nguyên. |
| Cơ chế cờ hiện tại "không dòng = bật"; `tenant_feature.updated_by` NOT NULL tham chiếu operator nên migration không thể chèn dòng "tắt" cho mọi spa. | Thêm `default_enabled` vào registry code; `session_actor` trả thêm danh sách cờ bật; hàm `platform_set_feature` trả `NULL` khi chưa có dòng để Rust áp mặc định ([architecture §3](../../architecture/record-history.md)). |
| `attendance.record`, `evaluation.daily_review` ghi audit với `entity_type = 'staff'`, `entity_id` = id nhân viên (không phải id ô). | Endpoint ô theo `(staff_id, date)`, lọc `date` trong JSON của dòng audit; dùng index `(tenant_id, entity_type, entity_id, at)` rồi lọc (~1.000 dòng / nhân viên / năm). |
| `visit.correct` ghi `entity_id` = lượt **cũ**, lượt mới chỉ có `corrects_visit_id`. | Neo lịch sử là cả chuỗi; dấu `edited` của lượt mới = `corrects_visit_id IS NOT NULL` hoặc có `visit.update` / `visit.addition`. |
| Web Quầy đã có badge "Đã sửa" (bản cũ, `corrected_by_visit_id`) và "Đã hủy"; Lịch sử khách có "Lượt sửa lại". | Giữ nguyên các badge cũ (mọi người vẫn thấy); `edited` chỉ thêm cho bản đang hiệu lực. |
| Nhiều màn đã hiện "ai làm" (hồ sơ NV, thưởng/phạt, đánh giá tháng, chốt lương, gọi lại). | D11, đưa vào §4.2. |
| Không có quy tắc che SĐT nào trong web; quản lý thấy SĐT khách đầy đủ. | D8 che số **cũ** ở server cho người không có `audit.read`; số hiện tại giữ như hiện nay. |
| Ledger không có màn chi tiết; hàng hoàn tiền có id = refund id. | Sheet hóa đơn mới ở Sổ doanh thu (chỉ khi có quyền); `record=refund` → server đổi neo sang gói (`refund.customer_package_id`). |
| Bản nháp cho Lịch sử khách tự tải lịch sử từng thẻ (dòng thời gian dài → hàng chục request; và thẻ là "danh sách" theo câu 3 của EM). | Quy tắc tải ([design §1.2](../../design/record-history.md)): một bản ghi đang mở thì tự tải; nhiều bản ghi (thẻ lịch sử khách, các gói trong hồ sơ) chỉ có nút "Xem ai ghi, ai sửa", bấm mới tải. |
| Gói nhập sổ giấy: bản nháp định nhận ra qua dòng audit. | Cột có sẵn `customer_package.origin = 'opening_balance'` (0005) → `source = paper_book`. |
| Chữ mẫu "Chủ spa Lan hủy gói": `actorName` không có tiền tố vai trò. | Sửa thành "Lan hủy gói"; mọi chữ người làm đi qua `actorName`. |
| Đếm bảng §4 sai (44 / 20). | Đếm lại theo `Action` trong `kernel/src/audit.rs`: 35 có / 29 không = 64. |
| 0052 thêm quyền cho vai trò chủ spa của mọi spa: có làm vai trò "không theo mẫu" không? | Không đổi `follows_template` (mẫu owner cũng được thêm cùng migration) — ghi ở [architecture §2](../../architecture/record-history.md). |

**Lượt 2 — secure, perf, mobile, nhất quán spec / ADR**
| Phát hiện | Xử lý |
|---|---|
| Secure: endpoint chung theo `{record}` dễ quên kiểm quyền đọc của một loại. | Bảng `record → (permission đọc, phạm vi)` là `match` vét cạn trong Rust + test matrix mỗi loại × mỗi vai trò ([architecture §4](../../architecture/record-history.md)). |
| Secure: lịch sử có thể mang trường ngoài quyền màn gốc (tiền, ghi chú). | D7: lọc ở server sau khi gộp; test "quản lý không `sale.read` không có key `amount`". |
| Secure: operator đổi một permission bằng đọc-sửa-ghi danh sách vai trò → hai operator cùng lúc có thể ghi đè. | Hàm DB `platform_set_role_permission` khóa dòng role (`FOR UPDATE`), chỉ thêm / xóa đúng một dòng. |
| Secure: technician có thể được cấp qua mẫu vai trò. | Rào `NOT_FOR_TECHNICIAN` trong `check_grants` (mẫu + spa) + test. |
| Perf: Quầy nằm trong chunk khởi động sát ngân sách 125 KB. | Mã lịch sử (kể cả `sentence.ts`) ở chunk lazy; danh sách chỉ đọc boolean `edited`. |
| Perf: dấu `edited` cho 50–100 hàng không được thành N truy vấn. | `EXISTS` trong cùng câu SQL của danh sách, chỉ khi `$can_history`; mỗi hàng một lần dò index. |
| Mobile: lồng sheet trong sheet (`AppointmentSheet`, `PersonDaySheet`). | Lịch sử luôn mở **tại chỗ** (disclosure), không mở sheet thứ hai. |
| Nhất quán: ADR-0021 ghi "Không cờ tính năng" cho màn nhật ký; raw sáng nay "không hiện ai làm". | ADR-0022 sửa một phần ADR-0021 (D1) và đảo raw `actor-trace-data-only`; ghi vào ADR-0021 + management-views §11 + log. |
| Nhất quán: tên vai trò / chữ người làm. | Dùng `actorName` + "(đã nghỉ)" như bộ lọc Nhật ký thao tác; không thêm tiền tố "Quản lý". |
| Secure: thứ tự lỗi có thể thành "oracle" tồn tại bản ghi. | Kiểm `record.history.read` + quyền màn gốc **trước** khi tra id → người không quyền luôn `403`, không phân biệt id có / không. |
| Secure: replay idempotent trả body có `edited` theo quyền lúc ghi. | Đã kiểm `app.idempotency_record` khóa `(tenant_id, actor_user_id, key)`: chỉ chính người ghi nhận lại; `edited` không chứa tên. Chấp nhận, ghi ở threat model. |
| Secure checklist "rotate session khi đổi quyền". | Quyền đọc mỗi request (`session_actor`, ADR-0017) nên bật / tắt có hiệu lực ngay request kế tiếp, giống AC7.3 của sửa vai trò; không cần xoay session. |
| Code: `mayOpen.activity` có thật ở `apps/web/src/app/access.ts` (`can(me, "audit.read")`); `hasFeature` trả `true` khi `/me` thiếu `features`. | Đổi thành `can(…) && hasFeature(me, "activity_log")`; `/me` luôn có `features` khi có spa đang dùng. |
| Mobile: hàng Sổ doanh thu bấm được trên desktop là bảng tự dựng. | Nút trong ô đầu (không biến cả `<tr>` thành nút), thẻ điện thoại bấm cả thẻ; nhãn "Xem hóa đơn của chị …". |
