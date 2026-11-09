import { queryOptions, type QueryClient } from "@tanstack/react-query";
import type { Schemas } from "@vpyd/api-client";
import { api, ifMatch } from "../../lib/api";
import { addMessages, type FieldTexts } from "../../lib/errors";

// Error messages only the staff screens can meet: here, not in the startup chunk (lib/errors.ts).
addMessages(
  {
    "payroll.period_closed":
      "Tháng này đã chốt lương nên không sửa được. Hãy ghi một khoản thưởng/phạt điều chỉnh vào tháng đang mở.",
    "payroll.month_not_ended": "Tháng chưa kết thúc, chưa chốt lương được.",
    "payroll.unrecorded_days": "Còn ngày chưa chấm công. Chấm đủ hoặc chọn chốt dù còn thiếu.",
    "payroll.missing_data": "Còn người chưa có lương cứng hoặc chưa đánh giá tháng. Bổ sung theo danh sách rồi chốt lại.",
    "staff.account_taken": "Tài khoản này đã gắn với một nhân viên khác.",
    "staff.record_owner_only": "Chỉ chủ spa ghi được cho quản lý (quản lý không tự ghi cho mình).",
    "staff.link_required":
      "Tài khoản của bạn chưa gắn với hồ sơ nhân viên nên chưa ghi được. Nhờ chủ spa gắn tài khoản vào hồ sơ của bạn ở Hồ sơ nhân viên.",
    "adjustment.voided": "Khoản này vừa được xóa hoặc sửa ở máy khác. Tải lại để xem mới nhất.",
    "staff.already_left": "Nhân viên này đã nghỉ việc rồi. Tải lại để xem mới nhất.",
    "staff.already_active": "Nhân viên này đang làm. Tải lại để xem mới nhất.",
    "staff.has_upcoming_bookings":
      "Nhân viên còn lịch hẹn sau ngày nghỉ việc. Đổi KTV hoặc hủy các lịch dưới đây ở Lịch hẹn trước:",
    "staff.cannot_end_self": "Bạn không tự cho mình nghỉ việc được.",
    "staff.not_employed_in_month": "Tháng này bạn không làm ở spa: chỉ xem được các tháng đã làm.",
    "staff.linked_to_owner": "Hồ sơ này gắn với tài khoản chủ spa. Bỏ gắn tài khoản trước khi cho nghỉ việc.",
    "account.phone_taken": "Số điện thoại này đã có tài khoản đăng nhập. Dùng một số khác.",
    "staff.account_exists": "Hồ sơ này đã có tài khoản đăng nhập. Tải lại để xem mới nhất.",
    "staff.no_login": "Hồ sơ này chưa có tài khoản đăng nhập.",
    "account.locked": "Tài khoản đã bị khóa khi cho nghỉ việc. Dùng “Nhận lại làm” để mở lại trước.",
    "account.owner_protected": "Không đổi được tài khoản của chủ spa ở đây.",
    "account.other_spa": "Tài khoản này còn dùng ở spa khác nên không đặt lại mật khẩu ở đây được.",
  },
);

/** Field refusals of the staff forms (`fieldMessage(…, STAFF_FIELDS)`). */
export const STAFF_FIELDS: FieldTexts = {
  not_employed: "Ngày này nhân viên chưa vào làm hoặc đã nghỉ.",
  only_when_present: "Chỉ ghi giờ khi đi làm.",
  above_max: "Vượt điểm tối đa của tiêu chí.",
  amount_or_points_required: "Nhập số tiền hoặc số điểm.",
  not_closed: "Tháng này chưa chốt, không cần điều chỉnh.",
  not_earlier: "Tháng điều chỉnh phải trước tháng ghi.",
  invalid_month: "Tháng không hợp lệ.",
  before_joined_on: "Ngày nghỉ việc trước ngày vào làm.",
  not_a_member: "Tài khoản không thuộc spa này.",
  has_later_records: "Sau ngày này nhân viên vẫn có chấm công hoặc lượt khách. Chọn ngày làm cuối cùng.",
  period_ended: "Đợt làm việc này đã kết thúc. Dùng \u201cNhận lại làm\u201d để mở đợt mới.",
  overlaps_previous: "Ngày vào làm phải sau ngày nghỉ của đợt trước.",
  before_last_end: "Ngày nhận lại phải sau ngày nghỉ việc của đợt trước.",
};

export type StaffProfile = Schemas["StaffProfileDto"];
export type StaffProfileWrite = Schemas["StaffProfileWrite"];
export type StaffCreate = Schemas["StaffCreate"];
export type StaffHistoryEntry = Schemas["StaffHistoryEntryDto"];
export type StaffPayWrite = Schemas["StaffPayWrite"];
export type StaffAccount = Schemas["StaffAccountDto"];
export type AttendanceSheet = Schemas["AttendanceSheet"];
export type StaffSheet = Schemas["StaffSheet"];
export type AttendanceDay = Schemas["AttendanceDay"];
export type AttendanceEntryWrite = Schemas["AttendanceEntryWrite"];
export type ReviewSheet = Schemas["ReviewSheet"];
export type StaffReviews = Schemas["StaffReviews"];
export type DailyReview = Schemas["DailyReview"];
export type DailyReviewEntryWrite = Schemas["DailyReviewEntryWrite"];
export type EvaluationSheet = Schemas["EvaluationSheet"];
export type StaffEvaluation = Schemas["StaffEvaluation"];
export type EvaluationWrite = Schemas["EvaluationWrite"];
export type AdjustmentLog = Schemas["AdjustmentLog"];
export type Adjustment = Schemas["Adjustment"];
export type AdjustmentCreate = Schemas["AdjustmentCreate"];
export type PayrollMonth = Schemas["PayrollMonth"];
export type MissingPayrollData = Schemas["MissingPayrollData"];
export type MissingItem = Schemas["MissingItem"];
export type LeaverRefund = Schemas["LeaverRefund"];
export type Payslip = Schemas["Payslip"];
export type PayrollRules = Schemas["PayrollRules"];
export type PayrollRulesDto = Schemas["PayrollRulesDto"];
export type MyStaff = Schemas["MyStaff"];

const q = (month: string) => (month ? `?month=${month}` : "");

/** Query keys: everything under ["staff"], so one invalidation refreshes every tab. */
export const staffKeys = {
  all: ["staff"] as const,
  month: (kind: string, month: string) => ["staff", kind, month] as const,
};

export function attendanceQuery(month: string) {
  return queryOptions({
    queryKey: staffKeys.month("attendance", month),
    queryFn: async () => (await api(`/attendance${q(month)}`)) as AttendanceSheet,
  });
}

export function reviewsQuery(month: string) {
  return queryOptions({
    queryKey: staffKeys.month("reviews", month),
    queryFn: async () => (await api(`/daily-reviews${q(month)}`)) as ReviewSheet,
  });
}

export function evaluationsQuery(month: string) {
  return queryOptions({
    queryKey: staffKeys.month("evaluations", month),
    queryFn: async () => (await api(`/evaluations${q(month)}`)) as EvaluationSheet,
  });
}

export function adjustmentsQuery(month: string) {
  return queryOptions({
    queryKey: staffKeys.month("adjustments", month),
    queryFn: async () => (await api(`/adjustments${q(month)}`)) as AdjustmentLog,
  });
}

export function payrollQuery(month: string) {
  return queryOptions({
    queryKey: staffKeys.month("payroll", month),
    queryFn: async () => (await api(`/payroll${q(month)}`)) as PayrollMonth,
    // Money, and the screen where a month is closed: always asked again, never served from cache.
    staleTime: 0,
  });
}

export const rulesQuery = queryOptions({
  queryKey: ["staff", "rules"],
  queryFn: async () => (await api("/payroll/rules")) as PayrollRulesDto,
});

/** The whole roster (working and former staff); the tab filters it on the client. */
export const profilesQuery = queryOptions({
  queryKey: ["staff", "profiles"],
  queryFn: async () => (await api("/staff/profiles?status=all")) as StaffProfile[],
});

/** Who changed what on a profile (owner); pay changes only for payroll.read. */
export type Evaluation = Schemas["Evaluation"];
export type EvaluationHistory = Schemas["EvaluationHistory"];
export type HistoryMonth = Schemas["HistoryMonth"];

/** `GET /evaluations/history`: one person's last 12 months in one request (EM 2026-11-08). */
export function evaluationHistoryQuery(staffId: string) {
  return queryOptions({
    queryKey: ["staff", "evaluation-history", staffId] as const,
    queryFn: async () => (await api(`/evaluations/history?staff_id=${encodeURIComponent(staffId)}`)) as EvaluationHistory,
  });
}

export type MyEvaluationHistory = Schemas["MyEvaluationHistory"];

/** `GET /me/evaluations/history`: the signed-in staff member's own last 12 months. */
export const myEvaluationHistoryQuery = queryOptions({
  queryKey: ["staff", "my-evaluation-history"] as const,
  queryFn: async () => (await api("/me/evaluations/history")) as MyEvaluationHistory,
});

export function historyQuery(id: string) {
  return queryOptions({
    queryKey: ["staff", "history", id] as const,
    queryFn: async () => (await api(`/staff/${id}/history`)) as StaffHistoryEntry[],
  });
}

export const accountsQuery = queryOptions({
  queryKey: ["staff", "accounts"],
  queryFn: async () => (await api("/staff/accounts")) as StaffAccount[],
});

export function myMonthQuery(month: string) {
  return queryOptions({
    queryKey: staffKeys.month("me", month),
    queryFn: async () => (await api(`/me/staff${q(month)}`)) as MyStaff,
  });
}

export type MyDay = Schemas["MyDay"];
export type MyDayData = Schemas["MyDayData"];

/** One day of my month in detail (`GET /me/staff/day`): the person is the session's, never a parameter. */
export function myDayQuery(date: string) {
  return queryOptions({
    queryKey: ["staff", "me-day", date] as const,
    queryFn: async () => (await api(`/me/staff/day?date=${date}`)) as MyDay,
  });
}

export type MyCalendar = Schemas["MyCalendar"];
export type MyCalendarData = Schemas["MyCalendarData"];
export type MyCalendarEntry = Schemas["MyCalendarEntry"];

/** "Khách của tôi": my bookings and the visits I served over `from..to` (≤ 14 days, `GET /me/appointments`). */
export function myCalendarQuery(from: string, to: string) {
  return queryOptions({
    queryKey: ["staff", "me-calendar", from, to] as const,
    queryFn: async () => (await api(`/me/appointments?from=${from}&to=${to}`)) as MyCalendar,
    staleTime: 15_000,
  });
}

export function invalidateStaff(queryClient: QueryClient): Promise<void> {
  // Includes the front desk's picker (["staff", "active"]), which changes with profiles too.
  return queryClient.invalidateQueries({ queryKey: staffKeys.all });
}

export async function saveAttendance(entries: AttendanceEntryWrite[]): Promise<AttendanceDay[]> {
  return (await api("/attendance", { method: "PUT", body: { entries } })) as AttendanceDay[];
}

export async function saveReviews(entries: DailyReviewEntryWrite[]): Promise<unknown> {
  return api("/daily-reviews", { method: "PUT", body: { entries } });
}

export async function saveEvaluation(body: EvaluationWrite): Promise<unknown> {
  return api("/evaluations", { method: "PUT", body });
}

export async function createAdjustment(body: AdjustmentCreate, key: string): Promise<Adjustment> {
  return (await api("/adjustments", { method: "POST", body, headers: { "Idempotency-Key": key } })) as Adjustment;
}

/** Edit = the old entry is voided and the corrected one recorded (new id), one transaction. */
export async function updateAdjustment(id: string, body: AdjustmentCreate, key: string): Promise<Adjustment> {
  return (await api(`/adjustments/${id}`, { method: "PUT", body, headers: { "Idempotency-Key": key } })) as Adjustment;
}

/** Removes an entry recorded by mistake (voided server-side, kept for the audit trail). */
export async function deleteAdjustment(id: string): Promise<void> {
  await api(`/adjustments/${id}`, { method: "DELETE" });
}

export async function closePayroll(month: string, allowIncomplete: boolean, key: string): Promise<PayrollMonth> {
  return (await api("/payroll/close", {
    method: "POST",
    body: { month, allow_incomplete: allowIncomplete },
    headers: { "Idempotency-Key": key },
  })) as PayrollMonth;
}

export async function saveRules(rules: PayrollRules): Promise<PayrollRulesDto> {
  return (await api("/payroll/rules", { method: "PUT", body: rules })) as PayrollRulesDto;
}

export async function createProfile(body: StaffCreate): Promise<StaffProfile> {
  return (await api("/staff", { method: "POST", body })) as StaffProfile;
}

export async function updateProfile(id: string, version: number, body: StaffProfileWrite): Promise<StaffProfile> {
  return (await api(`/staff/${id}`, { method: "PUT", body, headers: ifMatch(version) })) as StaffProfile;
}

/** Links a login to the profile, or unlinks it (`null`). Never touches passwords. */
export async function setAccount(id: string, version: number, userId: string | null): Promise<StaffProfile> {
  return (await api(`/staff/${id}/account`, { method: "PUT", body: { user_id: userId }, headers: ifMatch(version) })) as StaffProfile;
}

/**
 * Nghỉ việc: closes the current period (soft; history stays). The linked login keeps only the
 * person's own past pay (ADR-0014), unless `lockLogin` ("Khóa tài khoản") locks it outright.
 */
export async function endEmployment(
  id: string,
  version: number,
  endedOn: string,
  reason: string,
  lockLogin = false,
): Promise<StaffProfile> {
  return (await api(`/staff/${id}/end`, {
    method: "POST",
    body: { ended_on: endedOn, reason, lock_login: lockLogin },
    headers: ifMatch(version),
  })) as StaffProfile;
}

/** Nhận lại làm: a new period after the last one. */
export async function rehire(id: string, version: number, startedOn: string): Promise<StaffProfile> {
  return (await api(`/staff/${id}/rehire`, { method: "POST", body: { started_on: startedOn }, headers: ifMatch(version) })) as StaffProfile;
}

export async function savePay(id: string, body: StaffPayWrite): Promise<unknown> {
  return api(`/staff/${id}/pay`, { method: "PUT", body });
}

// ------------------------------------------------------------- logins (ADR-0016, spec §17)

export type LoginRole = Schemas["LoginRole"];
export type StaffLoginCreated = Schemas["StaffLoginCreated"];
export type PasswordResetRequest = Schemas["PasswordResetRequestDto"];

export const LOGIN_ROLE_LABEL: Record<LoginRole, string> = { manager: "Quản lý", technician: "Kỹ thuật viên" };

/** Creates the profile's login (owner, re-auth): the temporary password comes back this once. */
export async function createLogin(id: string, version: number, phone: string, role: LoginRole): Promise<StaffLoginCreated> {
  return (await api(`/staff/${id}/login`, { method: "POST", body: { phone, role }, headers: ifMatch(version) })) as StaffLoginCreated;
}

/** A new temporary password (owner, re-auth); the person's sessions end. */
export async function resetLoginPassword(id: string): Promise<string> {
  const reply = (await api(`/staff/${id}/login/reset-password`, { method: "POST" })) as Schemas["TemporaryPasswordDto"];
  return reply.temporary_password;
}

export async function setLoginRole(id: string, role: LoginRole): Promise<StaffProfile> {
  return (await api(`/staff/${id}/login/role`, { method: "PUT", body: { role } })) as StaffProfile;
}

/** Forgotten-password requests waiting for the owner (option A). */
export const resetRequestsQuery = queryOptions({
  queryKey: ["staff", "reset-requests"],
  queryFn: async () => (await api("/staff/password-reset-requests")) as PasswordResetRequest[],
});

export async function dismissResetRequest(id: string): Promise<void> {
  await api(`/staff/password-reset-requests/${id}/dismiss`, { method: "POST" });
}

/** Confirms the owner's password (and code) again before a login action (`/auth/reauth`). */
export async function reauth(password: string, code: string): Promise<void> {
  await api("/auth/reauth", { method: "POST", body: code ? { password, code } : { password } });
}
