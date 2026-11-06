---
title: "Spec: Quản lý nhân viên (chấm công, đánh giá, thưởng/phạt, bảng lương)"
status: draft
updated: 2026-11-06
sources: [raw/2026-11-06-em-review-round5.md, raw/2026-11-05-em-review-round4.md, raw/2026-11-04-idea-internal-platform.md]
related: [../compensation-rules.md, ../personas.md, ../../architecture/backend.md, ../../architecture/data.md, ../../security/authz-matrix.md]
---

# Spec: Quản lý nhân viên

**Status: draft.** Backend, UI và dữ liệu demo đã build theo các mặc định dưới đây (PR `feat/staff-management`). Các câu hỏi cho EM nằm ở §9. Mỗi chỗ mâu thuẫn trong [compensation-rules](../compensation-rules.md) có một mặc định, và mặc định đó chỉnh được theo từng spa (§6).

## 1. Goal
EM (vòng 4, mục 2): "làm feature quản lý nhân viên, tính lương tính thưởng, phạt rồi đánh giá từng ngày như trong cái sheet". Spec này thay file Excel *Bảng chấm công đánh giá tháng* (mỗi người, mỗi tháng một sheet) bằng các màn sau:
- **Chấm công hằng ngày**: lưới tháng × nhân viên, mã ✓/P/ĐX/K như sheet, tự tính phút đi muộn.
- **Đánh giá hằng ngày**: khen, phàn nàn, ghi chú (đúng các cột của sheet). **Đánh giá tháng** theo 6 tiêu chí, thang 100 điểm.
- **Sổ thưởng/phạt**: lý do, số tiền và/hoặc điểm, ngày, người ghi.
- **Bảng lương tháng**: lương cứng, trừ ngày nghỉ, hoa hồng tính từ lượt khách đã ghi, thưởng, phạt, thực lĩnh. Có phiếu lương từng người, và chốt tháng thì khóa sổ.

Thước đo: quản lý không phải cộng tay lương và hoa hồng ([personas](../personas.md) #1). KTV tự kiểm tra được từng khoản trên phiếu lương của mình (persona #3).

## 2. Personas & quyền
| Permission (mới, migration 0017) | Chủ spa | Quản lý | KTV | Dùng cho |
|---|---|---|---|---|
| `staff.manage` | ✓ | — | — | Hồ sơ nhân viên, liên kết tài khoản đăng nhập |
| `attendance.record` | ✓ | ✓ | — | Xem và ghi lưới chấm công |
| `evaluation.record` | ✓ | ✓ | — | Đánh giá ngày, đánh giá tháng, sổ thưởng/phạt |
| `payroll.read` | ✓ | — (**Q-S1**) | — | Mọi con số lương: điều kiện lương, bảng lương, phiếu lương, quy tắc |
| `payroll.close` | ✓ | — | — | Chốt tháng, sửa quy tắc lương |
| `staff.read_own` | ✓ | ✓ | ✓ | `GET /me/staff`: tháng **của chính mình** |

- Chưa đăng nhập → `401`. Thiếu quyền → `403 auth.forbidden`. ID của spa khác → `404` (đường dẫn) hoặc `422 …staff_id unknown` (body).
- **Lương không bao giờ được gửi cho người thiếu `payroll.read`.** Response không chứa key `pay`, không chứa `null` rồi giấu ở UI (non-negotiable 4). Ghi điều kiện lương cần cả `staff.manage` và `payroll.read`.
- KTV chỉ thấy tháng của mình. Server lấy staff id từ user của session qua `staff.user_id`, không bao giờ lấy từ request (non-negotiable 3).

## 3. User stories & acceptance criteria
Áp dụng chung cho mọi route: đúng như G1–G3 của [spec R1-1](front-desk-and-packages.md). Mọi response có `Cache-Control: no-store`. Mọi thao tác ghi đều để lại `audit_log` có `actor_user_id`. Tham số `?month=YYYY-MM` mặc định là tháng hiện tại theo giờ spa; tháng sai định dạng → `422`.

### US1: Hồ sơ nhân viên (chủ spa)
*Là chủ spa, tôi muốn ghi vị trí, ngày vào làm, lương cứng, tỷ lệ hoa hồng và trạng thái của từng người, để bảng lương tự tính.*
- AC1.1 `POST /staff`, `PUT /staff/{id}` (`If-Match`) với `full_name`, `position`, `phone?`, `status` (`active`/`left`), `joined_on?`, `left_on?` (bắt buộc khi `left`, không được trước `joined_on`), `user_id?`, `shift_start?` (`HH:MM`). Sai → `422` theo từng field. Phiên bản cũ → `412`.
- AC1.2 `user_id` phải là thành viên đang hoạt động của spa, chưa liên kết với ai khác. Đã liên kết → `409 staff.account_taken`. `GET /staff/accounts` liệt kê các tài khoản có thể liên kết.
- AC1.3 `PUT /staff/{id}/pay`: `base_salary` 0–1 tỷ, `commission_rate_bp` 0–10000 (500 = 5 %: Lựa chọn 1; 700: LC2; 1000: LC3), `loyalty_bonus_monthly`, `meal_allowance`. Lần sửa áp dụng cho mọi tháng **chưa chốt**; tháng đã chốt giữ nguyên số đã dùng (test `a_closed_month_is_locked…`).
- AC1.4 Có `staff.manage` nhưng thiếu `payroll.read` (role tùy biến) → hồ sơ trả về **không có** key `pay`, `pay_visible: false`; `PUT …/pay` → `403` (test `salary_is_only_sent_to_payroll_readers`, đã mutation-check).
- AC1.5 Không xóa nhân viên. Nghỉ việc = `left` + `left_on`. Người đã nghỉ vẫn có trong các tháng họ còn làm.

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
  - Mỗi tiêu chí chấm theo mức: `m1` 100 %, `m2` 80 %, `m3` 60 %, `fail` 30 %, **làm tròn xuống**.
  - Riêng `fail` được nhập điểm thấp hơn trần 30 %. Nhập cao hơn trần → `422 above_ceiling`. Nhập điểm cho mức khác `fail` → `422 only_for_fail`.
- AC3b.2 Kết quả: ≥ 85 → Mức 1; 70–84 → Mức 2; 50–69 → Mức 3; < 50 → Chưa đạt. `serious_issue` giới hạn kết quả **tối đa Mức 3**, không bao giờ làm kết quả tốt lên. Kết quả Mức 3 hoặc Chưa đạt mà không có `coaching_plan` → `422 coaching_plan required`.
- AC3b.3 Tháng sau tháng hiện tại → `422`. Tháng đã chốt → `409`. Ghi lại thì chấm lại (upsert, `version` tăng, có audit).
- AC3b.4 `GET /evaluations?month=` trả về tiêu chí, phiếu của từng người (hoặc `null`), cùng tổng khen/phàn nàn và tổng chấm công của tháng làm ngữ cảnh.

### US4: Thưởng / phạt
- AC4.1 `POST /adjustments` (`Idempotency-Key`): `staff_id`, `kind` (`bonus`/`penalty`), `amount?` (1–100tr), `points?` (1–100), cần ít nhất một trong hai. `reason` bắt buộc. `occurred_on` mặc định hôm nay, không được ở tương lai. Người ghi lấy từ session.
- AC4.2 Chỉ thêm, không sửa hay xóa (trigger `reject_mutation`). Ghi sai thì ghi một dòng ngược loại để bù.
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
  - `thực lĩnh = lương cứng − trừ nghỉ − trừ muộn + hoa hồng làm + hoa hồng chốt + tiền ăn + thưởng − phạt`.
  - **Ghi nhận, không cộng vào thực lĩnh**: Thưởng gắn bó của tháng (trả ở mốc 12/24) và mức Tết của tháng (mức thấp hơn giữa mức theo ngày nghỉ và mức theo đánh giá; số tiền theo `tet_level_amounts`).
- AC5.3 Cờ trên phiếu lương:
  - `no_pay_terms`: chưa có điều kiện lương, mọi khoản bằng 0;
  - `unrecorded_days`: có ngày làm việc chưa chấm công;
  - `absent_k5_plus`: K ≥ 5 ngày trong tháng (Nhóm A #8). Hệ thống chỉ gắn cờ, chủ spa quyết định;
  - `evaluation_missing`.
- AC5.4 `POST /payroll/close {month, allow_incomplete?}` (`payroll.close`, `Idempotency-Key`):
  - tháng chưa kết thúc → `422 payroll.month_not_ended`;
  - còn ngày chưa chấm mà `allow_incomplete` không bật → `422 payroll.unrecorded_days`;
  - đã chốt → `409 payroll.period_closed`; gửi lại cùng key thì trả lại response cũ.
  Khi chốt, mọi phiếu lương được lưu thành bản chụp và tháng bị khóa: chấm công, đánh giá ngày, đánh giá tháng, thưởng/phạt của tháng đó đều → `409`. Khóa có ở cả service (row lock `FOR SHARE`/`FOR UPDATE` trên `payroll_period`) và DB (trigger `reject_closed_period`). Tháng đã chốt **không bao giờ mở lại** (trigger `payroll_period_forward_only`).
- AC5.5 `GET /payroll/payslips/{staff_id}?month=` trả về phiếu lương một người, gồm hoa hồng theo từng dịch vụ. Nhân viên của spa khác → `404`.
- AC5.6 Lượt khách bị hủy (void) **sau khi** đã chốt tháng không làm đổi phiếu đã chốt. Chủ spa ghi một khoản điều chỉnh vào tháng đang mở.

### US6: KTV xem tháng của mình
- AC6.1 `GET /me/staff?month=` → `{linked: false}` nếu tài khoản chưa liên kết. Nếu đã liên kết thì trả về chấm công, đánh giá ngày, đánh giá tháng, thưởng/phạt và phiếu lương **của chính người đó**. Phiếu lương có khi đã có điều kiện lương; tháng chưa chốt thì là tạm tính.
- AC6.2 Không có tham số nào chọn được người khác (`?staff_id=` → `400`). Mọi route quản lý khác → `403` (test `a_technician_sees_only_their_own_month`).

## 4. UX
> **Vòng 5 thay mục này** cho quản lý: xem §10 (sổ hằng ngày một trang, bảng công tháng dạng lưới, đánh giá tháng nhắc khi đến hạn). Phần dưới giữ để đối chiếu.

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
| 5 | Tiền ăn 300–600k, hỗ trợ học viên tháng 2 | Nhập tay theo từng người (`meal_allowance`, `base_salary`) | ✓ (dữ liệu theo người) | Q-C5 |
| 6 | Ngày lễ / Chủ nhật | Không xử lý riêng: mọi ngày như nhau, mẫu số lương ngày vẫn là số ngày − 2 | Chưa (chưa có quy tắc để cấu hình) | Q-C6 |
| 7 | Vào làm giữa tháng: Thưởng gắn bó tính thế nào | Theo tỷ lệ số ngày làm trong tháng; ngày trước khi vào làm không bị tính là nghỉ | ✓ `loyalty_partial_month` = `prorate`/`full`/`none_if_partial` | Q-C7 |
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
- **Q-S4**: Người vào làm hoặc nghỉ giữa tháng: lương cứng có tính theo tỷ lệ ngày không? Hiện tại là **không** (giống Excel); ngày trước khi vào làm chỉ không bị tính là nghỉ.
- **Q-C1 … Q-C8**: các dòng tương ứng ở §6 (giữ mặc định hay chọn cách khác).

## 10. Vòng 5 (EM 2026-11-06, [raw](../../../raw/2026-11-06-em-review-round5.md), mục 9–11)
EM: ba tab Chấm công / Đánh giá / Thưởng-phạt làm người dùng không biết phải điền đủ ba chỗ; ngày công dạng danh sách 30 dòng khó xem; tài khoản KTV thiếu lịch sử "đã làm cho ai, ai đặt lịch mình".

**Cách làm thực tế (nghiên cứu, 2026-11-06).** Spa nhỏ không có máy chấm công: quản lý điểm danh một lần đầu ca, ghi sự việc trong ngày (khen, phàn nàn, đi muộn, phạt) vào sổ, cuối tháng mới cộng và chấm điểm. Các app chấm công cho tiệm nhỏ đều xoay quanh **một màn trong ngày** (ai đã vào ca, ai chưa) và **một báo cáo tháng dạng lưới** (nhân viên theo hàng, ngày theo cột, như mẫu bảng chấm công VN), còn việc cuối tháng là một bước riêng có nhắc ([ClockIt cho salon/spa](https://clockit.io/use-cases/time-clock-for-salons-and-spas/), [Everhour Shifts](https://everhour.com/shifts/salon-spa-scheduling-software), [MISA: lập bảng chấm công](https://helpact.misa.vn/kb/lap-bang-cham-cong/), [luatvietnam: mẫu bảng chấm công](https://luatvietnam.vn/bieu-mau/mau-bang-cham-cong-571-19464-article.html)). Sheet Excel của spa cũng đúng như vậy (cột ngày + tổng cuối hàng).

Không đổi API hay quy tắc dữ liệu cho mục 9–10 (khóa tháng, quyền giữ nguyên). Mục 11 thêm một route chỉ đọc.

### US7: Sổ hằng ngày một trang (quản lý)
*Là quản lý, tôi muốn mở một trang cho một ngày, đi lần lượt từng người và ghi hết mọi thứ của người đó ngay tại chỗ, để không phải nhớ còn tab nào chưa điền.*
- AC7.1 `/nhan-vien` mở tab **"Sổ hằng ngày"** (mặc định). Mỗi người có làm trong ngày là một thẻ: chấm công ✓/P/ĐX/K (một chạm là lưu), bộ đếm Khen / Phàn nàn, các khoản thưởng/phạt đã ghi ngày đó, nút "+ Thưởng/phạt" (form mở sẵn đúng người, đúng ngày) và nút "Chi tiết" (giờ đến/về, ghi chú chấm công, ghi chú đánh giá).
- AC7.2 Đầu trang có **thanh tiến độ** "Đã chấm công x/y người" và danh sách "Còn thiếu: …" (bấm tên → cuộn tới thẻ). Chỉ chấm công là bắt buộc; khen/phàn nàn/thưởng/phạt là tùy chọn và trang nói rõ điều đó. Chấm đủ → "Xong ngày này".
- AC7.3 Có ngày đã qua trong tháng còn người chưa chấm → banner "Còn n ngày chưa chấm đủ" với các ngày bấm được (mở đúng ngày đó). Ngày tương lai không chấm được.
- AC7.4 **Đánh giá tháng chỉ nhắc khi đến hạn**: từ ngày cuối tháng đến ngày 10 tháng sau, nếu tháng trước (hoặc tháng này vào ngày cuối) chưa chốt và còn người chưa có phiếu → banner "Đến hạn đánh giá tháng m: còn n người" + nút mở tab "Đánh giá tháng" đúng tháng đó. Ngoài thời gian này không có banner. *(Lead's decision — EM to review: cửa sổ nhắc = ngày cuối tháng → ngày 10 tháng sau; spec gốc nói ngày 1–5.)*
- AC7.5 Tab còn lại: "Bảng công tháng" (US8), "Đánh giá tháng" (phiếu 100 điểm như AC3b, có dấu đếm số người còn thiếu khi đến hạn), "Bảng lương" và "Hồ sơ" (như cũ, theo quyền). Bỏ tab "Thưởng, phạt" riêng: sổ thưởng/phạt của tháng nằm cuối "Bảng công tháng". Link cũ `?tab=attendance|evaluation|adjustments` mở tab mới tương ứng (không 404). *(Lead's decision — EM to review.)*
- AC7.6 Âm: tháng đã chốt → mọi nút ghi bị tắt + banner (server vẫn `409`); quyền: thiếu `attendance.record` thì không có nút chấm công, thiếu `evaluation.record` thì không có khen/phàn nàn/thưởng/phạt (server vẫn `403`); lỗi mạng khi lưu → thông báo lỗi tại trang, trạng thái thẻ không đổi (dữ liệu lấy lại từ server).

### US8: Bảng công tháng dạng lưới
*Là quản lý, tôi muốn xem cả tháng trong một bảng (người × ngày), để nhìn là thấy ai nghỉ, ai muộn.*
- AC8.1 Lưới: hàng = nhân viên, cột = ngày (thứ, CN tô màu), ô = mã ✓/P/ĐX/K (muộn có gạch đỏ, chưa chấm "–", ngoài thời gian làm / tương lai "·"), cột tổng cuối hàng (công, muộn, P/ĐX/K, khen/phàn nàn). Cột tên dính bên trái khi cuộn ngang; **dùng được ở 390 px** (không còn chỉ desktop). Bấm ô (tháng chưa chốt) → sheet sửa ô đó.
- AC8.2 KTV ("Công và lương của tôi"): **lịch tháng thu gọn** (7 cột T2…CN, ô = ngày + mã) thay cho danh sách, kèm ô tổng (đi làm, muộn, P, ĐX, K, khen, phàn nàn). Không có thanh cuộn ngang ở 390 px.
- AC8.3 Âm: tháng chưa có người nào → trạng thái trống; lỗi tải → trạng thái lỗi có nút thử lại.

### US9: KTV xem khách đã làm và lịch hẹn của mình
*Là KTV, tôi muốn xem mình đã làm cho khách nào và ai đã đặt lịch với mình, để chuẩn bị và tự đối chiếu hoa hồng.*
- AC9.1 `GET /me/work?month=YYYY-MM` (`staff.read_own`, `Cache-Control: no-store`). Staff lấy từ `staff.user_id = session.user` (như `/me/staff`); tài khoản chưa gắn → `200 {linked: false}`.
- AC9.2 `served[]`: các lượt **completed** của tháng có ít nhất một dòng dịch vụ do chính người đó làm; mỗi lượt: `visit_id`, `business_date`, `check_in_at`, `customer_name`, `services[]` = **chỉ các dòng của người đó** (tên dịch vụ, số lượng, `source` lẻ/gói). Mới nhất trước. Lượt đã hủy không có.
- AC9.3 `upcoming[]`: lịch hẹn `booked`/`arrived` gán cho người đó, từ hôm nay tới 30 ngày sau, sớm nhất trước: `appointment_id`, `starts_at`, `ends_at`, `status`, `customer_name` (tên hồ sơ hoặc tên khách mới), `services[]` (tên).
- AC9.4 **Chỉ các trường trên.** Không có giá, doanh thu, giá trị dòng, SĐT, ghi chú buổi, ghi chú sức khỏe, ghi chú lịch hẹn, id khách (test kiểm không có key nào khác). KTV vẫn không có `customer.read`/`visit.read`/`appointment.manage`. *(Lead's decision — EM to review: KTV thấy tên khách mình phục vụ, không thấy SĐT; tên dịch vụ đã có trên phiếu lương.)*
- AC9.5 Âm: chưa đăng nhập → `401`; role không có `staff.read_own` → `403`; tham số lạ (`?staff_id=`) → `400`; tháng sai → `422`; KTV A không bao giờ thấy lượt/lịch của KTV B, kể cả lượt có cả hai cùng làm (A chỉ thấy dòng của A); spa khác không thấy gì (lọc tenant + RLS). Quản lý/chủ spa: các route quản lý không đổi.
- AC9.6 UI: mục nav **"Khách của tôi"** (`/khach-cua-toi`, tab bar "Khách") cho người có `staff.read_own` mà không có `visit.read` (quản lý/chủ spa đã có màn quầy). Hai phần: "Lịch hẹn sắp tới" (nhóm theo ngày) và "Khách đã làm" của tháng (chọn tháng, nhóm theo ngày). Trạng thái: đang tải, trống, lỗi, chưa gắn hồ sơ.
