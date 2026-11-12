/**
 * Words of "Nhật ký thao tác" (ADR-0021): the server sends structured entries, this file turns each
 * one into a plain Vietnamese sentence ("Quản lý Hà hủy lượt của chị Lê Thị Mai"), its money and
 * its changed values. `PHRASES` is a `Record<AuditAction, …>`: an action the API adds fails the
 * typecheck until it has words here; a key the contract does not list (a legacy row) reads as
 * "làm một thao tác khác".
 */
import type { ActivityChange, ActivityEntry, ActivityKind, ActivitySubject, AuditAction } from "@vpyd/api-client";
import { formatVnd } from "@vpyd/ui";
import { spaDateTime, spaTime } from "../../lib/format";

export const KIND_LABEL: Record<ActivityKind, string> = {
  front_desk: "Quầy",
  packages: "Gói liệu trình",
  appointments: "Lịch hẹn",
  customers: "Khách hàng",
  staff: "Nhân sự",
  payroll: "Lương",
  settings: "Danh mục & cài đặt",
  accounts: "Tài khoản & bảo mật",
  other: "Khác",
};

/** Who did it: a login's name, the platform team, or nobody signed in. */
export function actorName(e: Pick<ActivityEntry, "actor">): string {
  if (e.actor.kind === "platform") return "Đội vận hành";
  if (e.actor.kind === "system") return "Hệ thống";
  const name = e.actor.name ?? "Một tài khoản đã xóa";
  // Same rule as the person filter (ADR-0022 edge case 6).
  return e.actor.left ? `${name} (đã nghỉ)` : name;
}

const dmy = (iso: string) => iso.split("-").reverse().join("/");

/** The subject as said in a sentence: "chị Lê Thị Mai", "KTV Lan", a neutral label when gone. */
export function subjectName(s: ActivitySubject | null | undefined): string {
  if (!s) return "";
  switch (s.kind) {
    case "customer": {
      if (!s.name) return "một khách (đã xóa thông tin)";
      if (s.name === "Khách vãng lai") return "khách vãng lai";
      const title = s.gender === "female" ? "chị" : s.gender === "male" ? "anh" : "khách";
      return `${title} ${s.name}`;
    }
    case "staff":
      return s.name ?? "một nhân viên đã xóa";
    case "account":
      return s.name ? `tài khoản ${s.name}` : "một tài khoản";
    case "role":
      return `vai trò ${ROLE[s.name ?? ""] ?? s.name ?? ""}`.trim();
    case "spa":
      return "spa";
    default:
      return s.name ? `“${s.name}”` : "(đã xóa)";
  }
}

/** One sentence: `lead`, then the subject (unless `about` is false), then `tail`. */
export interface Phrase {
  lead: string;
  about?: boolean;
  tail?: string | undefined;
}

type Words = (e: ActivityEntry) => Phrase;

const pkg = (e: ActivityEntry) => (e.subject?.package ? ` ${e.subject.package}` : "");
/** The packages the same action sold or cancelled (folded into its line by the server): " A, B". */
const sold = (e: ActivityEntry) => (e.packages.length > 0 ? ` ${e.packages.join(", ")}` : "");
/** A checkout that also sold packages: "… kèm bán gói A". */
const withPackages = (e: ActivityEntry) => (e.packages.length > 0 ? `kèm bán gói${sold(e)}` : undefined);
/** The same, glued to a history verb: "ghi lượt kèm bán gói A"; "" without packages. */
const withSold = (e: ActivityEntry) => (e.packages.length > 0 ? ` kèm bán gói${sold(e)}` : "");
const month = (e: ActivityEntry) => (e.detail ? ` tháng ${e.detail.slice(5, 7)}/${e.detail.slice(0, 4)}` : "");
const flagTo = (e: ActivityEntry, field: string) => e.changes.find((c) => c.field === field)?.new;

const PHRASES: Record<AuditAction, Words> = {
  // Quầy. A visit's line carries its sales and the packages its checkout sold (one line per
  // action, EM 2026-11-11); a sale reads on its own only when no visit action wrote it.
  "visit.create": (e) => ({
    lead: e.detail === "backdated" ? "nhập bù lượt cho" : "ghi lượt cho",
    tail: withPackages(e),
  }),
  "visit.addition": (e) => ({ lead: "thêm vào lượt của", tail: withPackages(e) }),
  "visit.update": () => ({ lead: "sửa lượt của" }),
  "visit.correct": (e) => ({ lead: "sửa lượt (tính lại tiền) của", tail: withPackages(e) }),
  "visit.void": () => ({ lead: "hủy lượt của" }),
  "visit.convert_to_package": (e) => ({ lead: `chuyển lượt sang gói${sold(e)} cho` }),
  "sale.create": (e) => ({ lead: e.packages.length > 0 ? `bán gói${sold(e)} cho` : "thu tiền của" }),
  "sale.void": (e) => ({ lead: e.packages.length > 0 ? `hủy hóa đơn bán gói${sold(e)} của` : "hủy hóa đơn của" }),
  "sale.note_update": () => ({ lead: "sửa ghi chú gói của" }),
  // Gói liệu trình
  "customer_package.create": (e) => ({ lead: `bán gói${pkg(e)} cho` }),
  "customer_package.extend": (e) => ({ lead: `gia hạn gói${pkg(e)} của` }),
  "customer_package.cancel": (e) => ({ lead: `hủy gói${pkg(e)} của` }),
  "customer_package.use_expired_override": (e) => ({ lead: "cho", tail: `dùng gói${pkg(e)} đã hết hạn` }),
  "opening_balance.import": () => ({ lead: "nhập gói từ sổ giấy cho" }),
  "sale.paper_book": () => ({ lead: "nhập tiền từ sổ giấy cho" }),
  // Lịch hẹn
  "appointment.create": () => ({ lead: "đặt lịch hẹn cho" }),
  "appointment.update": () => ({ lead: "đổi lịch hẹn của" }),
  "appointment.arrive": () => ({ lead: "đánh dấu", tail: "đã đến" }),
  "appointment.undo_arrival": () => ({ lead: "bỏ đánh dấu đã đến của" }),
  "appointment.complete": () => ({ lead: "hoàn tất lịch hẹn của" }),
  "appointment.no_show": () => ({ lead: "đánh dấu", tail: "không đến" }),
  "appointment.cancel": () => ({ lead: "hủy lịch hẹn của" }),
  "appointment.settings": () => ({ lead: "sửa cài đặt lịch hẹn", about: false }),
  // Khách hàng
  "customer.create": () => ({ lead: "thêm khách" }),
  "customer.update": () => ({ lead: "sửa hồ sơ của" }),
  "customer.health_note_set": () => ({ lead: "sửa lưu ý sức khỏe của" }),
  "customer.follow_up": (e) => ({ lead: "gọi chăm sóc", tail: OUTCOME[e.detail ?? ""] ?? "" }),
  // Nhân sự
  "attendance.record": (e) => ({ lead: e.detail === "cleared" ? "xóa ô chấm công của" : "chấm công cho" }),
  "evaluation.daily_review": () => ({ lead: "ghi khen / phàn nàn cho" }),
  "evaluation.record": (e) => ({ lead: `đánh giá${month(e)} cho` }),
  "staff.adjustment": (e) => ({ lead: `ghi ${e.detail === "penalty" ? "phạt" : "thưởng"} cho` }),
  "staff.adjustment_void": () => ({ lead: "hủy một khoản thưởng / phạt của" }),
  "staff.create": () => ({ lead: "thêm nhân viên" }),
  "staff.update": () => ({ lead: "sửa hồ sơ nhân viên" }),
  "staff.end": () => ({ lead: "cho", tail: "nghỉ việc" }),
  "staff.rehire": () => ({ lead: "nhận lại" }),
  // Lương
  "staff.pay_update": () => ({ lead: "sửa lương của" }),
  "payroll.rules_update": () => ({ lead: "sửa quy tắc lương", about: false }),
  "payroll.close": (e) => ({ lead: `chốt lương${month(e)}`, about: false }),
  // Danh mục & cài đặt
  "catalog.category_create": () => ({ lead: "thêm nhóm dịch vụ" }),
  "catalog.category_update": () => ({ lead: "sửa nhóm dịch vụ" }),
  "catalog.service_create": () => ({ lead: "thêm dịch vụ" }),
  "catalog.service_update": () => ({ lead: "sửa dịch vụ" }),
  "catalog.plan_create": () => ({ lead: "thêm mẫu gói" }),
  "catalog.plan_update": () => ({ lead: "sửa mẫu gói" }),
  // Tài khoản & bảo mật
  "staff.account_link": () => ({ lead: "gắn tài khoản đăng nhập cho" }),
  "staff.login_create": (e) => ({ lead: "tạo tài khoản đăng nhập cho", tail: e.detail ? `(${ROLE[e.detail] ?? e.detail})` : "" }),
  "staff.login_password_reset": () => ({ lead: "đặt lại mật khẩu cho" }),
  "staff.login_role": () => ({ lead: "đổi vai trò của" }),
  "account.password_change": () => ({ lead: "đổi mật khẩu của mình", about: false }),
  "account.password_reset_requested": () => ({ lead: "nhận yêu cầu quên mật khẩu của" }),
  "account.password_reset_dismissed": () => ({ lead: "bỏ qua yêu cầu quên mật khẩu của" }),
  "account.locked_by_platform": () => ({ lead: "khóa" }),
  "account.unlocked_by_platform": () => ({ lead: "mở khóa" }),
  "account.mfa_reset_by_platform": () => ({ lead: "đặt lại xác thực 2 bước của" }),
  "account.sessions_revoked_by_platform": () => ({ lead: "đăng xuất mọi thiết bị của" }),
  "account.password_reset_by_platform": () => ({ lead: "đặt lại mật khẩu của" }),
  "role.permissions_changed": () => ({ lead: "sửa quyền của" }),
  "role.permissions_reset": () => ({ lead: "đưa về mặc định quyền của" }),
  "feature.changed": (e) => ({
    lead: `${flagTo(e, "enabled") === "false" ? "tắt" : "bật"} tính năng ${FEATURE[e.detail ?? ""] ?? e.detail ?? ""}`.trim(),
    about: false,
  }),
  "tenant.created": () => ({ lead: "tạo spa", about: false }),
  "tenant.suspended": () => ({ lead: "tạm ngưng spa", about: false }),
  "tenant.unsuspended": () => ({ lead: "mở lại spa", about: false }),
  "tenant.owner_mfa_changed": (e) => ({
    lead: `${flagTo(e, "owner_mfa_required") === "false" ? "tắt" : "bật"} bắt buộc xác thực 2 bước cho chủ spa`,
    about: false,
  }),
};

/**
 * The verb of a line in one record's own history (ADR-0022, design record-history §1.5): the record
 * is already on screen, so a line says who and what ("Hà ghi lượt") and never names the subject
 * again. `null` = the action has no history on a record (spec record-history §4.2), so the server
 * never sends it there. A `Record<AuditAction, …>` like `PHRASES`: an action the API adds fails
 * the typecheck until it is placed here.
 */
export const HISTORY: Record<AuditAction, ((e: ActivityEntry) => string) | null> = {
  // Quầy: a visit's line carries the sales and packages of its checkout (ADR-0021 D9).
  "visit.create": (e) => `${e.detail === "backdated" ? "nhập bù lượt" : "ghi lượt"}${withSold(e)}`,
  "visit.addition": (e) => `thêm vào lượt${withSold(e)}`,
  "visit.update": () => "sửa lượt",
  "visit.correct": (e) => `sửa lượt (tính lại tiền)${withSold(e)}`,
  "visit.convert_to_package": (e) => `chuyển lượt sang gói${sold(e) || pkg(e)}`,
  "visit.void": () => "hủy lượt",
  "sale.create": (e) => (e.packages.length > 0 ? `bán gói${sold(e)}` : "thu tiền"),
  "sale.void": () => "hủy hóa đơn",
  "sale.note_update": () => "sửa ghi chú gói",
  // Gói liệu trình
  "customer_package.create": (e) => `bán gói${sold(e) || pkg(e)}`,
  "customer_package.extend": () => "gia hạn gói",
  "customer_package.cancel": () => "hủy gói",
  "customer_package.use_expired_override": () => "cho dùng gói đã hết hạn",
  "opening_balance.import": () => "nhập từ sổ giấy",
  "sale.paper_book": () => "nhập từ sổ giấy",
  // Khách hàng
  "customer.create": () => "thêm khách",
  "customer.update": () => "sửa hồ sơ",
  "customer.health_note_set": () => "sửa lưu ý sức khỏe",
  // Lịch hẹn (wave 2)
  "appointment.create": () => "đặt lịch",
  "appointment.update": () => "đổi lịch",
  "appointment.arrive": () => "đánh dấu khách đến",
  "appointment.undo_arrival": () => "bỏ đánh dấu khách đến",
  "appointment.complete": () => "hoàn tất lịch hẹn",
  "appointment.no_show": () => "đánh dấu không đến",
  "appointment.cancel": () => "hủy lịch",
  "appointment.settings": () => "sửa số khách tối đa",
  // Nhân sự (wave 2)
  "attendance.record": (e) => (e.detail === "cleared" ? "xóa chấm công" : "chấm công"),
  "evaluation.daily_review": () => "ghi khen / phàn nàn",
  // An edit is the void and the new entry folded into one line (ADR-0022, K21).
  "staff.adjustment": (e) =>
    `${e.folded.includes("staff.adjustment_void") ? "sửa khoản" : "ghi"} ${e.detail === "penalty" ? "phạt" : "thưởng"}`,
  "staff.adjustment_void": () => "xóa khoản thưởng / phạt",
  // Danh mục, quy tắc lương (wave 3)
  "catalog.service_create": () => "thêm dịch vụ",
  "catalog.service_update": () => "sửa dịch vụ",
  "catalog.plan_create": () => "thêm mẫu gói",
  "catalog.plan_update": () => "sửa mẫu gói",
  "catalog.category_create": () => "thêm nhóm dịch vụ",
  "catalog.category_update": () => "sửa nhóm dịch vụ",
  "payroll.rules_update": () => "sửa quy tắc lương",
  // Shown another way already (spec D11) or not about a record the spa opens (spec §4.2).
  "customer.follow_up": null,
  "evaluation.record": null,
  "staff.create": null,
  "staff.update": null,
  "staff.end": null,
  "staff.rehire": null,
  "staff.pay_update": null,
  "staff.account_link": null,
  "payroll.close": null,
  "staff.login_create": null,
  "staff.login_password_reset": null,
  "staff.login_role": null,
  "account.password_change": null,
  "account.password_reset_requested": null,
  "account.password_reset_dismissed": null,
  "account.locked_by_platform": null,
  "account.unlocked_by_platform": null,
  "account.mfa_reset_by_platform": null,
  "account.sessions_revoked_by_platform": null,
  "account.password_reset_by_platform": null,
  "role.permissions_changed": null,
  "role.permissions_reset": null,
  "feature.changed": null,
  "tenant.created": null,
  "tenant.suspended": null,
  "tenant.unsuspended": null,
  "tenant.owner_mfa_changed": null,
};

/** The verb of a history line; a key without words there (a legacy row) reads generically, never as a crash. */
export function historyVerb(e: ActivityEntry): string {
  const words = (HISTORY as Partial<Record<string, ((e: ActivityEntry) => string) | null>>)[e.action];
  return words ? words(e) : "làm một thao tác khác";
}

/** The words of an entry; a key the contract does not list reads generically, never as a crash. */
export function phraseOf(e: ActivityEntry): Phrase {
  const words = (PHRASES as Partial<Record<string, Words>>)[e.action];
  return words ? words(e) : { lead: "làm một thao tác khác với", about: Boolean(e.subject?.name) };
}

/** The day a record belongs to, when it is not the day of the entry ("lượt ngày 10/11/2026"). */
export function aboutDay(e: ActivityEntry): string {
  const on = e.subject?.on;
  return on && on !== e.day ? `ngày ${dmy(on)}` : "";
}

/** The whole sentence as text (tests, screen readers): "Quản lý Hà hủy lượt của chị Lê Thị Mai". */
export function sentence(e: ActivityEntry): string {
  const p = phraseOf(e);
  const about = p.about === false ? "" : subjectName(e.subject);
  return [actorName(e), p.lead, about, aboutDay(e), p.tail ?? ""].filter(Boolean).join(" ");
}

const AMOUNT_LABEL: Partial<Record<AuditAction, string>> = {
  "visit.create": "Thu",
  "visit.addition": "Thu",
  "sale.create": "Thu",
  // What a cancel gave back (ADR-0022: "Trả lại", not the bookkeeping word "Đảo").
  "visit.void": "Trả lại",
  "sale.void": "Trả lại",
  "visit.correct": "Chênh lệch",
  "visit.convert_to_package": "Chênh lệch",
  "customer_package.create": "Giá",
  "customer_package.cancel": "Hoàn",
  "sale.paper_book": "Số tiền",
  "staff.adjustment": "Số tiền",
  "payroll.close": "Tổng thực lĩnh",
};

/** "Thu 140.000 ₫", "Hoàn 450.000 ₫", "Chênh lệch +50.000 ₫"; "" without money. */
export function moneyOf(e: ActivityEntry): string {
  if (e.amount === undefined || e.amount === null) return "";
  const label = (AMOUNT_LABEL as Partial<Record<string, string>>)[e.action] ?? "Số tiền";
  const signed = label === "Chênh lệch" && e.amount > 0 ? `+${formatVnd(e.amount)}` : formatVnd(e.amount);
  return `${label} ${signed}`;
}

export function discountOf(e: ActivityEntry): string {
  return e.discount ? `giảm ${formatVnd(e.discount)}` : "";
}

const FIELD: Record<string, string> = {
  full_name: "Tên",
  phone: "SĐT",
  gender: "Giới tính",
  birth_year: "Năm sinh",
  birth_date: "Ngày sinh",
  address: "Địa chỉ",
  source: "Nguồn khách",
  note: "Ghi chú hồ sơ",
  name: "Tên",
  duration_min: "Thời lượng (phút)",
  price: "Giá",
  price_female: "Giá Nữ",
  price_male: "Giá Nam",
  // The price list's own words (ServiceForm, PlanForm: "★ Khuyên dùng", "Đang bán").
  category: "Nhóm dịch vụ",
  recommended: "Khuyên dùng",
  active: "Đang bán",
  validity_months: "Hạn dùng (tháng)",
  items: "Dịch vụ trong gói",
  sort_order: "Thứ tự",
  max_per_slot: "Số khách tối đa mỗi khung giờ",
  free_leave_days: "Ngày nghỉ không trừ lương",
  free_leave_codes: "Loại nghỉ tính vào ngày không trừ lương",
  default_shift_start: "Giờ vào ca mặc định",
  late_grace_minutes: "Số phút đi muộn bỏ qua",
  late_penalty_amount: "Tiền phạt đi muộn",
  commission_on_bonus_sessions: "Hoa hồng buổi tặng",
  closing_commission: "Hoa hồng chốt gói",
  loyalty_partial_month: "Thưởng gắn bó tháng lẻ",
  meal_allowance_basis: "Cách tính tiền ăn",
  tet_level_amounts: "Thưởng Tết theo mức",
  base_salary: "Lương cơ bản",
  commission_rate: "Tỉ lệ hoa hồng",
  loyalty_bonus: "Thưởng gắn bó",
  meal_allowance: "Tiền ăn",
  position: "Vị trí",
  shift_start: "Giờ vào ca",
  joined_on: "Ngày vào làm",
  has_phone: "SĐT",
  has_note: "Ghi chú",
  status: "Trạng thái",
  check_in: "Giờ vào",
  check_out: "Giờ ra",
  compliments: "Khen",
  complaints: "Phàn nàn",
  total_points: "Tổng điểm",
  result: "Xếp loại",
  role: "Vai trò",
  expires_on: "Hạn dùng",
  starts_at: "Giờ hẹn",
  // appointment.update (ADR-0022 wave 2): what the edit touched; the booking note, never its text.
  services: "Dịch vụ",
  booking_note: "Ghi chú lịch hẹn",
  check_in_at: "Giờ vào",
  staff: "KTV",
  // A bonus / penalty edited (one line: the replaced entry → the new one).
  person: "Nhân viên",
  kind: "Loại",
  amount: "Số tiền",
  points: "Điểm",
  occurred_on: "Ngày",
  corrects_month: "Điều chỉnh tháng",
  reason: "Lý do",
  // visit.update (ADR-0022 S3): the session note, set or not; never its text.
  session_note: "Ghi chú buổi",
  companions: "Người đi cùng",
  owner_mfa_required: "Bắt buộc xác thực 2 bước",
  enabled: "Bật",
};

const ROLE: Record<string, string> = { owner: "Chủ spa", manager: "Quản lý", technician: "Kỹ thuật viên" };

const CODE: Record<string, Record<string, string>> = {
  status: {
    present: "Đi làm",
    leave_p: "Nghỉ phép (P)",
    leave_dx: "Nghỉ đột xuất (ĐX)",
    absent_k: "Nghỉ không báo (K)",
    active: "Đang hoạt động",
    suspended: "Tạm ngưng",
  },
  role: ROLE,
  gender: { female: "Nữ", male: "Nam" },
  kind: { bonus: "Thưởng", penalty: "Phạt" },
  result: { m1: "Mức 1", m2: "Mức 2", m3: "Mức 3", fail: "Chưa đạt" },
  // The payroll rules' choices, as RulesSheet words them (shortened).
  loyalty_partial_month: {
    fifteen_days: "Đủ tháng nếu làm từ 15 ngày",
    prorate: "Theo số ngày làm",
    full: "Đủ cả tháng",
    none_if_partial: "Không tính",
  },
  meal_allowance_basis: { monthly: "Cố định mỗi tháng", days_worked: "Theo số ngày đi làm" },
};

const OUTCOME: Record<string, string> = {
  no_answer: "(không nghe máy)",
  will_come: "(khách hẹn quay lại)",
  not_interested: "(khách không quan tâm)",
  other: "",
};

const FEATURE: Record<string, string> = {
  appointments: "Lịch hẹn",
  payroll: "Lương",
  evaluations: "Đánh giá",
  follow_ups: "Chăm sóc lại",
  notes_journal: "Nhật ký ghi chú",
  reports: "Báo cáo",
};

/** "+84905111222" → "0905 111 222". */
export function phoneText(e164: string): string {
  const local = e164.startsWith("+84") ? `0${e164.slice(3)}` : e164;
  return local.length === 10 ? `${local.slice(0, 4)} ${local.slice(4, 7)} ${local.slice(7)}` : local;
}

function valueText(c: ActivityChange, raw: string): string {
  switch (c.format) {
    case "money":
      return formatVnd(Number(raw));
    case "number":
      return Number(raw).toLocaleString("vi-VN");
    case "date":
      return dmy(raw);
    case "time":
      return raw.slice(0, 5);
    case "instant":
      return spaDateTime(raw);
    case "flag":
      return raw === "true" ? "Có" : "Không";
    case "percent_bp":
      return `${(Number(raw) / 100).toLocaleString("vi-VN")} %`;
    case "phone":
      return phoneText(raw);
    case "code":
      return CODE[c.field]?.[raw] ?? raw;
    default:
      return raw;
  }
}

/** One changed field: "SĐT: 0905 111 222 → 0912 333 444", "Trạng thái: Nghỉ đột xuất (ĐX)", "Địa chỉ (đã đổi)". */
export function changeText(c: ActivityChange): string {
  const label = FIELD[c.field] ?? c.field;
  const old = c.old === undefined || c.old === null ? null : valueText(c, c.old);
  const now = c.new === undefined || c.new === null ? null : valueText(c, c.new);
  if (old !== null && now !== null) return `${label}: ${old} → ${now}`;
  if (now !== null) return `${label}: ${now}`;
  if (old !== null) return `${label}: bỏ ${old}`;
  return `${label} (đã đổi)`;
}

/** The time on the row: "15:10". */
export const timeOf = (e: ActivityEntry) => spaTime(e.at);
