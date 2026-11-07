import { queryOptions, type QueryClient } from "@tanstack/react-query";
import type { Schemas } from "@vpyd/api-client";
import { api, ifMatch } from "../../lib/api";
import { addMessages } from "../../lib/errors";

// Error messages only the staff screens can meet: here, not in the startup chunk (lib/errors.ts).
addMessages(
  {
    "payroll.period_closed":
      "Tháng này đã chốt lương nên không sửa được. Hãy ghi một khoản thưởng/phạt điều chỉnh vào tháng đang mở.",
    "payroll.month_not_ended": "Tháng chưa kết thúc, chưa chốt lương được.",
    "payroll.unrecorded_days": "Còn ngày chưa chấm công. Chấm đủ hoặc chọn chốt dù còn thiếu.",
    "staff.account_taken": "Tài khoản này đã gắn với một nhân viên khác.",
    "staff.record_owner_only": "Chỉ chủ spa ghi được cho quản lý (quản lý không tự ghi cho mình).",
    "adjustment.voided": "Khoản này vừa được xóa hoặc sửa ở máy khác. Tải lại để xem mới nhất.",
  },
  {
    not_employed: "Ngày này nhân viên chưa vào làm hoặc đã nghỉ.",
    only_when_present: "Chỉ ghi giờ khi đi làm.",
    above_max: "Vượt điểm tối đa của tiêu chí.",
    amount_or_points_required: "Nhập số tiền hoặc số điểm.",
    not_closed: "Tháng này chưa chốt, không cần điều chỉnh.",
    not_earlier: "Tháng điều chỉnh phải trước tháng ghi.",
    invalid_month: "Tháng không hợp lệ.",
    before_joined_on: "Ngày nghỉ việc trước ngày vào làm.",
    must_be_empty: "Nhân viên đang làm thì không có ngày nghỉ việc.",
    not_a_member: "Tài khoản không thuộc spa này.",
  },
);

export type StaffProfile = Schemas["StaffProfileDto"];
export type StaffProfileWrite = Schemas["StaffProfileWrite"];
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
  });
}

export const rulesQuery = queryOptions({
  queryKey: ["staff", "rules"],
  queryFn: async () => (await api("/payroll/rules")) as PayrollRulesDto,
});

export const profilesQuery = queryOptions({
  queryKey: ["staff", "profiles"],
  queryFn: async () => (await api("/staff/profiles")) as StaffProfile[],
});

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
export type MyDayVisit = Schemas["MyDayVisit"];
export type MyDayUpsell = Schemas["MyDayUpsell"];

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

export async function createProfile(body: StaffProfileWrite): Promise<StaffProfile> {
  return (await api("/staff", { method: "POST", body })) as StaffProfile;
}

export async function updateProfile(id: string, version: number, body: StaffProfileWrite): Promise<StaffProfile> {
  return (await api(`/staff/${id}`, { method: "PUT", body, headers: ifMatch(version) })) as StaffProfile;
}

export async function savePay(id: string, body: StaffPayWrite): Promise<unknown> {
  return api(`/staff/${id}/pay`, { method: "PUT", body });
}

/** Current month `YYYY-MM` in the spa's time zone. */
export function currentMonth(today: string): string {
  return today.slice(0, 7);
}
