---
title: "Spec: Quản lý nhân viên (chấm công, đánh giá, thưởng/phạt, bảng lương)"
status: draft
updated: 2026-11-10
sources: [raw/2026-11-08-em-request-accounts-and-passwords.md, raw/2026-11-08-em-fix-all-and-manager-self-view.md, raw/2026-11-08-live-test-manager-technician.md, raw/2026-11-08-live-test-owner.md, raw/2026-11-07-em-decisions-hr-notes-group-region.md, raw/2026-11-07-em-flow-review-hr-latency.md, raw/2026-11-07-em-technician-pages.md, raw/2026-11-07-em-decisions-after-live-test.md, raw/2026-11-06-em-review-round5.md, raw/2026-11-05-em-review-round4.md, raw/2026-11-04-idea-internal-platform.md]
related: [../compensation-rules.md, ../personas.md, ../../architecture/backend.md, ../../architecture/data.md, ../../security/authz-matrix.md]
---

# Spec: Quản lý nhân viên

**Status: draft.** Backend, UI và dữ liệu demo đã build theo các mặc định dưới đây (PR `feat/staff-management`). Các câu hỏi cho EM nằm ở §9. Mỗi chỗ mâu thuẫn trong [compensation-rules](../compensation-rules.md) có một mặc định, và mặc định đó chỉnh được theo từng spa (§6).

## 1. Goal
EM (vòng 4, mục 2): "làm feature quản lý nhân viên, tính lương tính thưởng, phạt rồi đánh giá từng ngày như trong cái sheet". Spec này thay file Excel *Bảng chấm công đánh giá tháng* (mỗi người, mỗi tháng một sheet) bằng các màn sau:
- **Chấm công hằng ngày**: lưới tháng × nhân viên, mã ✓/P/ĐX/K như sheet (UI nay hiện chữ + icon + màu, không còn mã: F20 ở §16), tự tính phút đi muộn.
- **Đánh giá hằng ngày**: khen, phàn nàn, ghi chú (đúng các cột của sheet). **Đánh giá tháng** theo 6 tiêu chí, thang 100 điểm.
- **Sổ thưởng/phạt**: lý do, số tiền và/hoặc điểm, ngày, người ghi.
- **Bảng lương tháng**: lương cứng, trừ ngày nghỉ, hoa hồng tính từ lượt khách đã ghi, thưởng, phạt, thực lĩnh. Có phiếu lương từng người, và chốt tháng thì khóa sổ.

Thước đo: quản lý không phải cộng tay lương và hoa hồng ([personas](../personas.md) #1). KTV tự kiểm tra được từng khoản trên phiếu lương của mình (persona #3).

## 2. Personas & quyền
| Permission (mới, migration 0017; cột role = mặc định của role hệ thống, platform admin chỉnh được từ 0040, [platform-admin](platform-admin.md)) | Chủ spa | Quản lý | KTV | Dùng cho |
|---|---|---|---|---|
| `staff.manage` | ✓ | — | — | Hồ sơ nhân viên: thêm, sửa, nghỉ việc, nhận lại, liên kết tài khoản (§13) |
| `staff.read` (0025) | ✓ | ✓ | — | Tab "Hồ sơ nhân viên" (trước là "Nhân sự", §13): tên, vị trí, các đợt làm việc, thâm niên (không SĐT, ghi chú, lý do nghỉ) |
| `attendance.record` | ✓ | ✓ | — | Xem và ghi lưới chấm công |
| `evaluation.record` | ✓ | ✓ | — | Đánh giá ngày, đánh giá tháng, sổ thưởng/phạt |
| `payroll.read` | ✓ | — (**Q-S1**) | — | Mọi con số lương: điều kiện lương, bảng lương, phiếu lương, quy tắc |
| `payroll.close` | ✓ | — | — | Chốt tháng, sửa quy tắc lương |
| `staff.read_own` | ✓ | ✓ | ✓ | `GET /me/staff`, `/me/staff/day`, `/me/evaluations/history`, `/me/appointments`: **của chính mình** |
| `staff.read_own_past` (0026) | — | — | — | Không gán cho role nào: server suy ra mỗi request cho tài khoản gắn hồ sơ đã nghỉ (§14, ADR-0014) |

- Chưa đăng nhập → `401`. Thiếu quyền → `403 auth.forbidden`. ID của spa khác → `404` (đường dẫn) hoặc `422 …staff_id unknown` (body).
- **Lương không bao giờ được gửi cho người thiếu `payroll.read`.** Response không chứa key `pay`, không chứa `null` rồi giấu ở UI (non-negotiable 4). Ghi điều kiện lương cần cả `staff.manage` và `payroll.read`.
- KTV chỉ thấy tháng của mình. Server lấy staff id từ user của session qua `staff.user_id`, không bao giờ lấy từ request (non-negotiable 3).

## 3. User stories & acceptance criteria
Áp dụng chung cho mọi route: đúng như G1–G3 của [spec R1-1](front-desk-and-packages.md). Mọi response có `Cache-Control: no-store`. Mọi thao tác ghi đều để lại `audit_log` có `actor_user_id`. Tham số `?month=YYYY-MM` mặc định là tháng hiện tại theo giờ spa; tháng sai định dạng → `422`.

### US1: Hồ sơ nhân viên (chủ spa)
*Là chủ spa, tôi muốn ghi vị trí, ngày vào làm, lương cứng, tỷ lệ hoa hồng và trạng thái của từng người, để bảng lương tự tính.*
- AC1.1 *(§13 thay phần trạng thái, ngày nghỉ, tài khoản: route riêng)* `POST /staff`, `PUT /staff/{id}` (`If-Match`) với `full_name`, `position`, `phone?`, `status` (`active`/`left`), `joined_on?`, `left_on?` (bắt buộc khi `left`, không được trước `joined_on`), `user_id?`, `shift_start?` (`HH:MM`). Sai → `422` theo từng field. Phiên bản cũ → `412`.
- AC1.2 `user_id` phải là thành viên đang hoạt động của spa, chưa liên kết với ai khác. Đã liên kết → `409 staff.account_taken`. `GET /staff/accounts` liệt kê các tài khoản có thể liên kết.
- AC1.3 `PUT /staff/{id}/pay`: `base_salary` 0–1 tỷ, `commission_rate_bp` 0–10000 (500 = 5 %: Lựa chọn 1; 700: LC2; 1000: LC3), `loyalty_bonus_monthly`, `meal_allowance`. Lần sửa áp dụng cho mọi tháng **chưa chốt**; tháng đã chốt giữ nguyên số đã dùng (test `a_closed_month_is_locked…`). **ADR-0019 D10:** bắt buộc `If-Match` = `pay.version` đã đọc (`"0"` khi người đó chưa có điều kiện lương); khác phiên bản hiện tại → `412 conflict.stale_version`, không ghi gì; thiếu header → `400 precondition.if_match_required`. Response có `version` và `ETag`. `PUT /payroll/rules` cũng vậy (`version` của `GET /payroll/rules`, `"0"` khi chưa lưu lần nào). Web gửi version đã đọc khi mở form; gặp 412 thì báo mở lại form để xem bản mới nhất.
- AC1.4 Có `staff.manage` nhưng thiếu `payroll.read` (role tùy biến) → hồ sơ trả về **không có** key `pay`, `pay_visible: false`; `PUT …/pay` → `403` (test `salary_is_only_sent_to_payroll_readers`, đã mutation-check).
- AC1.5 Không xóa nhân viên. Nghỉ việc = đóng đợt làm việc (§13, ADR-0013). Người đã nghỉ vẫn có trong các tháng họ còn làm.

### US2: Chấm công hằng ngày (quản lý)
*Là quản lý, tôi muốn chấm cả spa trong vài giây mỗi ngày, để cuối tháng không phải đếm lại.*
- AC2.1 `GET /attendance?month=` trả về mọi người **có làm** trong tháng (theo `joined_on`/`left_on`). Với mỗi người: các ngày đã ghi, giờ vào ca, và tổng tháng (đi làm, số lần muộn, số phút muộn, P, ĐX, K, số ngày chưa chấm).
- AC2.2 `PUT /attendance {entries: [1..100]}` ghi hoặc xóa (`status: null`) từng ô trong **một transaction**. Một lần sửa ô hay "cả spa đi làm hôm nay" đều dùng route này.
  - `status`: `present` (✓), `leave_p` (P), `leave_dx` (ĐX), `absent_k` (K).
  - `check_in` và `check_out` chỉ có khi `present`.
  - Phút đi muộn = `check_in − giờ vào ca` (giờ vào ca của người đó, nếu không có thì `default_shift_start` của spa, 09:00), do server tính.
- AC2.3 Lỗi:
  - ngày trong tương lai → `422 in_future`;
  - ngày ngoài thời gian làm việc của người đó → `422 not_employed`;
  - trùng (người, ngày) trong cùng batch → `422 duplicate`;
  - nhân viên của spa khác → `422 entries[i].staff_id unknown`;
  - tháng đã chốt → `409 payroll.period_closed`.
- AC2.4 Ghi lại một ô là ghi đè (upsert), không cần `If-Match`: hai người cùng chấm thì người sau thắng, và mọi thay đổi đều có trong audit (`before`/`after`).

### US3a: Đánh giá hằng ngày (quản lý)
- AC3a.1 `PUT /daily-reviews {entries}` ghi theo từng (người, ngày): `compliments`, `complaints` (0–99), `note` (≤ 500). Cả hai bằng 0 và không có ghi chú thì xóa ngày đó. Lỗi giống AC2.3.
- AC3a.2 `GET /daily-reviews?month=` trả về theo từng người, kèm tổng khen và tổng phàn nàn của tháng.

### US3b: Đánh giá tháng (quản lý, ngày 1–5 tháng sau)
- AC3b.1 `PUT /evaluations {staff_id, month, criteria[6], serious_issue, coaching_plan?, comment?}`. Có 6 tiêu chí: `technique` 40, `attitude` 15, `customer_feedback` 15, `diligence` 10, `hygiene` 10, `learning` 10.
  - **Quyết định D (EM 2026-11-07, thay cách chọn mức):** mỗi tiêu chí là `{key, points}`, người chấm **gõ điểm** 0 → tối đa. Thiếu điểm → `422 criteria.<key>.points required`; vượt tối đa → `422 criteria.<key>.points above_max`; thiếu tiêu chí → `criteria count`.
  - Mức của từng tiêu chí do server tính từ tỷ lệ điểm/tối đa, cùng ngưỡng với xếp loại tháng (≥ 85 % Mức 1, 70–84 % Mức 2, 50–69 % Mức 3, < 50 % Chưa đạt). Các mốc của sheet rơi đúng mức của nó (100 % → Mức 1, 80 % → Mức 2, 60 % → Mức 3, ≤ 30 % → Chưa đạt). Bản ghi cũ (chọn mức) đọc như cũ.
  - Form mới **để trống**, không điền sẵn Mức 1 / 100 điểm; chỉ lưu khi đủ 6 tiêu chí hợp lệ. Form có ghi chú giải thích các mức và mốc điểm (ví dụ Kỹ thuật: Mức 1: 40 · Mức 2: 32 · Mức 3: 24 · Chưa đạt: ≤ 12) và xếp loại theo tổng.
- AC3b.2 Kết quả: ≥ 85 → Mức 1; 70–84 → Mức 2; 50–69 → Mức 3; < 50 → Chưa đạt. `serious_issue` giới hạn kết quả **tối đa Mức 3**, không bao giờ làm kết quả tốt lên. Kết quả Mức 3 hoặc Chưa đạt mà không có `coaching_plan` → `422 coaching_plan required`.
- AC3b.3 Tháng sau tháng hiện tại → `422`. Tháng đã chốt → `409`. Ghi lại thì chấm lại (upsert, `version` tăng, có audit).
- AC3b.4 `GET /evaluations?month=` trả về tiêu chí, phiếu của từng người (hoặc `null`), cùng tổng khen/phàn nàn và tổng chấm công của tháng làm ngữ cảnh.

### US4: Thưởng / phạt
- AC4.1 `POST /adjustments` (`Idempotency-Key`): `staff_id`, `kind` (`bonus`/`penalty`), `amount?` (1–100tr), `points?` (1–100), cần ít nhất một trong hai. `reason` bắt buộc. `occurred_on` mặc định hôm nay, không được ở tương lai. Người ghi lấy từ session.
- AC4.2 ~~Chỉ thêm, không sửa hay xóa~~ **(EM 2026-11-07, §11):** một khoản ghi nhầm được **xóa** (`DELETE /adjustments/{id}` → `204`, có hộp xác nhận) hoặc **sửa** (`PUT /adjustments/{id}` + `Idempotency-Key` → `200` khoản mới, id mới: khoản cũ bị hủy và khoản đúng được ghi trong một transaction). "Xóa" = hủy (`voided_at`, `voided_by`, migration 0024): ra khỏi sổ và bảng lương, dòng vẫn còn cho audit (`staff.adjustment_void`). Đã hủy → `409 adjustment.voided`; tháng đã chốt → `409 payroll.period_closed`; quyền như khi ghi (§11 quyết định C). DB: chỉ được đặt `voided_at` một lần, mọi thay đổi khác và DELETE bị trigger chặn.
- AC4.2b **(ADR-0019 D2)** Tháng của `occurred_on` không có ngày nào thuộc thời gian làm việc của người đó (chưa vào làm, đã nghỉ) → `422 occurred_on not_employed`, không ghi gì. Trước đây khoản này được nhận rồi không vào phiếu lương nào. Điều chỉnh cho người đã nghỉ phải ghi vào một tháng họ còn làm và còn mở.
- AC4.3 Tháng của `occurred_on` đã chốt → `409`. Để sửa tháng đã chốt: ghi vào tháng đang mở, kèm `corrects_month` là tháng đã chốt (phải trước tháng ghi và đã chốt, nếu không → `422`).
- AC4.4 `GET /adjustments?month=`: mới nhất trước, kèm tên người ghi.

### US5: Bảng lương & chốt tháng (chủ spa)
- AC5.1 `GET /payroll?month=`: phiếu lương của mọi người có làm trong tháng, kèm tổng.
  - Tháng chưa chốt thì tính trực tiếp (tạm tính).
  - Tháng đã chốt thì đọc bản chụp đã lưu (`app.payslip`, chỉ thêm).
- AC5.2 Công thức (bộ tính thuần `hr::calc`, unit test theo các ô của sheet):
  - `lương ngày = lương cứng ÷ (số ngày của tháng − số ngày nghỉ miễn trừ)`.
  - `ngày bị trừ = max(0, ngày nghỉ thuộc mã miễn trừ − miễn trừ) + ngày nghỉ thuộc mã không miễn trừ`.
  - `tiền trừ nghỉ = round(ngày bị trừ × lương cứng ÷ (số ngày − miễn trừ))`, giống ô E55 của sheet.
  - `tiền trừ đi muộn = số lần muộn quá ngưỡng × mức phạt` (mặc định 0đ).
  - `hoa hồng làm liệu trình = round(tỷ lệ × Σ visit_line.line_value)` trên các lượt **completed** của tháng mà người đó làm.
  - `hoa hồng chốt liệu trình = round(tỷ lệ × Σ sale.total)` trên các hóa đơn gói người đó chốt, không tính hóa đơn đã hủy.
  - **Vòng 6 (EM 2026-11-06, Q-U3):** khoản này thành **hoa hồng giới thiệu / chốt (upsell)** = `round(tỷ lệ × (Σ giá gói đã thu mà người đó chốt + Σ line_value dịch vụ lẻ người đó giới thiệu trong lượt hoàn tất))`. Phiếu lương ghi `upsell_package_base`, `upsell_service_base`, `upsell_lines[]` (từng gói / dịch vụ); `closing_commission_base` = tổng. "Khách tự chọn" không tính cho ai. Hoa hồng làm không đổi.
  - `thực lĩnh = lương cứng − trừ nghỉ − trừ muộn + hoa hồng làm + hoa hồng chốt + tiền ăn + thưởng − phạt`.
  - **ADR-0019 D4:** "lương cứng" trong công thức là `base_pay` của tháng: tháng làm đủ thì bằng `base_salary`; vào làm hoặc nghỉ giữa tháng (kể cả khoảng nghỉ giữa hai đợt) thì `round_nghìn(base_salary × employed_days ÷ days_in_month)`. Lương ngày vẫn tính trên `base_salary` đủ tháng. `totals.base_salary` = Σ `base_pay`. Bản chụp chốt trước ADR-0019 không có `base_pay`: đọc ra bằng `base_salary`, dòng lưu trong DB không đổi.
  - **ADR-0019 D3:** `trừ nghỉ` tối đa bằng `base_pay`. Thực lĩnh `max(0, …)`; phần âm ghi ở `net_pay_shortfall` kèm cờ `net_pay_shortfall`, không trừ. Tiền ăn theo quy tắc `meal_allowance_basis`: `monthly` (mặc định, compensation-rules §8 ghi khoản cố định tháng) = đủ số khi tháng có ≥ 1 ngày đi làm, 0đ khi không có; `days_worked` = tiền ăn × ngày đi làm ÷ (số ngày − miễn trừ), tối đa đủ số, làm tròn nghìn.
  - **Bảng lương cộng ra thực lĩnh (2026-11-10):** cột của bảng `/nhan-vien?tab=payroll` là Lương cứng, Trừ nghỉ / muộn, Hoa hồng, **Tiền ăn**, Thưởng − phạt, Thực lĩnh; mỗi dòng và dòng tổng: `lương cứng − trừ + hoa hồng + tiền ăn + (thưởng − phạt) + net_pay_shortfall = thực lĩnh`. Khi có `net_pay_shortfall`, ô Thực lĩnh ghi thêm "+X trừ vượt lương, không trừ" (cùng cờ Lưu ý). `totals` có thêm `meal_allowance`, `net_pay_shortfall`. Thẻ trên điện thoại có dòng "Tiền ăn" như các khoản khác.
  - **Ghi nhận, không cộng vào thực lĩnh**: Thưởng gắn bó của tháng (trả ở mốc 12/24) và mức Tết của tháng (mức thấp hơn giữa mức theo ngày nghỉ và mức theo đánh giá; số tiền theo `tet_level_amounts`).
- AC5.3 Cờ trên phiếu lương:
  - `no_pay_terms`: chưa có điều kiện lương, mọi khoản bằng 0;
  - `unrecorded_days`: có ngày làm việc chưa chấm công;
  - `absent_k5_plus`: K ≥ 5 ngày trong tháng (Nhóm A #8). Hệ thống chỉ gắn cờ, chủ spa quyết định;
  - `evaluation_missing`;
  - `net_pay_shortfall` (ADR-0019 D3): các khoản trừ vượt lương của tháng, thực lĩnh 0đ.
- AC5.4 `POST /payroll/close {month, allow_incomplete?}` (`payroll.close`, `Idempotency-Key`):
  - tháng chưa kết thúc → `422 payroll.month_not_ended`;
  - còn ngày chưa chấm mà `allow_incomplete` không bật → `422 payroll.unrecorded_days`;
  - đã chốt → `409 payroll.period_closed`; gửi lại cùng key thì trả lại response cũ;
  - **ADR-0019 D1:** còn người trong bảng lương chưa có điều kiện lương hoặc chưa có đánh giá tháng → `409 payroll.missing_data`, `details.staff = [{staff_id, full_name, missing: ["pay_terms" | "evaluation"]}]`. `allow_incomplete` không bỏ qua được. Miễn: hồ sơ gắn với tài khoản có `payroll.close` (chủ spa). `GET /payroll` trả cùng danh sách ở `missing` (rỗng khi đã chốt); màn chốt lương hiện danh sách và khóa nút "Chốt lương".
  Khi chốt, mọi phiếu lương được lưu thành bản chụp và tháng bị khóa: chấm công, đánh giá ngày, đánh giá tháng, thưởng/phạt của tháng đó đều → `409`. Khóa có ở cả service (row lock `FOR SHARE`/`FOR UPDATE` trên `payroll_period`) và DB (trigger `reject_closed_period`). Tháng đã chốt **không bao giờ mở lại** (trigger `payroll_period_forward_only`).
- AC5.4b **(ADR-0019 D2b) Hoàn tiền gói sau khi người chốt đã nghỉ.** Khoản hoàn được trừ ở tháng hoàn tiền như ADR-0010 nếu người chốt còn làm trong tháng đó. Nếu không, nó thuộc **tháng làm cuối** của họ (tháng của ngày nghỉ việc gần nhất trước ngày hoàn): tháng đó còn mở thì phiếu lương tháng đó có dòng `package_refund` âm; đã chốt thì không thu hồi (spa chịu). `GET /payroll?month=` của tháng hoàn tiền liệt kê mọi khoản như vậy ở `leaver_refunds` (`deducted_in` = tháng đã trừ, `null` = không thu hồi); audit `payroll.close` ghi `leaver_refunds_deducted` (id các khoản trừ vào tháng đang chốt) và `leaver_refunds_not_clawed_back`. Một khoản tính là "đã trừ" khi được ghi trước lúc tháng làm cuối chốt.
- AC5.5 `GET /payroll/payslips/{staff_id}?month=` trả về phiếu lương một người, gồm hoa hồng theo từng dịch vụ. Nhân viên của spa khác → `404`.
- AC5.6 Lượt khách bị hủy (void) **sau khi** đã chốt tháng không làm đổi phiếu đã chốt. Chủ spa ghi một khoản điều chỉnh vào tháng đang mở.

### US6: KTV xem tháng của mình
- AC6.1 `GET /me/staff?month=` → `{linked: false}` nếu tài khoản chưa liên kết. Nếu đã liên kết thì trả về chấm công, đánh giá ngày, đánh giá tháng, thưởng/phạt và phiếu lương **của chính người đó**. Phiếu lương có khi đã có điều kiện lương; tháng chưa chốt thì là tạm tính.
- AC6.2 Không có tham số nào chọn được người khác (`?staff_id=` → `400`). Mọi route quản lý khác → `403` (test `a_technician_sees_only_their_own_month`).

## 4. UX
> **Vòng 5 thay mục này** cho quản lý: xem §10 (sổ hằng ngày một trang, bảng công tháng dạng lưới, đánh giá tháng nhắc khi đến hạn). Phần dưới giữ để đối chiếu. Tên màn nay là **"Quản lý nhân sự"** (mục nav + h1, đổi 2026-11-08, PR #98); tab "Hồ sơ" nay là "Hồ sơ nhân viên" (§13).

Màn "Nhân viên" (`/nhan-vien`, lazy chunk) có các tab, trạng thái tab và tháng nằm trên URL (`useSearchState`):
1. **Chấm công**. Desktop là lưới tháng × nhân viên: bấm một ô để đổi mã; nút "Cả spa đi làm hôm nay" ghi một lần cho cả spa. Phone là **danh sách theo người** cho một ngày, dùng `BottomSheet` để nhập giờ đến/về và ghi chú.
2. **Đánh giá**: khen/phàn nàn theo ngày, và phiếu 6 tiêu chí của tháng (`BottomSheet` trên phone).
3. **Thưởng/phạt**: sổ của tháng, form trong `BottomSheet`.
4. **Bảng lương** (chỉ khi có `payroll.read`): `ResponsiveTable` (desktop là bảng, phone là thẻ), phiếu lương từng người, nút chốt tháng có hộp xác nhận.
5. **Hồ sơ** (chỉ khi có `staff.manage`): danh sách, form hồ sơ, form điều kiện lương (chỉ khi có `payroll.read`).
KTV mở "Nhân viên" thì thấy "Tháng của tôi". Mỗi tab có đủ các trạng thái: đang tải, trống, lỗi, tháng đã chốt (chỉ đọc, kèm banner).

## 5. Data
Migration `0017_staff_management.sql`. Tất cả có RLS theo tenant; FK đều có dạng `(tenant_id, x)`.
- Bảng `staff` thêm `user_id` (unique theo tenant) và `shift_start`.
- `staff_pay` lưu điều kiện lương. Đây là dữ liệu nhạy cảm; audit có ghi số tiền, chỉ chủ spa đọc được audit.
- `payroll_rules` (jsonb) lưu quy tắc của spa.
- `payroll_period` lưu trạng thái tháng: `open` → `closed`, không quay lại.
- `attendance_day`, `staff_daily_review`, `staff_evaluation`: khóa theo (người, ngày/tháng).
- `staff_adjustment`: chỉ thêm.
- `payslip`: bản chụp jsonb, chỉ thêm.
- Ghi chú chấm công và đánh giá là dữ liệu nhân sự, không phải dữ liệu sức khỏe, nên lưu dạng text. Audit chỉ ghi `has_note`, không chép nội dung.
- Lưu trữ: giữ vô thời hạn như sổ sách. Chính sách xóa dữ liệu khi nhân viên nghỉ thuộc spec #4.

## 6. Mâu thuẫn trong compensation-rules → mặc định + quy tắc chỉnh được
Quy tắc nằm ở `PUT /payroll/rules` (chủ spa) và áp dụng cho mọi tháng chưa chốt. Quy tắc dùng khi chốt được ghi vào audit của lần chốt.

| # | Mâu thuẫn | Mặc định đã build | Chỉnh được? | Câu hỏi EM |
|---|---|---|---|---|
| 1 | Thưởng cam kết 24 tháng (KTV có KN): trả 2tr + 2tr hay chỉ trả ở mốc 24 | Chưa tính khoản chi theo mốc. Phiếu lương chỉ **ghi nhận** Thưởng gắn bó hằng tháng. | Chưa (cần thêm dữ liệu hợp đồng: gói 12/24, cọc, ngày ký) | Q-C1 |
| 2 | 2 ngày miễn trừ áp dụng cho K không? | Như Excel: P, ĐX, K đều dùng 2 ngày miễn trừ | ✓ `free_leave_codes`, `free_leave_days` | Q-C2 |
| 3 | Hoa hồng: giá trị một buổi của gói / chốt khi trả góp / 2 KTV | Buổi gói = giá gói ÷ số buổi trả tiền, **buổi tặng = 0** (`line_value`); chốt = tổng hóa đơn (R1 luôn thu đủ, nên tiền đã thu = giá trị bán); mỗi dòng dịch vụ một KTV, hai người làm thì ghi hai dòng | ✓ `commission_on_bonus_sessions`, `closing_commission` | Q-C3 |
| 4 | Đi muộn: có phạt không, ngưỡng bao nhiêu | Chỉ ghi nhận, phạt 0đ, ngưỡng 0 phút | ✓ `late_grace_minutes`, `late_penalty_amount`, `default_shift_start` | Q-C4 |
| 5 | Tiền ăn 300–600k, hỗ trợ học viên tháng 2 | Nhập tay theo từng người (`meal_allowance`, `base_salary`). ADR-0019 D3: tiền ăn là khoản cố định tháng, 0đ khi tháng không có ngày đi làm | ✓ (dữ liệu theo người) + `meal_allowance_basis` = `monthly`/`days_worked` | Q-C5 |
| 6 | Ngày lễ / Chủ nhật | Không xử lý riêng: mọi ngày như nhau, mẫu số lương ngày vẫn là số ngày − 2 | Chưa (chưa có quy tắc để cấu hình) | Q-C6 |
| 7 | Vào làm giữa tháng: Thưởng gắn bó tính thế nào | ~~Theo tỷ lệ số ngày làm trong tháng~~ → **ADR-0019 D4:** "tháng làm việc" = tháng có ≥ 15 ngày thuộc thời gian làm việc: đủ 500k, dưới 15 ngày thì 0đ. Ngày trước khi vào làm không bị tính là nghỉ | ✓ `loyalty_partial_month` = `fifteen_days` (mặc định)/`prorate`/`full`/`none_if_partial`. Spa đã lưu quy tắc trước đây giữ giá trị đã lưu | Q-C7 (đã quyết) |
| 8 | Excel trừ 500k cả với học viên có cọc | Theo văn bản: lương cứng nhập là **mức thực nhận**, hệ thống không tự trừ 500k. Người có cọc thì đặt `loyalty_bonus_monthly = 0` | ✓ (dữ liệu theo người) | Q-C8 |

Một số mặc định khác cũng lấy từ sheet và chỉnh được: mức Tết 500k/400k/300k (`tet_level_amounts`), giờ vào ca 09:00.

## 7. Non-functional
- Bảo mật: chạy qua `secure`. Lương gửi đi chỉ khi có quyền (test đã mutation-check). Lưới chấm công có khóa tháng ở hai lớp. Bảng chỉ thêm có trigger. RLS đã kiểm dưới `app_rw`. Ghi chú không vào audit.
- Hiệu năng: 3–8 người × 31 ngày, mỗi màn một lần query. Bảng lương tính trực tiếp từ một query gom `visit_line` theo (KTV, dịch vụ) trong tháng, có index `visit_day_status_idx`.
- Web: lazy chunk, startup JS ≤ 125 KB gzip, WCAG 2.2 AA, giao diện phone riêng.

## 8. Out of scope (lần này)
Hoa hồng bán sản phẩm (chưa có module sản phẩm). Chi trả theo mốc (Thưởng gắn bó 12/24, thưởng cam kết, hoàn cọc). Hợp đồng và chương trình học viên/thử việc (gói 12/24, cọc, lựa chọn lương cố định suốt HĐ). Bảng 10 năm tăng 10 % (lương cứng đang nhập tay). Bảo hiểm (BHXH 308k từ tháng 13). Tự check-in bằng điện thoại của KTV. In phiếu lương ra PDF. Cảnh báo kỷ luật "Chưa đạt 2 tháng liên tiếp / 3 trong 6 tháng" (chỉ mới có cờ K ≥ 5).

## 9. Câu hỏi cho EM
- **Q-S1**: Quản lý có được xem lương không? Mặc định là **không** (chỉ chủ spa có `payroll.*`). Quản lý vẫn chấm công, đánh giá và ghi thưởng/phạt (thấy số tiền thưởng/phạt mình ghi, không thấy lương cứng, hoa hồng hay thực lĩnh).
- **Q-S2**: "Đánh giá từng ngày" nghĩa là khen/phàn nàn/ghi chú hằng ngày như cột của sheet (đã build), hay muốn chấm cả 6 tiêu chí mỗi ngày? Sheet chỉ chấm 6 tiêu chí một lần mỗi tháng.
- **Q-S3**: Ghi thưởng/phạt bằng tiền: quản lý ghi được (mặc định) hay chỉ chủ spa?
- ~~**Q-S4**: Người vào làm hoặc nghỉ giữa tháng: lương cứng có tính theo tỷ lệ ngày không?~~ → **ADR-0019 D4: có**, theo số ngày thuộc thời gian làm việc ÷ số ngày trong tháng, làm tròn nghìn đồng (AC5.2).
- **Q-C1 … Q-C8**: các dòng tương ứng ở §6 (giữ mặc định hay chọn cách khác).

## 10. Vòng 5 (EM 2026-11-06, [raw](../../../raw/2026-11-06-em-review-round5.md), mục 9–11)
EM: ba tab Chấm công / Đánh giá / Thưởng-phạt làm người dùng không biết phải điền đủ ba chỗ; ngày công dạng danh sách 30 dòng khó xem; tài khoản KTV thiếu lịch sử "đã làm cho ai, ai đặt lịch mình".

**Cách làm thực tế (nghiên cứu, 2026-11-06).** Spa nhỏ không có máy chấm công: quản lý điểm danh một lần đầu ca, ghi sự việc trong ngày (khen, phàn nàn, đi muộn, phạt) vào sổ, cuối tháng mới cộng và chấm điểm. Các app chấm công cho tiệm nhỏ đều xoay quanh **một màn trong ngày** (ai đã vào ca, ai chưa) và **một báo cáo tháng dạng lưới** (nhân viên theo hàng, ngày theo cột, như mẫu bảng chấm công VN), còn việc cuối tháng là một bước riêng có nhắc ([ClockIt cho salon/spa](https://clockit.io/use-cases/time-clock-for-salons-and-spas/), [Everhour Shifts](https://everhour.com/shifts/salon-spa-scheduling-software), [MISA: lập bảng chấm công](https://helpact.misa.vn/kb/lap-bang-cham-cong/), [luatvietnam: mẫu bảng chấm công](https://luatvietnam.vn/bieu-mau/mau-bang-cham-cong-571-19464-article.html)). Sheet Excel của spa cũng đúng như vậy (cột ngày + tổng cuối hàng).

Không đổi API hay quy tắc dữ liệu cho mục 9–10 (khóa tháng, quyền giữ nguyên). Mục 11 thêm một route chỉ đọc.

### US7: Sổ hằng ngày một trang (quản lý)
*Là quản lý, tôi muốn mở một trang cho một ngày, đi lần lượt từng người và ghi hết mọi thứ của người đó ngay tại chỗ, để không phải nhớ còn tab nào chưa điền.*
- AC7.1 `/nhan-vien` mở tab **"Sổ hằng ngày"** (mặc định). Mỗi người có làm trong ngày là một thẻ: chấm công ✓/P/ĐX/K (một chạm là lưu; nay là chữ + icon, F20 §16), bộ đếm Khen / Phàn nàn, các khoản thưởng/phạt đã ghi ngày đó, nút "+ Thưởng/phạt" (form mở sẵn đúng người, đúng ngày) và nút "Chi tiết" (giờ đến/về, ghi chú chấm công, ghi chú đánh giá).
- AC7.2 Đầu trang có **thanh tiến độ** "Đã chấm công x/y người" và danh sách "Còn thiếu: …" (bấm tên → cuộn tới thẻ). Chỉ chấm công là bắt buộc; khen/phàn nàn/thưởng/phạt là tùy chọn và trang nói rõ điều đó. Chấm đủ → "Xong ngày này".
- AC7.3 Có ngày đã qua trong tháng còn người chưa chấm → banner "Còn n ngày chưa chấm đủ" với các ngày bấm được (mở đúng ngày đó). Ngày tương lai không chấm được.
- AC7.4 **Đánh giá tháng chỉ nhắc khi đến hạn**: từ ngày cuối tháng đến ngày 10 tháng sau, nếu tháng trước (hoặc tháng này vào ngày cuối) chưa chốt và còn người chưa có phiếu → banner "Đến hạn đánh giá tháng m: còn n người" + nút mở tab "Đánh giá tháng" đúng tháng đó. Ngoài thời gian này không có banner. *(Lead's decision — EM to review: cửa sổ nhắc = ngày cuối tháng → ngày 10 tháng sau; spec gốc nói ngày 1–5.)*
- AC7.5 Tab còn lại: "Bảng công tháng" (US8), "Đánh giá tháng" (phiếu 100 điểm như AC3b, có dấu đếm số người còn thiếu khi đến hạn), "Bảng lương" và "Hồ sơ" (như cũ, theo quyền; nay "Hồ sơ nhân viên", §13). Bỏ tab "Thưởng, phạt" riêng: sổ thưởng/phạt của tháng nằm cuối "Bảng công tháng". Link cũ `?tab=attendance|adjustments` mở tab mới tương ứng (`day` / `month`; `?tab=evaluation` vẫn là tab "Đánh giá tháng"), không 404. *(Lead's decision — EM to review.)*
- AC7.6 Âm: tháng đã chốt → mọi nút ghi bị tắt + banner (server vẫn `409`); quyền: thiếu `attendance.record` thì không có nút chấm công, thiếu `evaluation.record` thì không có khen/phàn nàn/thưởng/phạt (server vẫn `403`); lỗi mạng khi lưu → thông báo lỗi tại trang, trạng thái thẻ không đổi (dữ liệu lấy lại từ server).

### US8: Bảng công tháng dạng lưới
*Là quản lý, tôi muốn xem cả tháng trong một bảng (người × ngày), để nhìn là thấy ai nghỉ, ai muộn.*
- AC8.1 Lưới: hàng = nhân viên, cột = ngày (thứ, CN tô màu), ô = mã ✓/P/ĐX/K (muộn có gạch đỏ, chưa chấm "–", ngoài thời gian làm / tương lai "·"), cột tổng cuối hàng (công, muộn, P/ĐX/K, khen/phàn nàn). *(F20 §16 thay: ô = icon trạng thái, cột "P/ĐX/K" → "Nghỉ" với icon + số.)* Cột tên dính bên trái khi cuộn ngang; **dùng được ở 390 px** (không còn chỉ desktop). Bấm ô (tháng chưa chốt) → sheet sửa ô đó.
- AC8.2 *(thay bằng §12 US11)* KTV ("Công và lương của tôi"): **lịch tháng thu gọn** (7 cột T2…CN, ô = ngày + mã) thay cho danh sách, kèm ô tổng (đi làm, muộn, P, ĐX, K, khen, phàn nàn). Không có thanh cuộn ngang ở 390 px.
- AC8.3 Âm: tháng chưa có người nào → trạng thái trống; lỗi tải → trạng thái lỗi có nút thử lại.

### US9: KTV xem khách đã làm và lịch hẹn của mình *(AC9.1–9.3, 9.6 thay bằng §12; `GET /me/work` đã xóa)*
*Là KTV, tôi muốn xem mình đã làm cho khách nào và ai đã đặt lịch với mình, để chuẩn bị và tự đối chiếu hoa hồng.*
- AC9.1 `GET /me/work?month=YYYY-MM` (`staff.read_own`, `Cache-Control: no-store`). Staff lấy từ `staff.user_id = session.user` (như `/me/staff`); tài khoản chưa gắn → `200 {linked: false}`.
- AC9.2 `served[]`: các lượt **completed** của tháng có ít nhất một dòng dịch vụ do chính người đó làm; mỗi lượt: `visit_id`, `business_date`, `check_in_at`, `customer_name`, `services[]` = **chỉ các dòng của người đó** (tên dịch vụ, số lượng, `source` lẻ/gói). Mới nhất trước. Lượt đã hủy không có.
- AC9.3 `upcoming[]`: lịch hẹn `booked`/`arrived` gán cho người đó, từ hôm nay tới 30 ngày sau, sớm nhất trước: `appointment_id`, `starts_at`, `ends_at`, `status`, `customer_name` (tên hồ sơ hoặc tên khách mới), `services[]` (tên).
- AC9.4 **Chỉ các trường trên.** Không có giá, doanh thu, giá trị dòng, SĐT, ghi chú buổi, ghi chú sức khỏe, ghi chú lịch hẹn, id khách (test kiểm không có key nào khác). KTV vẫn không có `customer.read`/`visit.read`/`appointment.manage`. *(Lead's decision — EM to review: KTV thấy tên khách mình phục vụ, không thấy SĐT; tên dịch vụ đã có trên phiếu lương.)*
- AC9.5 Âm: chưa đăng nhập → `401`; role không có `staff.read_own` → `403`; tham số lạ (`?staff_id=`) → `400`; tháng sai → `422`; KTV A không bao giờ thấy lượt/lịch của KTV B, kể cả lượt có cả hai cùng làm (A chỉ thấy dòng của A); spa khác không thấy gì (lọc tenant + RLS). Quản lý/chủ spa: các route quản lý không đổi.
- AC9.6 UI: mục nav **"Khách của tôi"** (`/khach-cua-toi`, tab bar "Khách") cho người có `staff.read_own` mà không có `visit.read` (quản lý/chủ spa đã có màn quầy). Hai phần: "Lịch hẹn sắp tới" (nhóm theo ngày) và "Khách đã làm" của tháng (chọn tháng, nhóm theo ngày). Trạng thái: đang tải, trống, lỗi, chưa gắn hồ sơ.

## 11. Sau test thật (EM 2026-11-07, [raw](../../../raw/2026-11-07-em-decisions-after-live-test.md))

### Quyết định C: ai ghi cho ai (server bắt buộc)
- Chủ spa (`staff.manage`) chấm công, ghi khen/phàn nàn, thưởng/phạt và đánh giá tháng cho **mọi người**.
- Quản lý chỉ ghi cho KTV: **không bao giờ cho chính mình, không cho quản lý khác** (cũng không cho chủ spa). Sai → `403 staff.record_owner_only`, cả batch không ghi gì.
- "Quản lý" lấy từ dữ liệu, không từ chữ "Vị trí": hồ sơ gắn với tài khoản có role mang `attendance.record`, `evaluation.record` hoặc `staff.manage`; cộng hồ sơ gắn với chính người gọi. Hồ sơ **chưa gắn tài khoản** tính như KTV (không đăng nhập thì không quản lý được ai). → Chủ spa cần gắn tài khoản cho hồ sơ quản lý ở tab Hồ sơ (nay "Hồ sơ nhân viên"; dữ liệu demo đã gắn). *(§16 US16c thay: "quản lý" = role của tài khoản; quản lý chưa gắn hồ sơ không ghi được gì, `403 staff.link_required`.)*
- Route: `PUT /attendance`, `PUT /daily-reviews`, `PUT /evaluations`, `POST /adjustments`, `PUT /adjustments/{id}`, `DELETE /adjustments/{id}` (sửa: cả người cũ và người mới phải trong phạm vi). Code: `hr::scope::RecordScope`.
- UI: các view tháng trả `can_record` mỗi người (`GET /attendance`, `/daily-reviews`, `/evaluations`) và `can_edit` mỗi khoản thưởng/phạt (`GET /adjustments`). Dòng không ghi được: nút chấm công, bộ đếm bị tắt, không có "Ghi thưởng / phạt", "Giờ & ghi chú", "Chấm điểm"; có câu "Chỉ chủ spa chấm công, ghi khen/phàn nàn và thưởng/phạt cho quản lý." / "Chỉ chủ spa chấm cho quản lý"; bảng công tháng ghi "Chủ spa chấm" dưới tên và ô không bấm được; "Cả spa đi làm" bỏ qua những dòng đó.

### Quyết định D: phiếu đánh giá tháng gõ điểm
Xem AC3b.1. Giữ tối đa 100 điểm. **Giới hạn Mức 3** giữ nguyên ý: tick "khiếu nại nghiêm trọng / vi phạm nhiều lần" thì kết quả tháng tối đa Mức 3, không bao giờ làm kết quả tốt lên; form ghi rõ "Tổng X điểm là Mức 1, nhưng có khiếu nại nghiêm trọng nên kết quả tháng tối đa Mức 3". Mức 3 / Chưa đạt vẫn bắt buộc kế hoạch kèm cặp.

### Chấm nghỉ cho người đã có lượt khách
`GET /attendance` trả `visit_days` mỗi người (ngày có dòng dịch vụ do người đó làm trong lượt hoàn tất). Chọn P/ĐX/K cho ngày có lượt → **hỏi xác nhận rồi mới lưu** ("Lan có lượt khách ngày này … Vẫn chấm Nghỉ không phép / Thôi"); trong sheet "Giờ & ghi chú" là cảnh báo cạnh lựa chọn, bấm Lưu là xác nhận. *(Lead's decision — EM to review: xác nhận chứ không chặn, vì lượt có thể ghi nhầm KTV hoặc người đó về giữa ca; server không chặn.)*

### Thưởng/phạt
- Nút "Lưu" tắt thì có câu nói thiếu gì: "Để lưu, cần chọn nhân viên, nhập số tiền hoặc số điểm, ghi lý do."
- Khoản đã ghi có "Sửa" / "Xóa" (khi `can_edit`): trong sổ hằng ngày (dòng dưới người) và sổ thưởng/phạt cuối "Bảng công tháng". Xóa có hộp xác nhận. Xem AC4.2.

### Hoa hồng chốt khi gói bị hủy có hoàn tiền
[ADR-0010](../../decisions/ADR-0010-commission-follows-kept-money.md): khoản hoàn trừ vào cơ sở hoa hồng chốt của người chốt, ở tháng hoàn tiền (dòng `package_refund` âm trên phiếu lương). *(Lead's decision — EM to review.)*


## 12. Màn của KTV (EM 2026-11-07, [raw](../../../raw/2026-11-07-em-technician-pages.md))

EM xem bằng tài khoản KTV: "cách sắp xếp chưa hợp lý" (phiếu lương), "UI xấu không cân đối" (lịch công), "hiện như google calendar giống trong chủ spa" (Khách của tôi), và "xem chi tiết hơn ngày hôm đó". Thay AC8.2 và US9 (AC9.1–AC9.3, AC9.6); AC9.4–AC9.5 (allowlist, âm) giữ nguyên ý cho endpoint mới.

### US10: "Công và lương của tôi" sắp lại
- AC10.1 Thứ tự: **Tổng quan tháng** (Thực lĩnh, Hoa hồng làm + giới thiệu/chốt, Thưởng/phạt, **trạng thái đánh giá tháng** dạng chip "Chưa đánh giá" / "80 điểm · Mức 2", chip "Tạm tính"/"Đã chốt lương", các cờ cần xem) → Ngày công → Đánh giá và thưởng/phạt → Phiếu lương. Dòng "⚠ Chưa đánh giá tháng" không còn nằm lẻ ở cuối trang; cờ của phiếu lương (cả sheet phiếu lương của chủ spa) là chip ngay dưới dòng trạng thái.
- AC10.2 "Dịch vụ đã làm" **nhóm theo loại**: khối "Dịch vụ lẻ" và "Buổi liệu trình", mỗi dịch vụ một lần trong khối, xếp theo giá trị; mỗi khối có dòng "Cộng …" (lượt, giá trị); cuối bảng "Tổng tính hoa hồng" = `service_commission_base` và "Thành hoa hồng làm" = `tỷ lệ × tổng` = `service_commission` (khớp dòng "Hoa hồng làm liệu trình" của phiếu). Spa tính buổi tặng đủ giá (`commission_on_bonus_sessions`) thì bảng cộng `full_value` (nhận ra khi base = Σ full_value). *Lý do chọn nhóm theo loại thay vì theo dịch vụ:* KTV nghĩ "khách lẻ / khách gói", phiếu lương có dòng "làm liệu trình", và mỗi khối có tổng riêng; nhóm theo dịch vụ thì loại thành chữ phụ lặp lại ở mọi dòng.
- AC10.3 "Giới thiệu / chốt được" cùng cách: "Gói liệu trình chốt được", "Dịch vụ lẻ giới thiệu", "Hoàn tiền gói (trừ lại)" (âm, ADR-0010, không đếm vào số lượng); tổng = `closing_commission_base`, "Thành hoa hồng giới thiệu / chốt" = `closing_commission`.
- AC10.4 Bảng 3 cột cố định (tên | lượt | giá trị), số `tabular-nums` canh phải, trang một cột tối đa 56rem (không giãn cột ở 1280); 390 px giữ bảng, tên xuống dòng.

### US11: Lịch Ngày công cân đối
- AC11.1 7 cột T2…CN đều nhau; ô vuông ở điện thoại, 3:2 từ 640 px. Số ngày góc trên trái (nhỏ), mã ✓/P/ĐX/K giữa ô (lớn; F20 §16: nay icon trạng thái), **phút muộn là chip "12′" góc trên phải** (màu warning, thay gạch đỏ). Hôm nay viền primary; ngày tương lai chỉ có viền mảnh + số (không nền); chưa chấm "–". CN: tiêu đề và số ngày màu chữ chính, đậm (không mang nghĩa màu).
- AC11.2 Chú thích là các chip nhỏ đúng màu ô (✓ Đi làm, P, ĐX, K, – Chưa chấm, 12′ Đi muộn), không còn một câu ký hiệu. *(F20 §16: chip = icon của ô + chữ "Đi làm" / "Nghỉ có phép" / "Nghỉ đột xuất" / "Nghỉ không phép" / "Chưa chấm", `AttendanceLegend`.)*
- AC11.3 Ô tổng (Đi làm, Đi muộn "1 lần" + "14 phút", Nghỉ P/ĐX, Nghỉ K, Khen/Phàn nàn; F20 §16: "Nghỉ P/ĐX" + "Nghỉ K" → ba ô "Nghỉ có phép" / "Nghỉ đột xuất" / "Nghỉ không phép") ở trên lịch, cùng bề rộng cột đọc; điện thoại 2 cột (ô lẻ cuối chiếm cả hàng).

### US12: Chi tiết một ngày
*Là KTV, tôi muốn bấm một ngày để xem hôm đó mình làm gì và được bao nhiêu.*
- AC12.1 Ngày ≤ hôm nay là nút (tên truy cập = "T5, 05/11/2026: Đi làm, muộn 12 phút"); bấm → sheet "Ngày T5, 05/11/2026" (bottom sheet ở điện thoại, dialog từ 640 px, có focus trap + Esc). "Ngày trước / Ngày sau" đổi ngày ngay trong sheet (sau hôm nay thì tắt). Ngày mở nằm trên URL (`?day=`), tháng đi theo ngày.
- AC12.2 `GET /me/staff/day?date=YYYY-MM-DD` (`staff.read_own`, `no-store`, mặc định hôm nay). Người lấy từ session; `?staff_id=` → `400`; ngày sai/ngoài 2020–2100 → `422 invalid_date`; chưa gắn hồ sơ → `{linked:false}`.
- AC12.3 Nội dung: chấm công (trạng thái, giờ vào/ra, phút muộn, ghi chú — cùng dữ liệu `/me/staff` đã cho), khen/phàn nàn + ghi chú, thưởng/phạt ghi ngày đó, **lượt đã làm** (giờ, tên khách, chỉ dòng của mình: dịch vụ, lẻ/buổi liệu trình, số lượng, giá trị tính hoa hồng, hoa hồng = tỷ lệ × giá trị làm tròn), **giới thiệu / chốt** của ngày (gói chốt, dịch vụ lẻ giới thiệu, hoàn tiền gói mình chốt; tên khách; hoa hồng). Tỷ lệ: tháng mở = điều kiện lương hiện tại, tháng đã chốt = tỷ lệ trong phiếu đã chốt; chưa có điều kiện lương → hoa hồng `null` ("chưa tính được"). Ghi chú cuối sheet: phiếu lương làm tròn một lần cho cả tháng, có thể lệch vài đồng.
- AC12.4 Không SĐT, không ghi chú lượt/hồ sơ/sức khỏe, không id khách, không dữ liệu người khác (dòng của KTV khác trong lượt chung, thưởng của người khác). Test `staff.rs::a_technician_sees_their_own_day_in_detail`.

### US9 (thay): "Khách của tôi" là lịch như Google Calendar
- AC9.1' `GET /me/appointments?from=&to=` (`staff.read_own`, `no-store`, ≤ 14 ngày như `GET /appointments`, `to < from` / quá 14 ngày → `422`, `?staff_id=` → `400`). Thay `GET /me/work` (đã xóa).
- AC9.2' `entries[]` sớm nhất trước, mọi phase: lịch hẹn `staff_id = mình` (mọi trạng thái, thời lượng đúng số phút, phase như lịch của quầy, `group_size` = số người trong nhóm ADR-0009 trừ người đã hủy) + lượt `completed` có dòng của mình mà không phải lượt của một lịch hẹn của mình (`walk_in` = không lịch hẹn nào trỏ tới; dịch vụ = chỉ dòng của mình, thời lượng từ các dòng đó, `from_package`). Trường: `id, kind, business_date, starts_at, ends_at, duration_min, phase, customer_name, services[{service_name, category_name, from_package}], group_size, walk_in` — không gì khác (test kiểm tập key).
- AC9.6' UI `/khach-cua-toi`: `TimeGrid` dùng chung với Lịch hẹn của quầy (thẻ đúng độ dài, chấm phase, "Vãng lai" — nay "Không đặt lịch", [appointments](appointments.md) §10.14 F23 —, "Nhóm · 3 người", vạch giờ hiện tại). Desktop: Ngày (một cột) / **Tuần (7 cột ngày, mặc định)** / Danh sách; điện thoại: Ngày (mặc định) / Danh sách tuần (nhóm theo ngày). Thanh: ‹ Hôm nay ›; chọn tháng ở đầu trang như "Công và lương của tôi" (tháng → ngày 1, tháng này → hôm nay; tới tháng sau vì lịch hẹn đặt trước tới 30 ngày). Bấm thẻ → sheet chỉ đọc (giờ, khách, dịch vụ + lẻ/gói, thời lượng, phase). Không có ô đặt lịch, không nút sửa. URL: `ngay`, `kieu`.
- Test: `staff.rs::a_technician_sees_only_their_own_calendar`; vitest `/khach-cua-toi`; e2e `technician.spec.ts`.

## 13. Hồ sơ nhân viên (trước là tab "Nhân sự"): danh sách, vào làm, nghỉ việc, nhận lại (EM 2026-11-07, [raw](../../../raw/2026-11-07-em-flow-review-hr-latency.md) mục 9)

EM (đăng nhập quản lý): "mới có chấm công mới có đánh giá nhưng lại không có quản lý nhân sự… biết người đó làm bao lâu rồi cần remove hay add vào list của spa… bây giờ là như hardcode cứng".

**Khoảng trống trước bản này.** Tab "Hồ sơ" có sẵn (thêm / sửa hồ sơ, gắn tài khoản, lương) nhưng:
- chỉ chủ spa thấy (`staff.manage`), nên quản lý không thấy gì, đúng như EM nói;
- nghỉ việc = điền ô "Ngày nghỉ việc" trong form: không lý do, không xác nhận, không kiểm lịch hẹn còn treo;
- nhận lại làm = xóa ngày nghỉ, mất dấu đợt trước (tháng cũ thành `not_employed`);
- không có thâm niên, không lọc Đang làm / Đã nghỉ, không lịch sử thay đổi;
- tài khoản của người đã nghỉ vẫn đăng nhập được;
- dữ liệu demo / CLI chỉ thêm người (`add-staff`).

Không có quy tắc thưởng nào đang hard-code thâm niên: thưởng gắn bó chỉ tính theo tỷ lệ ngày làm trong tháng (Q-C7, nay theo các đợt làm việc thật), mốc 12/24 tháng vẫn ngoài phạm vi (§8) và sẽ đọc `Employment::tenure`.

Mô hình: [ADR-0013](../../decisions/ADR-0013-employment-periods.md) (đợt làm việc, nghỉ việc hủy mềm), migration **0025**.

### US13: Danh sách nhân sự
- AC13.1 Tab **"Nhân sự"** (thay "Hồ sơ"; **đổi tên lần nữa 2026-11-08 thành "Hồ sơ nhân viên"**, URL vẫn `?tab=profiles`) cho người có `staff.read` (chủ spa, quản lý). Bảng: Họ tên, Vị trí, chip trạng thái ("Đang làm" / "Đã nghỉ từ dd/mm/yyyy"), Ngày vào làm (đợt hiện tại), **Thâm niên** ("1 năm 8 tháng", "19 ngày", "Chưa rõ ngày vào làm"), Tài khoản (chủ spa: **SĐT đăng nhập** của tài khoản, kèm "Đã khóa" nếu bị khóa, hoặc "Chưa gắn" (ADR-0019 D20, trước là tên tài khoản); quản lý: "Đã gắn" / "Chưa gắn"), lương + hoa hồng nếu `payroll.read`. Lọc **Đang làm (mặc định) / Đã nghỉ / Tất cả** kèm số người, trên URL (`roster`).
- AC13.2 `GET /staff/profiles?status=active|left|all` (mặc định `all`) và `GET /staff/{id}`: `staff.read`. Mỗi người có `periods[]` (cũ trước), `joined_on` (đầu đợt mới nhất), `first_joined_on`, `left_on`, `tenure_days`, `tenure_months` (cộng các đợt tới hôm nay theo giờ spa; `null` nếu thiếu ngày bắt đầu), `login_linked`. **Chỉ chủ spa (`staff.manage`)** nhận key `details` (SĐT, ghi chú, tài khoản) và `end_reason` của từng đợt; quản lý không nhận các key đó (không gửi rồi ẩn). Lương như AC1.4.
- AC13.3 Sheet một người: trạng thái, thâm niên, ngày vào làm (+ "Vào làm lần đầu" khi đã nhận lại), giờ vào ca, **Quá trình làm việc** (mỗi đợt: từ – đến; lý do nghỉ cho chủ spa). Chủ spa có thêm: SĐT, ghi chú, **Tài khoản đăng nhập**, **Lịch sử thay đổi**, nút "Sửa hồ sơ", "Cho nghỉ việc" (`btn-danger-outline`) hoặc "Nhận lại làm".
- AC13.4 Hồ sơ "Quản lý" đang làm mà chưa gắn tài khoản có dòng "Chưa gắn: khi ghi công được tính như KTV" (quyết định C đọc tài khoản, không đọc chữ "Vị trí").

### US14: Thêm, sửa
- AC14.1 `POST /staff` (`staff.manage`): `full_name`, `position` (gợi ý: Kỹ thuật viên, Quản lý, Thu ngân, Lễ tân, Học viên; vẫn gõ tự do), `phone?` (không bắt buộc), `joined_on?` (form điền sẵn hôm nay; bỏ trống = chưa rõ, thâm niên "Chưa rõ ngày vào làm"), `shift_start?`, `note?` (≤ 1000, chỉ chủ spa thấy, audit chỉ ghi `has_note`), `user_id?`. Mở đợt đầu. Lương ghi riêng (`PUT /staff/{id}/pay`, như AC1.3).
- AC14.2 `PUT /staff/{id}` (`staff.manage`, `If-Match`): tên, vị trí, SĐT, giờ vào ca, ghi chú, `joined_on` = **ngày vào của đợt hiện tại** (sửa nhầm). Người đã nghỉ đổi `joined_on` → `422 joined_on period_ended`; trước ngày nghỉ của đợt trước → `422 overlaps_previous`. `status`, `left_on`, `user_id` không còn trong body (có route riêng). Mỗi lần sửa có audit before/after (vị trí, ngày, giờ vào ca; lương ở `staff.pay_update`).
- AC14.3 `GET /staff/{id}/history` (`staff.manage`): mới nhất trước; mỗi mục = hành động (thêm, sửa, đổi tài khoản, nghỉ việc + lý do, nhận lại, đổi lương), người làm, ngày, các trường đổi (trước → sau). Đổi lương chỉ hiện cho `payroll.read`.

### US15: Nghỉ việc (hủy mềm) và nhận lại
- AC15.1 `POST /staff/{id}/end {ended_on, reason}` (`staff.manage`, `If-Match`) đóng đợt hiện tại. Lỗi:
  - lý do trống → `422 reason required`; ngày sau hôm nay → `422 ended_on in_future`; trước ngày vào → `before_joined_on`;
  - còn chấm công hoặc lượt khách hoàn tất **sau** ngày đó → `422 ended_on has_later_records`;
  - còn lịch hẹn `booked`/`arrived` gán cho người đó sau ngày đó → `409 staff.has_upcoming_bookings` (đổi KTV / hủy lịch trước);
  - đã nghỉ → `409 staff.already_left`; hồ sơ gắn tài khoản của chính người gọi → `422 staff.cannot_end_self`; gắn tài khoản có `staff.manage` → `422 staff.linked_to_owner`.
- AC15.2 Sau khi nghỉ:
  - không còn trong mọi picker (`GET /staff` mặc định `active`: KTV làm ở quầy, đặt lịch, người chốt, giỏ) và server từ chối gán (`422 staff.inactive`);
  - không còn trong sổ hằng ngày, bảng công, đánh giá, bảng lương của **các tháng sau ngày nghỉ**;
  - vẫn còn trong tháng có làm, phiếu lương đã chốt, lượt khách, báo cáo, hoa hồng, và trong lọc "Đã nghỉ";
  - ~~tài khoản đã gắn bị khóa ở spa này~~ → **thay bởi §14 (Q-S6, 2026-11-07)**: tài khoản vẫn đăng nhập, request kế tiếp chỉ còn `staff.read_own_past` (công và lương của chính mình, các tháng có làm); chủ spa tick "Khóa tài khoản" (`lock_login: true`) thì khóa hẳn như trước.
  UI: hộp xác nhận nói rõ các điều trên; "Xác nhận nghỉ việc" là `btn-danger`, tắt khi chưa có lý do. Thành công thì sheet đóng hẳn, về danh sách (ADR-0019 D20), không quay lại sheet của người đó.
- AC15.3 `POST /staff/{id}/rehire {started_on}` (`staff.manage`, `If-Match`): đợt mới, `started_on` phải sau ngày nghỉ của đợt trước (`422 before_last_end`); đang làm → `409 staff.already_active`. Tài khoản đã gắn lấy lại quyền của role (không còn là người đã nghỉ) và được mở khóa nếu đã khóa. Khoảng nghỉ giữa hai đợt không phải ngày làm (không "chưa chấm", không tính thưởng gắn bó).
- AC15.4 DB: đợt đã kết thúc không sửa được (trigger), không xóa đợt (app_rw không có DELETE), không chồng đợt (`EXCLUDE`), tối đa một đợt mở; hồ sơ không bao giờ bị xóa.

### US16: Tài khoản đăng nhập
- AC16.1 `PUT /staff/{id}/account {user_id | null}` (`staff.manage`, `If-Match`): gắn / bỏ gắn. Tài khoản phải là thành viên đang hoạt động (`422 user_id not_a_member`), một tài khoản một hồ sơ (`409 staff.account_taken`). Không bao giờ xem hay đặt mật khẩu ở đây (tạo tài khoản vẫn qua CLI `add-user` — **thay bằng §17 US17a**: chủ spa tạo tài khoản từ hồ sơ; route này chỉ còn cho tài khoản cũ, US17g). Bỏ gắn hồ sơ quản lý → quản lý khác ghi được cho người đó như KTV (quyết định C).

### Phân quyền (lead's decision — EM to review)
- **Chủ spa** làm mọi thứ. **Quản lý chỉ xem** danh sách (tên, vị trí, các đợt, thâm niên, đã gắn tài khoản hay chưa). Quản lý không thêm, sửa, cho nghỉ, nhận lại hay gắn tài khoản. Lý do: US1 cho `staff.manage` chỉ chủ spa; nghỉ việc và đổi vị trí ảnh hưởng lương và quyền đăng nhập. Mở cho quản lý thêm / cho nghỉ KTV là **Q-S5** (nếu EM đồng ý: permission riêng, giới hạn theo `hr::scope` như quyết định C).
- Vì sao quản lý được **xem**: quản lý là người chấm công, đánh giá, xếp KTV hằng ngày, và đã thấy tên + vị trí người đang làm qua picker và lưới chấm công; danh sách chỉ thêm người đã nghỉ, các đợt và thâm niên. Response của quản lý là allowlist (không lương, SĐT, ghi chú, id tài khoản, lý do nghỉ); KTV `403`. Lương, lịch sử, gắn tài khoản và mọi ghi vẫn chỉ chủ spa.
- Permission mới `staff.read` (chủ spa, quản lý). Test: `staff.rs::the_roster_hires_ends_and_rehires_without_losing_history`, `::the_login_link_is_owner_only` (trước tên `ending_employment_locks_the_login_and_the_link_is_owner_only`; nghỉ việc không còn khóa tài khoản từ §14); `authz_matrix.rs` +4 dòng, 2 dòng đổi quyền; vitest "Hồ sơ nhân viên (roster)"; e2e `staff.spec.ts` (thêm → nghỉ việc → Đã nghỉ → nhận lại).

### Câu hỏi cho EM
- **Q-S5 — đã quyết (EM 2026-11-07, [raw](../../../raw/2026-11-07-em-decisions-hr-notes-group-region.md) câu 1):** "không nhé chỉ spa mới có quyền edit". Chỉ chủ spa thêm, sửa, cho nghỉ, nhận lại, gắn tài khoản (`staff.manage`); quản lý chỉ xem (`staff.read`). Đúng như đang chạy, không đổi code.
- **Q-S6 — đã quyết (EM 2026-11-07, câu 2): "có".** Người đã nghỉ vẫn đăng nhập để xem phiếu lương cũ: §14.

## 14. Người đã nghỉ xem công và lương cũ (EM 2026-11-07 Q-S6, [ADR-0014](../../decisions/ADR-0014-former-staff-login.md), migration 0026)

### US17: Người đã nghỉ tự xem phiếu lương cũ
*Là người đã nghỉ việc, tôi muốn đăng nhập xem lại ngày công và phiếu lương các tháng tôi đã làm.*
- AC17.1 Cho nghỉ việc không còn khóa tài khoản. Từ request kế tiếp, tài khoản gắn với hồ sơ đã nghỉ (`staff.status = 'left'`) chỉ có **`staff.read_own_past`**, bất kể role là KTV hay quản lý: quyền suy ra ở server mỗi request (`kernel::permission::effective_permissions`), không phải cờ client, không lưu vào role. Tài khoản có `staff.manage` (chủ spa) không bao giờ bị hạ quyền.
- AC17.2 Hai endpoint nhận quyền này: `GET /me/staff` (tháng mặc định = tháng nghỉ việc; `left_on` = ngày làm cuối) và `GET /me/staff/day` (ngày mặc định = ngày làm cuối; không kèm ghi chú buổi). Tháng không có ngày làm (sau ngày nghỉ, trước ngày vào) → `403 staff.not_employed_in_month`. Mọi endpoint khác → `403 auth.forbidden` (khách, Quầy, lịch hẹn, danh mục, "Khách của tôi", sổ nhân viên…).
- AC17.3 Nhận lại làm → quyền của role trở lại ngay request kế tiếp. Chủ spa tick **"Khóa tài khoản"** khi cho nghỉ (`POST /staff/{id}/end` với `lock_login: true`) → khóa hẳn (`401/403 auth.not_a_member`) như trước; nhận lại làm mở khóa.
- AC17.4 Web: nav chỉ còn Tổng quan, "Công và lương của tôi", Cài đặt. Màn "Công và lương của tôi" có dòng "Bạn đã nghỉ việc từ …" và chọn tháng không qua tháng nghỉ việc; tháng không làm → trạng thái trống "Tháng này bạn không làm ở spa" + nút "Về tháng nghỉ việc".
- AC17.5 Dữ liệu cũ: migration 0026 mở lại membership đã bị khóa theo quy tắc cũ (mọi `disabled` của người đã nghỉ, vì trước đây chỉ `end` ghi `disabled`).
- Test: `staff.rs::a_leaver_keeps_only_their_own_past_pay`, `::the_owners_login_is_never_downgraded`, `authz_matrix.rs::a_former_staff_member_reaches_only_their_past_pay` (lớp người đã nghỉ qua mọi route), `kernel::permission` unit test; vitest "former staff (ADR-0014)", "owner: ends employment…" (checkbox Khóa tài khoản), App shortcut.

### Ghi chú buổi cho KTV (EM 2026-11-07 câu 3)
EM: "thêm cả KTV đi vì họ có thể xem làm cho ai mà chỉ là không được xem thông tin cá nhân của khách ngoài giới tính và tên thôi".
- `GET /me/appointments` ("Khách của tôi") và `GET /me/staff/day` (chi tiết ngày) thêm **một key `note`**: ghi chú buổi (`visit.note`, mã hóa) của lượt **có dòng do chính KTV làm** (server lọc theo `own_staff_id`; lịch hẹn chưa xong hoặc lượt không có ghi chú → `null`). Không bao giờ ghi chú lịch hẹn, ghi chú hồ sơ khách, hồ sơ sức khỏe, SĐT, ngày sinh, địa chỉ.
- Allowlist cũ giữ nguyên, chỉ thêm `note` (test kiểm tập key); test âm: ghi chú lượt chỉ của KTV khác không bao giờ xuất hiện (`staff.rs::a_technician_sees_only_their_own_calendar`, `::a_technician_sees_their_own_day_in_detail`). Thay AC9.2' (key list) và AC12.4 ("không ghi chú lượt").
- UI: sheet của "Khách của tôi" có dòng "Ghi chú buổi"; chi tiết ngày ghi "Ghi chú buổi: …" dưới tên khách.

## 15. Xem lại đánh giá và lịch sử (EM 2026-11-08, [raw](../../../raw/2026-11-08-em-decisions-evaluation-history.md))
EM: *"trong mục nhân viên không có xem lại đánh giá hoặc lịch sử của đánh giá … các ngày trước hoặc tháng trước"*. Trước đây "Đánh giá tháng" chỉ có tổng + mức của tháng đang chọn và nút "Chấm điểm / Chấm lại" (ẩn khi tháng đã chốt); không xem được phiếu, không có lịch sử từng người.

**US15a — Xem phiếu đánh giá (chỉ đọc).** Mọi dòng đã chấm trong "Đánh giá tháng" có nút **"Xem"**, cả tháng đã chốt lương và dòng người xem không được chấm (phiếu của quản lý khác do chủ spa chấm; ~~phiếu của chính mình~~ — từ §16 quản lý xem phiếu của mình ở "Công và lương của tôi"). Phiếu: từng tiêu chí điểm/tối đa + mức, tổng + kết quả, khiếu nại nghiêm trọng (giới hạn Mức 3), kế hoạch kèm cặp, nhận xét, **ai chấm, lúc nào** (`by_name`, `updated_at`). "Chấm lại" chỉ ở chỗ được phép như cũ (tháng mở + quyết định C), có thêm ở chân phiếu. Không API mới: `GET /evaluations?month=` vốn đã trả cả phiếu cho `evaluation.record`.

**US15b — Lịch sử đánh giá một người.** Bấm tên trong "Đánh giá tháng" hoặc "Lịch sử đánh giá" trong sheet của tab Hồ sơ nhân viên (chỉ khi có `evaluation.record`) → `/nhan-vien?tab=evaluation&staff=<id>`: biểu đồ cột nhỏ điểm 12 tháng gần nhất (cũ → mới, mỗi cột có tên truy cập "Tháng 10/2026: 81 điểm, Mức 2"), rồi mỗi tháng một dòng: tháng (+ "đã chốt lương"), tổng · mức, khen / phàn nàn, số ngày nghỉ, số lần muộn; "Xem phiếu", "Từng ngày", "Chấm điểm / Chấm lại" (tháng mở + được chấm → mở tháng đó trong danh sách). "Xem mọi ghi chú về …" → `/nhat-ky-ghi-chu?staff=<id>` khi có `report.read`. Chỉ các tháng người đó có làm (đợt làm việc, ADR-0013).
- API **`GET /evaluations/history?staff_id=&from=YYYY-MM&to=YYYY-MM`** (`evaluation.record` — đúng quy tắc của `GET /evaluations`, vốn đã cho người giữ quyền xem phiếu của mọi người): mặc định 12 tháng tới tháng hiện tại, tối đa 24 tháng (`422 to range_too_long`), `from` sau `to` → `422 from after_to`, thiếu `staff_id` → `422 staff_id required` (kiểm quyền trước khi đọc tham số, nên KTV luôn `403`), id lạ hoặc của spa khác → `404`; `deny_unknown_fields`. Một request, một transaction (chấm công + khen/phàn nàn đọc một lần cho cả khoảng; mỗi tháng một câu đánh giá + một câu kỳ lương), `no-store`. Trả `can_record` (quyết định C) để UI chỉ hiện "Chấm lại" đúng chỗ.

**US15c — Từng ngày phía sau.** "Từng ngày" → `/nhan-vien?tab=month&month=M&staff=<id>`: "Bảng công tháng" chỉ còn hàng của người đó (chip "Chỉ xem: <tên> ✕" để về cả spa) và danh sách **Từng ngày** chỉ đọc: những ngày có ghi — chấm công (giờ đến/về, phút muộn, ghi chú), khen / phàn nàn + ghi chú, thưởng / phạt. Dùng lại các query tháng sẵn có (`/attendance`, `/daily-reviews`, `/adjustments`), không API mới; ô của lưới vẫn mở sheet sửa ngày như cũ khi được phép.

**US15d — KTV và người đã nghỉ.** Không thấy lịch sử của người khác (`GET /evaluations/history` → `403`, kể cả id của chính mình). "Công và lương của tôi" thêm **"Xem phiếu đánh giá"** (chỉ đọc) cho tháng đang xem, từ `GET /me/staff` (vốn đã trả phiếu của chính mình; không tham số chọn người, `?staff_id=` → `400`); người đã nghỉ chỉ trong các tháng có làm (ADR-0014, quy tắc cũ của `/me/staff`). Đổi tháng bằng ô chọn tháng như cũ.

**Đã quyết cùng đợt (cùng raw):** Q-M1 giữ một dòng sổ doanh thu mỗi hóa đơn gói ([front-desk](front-desk-and-packages.md)); Q-S7 không cần nút khóa / mở khóa tài khoản riêng ([ADR-0014](../../decisions/ADR-0014-former-staff-login.md)); "Đổi cả nhóm" giữ là lựa chọn mặc định ([appointments](appointments.md) §10.13).

**AC**
- AC15.1 Chủ spa + quản lý: "Xem" mở phiếu chỉ đọc cho mọi dòng đã chấm, cả tháng đã chốt và dòng chỉ chủ spa chấm; "Chấm lại" chỉ khi tháng mở và `can_record`.
- AC15.2 `GET /evaluations/history`: chủ spa + quản lý `200`; KTV + người đã nghỉ `403`; ẩn danh `401`; id spa khác / id lạ `404`; thiếu id / khoảng sai / quá 24 tháng `422`; tham số lạ `400`. Test `app/tests/evaluation_history.rs`, matrix +1 dòng.
- AC15.3 KTV đọc phiếu của chính mình (tiêu chí, nhận xét, người chấm) qua `/me/staff?month=`, không bao giờ của người khác.

**US15e — Lịch sử đánh giá của chính mình (EM 2026-11-08, [raw](../../../raw/2026-11-08-em-decision-own-evaluation-history.md): "làm nốt feature đó đi").** "Công và lương của tôi" thêm khối **"Lịch sử đánh giá"**: nút "Xem 12 tháng gần nhất" mở (tải lười) đúng biểu đồ + các dòng tháng của US15b (component chung `MonthsHistory`); "Xem phiếu" mở phiếu của chính mình (chỉ đọc); "Từng ngày" chuyển ô chọn tháng của trang sang tháng đó. Không "Chấm lại", không link nhật ký ghi chú.
- API **`GET /me/evaluations/history?from=&to=`** (`staff.read_own`, hoặc `staff.read_own_past` cho người đã nghỉ): người = hồ sơ gắn với tài khoản của session, không tham số chọn người (`?staff_id=` → `400`). Cùng code với US15b (`evaluation::history_range` + `history_months`), cùng quy tắc khoảng (12 tháng mặc định, ≤ 24, `422`). Response `{linked, left_on?, data?: {full_name, position, from, to, criteria, months}}` — như `/me/staff`, **không** có `can_record`, `staff_id`. Chưa gắn hồ sơ (vd. chủ spa) → `200 {linked: false}`, như `/me/staff`.
- **Người đã nghỉ (ADR-0014): không bao giờ bị từ chối cả khoảng.** `to` mặc định là tháng nghỉ việc và bị kẹp về tháng đó nếu xin muộn hơn; các tháng không làm (trước khi vào, khoảng nghỉ giữa hai đợt, sau khi nghỉ) đơn giản là không có trong `months`; khoảng nằm hẳn ngoài thời gian làm → `months: []`. Có `left_on`.
- AC15.4 KTV chỉ thấy tháng của mình; người đã nghỉ chỉ các tháng có làm (kẹp, không `403`); quản lý / chủ spa gọi được cho hồ sơ gắn với chính mình hoặc nhận `linked: false`; ẩn danh `401`; spa khác không bao giờ thấy người của spa này. Test `evaluation_history.rs` (+3), matrix +1 dòng (`StaffReadOwn`, mọi role, `.former_staff()`), vitest, e2e KTV (axe 390 tối).

## 16. Quản lý không quản lý chính mình + sửa lỗi test thật (EM 2026-11-08, [raw](../../../raw/2026-11-08-em-fix-all-and-manager-self-view.md))
EM: *"nếu đã là account quản lý thì đâu cần show trong mục nhân viên account của quản lý mà muốn xem thông tin thì show như của KTV ấy"*. Phát hiện từ [live test quản lý + KTV](../../../raw/2026-11-08-live-test-manager-technician.md) và [phần chủ spa](../../../raw/2026-11-08-live-test-owner.md).

**US16a — Quản lý không thấy chính mình trong "Nhân viên" (nay "Quản lý nhân sự") (server, entitled data).** Hồ sơ "của mình" = hồ sơ gắn với tài khoản đăng nhập (`staff.user_id = session.user`). Người không có `staff.manage` (quản lý) không nhận hồ sơ đó ở mọi sổ đọc: `GET /attendance`, `/daily-reviews`, `/evaluations` (dòng `staff`), `GET /adjustments` (mục của mình), `GET /staff/profiles`; `GET /staff/{id}` và `GET /evaluations/history?staff_id=` của chính mình → `404` (như id lạ). Một chỗ quyết định: `hr::scope::RecordScope::lists` (sổ) và `org::profile::hidden_from` (Hồ sơ nhân viên). Chủ spa vẫn thấy mọi người, kể cả chính mình. Quản lý khác vẫn hiện cho quản lý, chỉ đọc (quyết định C, như cũ). Không đổi quyền ghi: quyết định C vốn đã chặn tự ghi. Bảng lương (`payroll.read`) chỉ chủ spa nên không đổi.
- AC16.1 Quản lý: không có dòng của mình ở 4 tab + lịch sử; chủ spa có. Test `staff.rs::a_manager_reads_their_own_month_only_through_me`, `evaluation_history.rs::decision_c_shows_who_may_rescore_and_hides_the_managers_own` (thay test cũ "quản lý đọc lịch sử của mình": giờ là `404`).

**US16b — Quản lý xem công và lương của mình như KTV.** Không cần migration: role Quản lý vốn có `staff.read_own` từ 0017 (`/me/staff`, `/me/staff/day`, `/me/evaluations/history`, `/me/appointments`). Thêm màn **`/nhan-vien/cua-toi` "Công và lương của tôi"** (cùng component với KTV, `MyPayPage`, chunk lazy của Nhân viên) và mục nav cùng tên cho người giữ sổ nhân viên mà không có `staff.manage`; mục "Nhân viên" (nay "Quản lý nhân sự") của quản lý chỉ sáng ở `/nhan-vien` (`exact`). KTV giữ `/nhan-vien` như cũ. Quản lý cũng có **"Khách của tôi"** (`/khach-cua-toi`, **EM xác nhận 2026-11-08**, [raw](../../../raw/2026-11-08-em-decision-role-by-account.md): "quản lý cũng phải làm cho khách như KTV"): đúng ý "show như của KTV"; chưa gắn hồ sơ thì màn báo "Tài khoản chưa gắn với hồ sơ nhân viên". Chủ spa không có hai mục này.
- AC16.2 Lương của quản lý chỉ tới quản lý qua `/me/staff` (phiếu lương), không qua sổ nào (`/payroll` `403`, không key lương trong `/attendance`, `/evaluations`, `/staff/profiles`). Test như AC16.1; vitest "Công và lương của tôi for a manager", App shortcut quản lý.

**US16c — Quản lý chưa gắn hồ sơ (F19; thay 2026-11-08 theo [quyết định EM "vai trò theo tài khoản"](../../../raw/2026-11-08-em-decision-role-by-account.md)).** Chưa gắn thì app không biết hồ sơ nào là của quản lý: trước đây quản lý chưa gắn tự chấm "Đi làm" cho mình được. ~~#95: chức danh "Quản lý" luôn chỉ chủ spa ghi~~ — bỏ: **vai trò lấy từ role của tài khoản, không bao giờ từ chức danh.**
- Server: tài khoản có quyền ghi sổ nhân viên mà không có `staff.manage` (quản lý) và **chưa gắn hồ sơ** không ghi gì: mọi route ghi → `403 staff.link_required`, không ghi gì; chủ spa miễn. Sổ trả `link_required: true` và mọi `can_record: false`. Đã gắn: quyết định C như cũ (ghi cho KTV, không cho mình / quản lý khác). Hồ sơ chưa có tài khoản = KTV, chức danh gì cũng vậy.
- **Vị trí theo vai trò:** hồ sơ đã gắn tài khoản hiện tên role của tài khoản (Quản lý / Kỹ thuật viên / Chủ spa) ở mọi chỗ (Hồ sơ nhân viên, sổ, danh sách chọn KTV, phiếu lương), suy ra lúc đọc; ô "Vị trí" chỉ đọc ("Theo vai trò của tài khoản đăng nhập."). Chữ tự do chỉ cho hồ sơ chưa gắn, giữ nguyên để hiện lại khi bỏ gắn.
- UI: quản lý chưa gắn thấy ở Sổ hằng ngày / Bảng công tháng / Đánh giá tháng: "Tài khoản của bạn chưa gắn với hồ sơ nhân viên, nên bạn chỉ xem được. Nhờ chủ spa gắn…"; mọi nút ghi tắt. Chủ spa: cảnh báo "Tài khoản quản lý chưa gắn với hồ sơ" (từ role, `GET /staff/accounts` → `is_manager`; **§17 US17g thay**: nay là "Tài khoản đăng nhập chưa gắn với hồ sơ" cho mọi tài khoản không có `staff.manage` chưa gắn, mỗi dòng "Gắn vào hồ sơ") ở Sổ hằng ngày (nút "Mở Hồ sơ nhân viên") và tab "Hồ sơ nhân viên" (trước là "Nhân sự"); ở tab này thêm một dòng nhắc ai còn chưa có tài khoản (để mọi người có tài khoản).
- AC16.3 Test `staff.rs::a_manager_without_a_linked_profile_records_nothing`, `::a_linked_profile_shows_its_account_role`, unit `hr::scope::tests::an_unlinked_manager_records_nobody`, vitest (cảnh báo theo role, nhắc tài khoản, thông báo cho quản lý chưa gắn).

**Các phát hiện khác của phần nhân sự**
- **F20 — bỏ mã P / ĐX / K ở mọi màn:** trạng thái luôn là chữ ("Nghỉ có phép", "Nghỉ đột xuất", "Nghỉ không phép") hoặc icon + màu của ô sổ hằng ngày (check / lịch / cảnh báo / x) kèm chữ hoặc tên truy cập. Sửa: lựa chọn trong "Giờ & ghi chú", Bảng công tháng (ô = icon, cột "P/ĐX/K" → "Nghỉ" với icon + số, chú thích bằng chính các ô), lịch tháng + chú thích của KTV, ô "Nghỉ P / ĐX" + "Nghỉ K" → ba ô "Nghỉ có phép / đột xuất / không phép", chi tiết ngày, gợi ý trừ ngày nghỉ và quy tắc ở Bảng lương, cờ "Nghỉ không phép từ 5 ngày". `STATUS_CODE` bị xóa; một bộ `STATUS_LABEL` + `STATUS_ICON`.
- **F22 — lưới "Khách của tôi" mở đúng giờ:** giữ khoảng giờ rộng của #89 (lượt 02:00 vẫn hiện), nhưng mỗi lần đổi ngày / tuần / kiểu xem trang tự cuộn: hôm nay → một giờ trước bây giờ; ngày khác → giờ mở cửa, hoặc khối đầu tiên khi mọi khối đều trước giờ mở cửa (`focusMinute`). Trang vẫn cuộn một khối (không hộp cuộn lồng); không cuộn lại khi tải lại dữ liệu.
- **F24 — từ chối nghỉ việc vì còn lịch hẹn nêu rõ lịch nào:** `409 staff.has_upcoming_bookings` thêm `details.bookings` = `[{id, date, time (giờ spa), customer}]` (sớm nhất trước, tối đa 20, `more`), không SĐT / ghi chú. Kernel thêm biến thể `AppError::ConflictDetails` (Problem có `details`, additive). Sheet nghỉ việc liệt kê từng lịch, mỗi dòng mở `/quay?view=lich&ngay=…&ktv=…`.
- **F25 — "Ngày vào làm: 01/03/2025 → trống" sau khi nghỉ việc:** lỗi hiển thị nhật ký, **không mất dữ liệu** (đợt làm việc và `staff.joined_on` giữ nguyên). Audit `staff.end` ghi `joined_on` chỉ ở `before` làm ngữ cảnh; hàm so sánh coi key thiếu ở `after` là "đổi thành trống". Sửa gốc: key chỉ có ở `before` là ngữ cảnh, không phải thay đổi (`org::profile::changes`), nên cả các dòng nhật ký cũ cũng hiện đúng. Test unit + `staff.rs::the_roster_hires_ends_and_rehires_without_losing_history`.
- **F28 — người đã nghỉ:** chip tài khoản ghi "Đã nghỉ việc" thay vì vai trò cũ.

## 17. Tài khoản đăng nhập và mật khẩu (EM 2026-11-08, [raw](../../../raw/2026-11-08-em-request-accounts-and-passwords.md), [ADR-0016](../../decisions/ADR-0016-accounts-and-passwords.md))
EM: *"giả sử 1 spa mới chưa có data chỉ có mỗi account của chủ spa thì làm sao để tạo mới cho quản lý và nhân viên … chưa cho phép đổi mật khẩu"*; theo sau: bỏ bước "Gắn tài khoản" riêng; "ok cách A đi" cho quên mật khẩu.

**US17a — Chủ spa tạo tài khoản từ hồ sơ.** Hồ sơ nhân viên → sheet → khối "Tài khoản đăng nhập": chưa có tài khoản → **"Tạo tài khoản đăng nhập"**: SĐT đăng nhập (di động VN) + vai trò (Quản lý / Kỹ thuật viên). Cũng có trong form "Thêm nhân viên" (tùy chọn): tạo hồ sơ xong tạo tài khoản ngay.
- `POST /staff/{id}/login` `{phone, role: manager|technician}` (`staff.manage`, re-auth trong 5 phút): một transaction tạo tài khoản (tên = tên hồ sơ), membership + role, gắn vào hồ sơ. Trả hồ sơ + `temporary_password` **một lần** (`no-store`).
- Lỗi: SĐT sai `422 phone invalid_format`; SĐT đã có tài khoản (ở spa này hay spa khác) `409 account.phone_taken`; hồ sơ đã có tài khoản `409 staff.account_exists`; hồ sơ đã nghỉ `409 staff.inactive`; chưa re-auth `401 auth.reauth_required`.
- Vị trí của hồ sơ = tên vai trò (ADR-0015 update).

**US17b — Mật khẩu tạm và bắt đổi.** Màn hiện mật khẩu tạm một lần, nút "Sao chép", ô tick "Tôi đã đưa mật khẩu cho nhân viên" rồi mới đóng. Tài khoản `must_change_password`: đăng nhập trả `next: password_change`; màn duy nhất là "Đặt mật khẩu mới" (mật khẩu tạm + mật khẩu mới ×2); server trả `403 auth.password_change_required` cho mọi route khác ngoài `POST /auth/password` và đăng xuất.

**US17c — Đổi mật khẩu (mọi người, của chính mình).** Cài đặt → "Đổi mật khẩu": mật khẩu hiện tại + mới ×2. `POST /auth/password` `{current_password, new_password}`. Chính sách: 10–128 ký tự, không nằm trong danh sách mật khẩu phổ biến (ADR-0019 D18), khác mật khẩu hiện tại, không phải SĐT đăng nhập (`422 new_password too_short|too_long|too_common|same_as_current|is_phone`; web: "Mật khẩu này quá phổ biến, dễ bị đoán…"). Mật khẩu tạm do hệ thống sinh không qua bước này theo nghĩa thực tế: nó ngẫu nhiên nên không bao giờ trùng danh sách. Sai mật khẩu hiện tại `401 auth.invalid_credentials` (đếm vào khóa tài khoản). Thành công: đăng xuất mọi thiết bị khác, thiết bị này nhận session mới.

**US17d — Chủ spa đặt lại mật khẩu.** Khối tài khoản → "Đặt lại mật khẩu" (xác nhận) → mật khẩu tạm mới một lần, bắt đổi, đăng xuất mọi thiết bị của người đó. `POST /staff/{id}/login/reset-password`. Không cho: chính mình / tài khoản chủ spa (`422 account.owner_protected`), tài khoản thuộc cả spa khác (`422 account.other_spa`), tài khoản bị khóa khi cho nghỉ (`409 account.locked` — "Nhận lại làm" mở khóa).

**US17e — Chủ spa đổi vai trò.** "Đổi vai trò" Quản lý ↔ Kỹ thuật viên: `PUT /staff/{id}/login/role` `{role}`; quyền mới ở request kế tiếp. Không đổi chính mình / chủ spa (`422 account.owner_protected`).

**US17f — Quên mật khẩu (cách A).** Màn đăng nhập → "Quên mật khẩu?" → SĐT → "Gửi yêu cầu" → luôn cùng một câu "Nếu số này có tài khoản, chủ spa sẽ thấy yêu cầu và đặt lại mật khẩu cho bạn." `POST /auth/password-reset-requests` `{phone}` → `202` như nhau dù SĐT có hay không. Ghi yêu cầu cho mỗi spa mà tài khoản là thành viên đang hoạt động không có `staff.manage`; một yêu cầu đang chờ mỗi spa; hết hạn 24 giờ. Chủ spa thấy ở Tổng quan và Quản lý nhân sự → Hồ sơ nhân viên: "Nguyễn Thị Lan xin đặt lại mật khẩu · 14:05" → "Đặt lại" (như US17d) / "Bỏ qua". `GET /staff/password-reset-requests`, `POST /staff/password-reset-requests/{id}/dismiss` (`staff.manage`).

**US17g — Dữ liệu cũ.** Tài khoản tạo bằng CLI chưa gắn hồ sơ: chủ spa thấy một thông báo (chỉ khi có) liệt kê các tài khoản đó, mỗi dòng "Gắn vào hồ sơ" (chọn hồ sơ chưa có tài khoản) — `PUT /staff/{id}/account` giữ cho lối đi này. Luồng chính không còn dropdown gắn / "Bỏ gắn".

**Bảo mật:** audit mọi hành động (không bao giờ có mật khẩu); re-auth cho tạo / đặt lại / đổi vai trò; giới hạn tần suất `/auth/*` theo IP + khóa theo tài khoản; tách spa (id hồ sơ spa khác `404`); người đã nghỉ bị khóa không đặt lại được.

**AC**
- AC17.1 Chủ spa tạo được tài khoản; quản lý / KTV `403`; spa khác `404`; mật khẩu tạm chỉ trong response tạo / đặt lại, không trong audit / log / response nào khác.
- AC17.2 Tài khoản mới đăng nhập → chỉ đổi mật khẩu được; đổi xong dùng bình thường; session cũ bị thu hồi.
- AC17.3 Đặt lại: session của người đó bị thu hồi; không đặt lại được chủ spa / chính mình / tài khoản spa khác / tài khoản bị khóa.
- AC17.4 Đổi vai trò có hiệu lực ở request kế tiếp.
- AC17.5 Quên mật khẩu: cùng response cho SĐT có / không; giới hạn tần suất; chỉ chủ spa thấy và xử lý; spa khác không thấy.
