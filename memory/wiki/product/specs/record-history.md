---
title: "Spec: Lịch sử trên từng bản ghi (ai ghi, ai sửa) + tạm ẩn Nhật ký thao tác"
status: approved
updated: 2026-11-12
sources: [raw/2026-11-11-em-decision-inline-record-history.md, raw/2026-11-11-em-actor-trace-data-only.md, raw/2026-11-11-em-decision-activity-log-and-customer-old-values.md, raw/2026-11-11-em-decision-activity-log-one-line-per-action.md]
related: [management-views.md, front-desk-and-packages.md, appointments.md, staff-management.md, platform-admin.md, ../../decisions/ADR-0022-inline-record-history.md, ../../decisions/ADR-0021-activity-log-for-the-owner.md, ../../architecture/record-history.md, ../../design/record-history.md, ../../plans/record-history.md]
---

# Spec: Lịch sử trên từng bản ghi + tạm ẩn "Nhật ký thao tác"

**Status: approved** (EM 2026-11-11 giao lead tự phân tích, tự review, không hỏi lại; [raw](../../../raw/2026-11-11-em-decision-inline-record-history.md)). **Đợt 1a đã build** (API, công tắc, cờ; quyết định khi build K1–K11 cuối Review log), **đợt 1b đã build** (các màn; K12–K20); đợt 1 lên staging 2026-11-12. **Đợt 2 đã build** (lịch hẹn, chấm công, khen / phàn nàn, thưởng / phạt; PR #190; K21–K36), lên staging 2026-11-12. **Đợt 3 đã build** (bảng giá, mẫu gói, nhóm dịch vụ, quy tắc lương, số khách tối đa; K37–K47), lên staging 2026-11-12. **Việc còn lại sau 3 đợt đã làm** (2026-11-12, K48–K55): form "Sửa nhóm" trên bảng giá (đóng K39), sheet ô ngày chỉ xem (đóng K30), mỗi lần mở bản ghi một request, chữ giá trị rỗng có nghĩa ("6 → không giới hạn"). **Bản sửa sau review độc lập (lượt 3)**, xem Review log cuối trang. Thiết kế backend: [architecture/record-history](../../architecture/record-history.md). Thiết kế UI: [design/record-history](../../design/record-history.md). Kế hoạch: [plans/record-history](../../plans/record-history.md). Quyết định: [ADR-0022](../../decisions/ADR-0022-inline-record-history.md).

**Vì sao là trang spec riêng, không phải một mục của management-views:** tính năng cắt ngang 4 spec (Quầy + gói, lịch hẹn, nhân sự, danh mục) và màn console nền tảng; management-views §11 chỉ nói về màn "Nhật ký thao tác" độc lập. §11 có một dòng trỏ về đây (màn đó nay ẩn mặc định).

## 1. Goal
Chủ spa muốn **truy được ai ghi, ai sửa, ai hủy** một bản ghi ngay tại chỗ đang xem (lượt, hóa đơn, gói, lịch hẹn, hồ sơ khách, ô chấm công…), không phải mở một màn nhật ký riêng rồi tìm. EM 2026-11-11 đảo quyết định sáng cùng ngày ("không cần show") và chọn:
1. Mỗi bản ghi có một **dòng nhỏ, màu nhạt**: "Ghi bởi Hà · 15:10 · đã sửa 1 lần". Bấm vào thì mở **lịch sử của chính bản ghi đó** (ai, lúc nào, đổi gì, lý do). **Mặc định đóng.**
2. **Mặc định hiện cho chủ spa.** Có **công tắc**, đặt ở **console admin nền tảng** (EM: "công tắc bật tắt là admin control nhé chơ không phải ở account của spa và quản lý đâu"). Operator bật/tắt riêng cho chủ spa và cho quản lý. KTV không bao giờ thấy.
3. **Danh sách** chỉ hiện dấu nhỏ "Đã sửa" / "Đã hủy"; **tên người và lịch sử đầy đủ** chỉ ở chỗ mở chi tiết.
4. Màn "Nhật ký thao tác" (`/nhat-ky-thao-tac`) **giữ trong code nhưng tạm ẩn ngay từ đợt 1** (EM: "tạm thời ẩn … off show cái nhật kí thao tác đi"), tắt từ admin (cờ tính năng theo spa, **mặc định tắt**).

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
| D1 | Công tắc = permission mới **`record.history.read`** ("Xem ai ghi, ai sửa trên từng bản ghi"), loại **riêng từng spa (spa-local)**: mẫu vai trò chỉ quyết giá trị **ban đầu của spa mới** (chủ spa có, quản lý không); áp mẫu cho các spa đang theo mẫu, so sánh "theo mẫu", và "Về mẫu" đều **bỏ qua** nó (giữ giá trị hiện tại của spa). | "Permission là dữ liệu", kiểm bằng `Actor::require` (deny by default). Spa-local để bật / tắt công tắc **không** làm vai trò "lệch mẫu" và sửa mẫu sau này không âm thầm đổi công tắc của spa (LEAD, review lượt 3). |
| D2 | Operator đổi bằng **panel riêng** "Lịch sử trên từng bản ghi" ở `/admin/spa/<id>` (hai hàng: Chủ spa, Quản lý), gọi endpoint riêng; hàm DB cứng permission = `record.history.read` và vai trò ∈ {owner, manager}, khóa dòng role, không đụng `follows_template`. Trình sửa vai trò hiện quyền này **chỉ đọc** ("đổi ở panel Lịch sử trên từng bản ghi"). | Một đường ghi duy nhất, nguyên tử; không thể dùng hàm này cấp quyền khác. |
| D3 | `record.history.read` **không** trong owner floor (tắt được cho chủ spa), **không bao giờ** cho KTV (`422 role.not_for_technician`, mẫu lẫn spa; hàm DB cũng từ chối). | EM: "bật tắt được", "default là hiện cho chủ spa"; KTV không thuộc diện. |
| D4 | Không có công tắc nào trong app của spa. Route `/admin/*` chỉ nhận cookie operator: session spa → `401`. | EM làm rõ: chỉ admin control. |
| D5 | Màn "Nhật ký thao tác" ẩn bằng **cờ tính năng mới `activity_log`**, **mặc định tắt từ đợt 1**. Cờ chỉ sở hữu **route** `/api/v1/audit-log`, **không** sở hữu `audit.read`. Khoảng trống: đợt 1 gồm cả gói (kể cả hủy / hoàn tiền) và hồ sơ khách; phần còn lại (lịch hẹn, chấm công, thưởng/phạt, danh mục, quy tắc lương, việc của đội vận hành) là **cửa sổ hồi quy** ghi ở [plan](../../plans/record-history.md); dữ liệu vẫn ở `audit_log`, operator bật cờ cho spa khi cần. Sửa ADR-0018 D6 / ADR-0003: khi cờ tắt, chủ spa **không** xem được việc của đội vận hành trên UI; không làm màn riêng cho việc đó. | EM nói rõ "tạm thời ẩn". Giữ test `administration_is_never_a_switchable_feature`. |
| D6 | Lịch sử đọc từ `app.audit_log` (không bảng mới). **Thành viên của lịch sử = theo id dòng audit** đã chọn cho bản ghi (không theo "cùng thực thể" hay "cùng thời điểm"). Chỉ lượt / hóa đơn / gói mới gộp một thao tác một dòng (ADR-0021 D9); các loại khác mỗi dòng audit một dòng. | Review B2: cùng thực thể / cùng thời điểm kéo nhầm vòng đời gói vào lượt và ngày công khác vào ô đang xem. |
| D7 | Dữ liệu giữ **quyền đọc của màn gốc**: (a) kiểm quyền của **loại neo sau khi đổi neo** (hóa đơn → lượt cần `visit.read`; hoàn tiền → gói cần `customer.read`), `403` trước khi đọc; (b) tiền chỉ khi có `sale.read`; (c) ghi chú buổi / ghi chú gói / lưu ý sức khỏe chỉ "đã đổi", và chỉ khi có `customer.health_notes.read` (dòng chỉ đổi ghi chú bị bỏ hẳn với người không có); (d) lương chỉ khi có `payroll.read`. | Entitled data only (non-negotiable #4). Tiền lệ: `GET /customers/{id}/history` cần `customer.read + visit.read + sale.read`. |
| D8 | SĐT cũ → mới của khách: chủ spa (`audit.read`) thấy đủ; người khác **không nhận giá trị nào** ("SĐT (đã đổi)"). Tên cũ → mới hiện đủ cho ai mở được hồ sơ. | Số cũ có thể đã thuộc người khác; che giữa vẫn lộ một phần (LEAD, review S8). |
| D9 | **"Đã sửa" chỉ khi điều đã ghi bị đổi** (giờ, KTV, dịch vụ / giá / khách qua Sửa lượt hoặc chuyển sang gói; lịch hẹn đổi giờ / KTV). Thêm vào lượt (upsell), gia hạn gói, dùng gói hết hạn, ghi chú… là **vòng đời**: chỉ ở lịch sử, không tính "đã sửa". Danh sách có dấu: lượt (Quầy, Lịch sử khách, hàng dịch vụ lẻ của Sổ doanh thu) và lịch hẹn (chỉ dạng danh sách). Một chip duy nhất, điều kiện **loại trừ nhau** (khớp thứ tự code hiện có: kiểm `corrected_by_visit_id` trước): **Đã thay** = `corrected_by_visit_id` có giá trị; **Đã hủy** = `status = voided` và không có `corrected_by_visit_id`; **Đã sửa** = `edited` và `status != voided`. Chip "Đã sửa" cũ của **bản lượt bị thay** đổi thành **"Đã thay"** cho mọi vai trò. | EM câu 3; review UX B1 + S11: "Đã sửa" đang mang nghĩa "bản cũ bị thay". |
| D10 | Dòng "ai ghi" + lịch sử tải bằng **một endpoint chung** `GET /record-history/...` khi mở chi tiết; DTO danh sách chỉ thêm `edited`. "Đã sửa N lần" = số **dòng đã gộp** (sau lọc D7) có hành động gốc tính là sửa; `edited` ⇔ N ≥ 1 (một định nghĩa, [architecture §4.4](../../architecture/record-history.md)). | Một chỗ kiểm quyền; danh sách không mang tên. |
| D11 | Chỗ đã hiện "ai làm" từ trước giữ nguyên: lịch sử hồ sơ nhân viên (`GET /staff/{id}/history`), "Chấm bởi" đánh giá tháng, "Chốt bởi" bảng lương, người gọi lại khách. **Thưởng/phạt** chỉ có cột "Người ghi" (người ghi bản hiện tại), không thấy sửa / xóa → **có** lịch sử (đợt 2). | Review UX S12. |
| D12 | "Ghi bởi / Tạo bởi" chỉ lấy từ **thao tác tạo** (lượt: `visit.create` gốc của chuỗi; gói: `customer_package.create` hoặc `opening_balance.import` có id gói; khách: `customer.create` hoặc `opening_balance.import` có `customer_created`); không có thì lấy cột `created_by` / `created_at` / `origin`, không bao giờ lấy "dòng đầu tiên bất kỳ". | Review S3: gói sổ giấy được gia hạn bị hiện người gia hạn là người ghi. |

## 3. User stories
- **US-H1** Là chủ spa, tôi muốn thấy ngay trên một lượt / hóa đơn / gói / lịch hẹn ai ghi, lúc nào, đã sửa mấy lần, để biết hỏi ai khi số liệu lạ.
- **US-H2** Là chủ spa, tôi muốn bấm vào dòng đó để xem từng thay đổi (ai, lúc nào, đổi gì, lý do), để truy một lần hủy / sửa giá / đổi SĐT khách.
- **US-H3** Là chủ spa, tôi muốn danh sách chỉ hiện dấu "Đã sửa" / "Đã hủy" nhỏ, để danh sách gọn mà vẫn biết dòng nào có chuyện.
- **US-H4** Là operator, tôi muốn bật / tắt lịch sử này riêng cho chủ spa và cho quản lý của một spa, có lý do và nhật ký, để làm đúng yêu cầu từng spa.
- **US-H5** Là operator, tôi muốn màn "Nhật ký thao tác" tắt mặc định và bật lại được cho từng spa, để giữ công cụ mà không làm rối menu chủ spa.
- **US-H6** Là quản lý được bật lịch sử, tôi muốn thấy cùng dòng "ai ghi" như chủ spa nhưng không thấy số điện thoại cũ của khách, để làm việc mà không lộ dữ liệu không cần.

## 4. Phạm vi: mọi action audit (64 key của `kernel::audit::Action`) → có lịch sử tại chỗ không
"Bản ghi" = thứ người dùng mở ra xem. Cột "Sửa?" = dòng đó có tính vào "Đã sửa" / "đã sửa N lần" không (D9). Cột "Đợt" = wave ở [plan](../../plans/record-history.md) (1 = 1a + 1b).

### 4.1 Có lịch sử tại chỗ
| Action | Thuộc lịch sử của | Màn | Sửa? | Đợt |
|---|---|---|---|---|
| `visit.create` | lượt (gốc chuỗi) | Quầy (`VisitDetails`), Lịch sử khách (thẻ lượt), Sổ doanh thu (hàng dịch vụ lẻ) | — (tạo) | 1 |
| `visit.addition` | lượt | như trên | không (upsell) | 1 |
| `visit.update` | lượt | như trên | **có** khi đổi giờ hoặc KTV; không khi chỉ người đi cùng / ghi chú | 1 |
| `visit.correct` (Sửa lượt) | chuỗi lượt | như trên, **một câu chuyện cho cả chuỗi** (§6.1) | **có** | 1 |
| `visit.convert_to_package` | chuỗi lượt | như trên | **có** | 1 |
| `visit.void` | lượt | như trên | — (trạng thái Đã hủy) | 1 |
| `sale.create` | hóa đơn dịch vụ lẻ → gộp vào dòng lượt; hóa đơn bán gói → lịch sử **gói** | Sổ doanh thu, "Bán gói không kèm lượt", gói của khách | — | 1 |
| `sale.void` | như `sale.create` | như trên | — (Đã hủy) | 1 |
| `sale.note_update` (chỉ "đã đổi") | gói (hóa đơn bán gói) | gói của khách, Sổ doanh thu; chỉ khi có `customer.health_notes.read` | không | 1 |
| `customer_package.create` | gói | gói của khách, sheet hàng hóa đơn gói / Hoàn tiền ở Sổ doanh thu; gộp vào dòng lượt khi bán trong giỏ (chỉ ở lịch sử lượt) | — (tạo) | 1 |
| `customer_package.extend` | gói | như trên | không (vòng đời) | 1 |
| `customer_package.cancel` | gói | như trên | — (Đã hủy) | 1 |
| `customer_package.use_expired_override` | gói **và** lượt dùng nó (`after.visit_id`) | gói của khách; lịch sử lượt | không | 1 |
| `opening_balance.import` | khách (nếu `customer_created`) + gói (nếu id gói trong `customer_package_ids`) | hồ sơ khách, gói của khách: "Nhập từ sổ giấy" | — (tạo) | 1 |
| `sale.paper_book` | gói của hóa đơn sổ giấy | gói của khách | — | 1 |
| `customer.create` | khách | hồ sơ khách | — (tạo) | 1 |
| `customer.update` (tên cũ → mới; SĐT theo D8; trường khác "đã đổi") | khách | hồ sơ khách | **có** | 1 |
| `customer.health_note_set` (chỉ "đã đổi") | khách | hồ sơ khách, chỉ khi có `customer.health_notes.read` | không | 1 |
| `appointment.create` | lịch hẹn | `AppointmentSheet` | — (tạo) | 2 |
| `appointment.update` | lịch hẹn | như trên | **có** khi đổi giờ hoặc KTV | 2 |
| `appointment.arrive`, `.undo_arrival`, `.complete`, `.no_show`, `.cancel` | lịch hẹn | như trên | không (vòng đời; Đã hủy / Không đến là trạng thái sẵn có) | 2 |
| `attendance.record` | ô chấm công (nhân viên × ngày, theo `date` của chính dòng) | `PersonDaySheet` phần chấm công (không nội dung ghi chú) | **có** (mọi lần sau lần đầu) | 2 |
| `evaluation.daily_review` | ô khen / phàn nàn | `PersonDaySheet` phần khen / phàn nàn | **có** (sau lần đầu) | 2 |
| `staff.adjustment`, `staff.adjustment_void` | khoản thưởng / phạt (chuỗi sửa: hủy bản cũ + ghi bản mới cùng lúc) | sổ thưởng/phạt (⋯ "Xem ai ghi, ai sửa") | **có** (sửa = thay bản) | 2 |
| `catalog.service_create`, `.service_update` | dịch vụ | form sửa dịch vụ | **có** (`_update`) | 3 |
| `catalog.plan_create`, `.plan_update` | mẫu gói | form sửa mẫu gói | **có** | 3 |
| `catalog.category_create`, `.category_update` | nhóm dịch vụ | form "Sửa nhóm" ở bảng giá (K48) | **có** | 3 |
| `payroll.rules_update` | quy tắc lương | `RulesSheet` | **có** (sau lần đầu) | 3 |
| `appointment.settings` | cài đặt lịch hẹn | sheet số khách tối đa | **có** (sau lần đầu) | 3 |

### 4.2 Không có lịch sử tại chỗ (và vì sao)
| Action | Vì sao không |
|---|---|
| `customer.follow_up` | Đã hiện "kênh · người gọi" trong Lịch sử khách và danh sách khách vắng (D11). |
| `evaluation.record` | Đã có "Chấm bởi … · giờ" ở đánh giá tháng (D11). |
| `staff.create`, `staff.update`, `staff.end`, `staff.rehire`, `staff.pay_update`, `staff.account_link` | Đã có "Lịch sử thay đổi" của hồ sơ nhân viên (`GET /staff/{id}/history`, chỉ chủ spa) (D11). |
| `payroll.close` | Đã có "Chốt bởi" (D11). |
| `staff.login_create`, `staff.login_password_reset`, `staff.login_role` | Việc bảo mật tài khoản chỉ chủ spa làm (`staff.manage` 🔒): người làm luôn là chủ spa; không phải bản ghi nghiệp vụ. |
| `account.password_change`, `account.password_reset_requested`, `account.password_reset_dismissed` | Sự kiện bảo mật cá nhân; yêu cầu quên mật khẩu đã có danh sách riêng. Không có "bản ghi" để gắn dòng. |
| `account.locked_by_platform`, `account.unlocked_by_platform`, `account.mfa_reset_by_platform`, `account.sessions_revoked_by_platform`, `account.password_reset_by_platform` | Việc của đội vận hành trên tài khoản; spa không có màn bản ghi tương ứng. Vẫn ở `audit_log`; hiện ở Nhật ký thao tác khi operator bật cờ (D5). |
| `role.permissions_changed`, `role.permissions_reset`, `feature.changed`, `tenant.created`, `tenant.suspended`, `tenant.unsuspended`, `tenant.owner_mfa_changed` | Cấu hình nền tảng: spa không có màn vai trò / cờ; console có nhật ký nền tảng riêng. |

Đếm: 4.1 có **37** key (9 Quầy + 6 gói + 8 lịch hẹn + 3 khách + 2 sổ nhân sự + 2 thưởng/phạt + 1 lương + 6 danh mục), 4.2 có **27** key (1 gọi lại + 1 đánh giá tháng + 6 hồ sơ NV + 1 chốt lương + 3 đăng nhập NV + 3 tài khoản + 5 việc operator trên tài khoản + 7 cấu hình nền tảng); tổng **64**. Web giữ `HISTORY: Record<AuditAction, ((e: ActivityEntry) => string) | null>` (null = không hiện tại chỗ), nên action mới chưa được xếp → typecheck đỏ (giống ADR-0021 D3).

### 4.3 Chỗ cố ý không đặt dấu / dòng
- **Ô lưới tháng chấm công** (`MonthSheet`): ô 1 ký tự, dấu là nhiễu; mở ô → `PersonDaySheet` có dòng "ai chấm".
- **Danh sách Bảng giá / mẫu gói:** dịch vụ nào cũng từng được sửa, dấu vô nghĩa; dòng "Sửa lần cuối bởi …" ở form sửa.
- **Danh sách khách** (`/khach-hang`) và **Tổng quan gói** (`/goi-lieu-trinh`): không dấu.
- **Gói của khách, "Bán gói không kèm lượt", hàng gói / Hoàn tiền ở Sổ doanh thu:** không dấu "Đã sửa" (vòng đời gói không phải "sửa", D9); vẫn có lịch sử khi mở.
- **Lịch dạng ngày / tuần** (khối giờ): không dấu; dạng danh sách (`kieu=ds`) có.
- **Màn của KTV, `/me/*`, "Khách của tôi", Tổng quan hôm nay, báo cáo số liệu, Nhật ký ghi chú:** không.

## 5. Acceptance criteria
Chung cho mọi endpoint mới: chưa đăng nhập `401`; thiếu `record.history.read` → `403 auth.forbidden` **trước khi đọc dữ liệu** (F17); thiếu quyền đọc của **loại neo cuối cùng** → `403`; id của spa khác hoặc không tồn tại → `404`; `Cache-Control: no-store`.

**AC-H1 (chủ spa thấy dòng "ai ghi")**
- Given chủ spa (mặc định có `record.history.read`) và một lượt Hà ghi lúc 15:10, When mở hàng lượt ở Quầy, Then thấy "Ghi bởi Hà · 15:10".
- Given lượt đó Lan đổi KTV lúc 15:30, Then "Ghi bởi Hà · 15:10 · đã sửa 1 lần"; hàng ở Quầy có chip "Đã sửa".
- Given Lan chỉ **thêm dịch vụ** vào lượt (upsell), Then lịch sử có dòng "Lan thêm vào lượt", meta **không** "đã sửa", không chip.
- Given Lan lưu "Sửa giờ / KTV" mà không đổi gì (mọi dòng `from = to`), Then không tính sửa, không chip; đổi KTV một dòng → "KTV: Thu → Hoa" (tên NV, mỗi dòng đổi một thay đổi).
- Given lượt ghi bù (giờ làm 10/11, ghi lúc 15:10 ngày 12/11), Then "Ghi bởi Hà · 15:10 12/11" và chữ sẵn có "Nhập bù · Ghi lúc …" không lặp lại khi dòng meta có mặt.
- Negative: KTV gọi `GET /record-history/visit/{id}` → `403`; `GET /visits` của KTV / quản lý tắt **không có key** `edited`.

**AC-H2 (mở lịch sử)**
- When bấm dòng "Ghi bởi …", Then mở danh sách thay đổi cũ → mới; mỗi dòng: giờ, người làm, việc, tiền (nếu có `sale.read`), giá trị đổi, lý do. Mặc định đóng.
- Given lượt có thu tiền, Then **một** dòng "Hà ghi lượt · Thu 300.000 ₫"; `audit_log` vẫn đủ hai dòng.
- Given lượt bị hủy, Then dòng "Lan hủy lượt · Trả lại 300.000 ₫ · lý do: khách đổi ý"; meta "… · đã hủy".
- Given lượt bán kèm gói X trong giỏ, sau đó gói X được gia hạn và bị hủy hoàn tiền, Then lịch sử **lượt** có "ghi lượt … kèm bán gói X" nhưng **không** có dòng gia hạn / hủy gói; lịch sử **gói X** có cả ba ("bán gói" / gia hạn / hủy · Hoàn …).
- Given Lan chỉ sửa ghi chú buổi, When người không có `customer.health_notes.read` mở, Then không có dòng đó; When chủ spa mở, Then "Lan sửa lượt · Ghi chú buổi: Có" (trường `session_note`, chỉ có / không, không nội dung); không tính "đã sửa". Ghi chú hồ sơ khách (`customer.update`, "Ghi chú hồ sơ") không bị lọc theo quyền sức khỏe.
- Negative: không bao giờ có nội dung ghi chú buổi, lưu ý sức khỏe, ghi chú gói, ghi chú chấm công; không `sale.read` → không key `amount` / `discount`.

**AC-H3 (Sửa lượt đọc như một lượt)**
- Given lượt A sửa thành B rồi B sửa thành C, When mở lịch sử ở A, B hoặc C, Then cùng một danh sách: "ghi lượt" (A) → "sửa lượt (tính lại tiền) · Chênh lệch …" (A→B) → "sửa lượt …" (B→C); meta của C: "Ghi bởi <người ghi A> · <giờ A> · đã sửa 2 lần"; chip của A, B "Đã thay", của C "Đã sửa".
- "Chuyển sang gói" là một dòng "chuyển lượt sang gói X" và tính **một** lần sửa (audit có cả `visit.correct` lẫn `visit.convert_to_package`, gộp thành một dòng).

**AC-H4 (quyền theo loại neo, B1)**
- Given người có `record.history.read` + `sale.read` + `customer.read` nhưng **không** `visit.read`, When `GET /record-history/sale/{id}` của hóa đơn dịch vụ lẻ trong lượt, Then `403` (không trả lịch sử lượt). When hóa đơn bán gói, Then `200` lịch sử gói, dòng mua **không** gộp dòng `visit.create` (gốc là "bán gói").
- `GET /record-history/refund/{id}` cần `sale.read` + `customer.read`; neo gói.

**AC-H5 (operator bật / tắt)**
- `PUT /admin/tenants/{id}/record-history/{role} {visible, reason}` (step-up, lý do 3–500): `owner` / `manager` → `200`; `technician` → `422 role.not_for_technician`; role lạ → `404`; giá trị đang có → `409 role.record_history_unchanged`; thiếu step-up → `403 platform.step_up_required`; spa không có → `404`.
- `follows_template` của vai trò **không đổi**; sau đó operator sửa mẫu vai trò quản lý → spa theo mẫu nhận thay đổi khác nhưng **giữ** công tắc của mình; "Về mẫu" giữ công tắc.
- Có hiệu lực ở request kế tiếp; audit nền tảng `platform.record_history.set` + audit spa `role.permissions_changed` (`platform_admin`, before/after, lý do).
- Negative: session spa gọi route này → `401`; mẫu / vai trò KTV có `record.history.read` → `422`; `PUT /admin/tenants/{id}/roles/manager` gửi danh sách có / không có `record.history.read` → giá trị công tắc giữ nguyên (spa-local).

**AC-H6 (màn Nhật ký thao tác ẩn)**
- Given spa không có dòng `tenant_feature` cho `activity_log` (mọi spa hiện có và spa mới), When chủ spa gọi `GET /audit-log`, Then `403 feature.disabled`; `/me.features` không có `activity_log`; nav không có mục; vào thẳng `/nhat-ky-thao-tac` → trang sẵn có "Không có quyền mở màn hình này" (route guard `only()` ném `ForbiddenRoute`, `router.tsx` / `StatusPage.tsx`).
- Test Rust / e2e hiện có của màn nhật ký bật cờ trong setup (helper `common::enable_feature`, e2e qua API admin), không bỏ spec.
- When operator bật, Then chủ spa thấy lại màn ở request kế tiếp; quản lý vẫn `403`.
- `GET /admin/tenants/{id}/features` trả `activity_log` `enabled: false`, `default_enabled: false`; 6 cờ cũ `default_enabled: true`, hành vi không đổi; `administration_is_never_a_switchable_feature` xanh.

**AC-H7 (PII)**
- Given khách đổi SĐT 0905 111 222 → 0912 333 444, When chủ spa mở lịch sử hồ sơ, Then "SĐT: 0905 111 222 → 0912 333 444"; When quản lý được bật mở, Then "SĐT (đã đổi)" và response không có chuỗi số nào của hai SĐT trong `changes`.
- Given khách đã ẩn danh, Then không tên, không giá trị cũ / mới (ADR-0021).
- Negative: không SĐT nhân viên, ngày sinh khách, `before` / `after` thô, id / tên operator.

**AC-H8 (dấu danh sách)**
- `edited` có mặt chỉ khi người gọi có `record.history.read`, vắng mặt khi không; `GET /appointments` chỉ tính khi `marks=true` (web gửi ở dạng danh sách).
- `edited` = true ⇔ "đã sửa N lần" có N ≥ 1 với cùng người gọi (test đối chiếu).
- Chip theo điều kiện loại trừ của D9; Sổ doanh thu chỉ hàng `entry = sale`, dịch vụ lẻ, chưa hủy mới có thể "Đã sửa" (hàng hủy hóa đơn của bản B bị thay không bao giờ "Đã sửa").
- Dữ liệu test của `edited_agrees_with_summary_edits` gồm một chuỗi A→B→C và một lần chuyển sang gói.

**AC-H9 (UI)** 360 px không cuộn ngang; sáng + tối; axe không lỗi serious / critical; nút mở là `<button>` có `aria-expanded`; bấm vào vùng lịch sử trong thẻ không gập thẻ; tải / lỗi / trống có chữ (design page).

## 6. Edge cases
1. **Sửa lượt (chuỗi).** Chuỗi `corrects_visit_id` lên và xuống (≤ 20 bản). Mở từ bản nào cũng ra cùng câu chuyện. "Ghi bởi" = người làm `visit.create` của bản gốc; số lần sửa theo D9. Bản cũ: chip "Đã thay", chữ "Đã thay bằng lượt mới."
2. **Chuyển sang gói.** Gộp theo hạng 5 của `fold_rule`; tên gói trong dòng; gói mới có lịch sử riêng bắt đầu bằng dòng mua (gốc "bán gói" trong lịch sử gói).
3. **Lượt + hóa đơn + gói trong giỏ.** Lịch sử lượt gộp như D9 (kèm tên gói, Thu = mọi hóa đơn). Hóa đơn bán gói (kể cả bán trong giỏ, `sale.visit_id` có giá trị) thuộc lịch sử **gói**: gia hạn / hủy / hoàn tiền / sửa ghi chú gói không vào lịch sử lượt.
4. **Dòng "Hoàn tiền" ở Sổ doanh thu** → lịch sử gói bị hủy.
5. **Khách đã ẩn danh:** chủ thể không tên, không link; `customer.update` không giá trị.
6. **Người làm đã nghỉ:** "Hà (đã nghỉ)" (cùng luật `left` của bộ lọc Nhật ký thao tác). Tài khoản không còn → "Một tài khoản đã xóa".
7. **Đội vận hành / Hệ thống:** "Đội vận hành" (không tên / id), "Hệ thống".
8. **Dữ liệu seed giờ lùi:** mọi giờ từ `audit_log.at` (ADR-0021 D8), không trộn `created_at`.
9. **Không có dòng tạo** (D12): gói sổ giấy → "Nhập từ sổ giấy · dd/mm/yyyy" (từ `opening_balance.import`, hoặc `customer_package.origin = 'opening_balance'` → chỉ ngày `starts_on`, không giờ); lượt / hóa đơn / gói / lịch hẹn → "Ghi bởi …" từ cột `created_by` / `created_at`; khách, dịch vụ, mẫu gói, ô chấm công không có dòng tạo → "Chưa rõ ai ghi"; danh sách trống "Chưa có lịch sử thay đổi."
10. **Nhiều thay đổi:** tối đa 100 dòng gần nhất, `omitted`; web "Còn {n} thay đổi cũ hơn."
11. **Công tắc đổi giữa phiên:** request kế tiếp `403` → web ẩn khối, chuyển focus về nút / tiêu đề của bản ghi, gọi lại `/me`.
12. **Tính năng tắt:** lịch hẹn tắt → `403 feature.disabled` cho lịch sử lịch hẹn; lương tắt → quy tắc lương tương tự.
13. **Ô chấm công:** phạm vi như sổ (`hr::scope::RecordScope::lists` + `employed_in(month)`): ô **của chính mình** → `404`; quản lý xem ô của quản lý khác / chủ spa → `200` (sổ hiện họ; web mở qua sheet chỉ xem, K49). Một lần lưu sổ ghi nhiều ô cùng thời điểm: lịch sử ô chỉ lấy dòng có `date` của ô đó.
17. **Thưởng / phạt sửa sang người khác:** `PUT /adjustments/{id}` có thể chuyển khoản sang nhân viên khác (`hr/src/adjustment.rs`); chuỗi nối bằng cùng thời điểm + cùng người làm + đúng một hủy + một ghi; phạm vi kiểm cho cả hai người.
14. **Hai thao tác cùng giây** (seed): không gộp nhầm (khóa gộp là id trong dòng audit, AC11.9).
15. **Key audit cũ:** câu chung "làm một thao tác khác".
16. **Gói sổ giấy được gia hạn:** "Nhập từ sổ giấy" vẫn là dòng tạo; gia hạn là dòng sau (không thành "người ghi").

## 7. Ma trận quyền
| Ai | Dòng "ai ghi" + lịch sử | Chip "Đã sửa" | SĐT cũ / mới của khách | Màn Nhật ký thao tác |
|---|---|---|---|---|
| Chủ spa, công tắc bật (mặc định) | ✓ (trong quyền đọc của loại neo) | ✓ | đủ | chỉ khi cờ `activity_log` bật (mặc định tắt) |
| Chủ spa, operator tắt | — (`403`, không key `edited`) | — | — | như trên |
| Quản lý, operator bật | ✓ (trong quyền đọc của loại neo: không lương, không danh mục / cài đặt) | ✓ | "SĐT (đã đổi)", không giá trị | — (`audit.read` owner-only) |
| Quản lý, tắt (mặc định) | — | — | — | — |
| KTV | — (không cấp được) | — | — | — |
| Người đã nghỉ | — | — | — | — |
| Operator | không gọi được route spa (`401`); chỉ bật / tắt ở `/admin` | — | — | bật / tắt cờ |

Chip "Đã thay" / "Đã hủy" (trạng thái sẵn có) hiện cho mọi người đọc được danh sách. Chi tiết route → permission: [authz-matrix](../../security/authz-matrix.md).

## 8. UX notes
Chi tiết ở [design/record-history](../../design/record-history.md). Tóm tắt: khi mở **một** bản ghi, dòng meta 13 px `--text-muted` là **phần tử cuối** của phần chi tiết; bấm → mở danh sách ngay bên dưới (disclosure, mặc định đóng). Ở màn **nhiều bản ghi**, nút mở là phần tử có sẵn (giờ của thẻ Lịch sử khách; nút cạnh chip trạng thái của thẻ gói; mục ⋯ ở "Bán gói không kèm lượt" và sổ thưởng/phạt), không thêm hàng. Sổ doanh thu: nút trong ô giờ / thẻ → sheet. Admin: panel "Lịch sử trên từng bản ghi"; cờ "Nhật ký thao tác" trong panel "Tính năng" với "Mặc định · tắt".

## 9. Data
- Không bảng mới; đọc `app.audit_log` + join tên. PII: tên + SĐT cũ / mới khách (ADR-0021), SĐT chỉ cho `audit.read` (D8). Không ghi gì mới vào audit ngoài thao tác của operator.
- Migration 0052 (đợt 1a): permission + spa-local trong hàm áp mẫu / sửa vai trò, hàm công tắc, `session_actor` + `enabled_features`, `platform_set_feature` trả null, 2 index một phần. Chi tiết + thứ tự deploy: [architecture](../../architecture/record-history.md).
- Lưu giữ: theo audit (giữ mãi, EM Q-DB4); `customer.anonymize` (chưa build) phải xóa tên / SĐT trong audit (ADR-0021).

## 10. Non-functional
- **Perf:** `GET /record-history/*` p95 ≤ 100 ms ở bench có **audit thật cỡ ~60.000 dòng / năm × 3 năm**; danh sách có `edited` p95 tăng ≤ 10 ms khi bật, 0 khi tắt. Web: JS khởi động **≤ 125 KiB** (số đo của reviewer 124,70 KiB), 4 module khởi động có đổi vài dòng (`lib/queryClient.ts`, `features/auth/session.ts`, `app/access.ts`, `components/ResponsiveTable.tsx`), mã lịch sử còn lại ở chunk lazy ([architecture §7](../../architecture/record-history.md), [design §1.1](../../design/record-history.md)).
- **Bảo mật:** non-negotiable #1–#4; [threat-model](../../security/threat-model.md) § Lịch sử trên từng bản ghi.
- **A11y:** WCAG 2.2 AA, vùng chạm ≥ 44 px trên điện thoại, focus không mất khi mở / đóng / bị ẩn do `403`.
- **i18n:** tiếng Việt; tên người làm như Nhật ký thao tác (`actorName`).

## 11. Out of scope
- Công tắc trong app của spa; quyền theo từng người.
- Màn riêng cho chủ spa xem việc của đội vận hành khi cờ nhật ký tắt (LEAD: không làm).
- Lịch sử hồ sơ nhân viên, đánh giá tháng, bảng lương, gọi lại khách (D11).
- Xuất / tìm kiếm trong lịch sử; đổi cách ghi audit (không thêm action, không đổi payload).

## 12. Open questions
Không (EM ủy quyền quyết). Các điểm xem lại ghi ở ADR-0022 § Revisit when.

## Review log
Hai lượt tự rà soát sau bản nháp (EM yêu cầu "review đi review lại kĩ").

**Lượt 1 — đối chiếu code + non-negotiables**
| Phát hiện | Xử lý |
|---|---|
| Bản nháp định đặt cờ màn nhật ký sở hữu `audit.read` → phá test `administration_is_never_a_switchable_feature` và làm chủ spa mất `audit.read` (owner floor). | D5: cờ chỉ sở hữu route `/api/v1/audit-log`, permission giữ nguyên. |
| Cơ chế cờ hiện tại "không dòng = bật"; `tenant_feature.updated_by` NOT NULL tham chiếu operator nên migration không thể chèn dòng "tắt" cho mọi spa. | Thêm `default_enabled` vào registry code; `session_actor` trả thêm danh sách cờ bật; hàm `platform_set_feature` trả `NULL` khi chưa có dòng để Rust áp mặc định ([architecture §3](../../architecture/record-history.md)). |
| ~~`attendance.record`, `evaluation.daily_review` ghi audit với `entity_type = 'staff'`, `entity_id` = id nhân viên (không phải id ô).~~ | ~~Endpoint ô theo `(staff_id, date)`, lọc `date` trong JSON của dòng audit; dùng index `(tenant_id, entity_type, entity_id, at)` rồi lọc (~1.000 dòng / nhân viên / năm).~~ *(thay ở lượt 3 B2b + S6: lọc theo `date` của dòng bằng index biểu thức `audit_log_staff_day_idx`)* |
| ~~`visit.correct` ghi `entity_id` = lượt **cũ**, lượt mới chỉ có `corrects_visit_id`.~~ | ~~Neo lịch sử là cả chuỗi; dấu `edited` của lượt mới = `corrects_visit_id IS NOT NULL` hoặc có `visit.update` / `visit.addition`.~~ *(thay ở lượt 4 B1: `edited` của lượt = có bản thay / bị thay / `visit.update` đổi giờ-KTV; `visit.addition` không tính (lượt 3 UX S11))* |
| ~~Web Quầy đã có badge "Đã sửa" (bản cũ, `corrected_by_visit_id`) và "Đã hủy"; Lịch sử khách có "Lượt sửa lại".~~ | ~~Giữ nguyên các badge cũ (mọi người vẫn thấy); `edited` chỉ thêm cho bản đang hiệu lực.~~ *(thay ở lượt 3 UX B1: bản bị thay đổi thành "Đã thay")* |
| Nhiều màn đã hiện "ai làm" (hồ sơ NV, thưởng/phạt, đánh giá tháng, chốt lương, gọi lại). | D11, đưa vào §4.2. |
| ~~Không có quy tắc che SĐT nào trong web; quản lý thấy SĐT khách đầy đủ.~~ | ~~D8 che số **cũ** ở server cho người không có `audit.read`; số hiện tại giữ như hiện nay.~~ *(thay ở lượt 3 LEAD + S8: không gửi giá trị SĐT cho người thiếu `audit.read`)* |
| Ledger không có màn chi tiết; hàng hoàn tiền có id = refund id. | Sheet hóa đơn mới ở Sổ doanh thu (chỉ khi có quyền); `record=refund` → server đổi neo sang gói (`refund.customer_package_id`). |
| ~~Bản nháp cho Lịch sử khách tự tải lịch sử từng thẻ (dòng thời gian dài → hàng chục request; và thẻ là "danh sách" theo câu 3 của EM).~~ | ~~Quy tắc tải ([design §1.2](../../design/record-history.md)): một bản ghi đang mở thì tự tải; nhiều bản ghi (thẻ lịch sử khách, các gói trong hồ sơ) chỉ có nút "Xem ai ghi, ai sửa", bấm mới tải.~~ *(thay ở lượt 3 UX S3: giờ của thẻ là nút mở)* |
| Gói nhập sổ giấy: bản nháp định nhận ra qua dòng audit. | Cột có sẵn `customer_package.origin = 'opening_balance'` (0005) → `source = paper_book`. |
| Chữ mẫu "Chủ spa Lan hủy gói": `actorName` không có tiền tố vai trò. | Sửa thành "Lan hủy gói"; mọi chữ người làm đi qua `actorName`. |
| ~~Đếm bảng §4 sai (44 / 20).~~ | ~~Đếm lại theo `Action` trong `kernel/src/audit.rs`: 35 có / 29 không = 64.~~ *(thay ở lượt 3: 37 / 27 sau khi thêm thưởng/phạt)* |
| ~~0052 thêm quyền cho vai trò chủ spa của mọi spa: có làm vai trò "không theo mẫu" không?~~ | ~~Không đổi `follows_template` (mẫu owner cũng được thêm cùng migration) — ghi ở [architecture §2](../../architecture/record-history.md).~~ *(thay ở lượt 3 LEAD spa-local: áp mẫu / so sánh theo mẫu bỏ qua quyền này)* |

**Lượt 2 — secure, perf, mobile, nhất quán spec / ADR**
| Phát hiện | Xử lý |
|---|---|
| Secure: endpoint chung theo `{record}` dễ quên kiểm quyền đọc của một loại. | Bảng `record → (permission đọc, phạm vi)` là `match` vét cạn trong Rust + test matrix mỗi loại × mỗi vai trò ([architecture §4](../../architecture/record-history.md)). |
| Secure: lịch sử có thể mang trường ngoài quyền màn gốc (tiền, ghi chú). | D7: lọc ở server sau khi gộp; test "quản lý không `sale.read` không có key `amount`". |
| ~~Secure: operator đổi một permission bằng đọc-sửa-ghi danh sách vai trò → hai operator cùng lúc có thể ghi đè.~~ | ~~Hàm DB `platform_set_role_permission` khóa dòng role (`FOR UPDATE`), chỉ thêm / xóa đúng một dòng.~~ *(thay ở lượt 3 LEAD: hàm `platform_set_record_history`, spa-local)* |
| Secure: technician có thể được cấp qua mẫu vai trò. | Rào `NOT_FOR_TECHNICIAN` trong `check_grants` (mẫu + spa) + test. |
| ~~Perf: Quầy nằm trong chunk khởi động sát ngân sách 125 KB.~~ | ~~Mã lịch sử (kể cả `sentence.ts`) ở chunk lazy; danh sách chỉ đọc boolean `edited`.~~ *(thay ở lượt 3 UX S1: `TodayVisits` đã lazy; lượt 4 S14)* |
| Perf: dấu `edited` cho 50–100 hàng không được thành N truy vấn. | `EXISTS` trong cùng câu SQL của danh sách, chỉ khi `$can_history`; mỗi hàng một lần dò index. |
| Mobile: lồng sheet trong sheet (`AppointmentSheet`, `PersonDaySheet`). | Lịch sử luôn mở **tại chỗ** (disclosure), không mở sheet thứ hai. |
| Nhất quán: ADR-0021 ghi "Không cờ tính năng" cho màn nhật ký; raw sáng nay "không hiện ai làm". | ADR-0022 sửa một phần ADR-0021 (D1) và đảo raw `actor-trace-data-only`; ghi vào ADR-0021 + management-views §11 + log. |
| Nhất quán: tên vai trò / chữ người làm. | Dùng `actorName` + "(đã nghỉ)" như bộ lọc Nhật ký thao tác; không thêm tiền tố "Quản lý". |
| ~~Secure: thứ tự lỗi có thể thành "oracle" tồn tại bản ghi.~~ | ~~Kiểm `record.history.read` + quyền màn gốc **trước** khi tra id → người không quyền luôn `403`, không phân biệt id có / không.~~ *(thay ở lượt 4 N2: người có quyền route nhưng thiếu quyền neo phân biệt được 404/403 (chấp nhận))* |
| Secure: replay idempotent trả body có `edited` theo quyền lúc ghi. | Đã kiểm `app.idempotency_record` khóa `(tenant_id, actor_user_id, key)`: chỉ chính người ghi nhận lại; `edited` không chứa tên. Chấp nhận, ghi ở threat model. |
| Secure checklist "rotate session khi đổi quyền". | Quyền đọc mỗi request (`session_actor`, ADR-0017) nên bật / tắt có hiệu lực ngay request kế tiếp, giống AC7.3 của sửa vai trò; không cần xoay session. |
| Code: `mayOpen.activity` có thật ở `apps/web/src/app/access.ts` (`can(me, "audit.read")`); `hasFeature` trả `true` khi `/me` thiếu `features`. | Đổi thành `can(…) && hasFeature(me, "activity_log")`; `/me` luôn có `features` khi có spa đang dùng. |
| ~~Mobile: hàng Sổ doanh thu bấm được trên desktop là bảng tự dựng.~~ | ~~Nút trong ô đầu (không biến cả `<tr>` thành nút), thẻ điện thoại bấm cả thẻ; nhãn "Xem hóa đơn của chị …".~~ *(thay ở lượt 3 UX S7 + lượt 4 S8: nút ở ô giờ, thẻ điện thoại có nút giờ thật)* |

**Review lượt 3 (độc lập)** — hai reviewer độc lập (bảo mật / backend và UX / frontend) đọc bản của PR #184; lead quyết các điểm xung đột (đánh dấu LEAD). Mỗi phát hiện đã được đối chiếu lại với code trước khi sửa. Vài kết luận của lượt 1–2 ở trên vì thế đã bị thay; bảng này là bản đúng.

| # | Phát hiện | Đối chiếu code | Xử lý |
|---|---|---|---|
| LEAD | Màn nhật ký: ẩn ngay hay chờ? | EM: "tạm thời ẩn … off show cái nhật kí thao tác đi" | `activity_log` mặc định tắt từ đợt 1. Đợt 1 gộp cả gói (kể cả hủy / hoàn tiền) và hồ sơ khách. Cửa sổ hồi quy ghi ở plan. ADR-0018 D6 / ADR-0003 được sửa: khi cờ tắt, chủ spa không xem được việc của operator trên UI. Không làm màn riêng (D5). |
| LEAD | Công tắc làm vai trò "lệch mẫu"; sửa mẫu có thể âm thầm đổi công tắc | `platform_apply_role_template` (0040) và `platform_set_tenant_role` (0041) ghi đè cả danh sách và đặt `follows_template` | `record.history.read` là **spa-local** (D1, D2). 0052 sửa hai hàm để bỏ qua / giữ quyền này. Hàm công tắc cứng permission và vai trò, không đụng `follows_template`. "Về mẫu" giữ giá trị hiện tại. |
| LEAD + S8 | Che giữa SĐT vẫn lộ một phần | — | Người không có `audit.read` không nhận giá trị nào: "SĐT (đã đổi)" (D8). Bỏ `mask_phone`. |
| B1 | Đổi neo bỏ qua quyền đọc của loại đích | `customers/{id}/history` cần `customer.read + visit.read + sale.read` (`reporting/src/http.rs`) | Đúng. Sau khi đổi neo thì kiểm quyền của loại neo cuối, `403` trước khi đọc. Lịch sử gói không gộp sang dòng `visit.create` (AC-H4). Thêm test matrix "có `sale.read`, thiếu `visit.read`". |
| B2a | Gói bán trong giỏ có `sale.visit_id` → vòng đời gói lọt vào lịch sử lượt; Ledger và DaySales tính `edited` khác nhau | 0021: "Every sale made during a visit … points at the visit" | Đúng. Thành viên tính theo **id dòng audit**: lượt = dòng của các lượt trong chuỗi + hóa đơn dịch vụ lẻ + `use_expired_override` của lượt. Hóa đơn bán gói thuộc gói. Một định nghĩa "tính là sửa" cho mỗi loại (D6, D10). |
| B2b | Ô chấm công: một lần `PUT /attendance` ghi nhiều ô cùng thời điểm, nên lấy "cùng thời điểm" là lọt ngày khác | `hr/src/attendance.rs`: vòng `for (e, before, after) in changed` trong cùng một transaction, `entity_id = staff_id` | Đúng. Các loại chỉ có hạng 0 (lịch hẹn, khách, ô ngày, thưởng/phạt, danh mục, cài đặt) bỏ bước "cùng thời điểm" và bỏ gộp. Ô lọc theo `date` của chính dòng. |
| S3 | "Ghi bởi" lấy từ dòng đầu bất kỳ | `opening_balance.rs` chỉ ghi `opening_balance.import` trên khách, `after` có `customer_created` và `customer_package_ids` | Đúng. D12: "Ghi bởi" chỉ lấy từ thao tác tạo, nếu không có thì lấy cột. Thêm edge case 16. |
| ~~S4~~ | ~~`fields()` của `visit.update` thiếu ghi chú / người đi cùng: dòng "sửa lượt" rỗng, và lộ việc đổi ghi chú~~ | ~~`activity.rs` `A::VisitUpdate => [check_in_at, staff]`; writer ghi `note`, `companions` (`operations/src/service.rs`)~~ | ~~Đúng. Thêm `/note` (chỉ có / không, không giá trị) và `/companions` vào `fields()` (màn nhật ký cũng đổi theo). Người không có `customer.health_notes.read`: bỏ thay đổi ghi chú, và bỏ cả dòng nếu dòng chỉ có thay đổi đó. Đổi ghi chú không tính "đã sửa".~~ *(thay ở lượt 4 S3: trường tên `session_note` (Flag), lọc theo (action, trường))* |
| S5 | Bench không có audit; con số "~6.800 dòng / năm" | `reports_bench.rs` có 0 lần `audit_log`. Con số 6.800 **có nguồn**: staging sau seed (log 2026-11-11: 6.787 dòng / 12 tháng), nhưng seed chỉ có 1.990 lượt / năm, còn bench có ~14.700 | Bác một phần (con số có nguồn), nhưng kết luận đúng. Bench seed audit ~60.000 / năm × 3. Index một phần `audit_log_edit_idx`. `edited` của lịch hẹn chỉ khi `marks=true`. Sổ doanh thu chạy `EXISTS` ở câu ngoài, sau `LIMIT`. |
| S6 | Ô chấm công dò cả lịch sử của nhân viên | — | Index biểu thức ngày một phần cho 2 action ship trong 0052. Query lặp đúng biểu thức. |
| S7 | Test đợt 3 sai: quản lý đọc được ô của quản lý khác / chủ spa | `RecordScope::lists` chỉ ẩn hồ sơ của chính mình (`hr/src/scope.rs`); `attendance.rs` lọc thêm `employed_in` | Đúng (tên kiểu cũng sai: `RecordScope`, không phải `Scope`). Sửa edge case 13 và test: ô của mình `404`, ô của quản lý khác `200`. |
| S9 | Hai bản phần làm giàu (list và `rows_by_ids`) | — | Làm ngay ở đợt 1a: `activity::list` = chọn id → `rows_by_ids` dùng chung. Bỏ test đối chiếu tạm. |
| Nits BE | `member_access`; cửa sổ deploy / rollback; REVOKE/GRANT sau DROP/CREATE; `AuditPage.tsx` ACTIONS; `no-store` cho danh sách có `edited` | `identity/src/repo.rs` có `member_access`, gọi từ `service::tenant_access` | Đúng hết. Đã ghi vào architecture §3, §8 và plan. |
| UX B1 | "Đã sửa" đang mang nghĩa "bản cũ bị thay" | `TodayVisits.tsx` (chip khi có `corrected_by_visit_id`), `CustomerHistory.tsx` `visit_voided` | Đúng. Đổi chip của bản bị thay thành "Đã thay" cho mọi vai trò. Một chip, điều kiện loại trừ nhau (D9, sửa ở lượt 4 S4). |
| UX S1 | Ngân sách JS; `TodayVisits` đã lazy | `FrontDeskPage.tsx`: `lazy(() => loadTodayVisits()…)` | Đúng: bản trước viết "Quầy nằm trong chunk khởi động" là sai. Không đo lại được trên máy này (thiếu RAM), nên dùng số của reviewer 124,70 / 125 KiB. Module options query nhỏ đặt ngoài chunk UI; prefetch bằng `useAfterFirstPaint`. |
| UX S2 | Bấm vào vùng lịch sử thì gập thẻ | Guard `closest("a, button, input, select, textarea, label, [role=menu]")` ở `ResponsiveTable.tsx` | Đúng. Thêm `[data-row-inert]` + Vitest. |
| UX S3 | Thêm hàng "Xem ai ghi, ai sửa" cho mỗi thẻ thì rối | — | Dùng giờ của thẻ làm nút mở; thẻ gói đặt nút cạnh chip. |
| UX S4 | "Bán bởi" lẫn với "Người chốt" | `TodayVisits.tsx`, `CustomerHistory.tsx` ("chốt bởi") | "Ghi bởi" ở mọi nơi. |
| UX S5 | Bảng động từ cần theo ngữ cảnh | — | `HISTORY: Record<AuditAction, ((e) => string) \| null>`. Chữ: "sửa lượt", "thêm khách", "thêm dịch vụ", "sửa mẫu gói", "nhập bù lượt". |
| ~~UX S7~~ | ~~Sổ doanh thu: ô đầu là "Giờ" 64 px; thẻ điện thoại có Link khách~~ | ~~`LedgerTab.tsx` `<col style={{ width: "64px" }}>`~~ | ~~Đúng. Tên nút "10:07, xem hóa đơn của chị …". Thẻ dùng guard `closest('a,button')`. Bỏ nút "Mở hồ sơ khách". Giữ mở sẵn, có lý do.~~ *(thay ở lượt 4 S8: giờ trên thẻ điện thoại là `<button>` thật; hàng `void` có title "Hủy hóa đơn")* |
| UX S8 | DaySales nên dùng ⋯, không mở modal | — | Thêm mục ⋯ "Xem ai ghi, ai sửa", hiện tại chỗ mở rộng sẵn có. |
| UX S9 | Invalidation rải rác | `lib/queryClient.ts` đã có `MutationCache({ onError })` | Thêm `onSuccess` invalidate `["record-history"]`. Reply DTO mang `edited` cho các chỗ dùng `setQueriesData`. |
| UX S10 | Wireframe không khớp component thật | — | Quy tắc chung: khối lịch sử luôn là phần tử cuối. Vị trí cụ thể theo `PersonDaySheet` (fieldset), `CustomerProfile`, `AppointmentSheet`. |
| UX S11 | "Đã sửa" quá rộng | — | Thu hẹp (D9): upsell, gia hạn, dùng gói hết hạn là vòng đời. |
| UX S12 | Thưởng/phạt chỉ hiện người ghi bản hiện tại | `AdjustmentsTab.tsx` cột "Người ghi" = `by_name` | Đúng. Thêm lịch sử (đợt 2), sửa D11. `use_expired_override` cũng vào lịch sử lượt. |
| UX S13 | A11y | — | Focus khi bị ẩn do `403`. Dùng lại mũi tên `.disclosure`. Bỏ `role=region`. Tên nút ổn định. `404` → "Không xem được lịch sử của mục này." |
| Nits UX | "Đảo" → "Trả lại"; chữ trống / bị cắt; tên nút admin; `hasFeature` mặc định true; "Nhập bù · Ghi lúc" lặp; dấu lịch hẹn chỉ ở dạng danh sách; `/goi-lieu-trinh` | `session.ts` `?? true`; `sentence.ts` `AMOUNT_LABEL` | Đã làm hết. `hasFeature(me, key, { defaultOff: true })`. Badge admin chọn "Đang hiện / Đang ẩn" vì công tắc nói về hiển thị, không phải bật tính năng. |

Không bác phát hiện nào. Riêng S5 chỉ bác phần "con số không có nguồn", có bằng chứng ở log, còn việc cần làm vẫn làm.

**Review lượt 4 (cổng cuối)** — reviewer cổng cuối đọc bản của PR #185: "cần sửa thêm một lần, chỉ docs, nhỏ". Mọi điểm đã được đối chiếu với code; không điểm nào bị bác. LEAD: S2 resolve tên NV, S11 giữ trang "Không có quyền", S1 e2e bật cờ qua API admin. Các hàng cũ đã bị thay được gạch ngang ở trên.

| # | Phát hiện | Đối chiếu code | Xử lý |
|---|---|---|---|
| B1 | "Tính là sửa" không nhất quán: chuyển sang gói ghi `visit.correct` + `visit.convert_to_package` cùng transaction, đếm theo dòng audit ra 2; gốc A của chuỗi A→B→C có `edited = false` trong khi `edits = 2` | `operations/src/service.rs`: `VisitCorrect` và `VisitConvertToPackage` cùng `entity_id = id` | Đúng. `edits` = số **dòng đã gộp** có hành động gốc tính là sửa. `edited` của lượt thêm điều kiện "bị thay" (`EXISTS visit w WHERE w.corrects_visit_id = v.id`). Test đối chiếu có dữ liệu chuỗi + chuyển sang gói (D10, architecture §4.4). |
| S1 | Cờ mặc định tắt làm đỏ test hiện có | `authz_matrix.rs` dòng `/api/v1/audit-log` (owner); `authz_matrix.rs` so `features.len()` với `Feature::ALL.len()`; `demo_seed.rs` gọi `/audit-log` chờ `200`; `demo/reset.rs` xóa `tenant_feature` | Đúng: plan cũ viết "xanh nguyên" là sai. Thêm helper `common::enable_feature`; matrix bật cờ trong setup; số cờ đếm theo mặc định bật; e2e bật qua API admin (LEAD); ghi vào architecture §7 + plan 1a. |
| S2 | `after.staff` là mảng `{line_id, from, to}` gồm cả dòng không đổi; `diff()` bỏ mảng; chỉ KTV lịch hẹn được resolve tên | `service.rs` đẩy mọi dòng gửi lên vào `staff_changes`; `activity.rs` `scalar()` trả `None` cho mảng; join `sb` / `sa` chỉ cho `appointment` | Đúng. "Đổi KTV" = có phần tử `from <> to` (SQL + `counts_as_edit`). Làm giàu resolve tên NV, một thay đổi mỗi dòng đổi (LEAD: giữ "KTV: Thu → Hoa"). |
| S3 | `note` trùng `FIELD.note` = "Ghi chú hồ sơ"; writer ghi `"[redacted]"` / `"cleared"`, không phải bool; bộ lọc sẽ xóa nhầm ghi chú hồ sơ của `customer.update` | `sentence.ts` `note: "Ghi chú hồ sơ"`; `crm/src/http.rs` `note` của khách không gắn quyền sức khỏe | Đúng. Đổi tên trường thành `session_note` (Flag, map `true` / `false`). Lọc theo (action, trường). |
| S4 | Điều kiện chip phải loại trừ nhau | `TodayVisits.tsx` / `CustomerHistory.tsx` kiểm `corrected_by_visit_id` trước | Đúng. Đã thay = `corrected_by_visit_id`; Đã hủy = voided && !corrected; Đã sửa = edited && !voided (D9, design §1.4). |
| S5 | Áp mẫu ghi audit `after = to_jsonb(p_permissions)` | `0040_roles_and_accounts.sql` | Đúng. `IS DISTINCT FROM`, dữ liệu ghi và audit `after` đều dùng tập mới của spa. Thêm test trên dòng audit. |
| S6 | Tra `use_expired_override` theo action + JSON không có index | — | Đúng. Đi qua visit_line → customer_package_item → gói, rồi `entity_type = 'customer_package'` + index entity. Thêm literal `entity_type` vào mệnh đề sổ giấy và thưởng/phạt. |
| S7 | Index một phần không dùng được với tham số | sqlx chuẩn bị câu có tham số (plan chung) | Đúng. Viết literal action và lặp đủ `IN` của index; so ngày `= $n::text`. |
| S8 | Hàng `void` của Sổ doanh thu chưa có đặc tả; hóa đơn đã hủy của bản B bị thay sẽ mang "Đã sửa" | `0021` `revenue_book`: nhánh `'void'` có id = id hóa đơn; vị từ gói `sale_line.line_type = 'package'` (`0021`, `reporting/src/day.rs`) | Đúng. Chip chỉ khi `entry = sale` && chưa hủy && dịch vụ lẻ && `edited`. Hàng hủy mở `{type: sale}` với title "Hủy hóa đơn". Giờ trên thẻ là `<button>`. Đổi neo dùng `line_type = 'package'`, bỏ `customer_package.sale_id`. |
| S9 | Thiếu đặc tả cho `purchase_voided`, `package_cancelled`, `package_extended` | `CustomerHistory.tsx` `KIND` | Đúng. Thêm bảng theo loại thẻ. Nút giờ là toggle có `aria-expanded` / `aria-controls`. |
| S10 | Tên truy cập phải bắt đầu bằng chữ hiển thị | — | "Lịch sử gói {tên}"; admin "Hiện cho Quản lý: …". |
| S11 | AC-H6 viết "về trang chủ" | `router.tsx` `only()` ném `ForbiddenRoute`; `StatusPage.tsx` "Không có quyền mở màn hình này" | Đúng. Sửa AC (LEAD: giữ trang này). |
| S12 | Route lịch hẹn / cài đặt của lịch sử không nằm trong prefix của cờ | `kernel/src/feature.rs` | Đúng. Thêm prefix vào `appointments` và `payroll`. |
| S13 | Sửa thưởng/phạt có thể chuyển sang nhân viên khác | `hr/src/adjustment.rs` `replace`: "Both the old and the new person must be in the caller's scope" | Đúng. Nối chuỗi bằng cùng thời điểm + cùng người + một hủy + một ghi; kiểm phạm vi cả hai người. |
| S14 | Diễn đạt ngân sách JS | — | Viết "≤ 125 KiB, mã lịch sử lazy" và liệt kê 4 module khởi động có đổi. |
| N1 | Hàm công tắc trả boolean nhưng audit cần danh sách | — | Trả `before` / `after` là `text[]` đọc dưới khóa. Response = `RoleGrants`, request = `RecordHistorySwitch`. |
| N2 | `Path<Uuid>` trả 400; "không phân biệt id" nói quá | Axum `Path` extractor | Đúng. Ghi rõ 400; người có `sale.read` nhưng thiếu quyền neo phân biệt được 404 / 403 (chấp nhận, threat model). |
| N3 | Hàng review cũ đã lỗi thời | — | Đã gạch ngang + ghi chỗ thay. |
| N4 | Test IDOR ô ngày thuộc đợt 2 | — | Sửa threat model. |
| N5 | Tiêu đề DaySales; thiếu "Meta, gói"; `book=review` lệch với `daily_review` | `DaySales.tsx` "Bán gói không kèm lượt" | Đúng. Sửa tên, thêm hàng chữ, dùng `book=attendance\|daily_review`. |
| N6 | `created.at` của sổ giấy = `starts_on` (không có giờ) | — | Chỉ có `day`, không có `at`. |
| N7 | Sheet Sổ doanh thu gặp 403 | — | Đóng sheet, đưa focus về nút giờ. Nút chỉ hiện khi có cả quyền của loại neo. |
| N8 | `reset-demo` xóa cờ operator đã bật | `demo/reset.rs` liệt kê `tenant_feature` | Ghi cạnh cách giảm thiểu trong plan. |
| N9 | Gốc của phần gộp tính thế nào trong lịch sử | — | Tính trong Rust: gốc = dòng không `folds_into` dòng nào khác trong tập (architecture §4.3 bước 5). |

**Quyết định khi build (đợt 1a, 2026-11-11)** — EM không muốn được hỏi; mỗi điểm chọn phương án khớp nhất với D1–D12, ghi lại ở đây.

| # | Chỗ thiết kế chưa rõ / lệch code | Quyết định |
|---|---|---|
| K1 | "Id sai dạng: `Path<Uuid>` trả `400` trước handler" mâu thuẫn với test plan "người không quyền gửi id rác vẫn `403`" | Handler nhận `Result<Path<Uuid>, PathRejection>`: `401` → quyền (`record.history.read` + quyền của route) → id sai dạng `400` → đổi neo `404` → quyền loại neo `403`. Người không quyền không phân biệt được id rác / id thật. |
| K2 | Nhập sổ giấy ghi **cả** `customer_package.create` (origin `opening_balance`) và, khi tạo khách, `customer.create` cùng lúc với `opening_balance.import` → lịch sử có hai dòng tạo | Khi bản ghi có dòng `opening_balance.import` thuộc nó, bỏ hai dòng do chính lần nhập ghi; dòng nhập là dòng tạo (D12): `source = paper_book`, người nhập là actor, không `at`; gói lấy `day = starts_on`, khách lấy ngày nhập. |
| K3 | "Test so `SPA_LOCAL` Rust với hằng trong SQL" | Hằng SQL là hàm `app.spa_local_permissions()` (IMMUTABLE, không cấp cho PUBLIC); hai hàm vai trò dùng nó; test `spa_local_permissions_agree_between_rust_and_sql` gọi hàm. |
| K4 | `edited` "trong cùng câu SQL" vs "EXISTS ở câu ngoài, sau LIMIT" | `VisitDto`: `CASE WHEN $can` trong câu `repo::visits`. Sổ doanh thu và Lịch sử khách: câu thứ hai `reporting::history::edited_visits(id của trang)` chỉ khi có quyền (sau LIMIT, một lần dò mỗi lượt). Cả ba lặp nguyên văn danh sách action của `audit_log_edit_idx`. |
| K5 | `HistoryEvent.edited` cho thẻ lượt nào | Có ở cả `visit` và `visit_voided` khi có quyền; web áp quy tắc chip D9 (đã hủy thì không "Đã sửa"). |
| K6 | `ActivityActor.left` chỉ cho lịch sử? | `rows_by_ids` tính cho cả màn nhật ký (cùng luật bộ lọc người); web `actorName` thêm " (đã nghỉ)" ở cả hai. |
| K7 | Dòng phụ của lượt (§4.3 bước 3) lọc "cùng `at`" bằng SQL | Chọn mọi `sale.create` / `customer_package.create` của hóa đơn bán gói có `sale.visit_id` ∈ chuỗi, rồi chỉ giữ dòng gộp được vào một dòng thành viên (gộp đòi cùng thời điểm + cùng người); dòng không gộp được bị bỏ, không bao giờ thành dòng riêng. |
| K8 | Hóa đơn bán nhiều gói / không gói không lượt | Neo gói đầu tiên theo id (hiện mỗi hóa đơn bán gói một gói); hóa đơn không dòng gói và không lượt → `404`. |
| K9 | Response công tắc `RoleGrants.follows_template` | Đọc lại từ dòng role trong cùng transaction (công tắc không đụng nó). Hàm DB từ chối cả `p_role` null (`22023`). |
| K10 | Màn nhật ký nhận trường mới của `visit.update` ngay từ 1a | `sentence.ts` thêm nhãn `session_note` "Ghi chú buổi", `companions` "Người đi cùng"; KTV hiện tên cũ → mới. Phần còn lại của `sentence.ts` (`HISTORY`, "Trả lại") để 1b. |
| K11 | e2e `activity.spec.ts` "bật cờ qua API admin trong setup" | Bật bằng một dòng `tenant_feature` (helper `e2e/support/db.ts` `switchFeatureOn`, như các chỉnh DB test khác): đăng nhập console qua API sẽ tiêu lần cài MFA đầu tiên mà `admin.spec.ts` (chạy sau) kiểm. Đường console thật đã có test Rust `the_activity_screen_is_off_until_an_operator_switches_it_on`. |

**Quyết định khi build (đợt 1b, 2026-11-11)** — cùng nguyên tắc: không hỏi EM, chọn phương án khớp D1–D12 và design, ghi lý do.

| # | Chỗ thiết kế chưa rõ / lệch code | Quyết định |
|---|---|---|
| K12 | Sổ doanh thu `403`: design N7 "đóng sheet, focus về nút giờ", nhưng nút giờ biến mất ngay khi `/me` mới không còn `record.history.read` (focus rơi về `<body>`) | Đóng sheet rồi focus **link tên khách của cùng hàng** (luôn còn, là "tiêu đề" của hàng). Màn khác: focus về phần tử không đổi theo quyền: nút tên khách của hàng (Quầy), nút ⋯ của hàng ("Bán gói không kèm lượt", thêm prop `id` cho `RowMenu`), tiêu đề thẻ / tên gói / "Thông tin khách" (`tabIndex=-1`, chỉ khi có quyền). |
| K13 | `403` lúc nào cũng kéo focus? | Chỉ chuyển focus khi focus đang ở trong khối lịch sử (rơi về `<body>` vì nút bị gỡ) hoặc ở nút mở nó (`aria-controls`); `403` ngay lần đọc đầu (focus đang ở chỗ khác) không giật focus. Luôn `invalidate(["me"])` và báo màn (`onForbidden`). |
| K14 | Chỉ `/quay` được nói prefetch chunk | Ngoài `/quay` (`useAfterFirstPaint`, như `TodayVisits`), các màn có nút mở (Lịch sử khách, Sổ doanh thu, gói của khách) tải trước chunk khi mount nếu `canTrace` (`usePreloadRecordHistory`), để bấm là có ngay; `LazyRecordTrace` gọi `usePrefetchQuery` để request chạy song song với chunk. Hồ sơ khách gắn `RecordTrace` sau lần vẽ đầu (`useAfterFirstPaint`, placeholder cùng chiều cao) để request của hồ sơ đi trước. |
| K15 | Đóng lịch sử của "Bán gói không kèm lượt" thế nào | Mục ⋯ bật / tắt: "Xem ai ghi, ai sửa" ↔ "Ẩn ai ghi, ai sửa"; mở thì focus vào nút meta (mục menu vừa bấm đã mất). |
| K16 | Tên nút giờ ở Sổ doanh thu cho hàng hủy / hoàn tiền; hàng dịch vụ lẻ không có lượt | "10:07, xem hủy hóa đơn của …" / "…, xem hoàn tiền của …" (hàng bán vẫn "xem hóa đơn"). Hàng dịch vụ lẻ không có `visit_id` (hóa đơn sổ giấy) không có nút (server trả `404`, K8). |
| K17 | Bố cục một dòng lịch sử; giờ ở lịch sử gói / khách (wireframe §2.5–2.6 ghi ngày trước, §1.2 ghi "10:30 12/11") | Theo hợp đồng component §1.2 ở mọi loại: `<time>` "10:30", khác ngày bản ghi "10:30 12/11", khác năm thêm năm. Dòng chính "Người + việc · tiền · giảm … · lý do: …" (lý do cùng dòng, đúng AC-H2), từng thay đổi một dòng phụ; "Còn {n} thay đổi cũ hơn." đứng **trước** danh sách (phần bị cắt là phần cũ). Tên truy cập của danh sách: "Lịch sử {noun}" (prop `noun`). `visit.correct` hiện lý do server gửi nguyên văn ("lý do: Sửa lượt: …"), không tự bỏ tiền tố. |
| K18 | CSS của khối lịch sử ở chunk hay global | `.record-trace*`, `.disclosure-arrow` (dùng chung hình mũi tên với `.disclosure`), `.time-button` ở `components.css`: placeholder trước khi chunk tải đã đúng khung (không CLS); không tính vào ngân sách JS. Vòng focus của `.time-button` và `.record-trace-toggle` vẽ **bên trong** (danh sách inset ở `styles.css`, như `.row-toggle`): giờ trên thẻ điện thoại nằm sát tên khách, vòng ngoài đè lên tên (gate `layout.spec.ts` "focus rings never cover neighbouring text"). |
| K19 | Hàm chip D9 đặt ở đâu | `features/record-history/chip.ts` (`visitChip`), ngoài chunk lazy cạnh `query.ts`: danh sách tự vẽ chip từ trường đã có, không cần chunk. "Đã thay bằng lượt mới. {lý do}" thay cả "Đã sửa, thay bằng lượt mới." (Quầy) lẫn "Đã sửa thành lượt mới ·" (Lịch sử khách). |
| K20 | Ngân sách: 124,72 KiB trên `main` (không phải 124,70) | Sau 1b **124,88 / 125 KiB** (+0,16: `onSuccess`, guard `[data-row-inert]`, prefetch ở `FrontDeskPage`, 3 tên chunk mới trong bảng deps của Vite). Còn 0,12 KiB: PR web kế tiếp đụng module khởi động phải tìm chỗ bớt trước. |

**Quyết định khi build (đợt 2, 2026-11-12)** — cùng nguyên tắc: không hỏi EM, chọn phương án khớp D1–D12, design §2.7–2.9 và architecture §4, ghi lý do.

| # | Chỗ thiết kế chưa rõ / lệch code | Quyết định |
|---|---|---|
| K21 | Sửa thưởng / phạt (`PUT /adjustments/{id}` = hủy bản cũ + ghi bản mới cùng transaction) đọc thành mấy dòng? D6 nói "các loại khác mỗi dòng audit một dòng", nhưng design §2.9 vẽ "Lan sửa: Số tiền 200.000 ₫ → 300.000 ₫" và §4.4 đếm "thay bản" là **một** lần sửa | **Một dòng** "sửa khoản thưởng / phạt" (`staff.adjustment` của bản mới, `folded = [staff.adjustment_void]`) với thay đổi cũ → mới so với dòng `staff.adjustment` của bản bị thay: người (theo tên, có thể đổi người, S13), loại, số tiền, điểm, ngày, tháng điều chỉnh, lý do. Thành viên vẫn theo id dòng (cả hai dòng đều thuộc chuỗi), nên D6 không bị nới: đây là gộp **bên trong** tập thành viên, như ADR-0021 D9. Dòng sửa không lặp `amount` (đã nằm trong thay đổi). Xóa (hủy không bản mới) là dòng "xóa khoản thưởng / phạt", không tính sửa. Tiền của dòng thưởng / phạt theo quyền của sổ (`evaluation.record`), không theo `sale.read` (D7: quyền đọc của màn gốc). |
| K22 | `GET /record-history/staff-day/{staff_id}/{date}?book=`: quyền của route phụ thuộc `book`, mà `book` là query (F17: quyền trước, query sau) | `record.history.read` + quyền của **một trong hai** sổ (`attendance.record` hoặc `evaluation.record`) trước khi đọc query → người không có luôn `403`, kể cả gửi rác (không oracle); rồi `book` (thiếu / lạ → `400 query.invalid`), rồi quyền của đúng sổ đó (`403`), rồi path (`400`), rồi nhân viên không thuộc spa / ô của chính mình / tháng không làm (`404`). |
| K23 | Ô chưa ai ghi bao giờ: spec edge case 9 nói ô không có dòng tạo → "Chưa rõ ai ghi" | Web: ô sổ nhân sự **không có dòng nào** → "Chưa có ai ghi" (không có gì để "chưa rõ"); "Chưa rõ ai ghi" giữ cho bản ghi có thật mà thiếu dòng tạo. Mọi đường ghi ô (API, seed demo) đều qua use case có audit. |
| K24 | Chuỗi thưởng / phạt có người ngoài sổ của người gọi (vd. khoản của chính quản lý được chủ spa chuyển sang KTV): `403` hay `404`? | `404`, như ô ngày ngoài sổ: web hiện "Không xem được lịch sử của mục này."; `403` sẽ ẩn khối và tải lại `/me` dù quyền vẫn còn. |
| K25 | Chỉ sửa chữ ghi chú chấm công / khen chê: writer ghi `has_note` (có / không) nên dòng đọc ra không có thay đổi nào | Dòng mà mọi trường giữ lại đều bằng nhau (kể cả `late_minutes`, không hiện) và `has_note` có ở cả hai phía → thay đổi `has_note` không giá trị: "Ghi chú (đã đổi)". Chính xác, vì writer chỉ ghi khi có khác và giữ mọi trường trừ chữ ghi chú. Áp cả ở màn nhật ký (cùng `entry()`). |
| K26 | `appointment.update` chỉ hiện "Giờ hẹn"; KTV so theo **tên** (hai người trùng tên thì không thấy đổi, lệch `edited` so theo id) | Thêm "Dịch vụ (đã đổi)" (`service_ids`), "Người đi cùng: n", "Ghi chú lịch hẹn (đã đổi)" (`note_changed`, không bao giờ chữ); KTV đổi khi **id** khác (vẫn hiện tên). "Tính là sửa" giữ đúng architecture §4.4: giờ hoặc KTV. |
| K27 | Meta của lịch hẹn / ô ngày / thưởng phạt: wireframe §2.9 ghi "Ghi bởi Hà · 03/11" | Theo hợp đồng §1.2 như K17: giờ ghi, thêm ngày khi ghi vào ngày khác ngày của bản ghi (ngày hẹn, ngày của ô, ngày phát sinh khoản). |
| K28 | `edited` ở `AppointmentDto` ngoài `GET /appointments?marks=true` | Có ở `GET /appointments/{id}` và reply của **mọi** thao tác lịch hẹn (đặt, đặt nhóm, sửa, đến, bỏ đến, đổi cả nhóm, hoàn tất, hủy, không đến) khi người gọi có `record.history.read`, để `appointments/cache.ts` ghi đè cache không làm mất chip; lưới ngày / tuần không gửi `marks` nên không dò. Replay idempotent mang `edited` theo quyền lúc ghi (như lượt, threat model). |
| K29 | `state` của lịch hẹn / thưởng phạt | Lịch hẹn hủy → `cancelled` ("· đã hủy"); không đến giữ `active` (sheet đã nói trạng thái). Thưởng / phạt: id hỏi đã bị sửa → `replaced`, đã xóa → `voided`, còn lại `active`. |
| K30 | Quản lý được bật xem ô của quản lý khác / chủ spa (`200`) ở đâu trên UI? `PersonDaySheet` chỉ mở cho ô người gọi ghi được (tháng mở, `can_record`) | ~~Không thêm sheet chỉ đọc (design không có): ở đợt 2 các ô đó chỉ xem được qua API; người ghi được (chủ spa với mọi người, quản lý với KTV) thấy lịch sử trong sheet.~~ **Thay bởi K49** (2026-11-12): sheet chỉ xem cho tháng đã chốt và ô "Chủ spa chấm". |
| K31 | Sổ thưởng, phạt đã có nút "Sửa" / "Xóa" trên hàng; ⋯ chỉ có một mục; focus khi `403` trong sheet | Giữ "Sửa" / "Xóa" như cũ, thêm cột ⋯ (chỉ khi `canTrace`) với "Xem ai ghi, ai sửa" ↔ "Ẩn ai ghi, ai sửa", lịch sử mở sẵn dưới hàng, focus vào nút meta; `403` → focus về tiêu đề "Sổ thưởng, phạt của tháng" (`tabIndex=-1`: cột ⋯ biến mất cùng quyền, như K12). `AppointmentSheet`, `PersonDaySheet`: `403` → focus về nút "Đóng" của sheet (không phụ thuộc quyền, K12). |
| K32 | Dòng ma trận cho route hai tham số | `{staff_id}` = nhân viên của fixture, `{date}` = một ngày cố định kèm `?book=attendance` (`authz_matrix.rs` `concrete`). |
| K33 | Bench đợt 1 seed ô ngày với `entity_id` ngẫu nhiên (không đo được một ô) và không có dòng `app.appointment` | Seed 8 người trên id thật (dòng đầu là KTV của bench), mỗi ngày một lần lưu sổ, 1/10 ô ghi lại; 25 lịch hẹn / ngày thật trong `app.appointment` (cả 14 ngày tới), 1/10 dời giờ (`appointment.update`). `marks=true` so **trung vị** với / không: màn 14 ngày tự nó ~450 ms trên debug nên p95 dao động ±25 ms giữa hai lần chạy (đo: +13 rồi −25 ms), che mất phần dò ~1 ms (EXPLAIN in trong bench). |
| K34 | Ngân sách JS khởi động còn 0,12 KiB | Mã đợt 2 nằm trong chunk lazy (`AppointmentsCalendar`, `StaffPage`, chunk `query` / `lazy` / `chunk` của lịch sử); khởi động **124,91 / 125 KiB** (+0,03: chỉ bảng `__vite__mapDeps` có thêm cạnh từ hai chunk màn tới chunk `query` / `lazy`). Không đụng module khởi động nào. Còn 0,09 KiB. |
| K35 | `GET /me/appointments` dùng chung `CalendarQuery` nên OpenAPI sẽ ghi cả `marks` cho lịch của KTV | Tách `MyCalendarQuery { from, to }` (vẫn `deny_unknown_fields`: `?marks=` / `?staff_id=` → `400` như trước); `marks` chỉ ở `GET /appointments`. |
| K36 | Xem thật trên app (agent-browser, chủ spa): dòng chấm công đầu tiên của mọi ô đọc "Ghi chú: Không" (cờ `has_note = false` của bản mới) | Ô sổ ghi lần đầu **không** ghi chú thì không nói gì về ghi chú; có ghi chú vẫn "Ghi chú: Có" (`diff`, chỉ trường `has_note` khi `before` vắng). Áp cả màn nhật ký. |

**Quyết định khi build (đợt 3, 2026-11-12)** — cùng nguyên tắc: không hỏi EM, chọn phương án khớp D1–D12, design §1.5 / §2.10 và architecture §4, ghi lý do.

| # | Chỗ thiết kế chưa rõ / lệch code | Quyết định |
|---|---|---|
| K37 | Meta danh mục / cài đặt: design §1.5 ghi "Sửa lần cuối bởi {Tên} · {dd/mm/yyyy}", không nói (a) bản ghi chỉ mới thêm, chưa ai sửa, (b) có kèm "đã sửa N lần" không | Người + ngày của **lần lưu mới nhất** (`summary.last`, không có thì dòng tạo): "Sửa lần cuối bởi Lan · 12/11/2026 · đã sửa 2 lần". Kèm "· đã sửa N lần" theo quy tắc chung §1.5 (mọi loại trừ gói, và gói bị loại có lý do D9; sửa danh mục **là** sửa, §4.1). Dịch vụ / mẫu gói / nhóm chỉ có dòng thêm: "Ghi bởi Hà · 03/02/2026" ("Sửa lần cuối" sẽ gọi sai việc thêm là sửa). Cài đặt không có thao tác tạo, lần lưu nào cũng là "sửa" (kể cả lần đầu, so với mặc định) nên luôn "Sửa lần cuối bởi …". Ngày đủ năm (bản ghi không có "ngày của nó"). |
| K38 | Cài đặt chưa ai lưu (vẫn là mặc định) và danh mục có sẵn từ lúc tạo spa: edge case 9 chỉ nói "dịch vụ, mẫu gói … không có dòng tạo → Chưa rõ ai ghi" | Danh mục seed (`seed_menu` ghi thẳng, không audit) → "Chưa rõ ai ghi" + "Chưa có lịch sử thay đổi." đúng edge case 9; danh mục không có `created_by` nên không có dự phòng (D12). Quy tắc lương / số khách tối đa chưa ai lưu → **"Chưa có ai sửa"**: như K23, không có gì để "chưa rõ" (giá trị mặc định không phải ai ghi). `updated_by` của hai bảng cài đặt là người lưu **cuối**, không dùng làm "người tạo" (D12). |
| K39 | Plan / spec §4.1 nói "form sửa nhóm", nhưng web **không có** form sửa nhóm dịch vụ (chỉ "Thêm nhóm dịch vụ", form tạo; EM giữ bố cục bảng giá 2026-11-06); API `PUT /service-categories/{id}` có | ~~Lịch sử chỉ ở form **sửa** (không ở form tạo) → nhóm dịch vụ **chưa có chỗ hiện trên web**; không tự thêm form sửa nhóm (đổi sản phẩm / bố cục EM đã duyệt).~~ Route `GET /record-history/service-category/{id}` vẫn build + test (đổi tên / thứ tự qua API vẫn có dấu vết, và form sửa nhóm sau này chỉ cần đặt `RecordTrace {type: "service_category"}` làm phần tử cuối). **Thay bởi K48** (2026-11-12, EM "làm tiếp cho xong hết"): form "Sửa nhóm" (chỉ tên) với `RecordTrace` là phần tử cuối trên hàng nút. |
| K40 | `catalog.category_update` chỉ ghi `after` (tên + thứ tự), không `before`: dòng sửa nhóm không nói được đổi gì | Khi đọc lịch sử **của một nhóm**, giá trị cũ của mỗi dòng sửa lấy từ `after` của dòng trước đó của chính nhóm (`activity::chain_category_updates`, chỉ lúc đọc, không đổi cách ghi audit — §11); không có dòng trước (nhóm seed) → dòng không có thay đổi. Màn nhật ký (theo trang, không có dòng trước) cũng không hiện thay đổi cho dòng thiếu `before` (hiện "Tên: X" khi tên không đổi sẽ sai). Trường `name` "Tên", `sort_order` "Thứ tự". |
| K41 | Đổi nhóm của dịch vụ đọc "Nhóm dịch vụ (đã đổi)" (id bị giấu) | Tên nhóm cũ → mới, resolve ở `rows_by_ids` như KTV của lịch hẹn (tên hiện tại của nhóm). Áp cả màn nhật ký. |
| K42 | Nhãn trường lệch chữ của form: `recommended` "Gợi ý cho khách" (form: "★ Khuyên dùng"), `active` "Đang dùng" (form: "Đang bán", chip "Ngừng bán"); `loyalty_partial_month` / `meal_allowance_basis` hiện mã thô ("prorate") | "Khuyên dùng", "Đang bán", "Loại nghỉ tính vào ngày không trừ lương"; nhãn mã theo lựa chọn của `RulesSheet` rút gọn ("Đủ tháng nếu làm từ 15 ngày", "Theo số ngày làm", "Đủ cả tháng", "Không tính"; "Cố định mỗi tháng", "Theo số ngày đi làm"). Áp cả màn nhật ký (cùng `FIELD` / `CODE`). Danh sách (`free_leave_codes`, `tet_level_amounts`, `items` của mẫu gói) vẫn "(đã đổi)" như mọi mảng (`diff` không gửi giá trị mảng). |
| K43 | Hai route cài đặt không có tham số: thứ tự quyền, `record`, dòng ma trận; có cần index không | `401` → cờ (`payroll` / `appointments`, guard route) → `record.history.read` → `payroll.read` / `appointment.settings` (`require_settings`) → đọc; không có `400` / `404`. `record = {type}` không `id` (như ô ngày). Thành viên: `entity_type='tenant' AND entity_id = spa của session AND action = …` (đúng cách hai use case ghi). Dòng `ROUTES` không `Target`. **Không migration**: EXPLAIN trên bench audit thật (183.508 dòng + 3 năm lưu cài đặt / sửa dịch vụ) đi `audit_log_entity_idx` (0001), 0,11 ms; p95 route 10–19 ms ([architecture §6](../../architecture/record-history.md)). |
| K44 | Giờ của từng dòng: §1.2 so với "ngày của bản ghi", danh mục / cài đặt không có ngày đó | Lấy **hôm nay** làm mốc: dòng hôm nay chỉ giờ, ngày khác "10:30 12/11", năm khác thêm năm (`recordDayOf`). |
| K45 | Focus khi `403` (công tắc tắt giữa phiên) | Form sửa dịch vụ / mẫu gói: về tiêu đề form ("Sửa: …", `tabIndex=-1` chỉ khi có lịch sử, như K12); sheet Quy tắc lương / Số khách tối đa: nút "Đóng" của sheet (K31). |
| K46 | Tải trước chunk lịch sử | `/bang-gia`, `/bang-gia/goi` tải trước khi `canTrace(me, "catalog")` (như K14); hai sheet cài đặt không thêm (chủ spa ở `/nhan-vien` và lịch hẹn đã tải trước cho thưởng / phạt và lịch hẹn). |
| K47 | Ngân sách JS khởi động còn 0,09 KiB | Mã đợt 3 nằm trong chunk màn (bảng giá, mẫu gói, nhân viên, lịch hẹn) và chunk lịch sử; `query.ts` bỏ bảng `ROUTE` (route = loại thay `_` bằng `-`) để bù phần thêm. Khởi động **124,89 / 125 KiB** (main 124,91: −0,02; index 124,48 + runtime 0,42). Không nâng ngân sách. |

**Quyết định khi làm việc còn lại sau 3 đợt (2026-11-12)** — EM: "làm tiếp cho xong hết", giao mọi quyết định thiết kế cho lead + agent, không hỏi lại. Bốn việc còn mở ở log 2026-11-12: K39 (form sửa nhóm dịch vụ), K30 (sheet ô ngày chỉ xem), mở bản ghi gọi lịch sử 2 lần, chữ "bỏ 6". Chỉ web; thứ tự dòng lịch sử và `/appointments` do agent khác làm ở API (K60+).

| # | Chỗ thiết kế chưa rõ / lệch code | Quyết định |
|---|---|---|
| K48 | K39: form sửa nhóm dịch vụ gồm những gì; API `PUT /service-categories/{id}` nhận cả tên lẫn `sort_order` | **Chỉ đổi tên.** Nút "Sửa nhóm" (`btn-secondary`, cạnh "Thêm dịch vụ" trên tiêu đề nhóm, `aria-label` "Sửa nhóm {tên}", chỉ khi `catalog.manage`) mở form ngay dưới tiêu đề nhóm, cùng khuôn `ServiceForm` (`row-form`, tiêu đề "Sửa nhóm: {tên}", một ô "Tên nhóm dịch vụ", "Lưu nhóm dịch vụ" / "Hủy", Esc đóng); gửi lại `sort_order` đúng như đã đọc, gợi ý "Chỉ đổi tên; nhóm giữ nguyên vị trí trong bảng giá." **Không làm sắp thứ tự:** app không có mẫu kéo / lên-xuống nào; dời một nhóm phải đổi `sort_order` của các nhóm khác (nhiều `PUT` không nguyên tử, mỗi cái là một dòng "sửa nhóm dịch vụ · Thứ tự" trên nhóm không ai định sửa) hoặc thêm endpoint sắp hàng loạt (việc API, ngoài phạm vi). Đổi thứ tự vẫn qua API, lịch sử vẫn đọc "Thứ tự: 1 → 3" (K40). |
| K49 | K30: sheet ô ngày chỉ xem mở cho ô nào; server (`reporting::history` staff-day) trả `404` cho ô của chính mình và tháng người đó không làm | Một ô người gọi **không ghi được** mở chỉ xem khi: (a) **tháng đã chốt**, cho mọi người đọc được sổ (như "Xem" ở phiếu đánh giá, EM 2026-11-08; ô lưới của tháng đã chốt trước đây là nút bấm không làm gì); (b) tháng mở, người "Chủ spa chấm" (quản lý khác / chủ spa với quản lý), **chỉ khi** người gọi xem được lịch sử của ít nhất một sổ mình giữ (`canTrace` "attendance" / "daily_review"): không có lịch sử thì thẻ đã có đủ giá trị, sheet chỉ lặp lại. Không bao giờ: ngày chưa tới, ngày ngoài đợt làm việc (lưới "·", Sổ hằng ngày chỉ có người làm ngày đó). Ô của chính mình: sổ không bao giờ liệt kê (`hr::scope` `lists`, cùng điều kiện với route lịch sử + `employed_in`), và `/me` không có id hồ sơ, nên web dựa vào danh sách server trả: không thẻ thì không có gì để mở, không request. Chữ: nút "Xem giờ & ghi chú" (động từ "Xem" của phiếu đánh giá + tên nút "Giờ & ghi chú" của thẻ) ở Sổ hằng ngày; ô lưới Bảng công tháng bấm được; sheet tiêu đề tên người, mô tả "{ngày} · vào ca {giờ} · chỉ xem", phần "Chấm công" / "Khen, phàn nàn" như `MyDaySheet` (`my-day-block`, `detail-grid`: trạng thái, muộn, giờ, ghi chú; số khen / phàn nàn, ghi chú), footer "Đóng"; `RecordTrace` là phần tử cuối của từng phần; `403` → focus nút đóng (K31). |
| K50 | Chữ "Số khách tối đa mỗi khung giờ: bỏ 6": trường nào có giá trị rỗng mang nghĩa | Bảng nhỏ `NULL_MEANS` trong `sentence.ts`, theo chữ "Để trống = …" của chính form: `max_per_slot` "không giới hạn", `duration_min` "theo tư vấn", `validity_months` "không thời hạn", `shift_start` "theo giờ chung của spa" (hồ sơ nhân viên, chỉ màn nhật ký), `staff` "chưa gán KTV" (lịch hẹn; lượt luôn có KTV). Hai chiều: "6 → không giới hạn", "không giới hạn → 6"; cả hai phía rỗng vẫn "(đã đổi)". **Giữ "bỏ …"** cho mọi trường khác (giờ vào / ra, SĐT, số tiền, điểm, lý do…). Không đưa giá Nữ / Nam vào: "không áp dụng" chỉ có nghĩa trong chế độ "Theo giới tính", và chuyển một giá → hai giá xóa `price` trong khi điền hai giá kia. `expires_on` không cần: gia hạn từ chối gói không hạn (`package_has_no_expiry`). Một hàm `changeText` cho cả lịch sử tại chỗ lẫn màn Nhật ký thao tác. |
| K51 | Mở một bản ghi gọi lịch sử 2 lần: `usePrefetchQuery` ở `LazyRecordTrace` + `useQuery` khi chunk tải xong, `staleTime: 0` | Mỗi lần mở **đúng một request**: `LazyRecordTrace` ghi `openedAt`; `RecordTrace` dùng `refetchOnMount: (q) => q.state.isInvalidated \|\| q.state.dataUpdatedAt < openedAt` (mặc định `openedAt` = lúc chính nó mount, cho `LedgerSaleSheet`). Trả lời của lần prefetch là của lần mở này → không hỏi lại; trả lời của lần mở trước (bản ghi mở lại sau đó, có thể trình duyệt khác đã ghi) → hỏi lại đúng một lần, hiện bản cũ trong lúc chờ; một lần ghi trong trình duyệt này (`MutationCache.onSuccess` invalidate `["record-history"]`) → hỏi lại kể cả mở lại ngay. Giữ `staleTime: 0`. Bỏ phương án `staleTime` vài giây: mở lại nhanh sẽ không hỏi lại, và chunk tải chậm hơn mốc đó vẫn gọi 2 lần. |
| K52 | Lưới Bảng công tháng ghi "Bấm một ô để sửa." kể cả khi ô chỉ xem hoặc không ô nào bấm được | Chữ theo ô bấm được: tất cả ghi được "Bấm một ô để sửa.", tất cả chỉ xem "Bấm một ô để xem.", lẫn cả hai "Bấm một ô để sửa hoặc xem.", không ô nào bấm được: không có dòng gợi ý. Hàng "Chủ spa chấm" thành nút khi được xem chỉ xem (K49), nhãn dưới tên giữ nguyên. |
| K53 | Chi tiết form sửa nhóm: focus, lưu không đổi gì, lỗi, cache, ghi đè | Focus vào ô tên khi mở (như "Thêm nhóm dịch vụ"); lưu tên không đổi (sau trim) thì đóng, **không gửi request** (một lần lưu là một dòng "sửa nhóm dịch vụ" tính "đã sửa" mà không đổi gì). Lỗi trường `name` dưới ô (`taken` "Tên này đã được dùng.", trống "Nhập tên nhóm." trước khi gửi), lỗi khác là `alert` như form dịch vụ. Thành công: invalidate `["catalog"]` (danh sách nhóm, dịch vụ, mẫu gói: mọi chỗ chọn dịch vụ đọc tên mới). Nhóm không có `version` (API không nhận `If-Match`): ghi sau thắng; chấp nhận vì chỉ chủ spa đổi tên và không có màn nào đổi thứ tự. |
| K54 | Ngân sách JS khởi động còn 0,11 KiB | **124,90 / 125 KiB** (main 124,89). Mã của chunk khởi động **giống từng byte** (index 403.500 B ở cả hai); +7 B gzip là do tên chunk (hash cùng độ dài) trong bảng preload của Vite đổi khi chunk màn đổi. Mã mới: chunk `PriceListPage` (`CategoryForm`), `StaffPage` (`PersonDayView`), chunk lịch sử (`openedAt`), chunk `sentence` (`NULL_MEANS`; `sentence.ts` không ở khởi động). Không nâng ngân sách. |
| K55 | E2E cho hai màn mới trên spa dùng chung | Form sửa nhóm: nhóm riêng có tem (`Nhóm thử …`, tạo + đổi tên qua API trong `beforeAll`, rồi đổi tên qua form), ở cuối bảng giá, không có dịch vụ; ở lại spa sau khi chạy (không có endpoint xóa nhóm), như dịch vụ / mẫu gói thử của đợt 3. Sheet chỉ xem **không** có e2e: cần một tháng đã chốt lương (không mở lại được, ADR-0008) hoặc operator bật lịch sử cho quản lý; Vitest phủ mọi nhánh (`RecordTrace.wave2.test.tsx`). |
