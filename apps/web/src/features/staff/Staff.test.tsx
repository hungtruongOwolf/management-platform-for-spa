import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { navFor, tabsFor } from "../../app/nav";
import { spaToday } from "../../lib/format";
import { ME } from "../reports/testServer";
import type {
  Adjustment,
  AdjustmentLog,
  AttendanceSheet,
  Evaluation,
  EvaluationHistory,
  EvaluationSheet,
  MyCalendar,
  MyDay,
  MyEvaluationHistory,
  MyStaff,
  Payslip,
  PayrollMonth,
  ReviewSheet,
  PasswordResetRequest,
  StaffAccount,
  StaffProfile,
} from "./api";
import { mondayOf } from "../appointments/api";
import { focusMinute, gridSpan, weekSummary } from "./myCalendar";
import type { EntryView } from "../appointments/api";
import { countsFullValue, serviceWork, upsellWork } from "./payslipWork";
import { dayTotals, shiftDay } from "./MyDaySheet";
import { daysOf, monthLabel, percentOfBp, shiftMonth, weekdayShort } from "./labels";
import { evaluationDueMonth, incompleteDays, missingOn } from "./progress";
import { staffSearch } from "./search";
import { tenureLabel, withoutLogin } from "./roster";
import { PayslipView } from "./PayrollTab";
import { signedVnd } from "./ui";

const MANAGER = ["catalog.read", "staff.read_basic", "staff.read", "attendance.record", "evaluation.record", "staff.read_own", "report.read"];
const OWNER = [...MANAGER, "staff.manage", "payroll.read", "payroll.close"];
const TECH = ["catalog.read", "staff.read_basic", "staff.read_own"];

const today = spaToday();
const month = today.slice(0, 7);

const totals = { present: 0, late_count: 0, late_minutes: 0, leave_p: 0, leave_dx: 0, absent_k: 0, unrecorded: 0 };

function sheet(options: ServeOptions = {}): AttendanceSheet {
  return {
    month,
    days_in_month: daysOf(month).length,
    today,
    closed: false,
    default_shift_start: "09:00",
    link_required: false,
    staff: [
      {
        staff_id: "s-lan",
        full_name: "Lê Thị Lan",
        position: "Kỹ thuật viên",
        shift_start: "09:00",
        joined_on: null,
        left_on: null,
        periods: [],
        days: [{ staff_id: "s-lan", date: today, status: "present", check_in: "09:12", check_out: null, late_minutes: 12, note: null }],
        totals: { ...totals, present: 1, late_count: 1, late_minutes: 12 },
        can_record: true,
        visit_days: options.lanServedToday ? [today] : [],
      },
      {
        staff_id: "s-mai",
        full_name: "Trần Thị Mai",
        position: "Kỹ thuật viên",
        shift_start: "09:00",
        joined_on: null,
        left_on: null,
        periods: [],
        days: [],
        totals,
        can_record: true,
        visit_days: [],
      },
      // Another manager's row (decision C): the owner's to write. The signed-in manager's own
      // row never comes: the server leaves it out of their books (EM 2026-11-08).
      {
        staff_id: "s-hoa",
        full_name: "Phạm Thị Hoa",
        position: "Quản lý",
        shift_start: "09:00",
        joined_on: null,
        left_on: null,
        periods: [],
        days: [],
        totals,
        can_record: false,
        visit_days: [],
      },
    ],
  };
}

/** The roster as the server sends it: `details` only with staff.manage (manager: none). */
function roster(details: boolean): StaffProfile[] {
  const base = { shift_start: "09:00", version: 1, pay_visible: false, position_from_account: false };
  const people: StaffProfile[] = [
    {
      ...base,
      id: "s-lan",
      full_name: "Lê Thị Lan",
      position: "Kỹ thuật viên",
      status: "active",
      joined_on: "2025-03-01",
      first_joined_on: "2025-03-01",
      left_on: null,
      periods: [{ started_on: "2025-03-01", ended_on: null }],
      tenure_days: 615,
      tenure_months: 20,
      login_linked: true,
      ...(details
        ? { details: { phone: "0905 111 222", note: null, user_id: "u-lan", user_name: "KTV Lan", login: { phone: "0912 300 111", role: "technician", locked: false } } }
        : {}),
    },
    {
      ...base,
      id: "s-hoa",
      full_name: "Phạm Thị Hoa",
      position: "Quản lý",
      status: "active",
      joined_on: "2026-10-20",
      first_joined_on: "2026-10-20",
      left_on: null,
      periods: [{ started_on: "2026-10-20", ended_on: null }],
      tenure_days: 19,
      tenure_months: 0,
      login_linked: false,
      ...(details ? { details: { phone: null, note: null, user_id: null, user_name: null } } : {}),
    },
    {
      ...base,
      id: "s-hang",
      full_name: "Đỗ Thị Hằng",
      position: "Kỹ thuật viên",
      status: "left",
      joined_on: "2025-01-06",
      first_joined_on: "2025-01-06",
      left_on: "2026-08-29",
      periods: [{ started_on: "2025-01-06", ended_on: "2026-08-29", ...(details ? { end_reason: "Chuyển về quê" } : {}) }],
      tenure_days: 601,
      tenure_months: 19,
      login_linked: false,
      ...(details ? { details: { phone: null, note: null, user_id: null, user_name: null } } : {}),
    },
  ];
  return people;
}

function lan() {
  const [s] = sheet().staff;
  if (!s) throw new Error("fixture has no staff");
  return s;
}

function slip(): Payslip {
  return {
    staff_id: "s-lan",
    full_name: "Lê Thị Lan",
    position: "Kỹ thuật viên",
    month,
    days_in_month: 30,
    employed_days: 30,
    base_salary: 5_000_000,
    commission_rate_bp: 500,
    day_rate: 178_571,
    attendance: totals,
    free_leave_days: 2,
    leave_days_counted: 0,
    deducted_leave_days: 0,
    leave_deduction: 0,
    late_penalised: 0,
    late_deduction: 0,
    service_commission_base: 300_000,
    service_commission: 15_000,
    closing_commission_base: 0,
    closing_commission: 0,
    upsell_package_base: 0,
    upsell_service_base: 0,
    meal_allowance: 300_000,
    bonuses: 0,
    penalties: 0,
    bonus_points: 0,
    penalty_points: 0,
    net_pay: 5_315_000,
    loyalty_bonus_accrued: 500_000,
    evaluation_level: null,
    tet_level: null,
    tet_amount: 0,
    commission_lines: [{ service_name: "Gội đầu thảo dược", source: "one_off", qty: 2, value: 300_000, full_value: 300_000 }],
    upsell_lines: [],
    flags: ["evaluation_missing"],
  };
}

function payroll(): PayrollMonth {
  const s = slip();
  return {
    month,
    closed: false,
    closed_at: null,
    closed_by_name: null,
    ended: false,
    payslips: [s],
    totals: { staff: 1, base_salary: s.base_salary, deductions: 0, commission: 15_000, bonuses: 0, penalties: 0, net_pay: s.net_pay },
  };
}

function reviews(): ReviewSheet {
  return {
    month,
    today,
    closed: false,
    staff: [
      {
        staff_id: "s-lan",
        full_name: "Lê Thị Lan",
        position: "Kỹ thuật viên",
        compliments: 2,
        complaints: 0,
        days: [{ staff_id: "s-lan", date: today, compliments: 2, complaints: 0, note: "Khách khen tay nghề" }],
        can_record: true,
      },
      { staff_id: "s-mai", full_name: "Trần Thị Mai", position: "Kỹ thuật viên", compliments: 0, complaints: 0, days: [], can_record: true },
      { staff_id: "s-hoa", full_name: "Phạm Thị Hoa", position: "Quản lý", compliments: 0, complaints: 0, days: [], can_record: false },
    ],
  };
}

function evaluations(m: string): EvaluationSheet {
  return {
    month: m,
    closed: false,
    link_required: false,
    criteria: [
      { key: "technique", label: "Kỹ thuật liệu trình", max: 40 },
      { key: "attitude", label: "Thái độ và giao tiếp", max: 15 },
    ],
    staff: [
      { staff_id: "s-mai", full_name: "Trần Thị Mai", position: "Kỹ thuật viên", compliments: 0, complaints: 0, attendance: totals, evaluation: null, can_record: true },
      { staff_id: "s-hoa", full_name: "Phạm Thị Hoa", position: "Quản lý", compliments: 0, complaints: 0, attendance: totals, evaluation: null, can_record: false },
    ],
  };
}

/** Monday of this week: the technician's calendar loads Monday–Sunday. */
const week = mondayOf(today);

function calendar(): MyCalendar {
  return {
    linked: true,
    data: {
      from: week,
      to: week,
      today,
      opens_at: "09:00",
      closes_at: "21:00",
      slot_minutes: 15,
      entries: [
        {
          id: "v-1",
          kind: "visit",
          business_date: today,
          starts_at: `${today}T03:00:00Z`,
          ends_at: `${today}T04:10:00Z`,
          duration_min: 70,
          phase: "done",
          customer_name: "Nguyễn Thị Hằng",
          services: [{ service_name: "Gội đầu thảo dược", category_name: "Gội đầu", from_package: true }],
          group_size: null,
          walk_in: true,
          note: "Khách đau vai gáy",
        },
        {
          id: "a-1",
          kind: "appointment",
          business_date: today,
          starts_at: `${today}T07:00:00Z`,
          ends_at: `${today}T08:00:00Z`,
          duration_min: 60,
          phase: "waiting",
          customer_name: "Đỗ Văn Bình",
          services: [{ service_name: "Massage cổ vai gáy", category_name: "Massage", from_package: false }],
          group_size: 3,
          walk_in: false,
          note: null,
        },
      ],
    },
  };
}

function myDay(date: string): MyDay {
  return {
    linked: true,
    data: {
      date,
      today,
      closed: false,
      attendance: { staff_id: "s-lan", date, status: "present", check_in: "09:12", check_out: "18:00", late_minutes: 12, note: "Kẹt xe" },
      review: { staff_id: "s-lan", date, compliments: 2, complaints: 1, note: "Khách khen tay nhẹ" },
      adjustments: [{ ...PENALTY_TEMPLATE, occurred_on: date, kind: "bonus", amount: 100_000, reason: "Ở lại dọn phòng" }],
      visits: [
        {
          visit_id: "v-1",
          check_in_at: `${date}T03:00:00Z`,
          customer_name: "Nguyễn Thị Hằng",
          note: "Khách đau vai gáy",
          lines: [
            { service_name: "Gội đầu thảo dược", source: "package", qty: 1, value: 120_000, commission: 6_000 },
            { service_name: "Ngâm chân", source: "one_off", qty: 2, value: 78_000, commission: 3_900 },
          ],
        },
      ],
      upsell: [{ kind: "package", description: "Gói 10 buổi", customer_name: "Nguyễn Thị Hằng", qty: 1, value: 1_200_000, commission: 60_000 }],
      commission_rate_bp: 500,
    },
  };
}

/** Knobs for the daily-book state tests: a closed month, or a custom answer to PUT /attendance. */
interface ServeOptions {
  closed?: boolean;
  attendancePut?: () => Promise<Response>;
  adjustments?: Adjustment[];
  /** Lan has a visit recorded today (the leave warning). */
  lanServedToday?: boolean;
  /** Lan left on this day (ADR-0014): `/me/staff` refuses later months. */
  leftOn?: string;
  /** Scorecards exist (Hoa's: only the owner scores it), and the month is closed for evaluations. */
  scored?: boolean;
  /** `GET /staff/accounts` (owner). */
  accounts?: StaffAccount[];
  /** `POST /staff/{id}/end` refuses: bookings still assigned (F24). */
  endRefused?: boolean;
  /** The caller's login is linked to no profile (`link_required`, every `can_record` false). */
  linkRequired?: boolean;
  /** `GET /staff/password-reset-requests` (owner). */
  resetRequests?: PasswordResetRequest[];
  /** `POST /staff/{id}/login` asks for re-auth until `POST /auth/reauth`. */
  reauthFirst?: boolean;
  /** `POST /auth/reauth` answers 401 `auth.unauthenticated`: the session itself ended. */
  reauthExpired?: boolean;
  /** The first `PUT /staff/{id}/pay` is refused (base salary over the server's bound). */
  payRefusedOnce?: boolean;
  /** `POST /adjustments` answers 422 with this field error. */
  adjustmentRefused?: { field: string; code: string };
}

/** A scorecard as `GET /evaluations` / `/me/staff` send it. */
const SCORECARD: Evaluation = {
  staff_id: "s-hoa",
  month,
  criteria: [
    { key: "technique", max: 40, level: "m2", points: 32 },
    { key: "attitude", max: 15, level: "m1", points: 15 },
  ],
  serious_issue: false,
  total_points: 81,
  result: "m2",
  coaching_plan: null,
  comment: "Tay nghề tốt, cần đúng giờ hơn",
  by_name: "Chủ Spa",
  updated_at: `${today}T03:00:00Z`,
  version: 1,
};

function history(staffId: string): EvaluationHistory {
  const previous = shiftMonth(month, -1);
  return {
    staff_id: staffId,
    full_name: "Trần Thị Mai",
    position: "Kỹ thuật viên",
    from: previous,
    to: month,
    criteria: [],
    can_record: true,
    months: [
      { month, closed: false, compliments: 1, complaints: 0, attendance: totals, evaluation: null },
      { month: previous, closed: true, compliments: 3, complaints: 1, attendance: { ...totals, late_count: 2 }, evaluation: { ...SCORECARD, staff_id: staffId, month: previous } },
    ],
  };
}

const PENALTY_TEMPLATE: Adjustment = {
  id: "adj-day",
  staff_id: "s-lan",
  occurred_on: today,
  kind: "penalty",
  amount: 50_000,
  points: null,
  reason: "",
  corrects_month: null,
  by_name: "Quản lý Hoa",
  created_at: `${today}T03:00:00Z`,
  can_edit: false,
};

const PENALTY: Adjustment = {
  id: "adj-1",
  staff_id: "s-lan",
  occurred_on: today,
  kind: "penalty",
  amount: 50_000,
  points: null,
  reason: "Đi muộn không báo",
  corrects_month: null,
  by_name: "Quản lý Hoa",
  created_at: `${today}T03:00:00Z`,
  can_edit: true,
};

function serve(permissions: string[], options: ServeOptions = {}) {
  const reauthed = { done: false };
  const payRefused = { left: options.payRefusedOnce ? 1 : 0 };
  const hired = { version: 0 };
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://app.test");
    const p = url.pathname.replace(/^\/api\/v1/, "");
    const method = (init?.method ?? "GET").toUpperCase();
    const ok = (json: unknown, status = 200) => Promise.resolve(Response.json(json, { status }));
    if (p === "/me") return ok(ME(permissions));
    if (p === "/attendance" && method === "PUT") return options.attendancePut ? options.attendancePut() : ok([]);
    if (p === "/attendance") {
      const att = sheet(options);
      return ok({
        ...att,
        closed: options.closed ?? false,
        link_required: options.linkRequired ?? false,
        staff: options.linkRequired ? att.staff.map((x) => ({ ...x, can_record: false })) : att.staff,
      });
    }
    if (p === "/payroll/rules" && method === "GET")
      return ok({
        version: 1,
        rules: {
          closing_commission: true,
          commission_on_bonus_sessions: false,
          default_shift_start: "09:00",
          free_leave_codes: ["leave_p", "leave_dx", "absent_k"],
          free_leave_days: 2,
          late_grace_minutes: 5,
          late_penalty_amount: 20_000,
          loyalty_partial_month: "prorate",
          tet_level_amounts: [],
        },
      });
    if (p === "/payroll") return permissions.includes("payroll.read") ? ok(payroll()) : ok({ code: "auth.forbidden" }, 403);
    if (p === "/me/staff") {
      const left = options.leftOn;
      const asked = url.searchParams.get("month") ?? (left ? left.slice(0, 7) : month);
      if (left && asked > left.slice(0, 7)) return ok({ code: "staff.not_employed_in_month" }, 403);
      return ok({
        linked: true,
        ...(left ? { left_on: left } : {}),
        data: {
          month: asked,
          closed: false,
          attendance: lan(),
          reviews: [],
          evaluation: options.scored ? { ...SCORECARD, staff_id: "s-lan", month: asked } : null,
          adjustments: [],
          payslip: slip(),
        },
      } satisfies MyStaff);
    }
    if (p === "/me/appointments") return ok(calendar());
    if (p === "/me/staff/day") return ok(myDay(url.searchParams.get("date") ?? today));
    if (p === "/daily-reviews" && method === "PUT") return ok([]);
    if (p === "/daily-reviews") return ok(reviews());
    if (p === "/adjustments" && method === "POST" && options.adjustmentRefused)
      return ok({ code: "validation_failed", errors: [options.adjustmentRefused] }, 422);
    if (p === "/adjustments" && method === "GET") return ok({ month, closed: false, items: options.adjustments ?? [] } satisfies AdjustmentLog);
    if (p.startsWith("/adjustments/") && method === "DELETE") return Promise.resolve(new Response(null, { status: 204 }));
    if (p === "/evaluations" && method === "PUT") return ok({});
    if (p === "/evaluations/history") return ok(history(url.searchParams.get("staff_id") ?? ""));
    if (p === "/me/evaluations/history") {
      const h = history("s-lan");
      const data = { full_name: "Lê Thị Lan", position: h.position, from: h.from, to: h.to, criteria: h.criteria, months: h.months };
      return ok({ linked: true, data } satisfies MyEvaluationHistory);
    }
    if (p === "/evaluations") {
      const e = evaluations(url.searchParams.get("month") ?? month);
      if (options.linkRequired) return ok({ ...e, link_required: true, staff: e.staff.map((x) => ({ ...x, can_record: false })) });
      return ok(
        options.scored
          ? { ...e, closed: true, staff: e.staff.map((x) => (x.staff_id === "s-hoa" ? { ...x, evaluation: SCORECARD } : x)) }
          : e,
      );
    }
    if (p.startsWith("/staff/profiles")) return ok(roster(permissions.includes("staff.manage")));
    if (p === "/staff/accounts") return ok(options.accounts ?? []);
    if (/^\/staff\/[^/]+\/account$/.test(p) && method === "PUT") return ok(roster(true)[1]);
    if (p === "/staff/password-reset-requests") return permissions.includes("staff.manage") ? ok(options.resetRequests ?? []) : ok({ code: "auth.forbidden" }, 403);
    if (/^\/staff\/password-reset-requests\/[^/]+\/dismiss$/.test(p)) return Promise.resolve(new Response(null, { status: 204 }));
    if (/^\/staff\/[^/]+\/login$/.test(p) && method === "POST") {
      if (options.reauthFirst && !reauthed.done) return ok({ code: "auth.reauth_required" }, 401);
      const lan = roster(true)[0] ?? roster(true)[1];
      return ok({ profile: { ...lan, details: { ...lan?.details, login: { phone: "0912 300 111", role: "manager", locked: false } } }, temporary_password: "ABCD-EFGH-JKLM" }, 201);
    }
    if (/^\/staff\/[^/]+\/login\/reset-password$/.test(p)) return ok({ temporary_password: "NEWT-EMPP-ASSW" }); // gitleaks:allow (a fake temporary password)
    if (p === "/auth/reauth" && options.reauthExpired) return ok({ code: "auth.unauthenticated" }, 401);
    if (p === "/auth/reauth") {
      reauthed.done = true;
      return Promise.resolve(new Response(null, { status: 204 }));
    }
    if (/^\/staff\/[^/]+\/history$/.test(p)) return ok([]);
    if (/^\/staff\/[^/]+\/end$/.test(p) && method === "POST" && options.endRefused)
      return ok(
        {
          code: "staff.has_upcoming_bookings",
          details: { bookings: [{ id: "a-1", date: "2026-11-20", time: "10:00", customer: "Nguyễn Thị Hằng" }], more: true },
        },
        409,
      );
    if (/^\/staff\/[^/]+\/end$/.test(p) && method === "POST") return ok(roster(true)[0]);
    if (/^\/staff\/[^/]+\/pay$/.test(p) && method === "PUT") {
      if (payRefused.left > 0) {
        payRefused.left -= 1;
        return ok({ code: "validation_failed", errors: [{ field: "base_salary", code: "out_of_range" }] }, 422);
      }
      return ok({});
    }
    if (p === "/staff" && method === "POST") {
      hired.version = 1;
      return ok({ ...roster(true)[1], id: "s-new", full_name: "Võ Thị Ngọc", version: hired.version }, 201);
    }
    if (p === "/staff/s-new" && method === "PUT") {
      hired.version += 1;
      return ok({ ...roster(true)[1], id: "s-new", full_name: "Võ Thị Ngọc", version: hired.version });
    }
    if (p === "/staff") return ok([]);
    return ok([]);
  });
}

const at = (path: string) => {
  const history = createMemoryHistory({ initialEntries: [path] });
  render(<App history={history} />);
  /** The current URL search as plain strings. */
  const search = () => Object.fromEntries(new URLSearchParams(history.location.search));
  return { search };
};

const requests = (spy: ReturnType<typeof serve>) =>
  spy.mock.calls.map(([input, init]) => ({
    url: typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
    method: (init?.method ?? "GET").toUpperCase(),
    body: typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : undefined,
  }));

const puts = (spy: ReturnType<typeof serve>, path: string) =>
  requests(spy).filter((r) => r.method === "PUT" && new URL(r.url, "http://app.test").pathname.endsWith(path));

const card = (id: string) => {
  const el = document.getElementById(`book-${id}`);
  if (!el) throw new Error(`no card for ${id}`);
  return within(el);
};

describe("staff helpers", () => {
  it("formats months and days like the sheet", () => {
    expect(monthLabel("2026-11")).toBe("Tháng 11/2026");
    expect(shiftMonth("2026-01", -1)).toBe("2025-12");
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(daysOf("2028-02")).toHaveLength(29);
    expect(weekdayShort("2026-11-08")).toBe("CN");
    expect(percentOfBp(500)).toBe("5%");
    expect(percentOfBp(750)).toBe("7,5%");
    expect(staffSearch.validate({ tab: "payroll", month: "2026-13", day: "x" })).toEqual({ tab: "payroll" });
  });

  it("keeps round-4 links working: attendance → daily book, adjustments → month sheet", () => {
    expect(staffSearch.parse({ tab: "attendance" }).tab).toBe("day");
    expect(staffSearch.parse({ tab: "adjustments" }).tab).toBe("month");
    expect(staffSearch.parse({ tab: "evaluation" }).tab).toBe("evaluation");
    expect(staffSearch.parse({ tab: "nope" }).tab).toBe("day");
  });

  it("prompts the monthly evaluation only from the month's last day to day 10 of the next", () => {
    expect(evaluationDueMonth("2026-11-30")).toBe("2026-11");
    expect(evaluationDueMonth("2026-12-01")).toBe("2026-11");
    expect(evaluationDueMonth("2026-12-10")).toBe("2026-11");
    expect(evaluationDueMonth("2027-01-05")).toBe("2026-12");
    expect(evaluationDueMonth("2026-12-11")).toBeNull();
    expect(evaluationDueMonth("2026-11-29")).toBeNull();
  });

  it("lists who is missing and the past days still incomplete", () => {
    const s = sheet();
    expect(missingOn(s.staff, today).map((p) => p.staff_id)).toEqual(["s-mai", "s-hoa"]);
    const people = s.staff.map((p) => ({ ...p, days: p.days.map((d) => ({ ...d, date: "2026-03-02" })) }));
    expect(incompleteDays("2026-03", people, "2026-03-04")).toEqual(["2026-03-01", "2026-03-02", "2026-03-03"]);
    const joined = people.map((p) => ({ ...p, joined_on: "2026-03-03" }));
    expect(incompleteDays("2026-03", joined, "2026-03-04")).toEqual(["2026-03-03"]);
    // ADR-0013: a rehired person's gap (03-02 here) is not a workday, whatever was recorded.
    const rehired = people.map((p) => ({
      ...p,
      periods: [
        { started_on: "2026-01-05", ended_on: "2026-03-01" },
        { started_on: "2026-03-03", ended_on: null },
      ],
    }));
    expect(incompleteDays("2026-03", rehired, "2026-03-04")).toEqual(["2026-03-01", "2026-03-03"]);
  });

  it("names the nav entries by what the user may do; technicians get 'Khách của tôi' in the tab bar", () => {
    const items = (perms: string[]) => navFor(ME(perms)).flatMap((g) => g.items);
    const item = (perms: string[]) => items(perms).find((i) => i.to === "/nhan-vien");
    expect(item(MANAGER)?.label).toBe("Quản lý nhân sự");
    expect(item(TECH)?.label).toBe("Công và lương của tôi");
    expect(item(["catalog.read"])).toBeUndefined();
    const mine = (perms: string[]) => items(perms).find((i) => i.to === "/khach-cua-toi");
    expect(mine(TECH)?.label).toBe("Khách của tôi");
    // EM 2026-11-08: a manager reads their own month and customers the way a technician does;
    // the owner keeps everyone in the books and has neither entry.
    const own = (perms: string[]) => items(perms).find((i) => i.to === "/nhan-vien/cua-toi");
    expect(own(MANAGER)?.label).toBe("Công và lương của tôi");
    expect(mine([...MANAGER, "visit.read"])?.label).toBe("Khách của tôi");
    expect(own(OWNER)).toBeUndefined();
    expect(mine([...OWNER, "visit.read"])).toBeUndefined();
    expect(own(TECH)).toBeUndefined();
    expect(item(MANAGER)?.exact).toBe(true);
    expect(tabsFor(navFor(ME(TECH))).map((t) => t.to)).toEqual(["/", "/nhan-vien", "/khach-cua-toi"]);
  });

  it("nudges for a login only for someone still working without one", () => {
    expect(withoutLogin({ login_linked: false, status: "active" })).toBe(true);
    expect(withoutLogin({ login_linked: true, status: "active" })).toBe(false);
    expect(withoutLogin({ login_linked: false, status: "left" })).toBe(false);
  });
});

describe("/nhan-vien", () => {
  it("manager: one daily book — attendance, praise and bonuses per person, with progress", async () => {
    const spy = serve(MANAGER);
    at("/nhan-vien");
    expect(await screen.findByRole("tab", { name: "Sổ hằng ngày" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Bảng công tháng" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Đánh giá tháng" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: /Thưởng/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Bảng lương" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Hồ sơ nhân viên" })).toBeInTheDocument();

    const progress = await screen.findByRole("status", { name: "Tiến độ chấm công" });
    expect(progress).toHaveTextContent("Đã chấm công 1/3 người");
    expect(screen.getByRole("button", { name: "Tới Trần Thị Mai" })).toBeInTheDocument();
    expect(card("s-lan").getByText(/muộn 12 phút/)).toBeInTheDocument();
    expect(card("s-lan").getByText(/Khách khen tay nghề/)).toBeInTheDocument();
    expect(card("s-mai").getByText("Chưa chấm công")).toBeInTheDocument();
    // EM 2026-11-07 (image 65): a card per person, titled with the name.
    expect(card("s-lan").getByRole("heading", { level: 2, name: "Lê Thị Lan" })).toBeInTheDocument();
    expect(card("s-mai").getByRole("heading", { level: 2, name: "Trần Thị Mai" })).toBeInTheDocument();

    // Attendance in words, one radiogroup per person; the chosen option says it, no extra chip.
    const lanChoice = card("s-lan").getByRole("radiogroup", { name: "Chấm công Lê Thị Lan" });
    expect(within(lanChoice).getAllByRole("radio").map((r) => r.textContent)).toEqual(["Đi làm", "Nghỉ có phép", "Nghỉ đột xuất", "Nghỉ không phép"]);
    expect(within(lanChoice).getByRole("radio", { name: "Đi làm" })).toHaveAttribute("aria-checked", "true");
    expect(card("s-lan").getAllByText("Đi làm")).toHaveLength(1);
    expect(card("s-lan").getByText("Đi muộn 12 phút")).toBeInTheDocument();
    expect(within(card("s-mai").getByRole("radiogroup")).queryByRole("radio", { checked: true })).not.toBeInTheDocument();

    await userEvent.click(card("s-mai").getByRole("radio", { name: "Nghỉ có phép" }));
    await waitFor(() => {
      expect(puts(spy, "/attendance")).toHaveLength(1);
    });
    expect(puts(spy, "/attendance")[0]?.body).toEqual({ entries: [{ staff_id: "s-mai", date: today, status: "leave_p" }] });

    // Praise inline: keeps the day's note.
    await userEvent.click(card("s-lan").getByRole("button", { name: "Thêm khen cho Lê Thị Lan" }));
    await waitFor(() => {
      expect(puts(spy, "/daily-reviews")).toHaveLength(1);
    });
    expect(puts(spy, "/daily-reviews")[0]?.body).toEqual({
      entries: [{ staff_id: "s-lan", date: today, compliments: 3, complaints: 0, note: "Khách khen tay nghề" }],
    });

    await userEvent.click(screen.getByRole("button", { name: "Cả spa đi làm" }));
    await waitFor(() => {
      expect(puts(spy, "/attendance")).toHaveLength(2);
    });
    expect(puts(spy, "/attendance")[1]?.body).toEqual({ entries: [{ staff_id: "s-mai", date: today, status: "present" }] });

    // Bonus/penalty opens for that person and day, no person picker.
    await userEvent.click(card("s-mai").getByRole("button", { name: "Ghi thưởng / phạt" }));
    const dialog = await screen.findByRole("dialog", { name: "Thưởng / phạt · Trần Thị Mai" });
    expect(within(dialog).queryByLabelText("Nhân viên")).not.toBeInTheDocument();
    expect(requests(spy).some((r) => r.url.includes("/payroll"))).toBe(false);
  });

  it("daily book: arrows move between options without saving; Space or Enter records the focused one", async () => {
    const spy = serve(MANAGER);
    at("/nhan-vien");
    const group = await waitFor(() => card("s-lan").getByRole("radiogroup"));
    const radios = within(group).getAllByRole("radio");
    // One tab stop per group: the chosen option.
    expect(radios.map((r) => r.tabIndex)).toEqual([0, -1, -1, -1]);
    radios[0]?.focus();
    await userEvent.keyboard("{ArrowRight}{ArrowRight}");
    expect(radios[2]).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}{End}");
    expect(radios[3]).toHaveFocus();
    await userEvent.keyboard("{Home}{ArrowLeft}");
    expect(radios[3]).toHaveFocus();
    expect(puts(spy, "/attendance")).toHaveLength(0);
    await userEvent.keyboard("{ArrowLeft} ");
    await waitFor(() => {
      expect(puts(spy, "/attendance")).toHaveLength(1);
    });
    expect(puts(spy, "/attendance")[0]?.body).toEqual({ entries: [{ staff_id: "s-lan", date: today, status: "leave_dx" }] });
  });

  it("daily book: shows the choice while saving, then rolls back and explains on the card when the save fails", async () => {
    let fail: (r: Response) => void = () => undefined;
    serve(MANAGER, {
      attendancePut: () =>
        new Promise<Response>((resolve) => {
          fail = resolve;
        }),
    });
    at("/nhan-vien");
    const group = await waitFor(() => card("s-mai").getByRole("radiogroup"));
    await userEvent.click(within(group).getByRole("radio", { name: "Nghỉ đột xuất" }));
    // Saving: the tapped option shows chosen at once, the others ignore taps but keep focus.
    expect(within(group).getByRole("radio", { name: "Nghỉ đột xuất" })).toHaveAttribute("aria-checked", "true");
    expect(within(group).getByRole("radio", { name: "Đi làm" })).toHaveAttribute("aria-disabled", "true");
    expect(within(group).getByRole("radio", { name: "Đi làm" })).toBeEnabled();
    expect(card("s-mai").getByRole("status")).toHaveTextContent("Đang lưu chấm công Trần Thị Mai");
    expect(card("s-mai").queryByText("Chưa chấm công")).not.toBeInTheDocument();

    fail(Response.json({ status: 500, code: "internal" }, { status: 500 }));
    expect(await card("s-mai").findByRole("alert")).toHaveTextContent(/^Chưa lưu được/);
    expect(within(group).queryByRole("radio", { checked: true })).not.toBeInTheDocument();
    expect(card("s-mai").getByText("Chưa chấm công")).toBeInTheDocument();
    expect(card("s-lan").queryByRole("alert")).not.toBeInTheDocument();
  });

  it("daily book: a closed month shows every choice read-only and no write buttons", async () => {
    serve(MANAGER, { closed: true });
    at("/nhan-vien");
    const group = await waitFor(() => card("s-lan").getByRole("radiogroup"));
    for (const r of within(group).getAllByRole("radio")) expect(r).toBeDisabled();
    expect(within(group).getByRole("radio", { name: "Đi làm" })).toHaveAttribute("aria-checked", "true");
    expect(card("s-lan").getByRole("button", { name: "Thêm khen cho Lê Thị Lan" })).toBeDisabled();
    expect(card("s-lan").queryByRole("button", { name: "Ghi thưởng / phạt" })).not.toBeInTheDocument();
    expect(card("s-lan").queryByRole("button", { name: "Giờ & ghi chú" })).not.toBeInTheDocument();
  });

  it("manager: the month sheet is a person × day grid with totals and the bonus log", async () => {
    serve(MANAGER);
    at("/nhan-vien?tab=month");
    const grid = await screen.findByRole("table", { name: /Bảng chấm công/ });
    expect(within(grid).getByRole("rowheader", { name: "Lê Thị Lan" })).toBeInTheDocument();
    expect(within(grid).getAllByRole("columnheader")).toHaveLength(1 + daysOf(month).length + 5);
    expect(within(grid).getByRole("button", { name: /Lê Thị Lan, .*: Đi làm, muộn 12 phút/ })).toBeInTheDocument();
    // Decision C: another manager's row is read-only, with the reason.
    expect(within(grid).getByRole("rowheader", { name: /Phạm Thị Hoa/ })).toHaveTextContent("Chủ spa chấm");
    expect(within(grid).queryByRole("button", { name: /Phạm Thị Hoa/ })).not.toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Sổ thưởng, phạt của tháng" })).toBeInTheDocument();
    // F20: no P / ĐX / K codes — a "Nghỉ" total, and a legend of the cells' own marks in words.
    expect(within(grid).getByRole("columnheader", { name: "Nghỉ" })).toBeInTheDocument();
    expect(screen.queryByText(/P\/ĐX\/K|ĐX/)).not.toBeInTheDocument();
    const legend = screen.getByRole("list", { name: "Chú thích" });
    expect(within(legend).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "Đi làm",
      "Nghỉ có phép",
      "Nghỉ đột xuất",
      "Nghỉ không phép",
      "–Chưa chấm",
      "Gạch đỏ dưới ô: đi muộn",
    ]);
    // "Giờ & ghi chú" (a cell): the options in words, no codes in brackets.
    await userEvent.click(within(grid).getByRole("button", { name: /Lê Thị Lan, .*: Đi làm, muộn 12 phút/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("radio", { name: /^Nghỉ có phép/ })).toBeInTheDocument();
    expect(within(dialog).queryByText(/\((P|ĐX|K)\)/)).not.toBeInTheDocument();
  });

  it("decision C: another manager's row has no write controls, only the reason", async () => {
    const spy = serve(MANAGER);
    at("/nhan-vien");
    const group = await waitFor(() => card("s-hoa").getByRole("radiogroup"));
    for (const r of within(group).getAllByRole("radio")) expect(r).toBeDisabled();
    expect(card("s-hoa").getByText(/Chỉ chủ spa chấm công, ghi khen\/phàn nàn và thưởng\/phạt cho quản lý/)).toBeInTheDocument();
    expect(card("s-hoa").getByRole("button", { name: "Thêm khen cho Phạm Thị Hoa" })).toBeDisabled();
    expect(card("s-hoa").queryByRole("button", { name: "Ghi thưởng / phạt" })).not.toBeInTheDocument();
    expect(card("s-hoa").queryByRole("button", { name: "Giờ & ghi chú" })).not.toBeInTheDocument();
    // "Cả spa đi làm" never includes a row the caller may not write.
    await userEvent.click(screen.getByRole("button", { name: "Cả spa đi làm" }));
    await waitFor(() => {
      expect(puts(spy, "/attendance")).toHaveLength(1);
    });
    expect(puts(spy, "/attendance")[0]?.body).toEqual({ entries: [{ staff_id: "s-mai", date: today, status: "present" }] });
  });

  it("daily book: a leave code for someone with visits that day asks first", async () => {
    const spy = serve(MANAGER, { lanServedToday: true });
    at("/nhan-vien");
    const group = await waitFor(() => card("s-lan").getByRole("radiogroup"));
    await userEvent.click(within(group).getByRole("radio", { name: "Nghỉ không phép" }));
    const ask = await screen.findByRole("dialog", { name: "Lê Thị Lan có lượt khách ngày này" });
    await userEvent.click(within(ask).getByRole("button", { name: "Thôi" }));
    expect(puts(spy, "/attendance")).toHaveLength(0);
    await userEvent.click(within(group).getByRole("radio", { name: "Nghỉ không phép" }));
    const again = await screen.findByRole("dialog", { name: "Lê Thị Lan có lượt khách ngày này" });
    await userEvent.click(within(again).getByRole("button", { name: "Vẫn chấm Nghỉ không phép" }));
    await waitFor(() => {
      expect(puts(spy, "/attendance")).toHaveLength(1);
    });
    expect(puts(spy, "/attendance")[0]?.body).toEqual({ entries: [{ staff_id: "s-lan", date: today, status: "absent_k" }] });
    // Someone without visits that day saves at once.
    await userEvent.click(card("s-mai").getByRole("radio", { name: "Nghỉ có phép" }));
    await waitFor(() => {
      expect(puts(spy, "/attendance")).toHaveLength(2);
    });
  });

  it("bonus/penalty: Lưu says what is missing, and an entry can be removed after a confirm", async () => {
    const spy = serve(MANAGER, { adjustments: [PENALTY] });
    at("/nhan-vien");
    await userEvent.click(await waitFor(() => card("s-mai").getByRole("button", { name: "Ghi thưởng / phạt" })));
    const form = await screen.findByRole("dialog", { name: "Thưởng / phạt · Trần Thị Mai" });
    expect(within(form).getByRole("button", { name: "Lưu" })).toBeDisabled();
    expect(within(form).getByText("Để lưu, cần nhập số tiền hoặc số điểm, ghi lý do.")).toBeInTheDocument();
    await userEvent.type(within(form).getByLabelText("Số tiền (đ)"), "100000");
    expect(within(form).getByText("Để lưu, cần ghi lý do.")).toBeInTheDocument();
    await userEvent.type(within(form).getByLabelText("Lý do"), "Khách khen");
    expect(within(form).getByRole("button", { name: "Lưu" })).toBeEnabled();
    await userEvent.click(within(form).getByRole("button", { name: "Đóng" }));

    await userEvent.click(card("s-lan").getByRole("button", { name: "Xóa Phạt 50.000đ của Lê Thị Lan" }));
    const confirm = await screen.findByRole("dialog", { name: /Xóa phạt 50\.000đ của Lê Thị Lan\?/ });
    await userEvent.click(within(confirm).getByRole("button", { name: "Xóa khoản này" }));
    await waitFor(() => {
      expect(requests(spy).filter((r) => r.method === "DELETE").map((r) => new URL(r.url, "http://app.test").pathname)).toEqual([
        "/api/v1/adjustments/adj-1",
      ]);
    });
  });

  // EM 2026-11-08: past and closed scorecards read back, and a row only the owner scores too.
  it("evaluation: every scored row opens its scorecard read-only, closed months and owner-only rows included", async () => {
    serve(MANAGER, { scored: true });
    at("/nhan-vien?tab=evaluation");
    await userEvent.click(await screen.findByRole("button", { name: "Xem phiếu đánh giá Phạm Thị Hoa" }));
    const card = await screen.findByRole("dialog", { name: "Phiếu đánh giá Phạm Thị Hoa" });
    expect(within(card).getByRole("rowheader", { name: "Kỹ thuật liệu trình" })).toBeInTheDocument();
    expect(within(card).getByText("32/40")).toBeInTheDocument();
    expect(within(card).getByText("81/100")).toBeInTheDocument();
    expect(within(card).getByText("Tay nghề tốt, cần đúng giờ hơn")).toBeInTheDocument();
    expect(within(card).getByText(/^Chấm bởi Chủ Spa/)).toBeInTheDocument();
    // Closed month and decision C: nothing to rescore.
    expect(within(card).queryByRole("button", { name: "Chấm lại" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Chấm (lại|điểm)$/ })).not.toBeInTheDocument();
  });

  it("evaluation history: a name opens the person's months, trend, scorecards and the month's days", async () => {
    const spy = serve(MANAGER);
    const { search } = at("/nhan-vien?tab=evaluation");
    await userEvent.click(await screen.findByRole("button", { name: "Trần Thị Mai: lịch sử đánh giá các tháng" }));
    expect(await screen.findByRole("heading", { name: "Lịch sử đánh giá · Trần Thị Mai" })).toBeInTheDocument();
    expect(search()).toMatchObject({ tab: "evaluation", staff: "s-mai" });
    // One request for the whole history.
    expect(requests(spy).filter((r) => r.url.includes("/evaluations/history?staff_id=s-mai"))).toHaveLength(1);
    const previous = shiftMonth(month, -1);
    expect(screen.getByRole("listitem", { name: `${monthLabel(previous)}: 81 điểm, Mức 2` })).toBeInTheDocument();
    expect(screen.getByRole("listitem", { name: `${monthLabel(month)}: chưa đánh giá` })).toBeInTheDocument();
    const days = screen.getByRole("link", { name: `Từng ngày ${monthLabel(previous).toLowerCase()}` });
    expect(days.getAttribute("href")).toContain(`month=${previous}`);
    expect(days.getAttribute("href")).toContain("staff=s-mai");
    // The open month may be scored; the closed one only read.
    expect(screen.getAllByRole("link", { name: /^Chấm (lại|điểm)$/ })).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: `Xem phiếu đánh giá ${monthLabel(previous).toLowerCase()}` }));
    expect(await screen.findByRole("dialog", { name: "Phiếu đánh giá Trần Thị Mai" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Xem mọi ghi chú về Trần Thị Mai" }).getAttribute("href")).toContain("staff=s-mai");
  });

  it("one person's month: their row of the grid and their days, read-only", async () => {
    serve(MANAGER, { adjustments: [PENALTY] });
    at("/nhan-vien?tab=month&staff=s-lan");
    const grid = await screen.findByRole("table", { name: /Bảng chấm công/ });
    expect(within(grid).getByRole("rowheader", { name: /Lê Thị Lan/ })).toBeInTheDocument();
    expect(within(grid).queryByRole("rowheader", { name: /Trần Thị Mai/ })).not.toBeInTheDocument();
    const days = screen.getByRole("region", { name: "Từng ngày · Lê Thị Lan" });
    expect(await within(days).findByText(/Đi muộn không báo/)).toBeInTheDocument();
    expect(within(days).getByText("Khách khen tay nghề")).toBeInTheDocument();
    expect(within(days).getByText(/muộn 12 phút/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Xem cả spa thay vì Lê Thị Lan" }));
    expect(await within(await screen.findByRole("table", { name: /Bảng chấm công/ })).findByRole("rowheader", { name: /Trần Thị Mai/ })).toBeInTheDocument();
  });

  it("technician: their own 12-month history, scorecards read-only, a month's days by moving the page", async () => {
    const spy = serve(TECH);
    const { search } = at("/nhan-vien");
    await userEvent.click(await screen.findByRole("button", { name: "Xem 12 tháng gần nhất" }));
    const list = await screen.findByRole("list", { name: "Các tháng của bạn" });
    const previous = shiftMonth(month, -1);
    expect(screen.getByRole("listitem", { name: `${monthLabel(previous)}: 81 điểm, Mức 2` })).toBeInTheDocument();
    // Only their own path; nothing to rescore, no notes journal.
    expect(requests(spy).some((r) => r.url.includes("/me/evaluations/history"))).toBe(true);
    expect(requests(spy).some((r) => /\/api\/v1\/evaluations/.test(r.url))).toBe(false);
    expect(within(list).queryByRole("link", { name: /^Chấm/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Xem mọi ghi chú/ })).not.toBeInTheDocument();
    await userEvent.click(within(list).getByRole("button", { name: `Xem phiếu đánh giá ${monthLabel(previous).toLowerCase()}` }));
    expect(await screen.findByRole("dialog", { name: "Phiếu đánh giá của bạn" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Đóng" }));
    await userEvent.click(within(list).getByRole("button", { name: `Từng ngày ${monthLabel(previous).toLowerCase()}` }));
    await waitFor(() => {
      expect(search()).toMatchObject({ month: previous });
    });
  });

  it("technician: reads their own month's scorecard, from /me/staff only", async () => {
    const spy = serve(TECH, { scored: true });
    at("/nhan-vien");
    await userEvent.click(await screen.findByRole("button", { name: "Xem phiếu đánh giá" }));
    const card = await screen.findByRole("dialog", { name: "Phiếu đánh giá của bạn" });
    expect(within(card).getByText("Tay nghề tốt, cần đúng giờ hơn")).toBeInTheDocument();
    expect(requests(spy).some((r) => r.url.includes("/evaluations"))).toBe(false);
  });

  it("monthly evaluation: starts empty, saves only when every criterion has a score, explains the levels", async () => {
    const spy = serve(MANAGER);
    at("/nhan-vien?tab=evaluation");
    // Decision C: no scoring button on the manager's own row.
    expect(await screen.findByText("Chỉ chủ spa chấm cho quản lý")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Chấm điểm" }));
    const sheet = await screen.findByRole("dialog", { name: "Đánh giá Trần Thị Mai" });
    const technique = within(sheet).getByLabelText(/Kỹ thuật liệu trình/);
    expect(technique).toHaveValue("");
    expect(within(sheet).getByText(/Mức 1 \(Xuất sắc\) = 100% điểm tối đa/)).toBeInTheDocument();
    expect(within(sheet).getByText("Mức 1: 40 · Mức 2: 32 · Mức 3: 24 · Chưa đạt: ≤ 12")).toBeInTheDocument();
    await userEvent.click(within(sheet).getByRole("button", { name: "Lưu đánh giá" }));
    expect(within(sheet).getByRole("alert")).toHaveTextContent("Còn 2 tiêu chí chưa nhập điểm");
    expect(puts(spy, "/evaluations")).toHaveLength(0);
    await userEvent.type(technique, "45");
    expect(within(sheet).getByText("Tối đa 40 điểm.")).toBeInTheDocument();
    await userEvent.clear(technique);
    await userEvent.type(technique, "36");
    await userEvent.type(within(sheet).getByLabelText(/Thái độ và giao tiếp/), "12");
    await userEvent.click(within(sheet).getByRole("button", { name: "Lưu đánh giá" }));
    // 48/100 here (two criteria in the fixture) → Chưa đạt: the coaching plan is asked for.
    expect(within(sheet).getByRole("alert")).toHaveTextContent("cần ghi kế hoạch kèm cặp");
    await userEvent.type(within(sheet).getByLabelText(/Kế hoạch kèm cặp/), "Kèm kỹ thuật");
    await userEvent.click(within(sheet).getByRole("button", { name: "Lưu đánh giá" }));
    await waitFor(() => {
      expect(puts(spy, "/evaluations")).toHaveLength(1);
    });
    expect(puts(spy, "/evaluations")[0]?.body).toMatchObject({
      staff_id: "s-mai",
      criteria: [
        { key: "technique", points: 36 },
        { key: "attitude", points: 12 },
      ],
      coaching_plan: "Kèm kỹ thuật",
    });
  });

  it("a negative bonus − penalty net uses the same minus sign as the deductions", () => {
    expect(signedVnd(-50_000)).toBe(`−50.000đ`);
    expect(signedVnd(20_000)).toBe("20.000đ");
    expect(signedVnd(0)).toBe("0đ");
  });

  it("owner: the pay rules stop at the server's bounds and say so", async () => {
    serve(OWNER);
    at("/nhan-vien?tab=payroll");
    await userEvent.click(await screen.findByRole("button", { name: "Quy tắc lương" }));
    const sheet = await screen.findByRole("dialog", { name: "Quy tắc lương của spa" });
    const days = await within(sheet).findByLabelText("Số ngày nghỉ không trừ lương mỗi tháng");
    await userEvent.clear(days);
    await userEvent.type(days, "11");
    expect(within(sheet).getByText("Tối đa 10 ngày.")).toBeInTheDocument();
    await userEvent.type(within(sheet).getByLabelText("Muộn dưới (phút) không tính"), "00");
    expect(within(sheet).getByText("Tối đa 240 phút.")).toBeInTheDocument();
    expect(within(sheet).getByRole("button", { name: "Lưu quy tắc" })).toBeDisabled();
  });

  it("bonus / penalty: points stop at 100, and the server's field errors show where they belong", async () => {
    serve(MANAGER, { adjustmentRefused: { field: "corrects_month", code: "not_closed" } });
    at("/nhan-vien");
    await userEvent.click(await waitFor(() => card("s-mai").getByRole("button", { name: "Ghi thưởng / phạt" })));
    const dialog = await screen.findByRole("dialog", { name: "Thưởng / phạt · Trần Thị Mai" });
    const points = within(dialog).getByLabelText("Điểm");
    await userEvent.type(within(dialog).getByLabelText("Lý do"), "Khách khen");
    await userEvent.type(points, "150");
    expect(within(dialog).getByText("Từ 1 đến 100 điểm.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Lưu" })).toBeDisabled();

    await userEvent.clear(points);
    await userEvent.type(points, "15");
    await userEvent.click(within(dialog).getByRole("button", { name: "Lưu" }));
    expect(await within(dialog).findByText("Tháng này chưa chốt, không cần điều chỉnh.")).toBeInTheDocument();
  });

  it("the month's bonus log names someone no longer in the active list from the month's sheet", async () => {
    serve(MANAGER, { adjustments: [{ ...PENALTY, staff_id: "s-mai" }] });
    at("/nhan-vien?tab=month");
    const log = await screen.findByRole("table", { name: "Sổ thưởng phạt" });
    expect(await within(log).findByText("Trần Thị Mai")).toBeInTheDocument();
    expect(within(log).queryByText("Nhân viên đã nghỉ")).not.toBeInTheDocument();
  });

  it("owner: payroll with net pay and a payslip that shows every line", async () => {
    serve(OWNER);
    at("/nhan-vien?tab=payroll");
    expect(await screen.findByText("Tạm tính")).toBeInTheDocument();
    await userEvent.click(await screen.findByRole("button", { name: "Lê Thị Lan" }));
    const sheetDialog = await screen.findByRole("dialog", { name: "Phiếu lương Lê Thị Lan" });
    expect(within(sheetDialog).getByText("Thực lĩnh")).toBeInTheDocument();
    expect(within(sheetDialog).getByText(/5\.315\.000/)).toBeInTheDocument();
    expect(within(sheetDialog).getByText("Gội đầu thảo dược")).toBeInTheDocument();
    // Two separate credits (Q-U3): performing, and upsell (packages + one-off services).
    expect(within(sheetDialog).getByText("Hoa hồng làm liệu trình")).toBeInTheDocument();
    expect(within(sheetDialog).getByText("Hoa hồng giới thiệu / chốt")).toBeInTheDocument();
    expect(within(sheetDialog).queryByRole("region", { name: "Giới thiệu / chốt được" })).not.toBeInTheDocument();
  });

  it("payslip lists what the upsell commission is computed from", () => {
    const base = slip();
    const withUpsell: Payslip = {
      ...base,
      closing_commission_base: 1_539_000,
      closing_commission: 76_950,
      upsell_package_base: 1_500_000,
      upsell_service_base: 39_000,
      upsell_lines: [
        { kind: "package", description: "Gội đầu 10 buổi + tặng 2", qty: 1, value: 1_500_000 },
        { kind: "service", description: "Ngâm chân thảo dược", qty: 1, value: 39_000 },
      ],
    };
    render(<PayslipView slip={withUpsell} closed={false} />);
    const [region] = screen.getAllByRole("region", { name: "Giới thiệu / chốt được" });
    if (!region) throw new Error("upsell section missing");
    expect(within(region).getAllByText("Gội đầu 10 buổi + tặng 2").length).toBeGreaterThan(0);
    expect(within(region).getByText("Dịch vụ lẻ giới thiệu")).toBeInTheDocument();
    expect(within(region).getByText("Gói liệu trình chốt được")).toBeInTheDocument();
    // The table ends in the payslip's base and the commission it becomes.
    expect(within(region).getByRole("row", { name: /Tổng tính hoa hồng 2 1\.539\.000đ/ })).toBeInTheDocument();
    expect(within(region).getByRole("row", { name: /Thành hoa hồng giới thiệu \/ chốt 5% × 1\.539\.000đ 76\.950đ/ })).toBeInTheDocument();
    expect(screen.getByText(/gói 1\.500\.000đ · dịch vụ lẻ 39\.000đ/)).toBeInTheDocument();
  });

  it("technician: own month as a calendar with totals, never the management APIs", async () => {
    const spy = serve(TECH);
    at("/nhan-vien");
    expect(await screen.findByRole("heading", { level: 1, name: "Công và lương của tôi" })).toBeInTheDocument();
    // Summary first: net pay, and the evaluation's status where it is read (not alone at the end).
    const summary = await screen.findByRole("region", { name: "Tổng quan tháng" });
    expect(within(summary).getByText(/5\.315\.000/)).toBeInTheDocument();
    expect(within(summary).getByText("Chưa đánh giá")).toBeInTheDocument();
    expect(screen.queryByText("⚠ Chưa đánh giá tháng")).not.toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    const calendar = screen.getByRole("list", { name: /Lịch công/ });
    const cells = within(calendar).getAllByRole("listitem").filter((li) => li.childElementCount > 0);
    expect(cells).toHaveLength(daysOf(month).length);
    // Days up to today open their detail; later days are quiet, not controls.
    const pastDays = daysOf(month).filter((d) => d <= today).length;
    expect(within(calendar).getAllByRole("button")).toHaveLength(pastDays);
    const late = within(calendar).getByRole("button", { name: /Đi làm, muộn 12 phút/ });
    expect(late).toHaveTextContent("12′");
    // F20: the days off in words, never "Nghỉ P / ĐX" or "Nghỉ K".
    const work = screen.getByRole("region", { name: "Ngày công" });
    for (const label of ["Nghỉ có phép", "Nghỉ đột xuất", "Nghỉ không phép"]) expect(within(work).getAllByText(label).length).toBeGreaterThan(0);
    expect(within(work).queryByText(/Nghỉ P|Nghỉ K|ĐX/)).not.toBeInTheDocument();
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname);
    expect(paths.some((p) => p.endsWith("/attendance") || p.endsWith("/payroll"))).toBe(false);
  });

  it("technician: a day opens its detail, with prev / next day, from /me/staff/day only", async () => {
    const spy = serve(TECH);
    at("/nhan-vien");
    const calendar = await screen.findByRole("list", { name: /Lịch công/ });
    await userEvent.click(within(calendar).getByRole("button", { name: /Đi làm, muộn 12 phút/ }));
    const sheetDialog = await screen.findByRole("dialog", { name: new RegExp(`Ngày .*${today.slice(8)}/${today.slice(5, 7)}`) });
    expect(await within(sheetDialog).findByText("Muộn 12 phút")).toBeInTheDocument();
    expect(within(sheetDialog).getByText("09:12 / 18:00")).toBeInTheDocument();
    expect(within(sheetDialog).getByText("Kẹt xe")).toBeInTheDocument();
    expect(within(sheetDialog).getByText("Khách khen tay nhẹ")).toBeInTheDocument();
    expect(within(sheetDialog).getByText("Ở lại dọn phòng")).toBeInTheDocument();
    expect(within(sheetDialog).getByText("Buổi liệu trình")).toBeInTheDocument();
    expect(within(sheetDialog).getByRole("row", { name: /Cộng 1 lượt khách 198\.000đ 9\.900đ/ })).toBeInTheDocument();
    expect(within(sheetDialog).getByText(/Chốt gói · Nguyễn Thị Hằng/)).toBeInTheDocument();
    // EM 2026-11-07 item 3: the session note of a visit I worked on.
    expect(within(sheetDialog).getByText("Ghi chú buổi: Khách đau vai gáy")).toBeInTheDocument();
    expect(within(sheetDialog).queryByText(/\+84|09\d{8}/)).not.toBeInTheDocument();
    // Next day is disabled on today; previous day reloads the sheet for that date.
    expect(within(sheetDialog).getByRole("button", { name: /Ngày sau/ })).toBeDisabled();
    await userEvent.click(within(sheetDialog).getByRole("button", { name: /Ngày trước/ }));
    const yesterday = shiftDay(today, -1);
    await waitFor(() => {
      expect(requests(spy).some((r) => r.url.includes(`/me/staff/day?date=${yesterday}`))).toBe(true);
    });
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname.replace(/^\/api\/v1/, ""));
    expect(paths.every((p) => p === "/me" || p.startsWith("/me/"))).toBe(true);
  });
});

describe("former staff (ADR-0014)", () => {
  const FORMER = ["staff.read_own_past"];
  const left = `${shiftMonth(month, -1)}-20`;

  it("sees only their own past months, from the month they left, with a minimal nav", async () => {
    const spy = serve(FORMER, { leftOn: left });
    at("/nhan-vien");
    expect(await screen.findByRole("heading", { level: 1, name: "Công và lương của tôi" })).toBeInTheDocument();
    expect(await screen.findByText(/Bạn đã nghỉ việc từ .*20\//)).toBeInTheDocument();
    expect(screen.getByText(monthLabel(left.slice(0, 7)))).toBeInTheDocument();
    // No month after they left.
    expect(screen.getByRole("button", { name: "Tháng sau" })).toBeDisabled();
    expect(navFor(ME(FORMER)).flatMap((g) => g.items.map((i) => i.to))).toEqual(["/", "/nhan-vien", "/cai-dat"]);
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname.replace(/^\/api\/v1/, ""));
    expect(paths.every((p) => p === "/me" || p.startsWith("/me/staff"))).toBe(true);
    expect(requests(spy).some((r) => r.url.endsWith("/me/staff"))).toBe(true);
  });

  it("explains a month they did not work in", async () => {
    serve(FORMER, { leftOn: left });
    at(`/nhan-vien?month=${month}`);
    expect(await screen.findByText("Tháng này bạn không làm ở spa")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Về tháng nghỉ việc" })).toBeInTheDocument();
  });

  it("cannot open Khách của tôi", async () => {
    serve(FORMER, { leftOn: left });
    at("/khach-cua-toi");
    expect(await screen.findByText(/Không có quyền/)).toBeInTheDocument();
  });
});

describe("payslip work tables", () => {
  const lines = [
    { service_name: "An Kiên Lạc", source: "package", qty: 2, value: 321_100, full_value: 321_100 },
    { service_name: "An Kiên Lạc", source: "one_off", qty: 1, value: 249_000, full_value: 249_000 },
    { service_name: "Thư Giãn", source: "package", qty: 3, value: 0, full_value: 237_000 },
    { service_name: "Thư Giãn", source: "one_off", qty: 1, value: 79_000, full_value: 79_000 },
  ];

  it("groups by type, each service once per group, with subtotals adding up to the payslip base", () => {
    const table = serviceWork({ ...slip(), commission_lines: lines, service_commission_base: 649_100, service_commission: 32_455 });
    expect(table.groups.map((g) => [g.label, g.rows.map((r) => r.name), g.qty, g.value])).toEqual([
      ["Dịch vụ lẻ", ["An Kiên Lạc", "Thư Giãn"], 2, 328_000],
      ["Buổi liệu trình", ["An Kiên Lạc", "Thư Giãn"], 5, 321_100],
    ]);
    expect(table.groups.reduce((n, g) => n + g.value, 0)).toBe(table.value);
    expect(table).toMatchObject({ qty: 7, value: 649_100, commission: 32_455, rateBp: 500 });
  });

  it("counts gift sessions at full price when the payslip's base says the spa does", () => {
    expect(countsFullValue(lines, 886_100)).toBe(true);
    expect(countsFullValue(lines, 649_100)).toBe(false);
    const table = serviceWork({ ...slip(), commission_lines: lines, service_commission_base: 886_100 });
    expect(table.groups[1]?.value).toBe(558_100);
  });

  it("upsell groups by kind; a refund is its own block and is not counted as an item sold", () => {
    const table = upsellWork({
      ...slip(),
      closing_commission_base: 1_239_000,
      upsell_lines: [
        { kind: "service", description: "Ngâm chân", qty: 1, value: 39_000 },
        { kind: "package", description: "Gói 10 buổi", qty: 1, value: 1_500_000 },
        { kind: "package_refund", description: "Gói 5 buổi", qty: 1, value: -300_000 },
      ],
    });
    expect(table.groups.map((g) => g.label)).toEqual(["Gói liệu trình chốt được", "Dịch vụ lẻ giới thiệu", "Hoàn tiền gói (trừ lại)"]);
    expect(table.qty).toBe(2);
    expect(table.groups.reduce((n, g) => n + g.value, 0)).toBe(table.value);
  });

  it("renders the groups as one table with fixed columns, tied to the payslip's commission", () => {
    render(<PayslipView slip={{ ...slip(), commission_lines: lines, service_commission_base: 649_100, service_commission: 32_455 }} closed={false} />);
    const region = screen.getByRole("region", { name: "Dịch vụ đã làm" });
    expect(within(region).getAllByRole("rowgroup").length).toBeGreaterThanOrEqual(3);
    expect(within(region).getByRole("row", { name: /Cộng dịch vụ lẻ 2 328\.000đ/ })).toBeInTheDocument();
    expect(within(region).getByRole("row", { name: /Cộng buổi liệu trình 5 321\.100đ/ })).toBeInTheDocument();
    expect(within(region).getByRole("row", { name: /Thành hoa hồng làm 5% × 649\.100đ 32\.455đ/ })).toBeInTheDocument();
    // The pending evaluation is a chip with the status line, before the figures.
    expect(screen.getByRole("list", { name: "Cần xem" })).toHaveTextContent("Chưa đánh giá tháng");
  });

  it("day totals add the lines; unknown commission stays unknown", () => {
    const d = myDay(today).data;
    if (!d) throw new Error("fixture");
    expect(dayTotals(d)).toEqual({ value: 198_000, commission: 9_900, upsell: 1_200_000, upsellCommission: 60_000 });
    expect(dayTotals({ ...d, commission_rate_bp: null }).commission).toBeNull();
    expect(shiftDay("2026-03-01", -1)).toBe("2026-02-28");
  });
});

describe("/khach-cua-toi", () => {
  it("technician: own bookings and visits as blocks on a week calendar, read-only, from /me/appointments only", async () => {
    const spy = serve(TECH);
    at("/khach-cua-toi");
    expect(await screen.findByRole("heading", { level: 1, name: "Khách của tôi" })).toBeInTheDocument();
    const grid = await screen.findByRole("group", { name: "Lịch của tôi trong tuần" });
    // Blocks at their real length: 10:00–11:10 (70 min), 14:00–15:00.
    const visit = within(grid).getByRole("button", { name: /^10:00–11:10 Nguyễn Thị Hằng, Xong, Không đặt lịch/ });
    expect(visit).toHaveAttribute("data-kind", "visit");
    expect(within(grid).getByRole("button", { name: /^14:00–15:00 Đỗ Văn Bình, Chưa đến, Nhóm · 3 người/ })).toBeInTheDocument();
    // Read-only: no booking anywhere, no slot buttons.
    expect(screen.queryByRole("button", { name: /Đặt lịch/ })).not.toBeInTheDocument();
    expect(screen.getByText(/Xong 1 · Đang làm 0 · Chưa đến 1/)).toBeInTheDocument();
    await userEvent.click(visit);
    const detail = await screen.findByRole("dialog", { name: "10:00–11:10 · Nguyễn Thị Hằng" });
    expect(within(detail).getByText("Buổi liệu trình", { exact: false })).toBeInTheDocument();
    expect(within(detail).getByText("70 phút")).toBeInTheDocument();
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname.replace(/^\/api\/v1/, ""));
    expect(paths.filter((p) => p !== "/me")).toEqual(expect.arrayContaining(["/me/appointments"]));
    expect(paths.some((p) => /^\/(visits|customers|appointments)/.test(p))).toBe(false);
    expect(requests(spy).some((r) => r.url.includes(`from=${week}`))).toBe(true);
  });

  it("the list view groups the week by day; the day view is one column", async () => {
    serve(TECH);
    at("/khach-cua-toi?kieu=ds");
    // The day's own section ("CN, 08/11/2026"): on a Sunday the week's title also ends with today.
    const today_ = await screen.findByRole("region", {
      name: new RegExp(`^(T\\d|CN), ${today.slice(8)}/${today.slice(5, 7)}`),
    });
    expect(within(today_).getAllByRole("button")).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "Ngày" }));
    expect(await screen.findByRole("group", { name: /Lịch của tôi ngày/ })).toBeInTheDocument();
  });

  it("opens the grid on now today, on opening hours another day (F22)", () => {
    const at = (start: number, minutes = 60) => ({ start, minutes }) as EntryView;
    const base = { gridOpens: 120, gridCloses: 1260, opens: 540, now: 900, showsToday: false, shown: [at(120), at(600)] };
    // A 02:00 visit widens the grid; another day still opens at 09:00.
    expect(focusMinute(base)).toBe(540);
    // Today: an hour before now (15:00 → 14:00).
    expect(focusMinute({ ...base, showsToday: true })).toBe(840);
    // Only work before opening: its first block.
    expect(focusMinute({ ...base, shown: [at(150)] })).toBeNull();
    expect(focusMinute({ ...base, gridOpens: 0, shown: [at(185)] })).toBe(180);
    // The grid already starts there: nothing to scroll past.
    expect(focusMinute({ ...base, gridOpens: 540, shown: [at(600)] })).toBeNull();
  });

  it("draws the technician's own work outside opening hours instead of hiding it", () => {
    const at = (start: number, minutes: number) => ({ start, minutes }) as EntryView;
    // Inside 09:00–21:00: the spa's hours.
    expect(gridSpan(540, 1260, 15, [at(600, 60)])).toEqual({ opens: 540, closes: 1260 });
    // A walk-in at 00:03 (VN midnight) and one ending 22:40: widened to whole slots around them.
    expect(gridSpan(540, 1260, 15, [at(3, 70), at(1300, 60)])).toEqual({ opens: 0, closes: 1365 });
    // Never past the day's edges.
    expect(gridSpan(540, 1260, 15, [at(1430, 60)])).toEqual({ opens: 540, closes: 1440 });
  });

  it("summarises the phases of the shown day or week", () => {
    const entries = calendar().data?.entries ?? [];
    expect(weekSummary(entries, null)).toBe("Xong 1 · Đang làm 0 · Chưa đến 1");
    expect(weekSummary(entries, "2000-01-01")).toBe("Xong 0 · Đang làm 0 · Chưa đến 0");
  });

});

describe("Hồ sơ nhân viên (roster)", () => {
  it("says how long someone has worked, from the server's tenure", () => {
    expect(tenureLabel({ tenure_days: 615, tenure_months: 20 })).toBe("1 năm 8 tháng");
    expect(tenureLabel({ tenure_days: 400, tenure_months: 12 })).toBe("1 năm");
    expect(tenureLabel({ tenure_days: 19, tenure_months: 0 })).toBe("19 ngày");
    expect(tenureLabel({ tenure_days: null, tenure_months: null })).toBe("Chưa rõ ngày vào làm");
  });

  it("manager: reads the roster (working first, former staff behind a filter) but changes nothing", async () => {
    serve(MANAGER);
    at("/nhan-vien?tab=profiles");
    const table = await screen.findByRole("table", { name: "Danh sách nhân sự" });
    expect(within(table).getByText("Lê Thị Lan")).toBeInTheDocument();
    expect(within(table).getByText("1 năm 8 tháng")).toBeInTheDocument();
    expect(within(table).queryByText("Đỗ Thị Hằng")).not.toBeInTheDocument();
    expect(within(table).getAllByText("Đã gắn")).toHaveLength(1);
    expect(screen.queryByRole("button", { name: /Thêm nhân viên/ })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Đã nghỉ (1)" }));
    const left = await screen.findByRole("table", { name: "Danh sách nhân sự" });
    expect(within(left).getByText("Đỗ Thị Hằng")).toBeInTheDocument();
    expect(within(left).getByText("Đã nghỉ từ 29/08/2026")).toBeInTheDocument();

    await userEvent.click(within(left).getByRole("button", { name: "Đỗ Thị Hằng" }));
    const sheet = await screen.findByRole("dialog", { name: "Đỗ Thị Hằng" });
    expect(within(sheet).getByText("06/01/2025 – 29/08/2026")).toBeInTheDocument();
    // No reason for a manager (the server never sends it), no write buttons.
    expect(within(sheet).queryByText(/Chuyển về quê/)).not.toBeInTheDocument();
    expect(within(sheet).queryByRole("button", { name: "Nhận lại làm" })).not.toBeInTheDocument();
    expect(within(sheet).queryByRole("heading", { name: "Tài khoản đăng nhập" })).not.toBeInTheDocument();
  });

  it("owner: a refused pay keeps the saved profile, and saving again updates it instead of hiring twice", async () => {
    const spy = serve(OWNER, { payRefusedOnce: true });
    at("/nhan-vien?tab=profiles");
    await userEvent.click(await screen.findByRole("button", { name: /Thêm nhân viên/ }));
    const form = await screen.findByRole("dialog", { name: "Thêm nhân viên" });
    const base = within(form).getByLabelText("Lương cứng thực nhận / tháng (đ)");
    await userEvent.type(within(form).getByLabelText("Họ tên"), "Võ Thị Ngọc");
    await userEvent.type(base, "2000000000");
    await userEvent.click(within(form).getByRole("button", { name: "Lưu hồ sơ" }));
    expect(await within(form).findByText(/^Tối đa 1\.000\.000\.000/)).toBeInTheDocument();

    await userEvent.clear(base);
    await userEvent.type(base, "6000000");
    await userEvent.click(within(form).getByRole("button", { name: "Lưu hồ sơ" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: "Thêm nhân viên" })).not.toBeInTheDocument();
    });
    const writes = requests(spy)
      .map((r, i) => ({ ...r, path: new URL(r.url, "http://app.test").pathname.replace(/^\/api\/v1/, ""), init: spy.mock.calls[i]?.[1] }))
      .filter((r) => r.method !== "GET");
    expect(writes.map((r) => `${r.method} ${r.path}`)).toEqual([
      "POST /staff",
      "PUT /staff/s-new/pay",
      "PUT /staff/s-new",
      "PUT /staff/s-new/pay",
    ]);
    // The update carries the version the create answered, not a stale one.
    expect(new Headers(writes[2]?.init?.headers).get("If-Match")).toBe('"1"');
  });

  it("owner: ends employment with a last day and a reason, confirmed", async () => {
    const spy = serve(OWNER);
    at("/nhan-vien?tab=profiles");
    const table = await screen.findByRole("table", { name: "Danh sách nhân sự" });
    // People without a login: nudged in Hồ sơ nhân viên (they count as technicians until they have one).
    expect(screen.getByText(/Chưa có tài khoản đăng nhập: Phạm Thị Hoa\./)).toBeInTheDocument();
    // "Tài khoản" shows the phone the person signs in with (ADR-0019 D20), not the login's name.
    expect(within(table).getByText("0912 300 111")).toBeInTheDocument();
    expect(within(table).queryByText("KTV Lan")).not.toBeInTheDocument();
    await userEvent.click(within(table).getByRole("button", { name: "Lê Thị Lan" }));
    const sheet = await screen.findByRole("dialog", { name: "Lê Thị Lan" });
    expect(within(sheet).getByRole("heading", { name: "Tài khoản đăng nhập" })).toBeInTheDocument();
    await userEvent.click(within(sheet).getByRole("button", { name: "Cho nghỉ việc" }));
    const confirm = await screen.findByRole("dialog", { name: "Cho Lê Thị Lan nghỉ việc" });
    // ADR-0014: the login stays, reaching only past pay, unless the owner locks it.
    expect(within(confirm).getByText(/Lê Thị Lan vẫn đăng nhập được, nhưng chỉ xem ngày công và phiếu lương/)).toBeInTheDocument();
    await userEvent.click(within(confirm).getByRole("checkbox", { name: /Khóa tài khoản/ }));
    expect(within(confirm).getByText(/Tài khoản đăng nhập của Lê Thị Lan sẽ bị khóa/)).toBeInTheDocument();
    const submit = within(confirm).getByRole("button", { name: "Xác nhận nghỉ việc" });
    expect(submit).toBeDisabled();
    await userEvent.type(within(confirm).getByLabelText("Lý do nghỉ việc"), "Chuyển công tác");
    await userEvent.click(submit);
    await waitFor(() => {
      expect(requests(spy).some((r) => r.method === "POST" && r.url.endsWith("/staff/s-lan/end"))).toBe(true);
    });
    const post = requests(spy).find((r) => r.url.endsWith("/staff/s-lan/end"));
    expect(post?.body).toEqual({ ended_on: today, reason: "Chuyển công tác", lock_login: true });
    // ADR-0019 D20: done means back to the roster, not to the person's sheet.
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  it("owner: a refusal for future bookings lists them, each opening its day of the calendar (F24)", async () => {
    serve(OWNER, { endRefused: true });
    at("/nhan-vien?tab=profiles");
    const table = await screen.findByRole("table", { name: "Danh sách nhân sự" });
    await userEvent.click(within(table).getByRole("button", { name: "Lê Thị Lan" }));
    const sheet = await screen.findByRole("dialog", { name: "Lê Thị Lan" });
    await userEvent.click(within(sheet).getByRole("button", { name: "Cho nghỉ việc" }));
    const confirm = await screen.findByRole("dialog", { name: "Cho Lê Thị Lan nghỉ việc" });
    await userEvent.type(within(confirm).getByLabelText("Lý do nghỉ việc"), "Chuyển công tác");
    await userEvent.click(within(confirm).getByRole("button", { name: "Xác nhận nghỉ việc" }));
    const list = await within(confirm).findByRole("list", { name: "Lịch hẹn còn gán cho nhân viên" });
    const link = within(list).getByRole("link", { name: "20/11/2026 lúc 10:00 · Nguyễn Thị Hằng" });
    expect(link.getAttribute("href")).toBe("/quay?view=lich&ngay=2026-11-20&ktv=s-lan");
    expect(within(confirm).getByText("Còn lịch khác sau các lịch trên.")).toBeInTheDocument();
  });

  it("owner: older logins with no profile are listed, each linked once to its profile; never the owner's", async () => {
    const accounts: StaffAccount[] = [
      { user_id: "u-owner", full_name: "Chủ Spa", roles: ["Chủ spa"], staff_id: null, is_manager: false, manages_staff: true },
      { user_id: "u-ha", full_name: "Quản lý Hà", roles: ["Quản lý"], staff_id: null, is_manager: true, manages_staff: false },
      { user_id: "u-lan", full_name: "KTV Lan", roles: ["Kỹ thuật viên"], staff_id: "s-lan", is_manager: false, manages_staff: false },
    ];
    const spy = serve(OWNER, { accounts });
    at("/nhan-vien");
    const warning = await screen.findByRole("status", { name: "Tài khoản đăng nhập chưa gắn với hồ sơ" });
    // A "Quản lý" title without a login is not a manager: only the manager login is listed.
    expect(within(warning).queryByText("Phạm Thị Hoa", { selector: "strong" })).not.toBeInTheDocument();
    expect(within(warning).getByText("Quản lý Hà")).toBeInTheDocument();
    expect(within(warning).queryByText("Chủ Spa", { selector: "strong" })).not.toBeInTheDocument();
    await userEvent.click(within(warning).getByRole("button", { name: "Mở Hồ sơ nhân viên" }));
    expect(await screen.findByRole("table", { name: "Danh sách nhân sự" })).toBeInTheDocument();
    const listed = screen.getByRole("status", { name: "Tài khoản đăng nhập chưa gắn với hồ sơ" });
    // ADR-0016: the one-time path for CLI-made logins — pick the profile, "Gắn vào hồ sơ".
    await userEvent.selectOptions(within(listed).getByLabelText("Hồ sơ của Quản lý Hà"), "s-hoa");
    await userEvent.click(within(listed).getByRole("button", { name: "Gắn vào hồ sơ" }));
    await waitFor(() => {
      expect(requests(spy).some((r) => r.method === "PUT" && r.url.endsWith("/staff/s-hoa/account"))).toBe(true);
    });
    expect(requests(spy).find((r) => r.url.endsWith("/staff/s-hoa/account"))?.body).toEqual({ user_id: "u-ha" });
  });

  it("manager: no link warning and no request for the logins (owner's data)", async () => {
    const spy = serve(MANAGER, { accounts: [{ user_id: "u-ha", full_name: "Quản lý Hà", roles: [], staff_id: null, is_manager: true, manages_staff: false }] });
    at("/nhan-vien?tab=profiles");
    await screen.findByRole("table", { name: "Danh sách nhân sự" });
    expect(screen.queryByRole("status", { name: "Tài khoản đăng nhập chưa gắn với hồ sơ" })).not.toBeInTheDocument();
    expect(requests(spy).some((r) => r.url.includes("/staff/accounts"))).toBe(false);
  });

  it("manager whose login is linked to no profile: told to ask the owner, every choice read-only", async () => {
    serve(MANAGER, { linkRequired: true });
    at("/nhan-vien");
    expect(await screen.findByText(/Tài khoản của bạn chưa gắn với hồ sơ nhân viên, nên bạn chỉ xem được/)).toBeInTheDocument();
    const group = await waitFor(() => card("s-lan").getByRole("radiogroup"));
    for (const r of within(group).getAllByRole("radio")) expect(r).toBeDisabled();
    // The notice explains; no card claims the owner-only rule for managers.
    expect(screen.queryByText(/Chỉ chủ spa chấm công, ghi khen\/phàn nàn/)).not.toBeInTheDocument();
  });

  it("manager whose login is linked to no profile: the evaluations do not blame the owner-only rule", async () => {
    serve(MANAGER, { linkRequired: true });
    at("/nhan-vien?tab=evaluation");
    expect(await screen.findByText(/Tài khoản của bạn chưa gắn với hồ sơ nhân viên, nên bạn chỉ xem được/)).toBeInTheDocument();
    expect(await screen.findByText("Trần Thị Mai")).toBeInTheDocument();
    expect(screen.queryByText("Chỉ chủ spa chấm cho quản lý")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Chấm điểm" })).not.toBeInTheDocument();
  });
});

describe("Công và lương của tôi for a manager (EM 2026-11-08)", () => {
  it("opens the manager's own month from /me/* only, beside the staff books", async () => {
    const spy = serve(MANAGER);
    at("/nhan-vien/cua-toi");
    expect(await screen.findByRole("heading", { level: 1, name: "Công và lương của tôi" })).toBeInTheDocument();
    expect(await screen.findByRole("region", { name: "Tổng quan tháng" })).toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname);
    expect(paths).toContain("/api/v1/me/staff");
    expect(paths.some((p) => /\/(attendance|daily-reviews|evaluations|adjustments|payroll|staff\/profiles)$/.test(p))).toBe(false);
    // The nav marks the own month, not the books.
    const nav = screen.getAllByRole("navigation").find((n) => within(n).queryByRole("link", { name: "Quản lý nhân sự" }));
    expect(nav && within(nav).getByRole("link", { name: "Công và lương của tôi" })).toHaveAttribute("aria-current", "page");
    expect(nav && within(nav).getByRole("link", { name: "Quản lý nhân sự" })).not.toHaveAttribute("aria-current");
  });
});

describe("logins (ADR-0016)", () => {
  it("owner: creates a login from the profile, asks for the password again, shows the temporary password once", async () => {
    const spy = serve(OWNER, { reauthFirst: true });
    at("/nhan-vien?tab=profiles");
    const table = await screen.findByRole("table", { name: "Danh sách nhân sự" });
    await userEvent.click(within(table).getByRole("button", { name: "Phạm Thị Hoa" }));
    const sheet = await screen.findByRole("dialog", { name: "Phạm Thị Hoa" });
    // No link dropdown in the normal flow: create the login here.
    expect(within(sheet).queryByRole("button", { name: "Gắn tài khoản" })).not.toBeInTheDocument();
    await userEvent.type(within(sheet).getByLabelText("Số điện thoại đăng nhập"), "0912300111");
    await userEvent.selectOptions(within(sheet).getByLabelText("Vai trò"), "manager");
    await userEvent.click(within(sheet).getByRole("button", { name: "Tạo tài khoản đăng nhập" }));
    const again = await screen.findByRole("dialog", { name: "Xác nhận lại mật khẩu" });
    await userEvent.type(within(again).getByLabelText("Mật khẩu của bạn"), "mat-khau-chu-spa");
    await userEvent.type(within(again).getByLabelText(/Mã xác thực/), "123456");
    await userEvent.click(within(again).getByRole("button", { name: "Xác nhận" }));
    const shown = await screen.findByRole("dialog", { name: "Mật khẩu tạm của Phạm Thị Hoa" });
    expect(within(shown).getByLabelText("Mật khẩu tạm")).toHaveTextContent("ABCD-EFGH-JKLM");
    const done = within(shown).getByRole("button", { name: "Xong" });
    expect(done).toBeDisabled();
    await userEvent.click(within(shown).getByRole("checkbox", { name: "Tôi đã đưa mật khẩu cho nhân viên" }));
    expect(done).toBeEnabled();
    const posts = requests(spy).filter((r) => r.method === "POST" && r.url.endsWith("/staff/s-hoa/login"));
    expect(posts).toHaveLength(2);
    expect(posts[1]?.body).toEqual({ phone: "0912300111", role: "manager" });
    expect(requests(spy).find((r) => r.url.endsWith("/auth/reauth"))?.body).toEqual({ password: "mat-khau-chu-spa", code: "123456" });
  });

  it("owner: an ended session during re-auth says so, not 'wrong password'", async () => {
    serve(OWNER, { reauthFirst: true, reauthExpired: true });
    at("/nhan-vien?tab=profiles");
    const table = await screen.findByRole("table", { name: "Danh sách nhân sự" });
    await userEvent.click(within(table).getByRole("button", { name: "Phạm Thị Hoa" }));
    const sheet = await screen.findByRole("dialog", { name: "Phạm Thị Hoa" });
    await userEvent.type(within(sheet).getByLabelText("Số điện thoại đăng nhập"), "0912300111");
    await userEvent.click(within(sheet).getByRole("button", { name: "Tạo tài khoản đăng nhập" }));
    const again = await screen.findByRole("dialog", { name: "Xác nhận lại mật khẩu" });
    await userEvent.type(within(again).getByLabelText("Mật khẩu của bạn"), "mat-khau-chu-spa");
    await userEvent.type(within(again).getByLabelText(/Mã xác thực/), "123456");
    await userEvent.click(within(again).getByRole("button", { name: "Xác nhận" }));
    expect(await within(again).findByText(/Phiên đăng nhập đã hết/)).toBeInTheDocument();
    expect(within(again).queryByText("Mật khẩu hoặc mã xác thực không đúng.")).not.toBeInTheDocument();
  });

  it("owner: forgotten-password requests in Hồ sơ nhân viên — reset shows the new temporary password, or Bỏ qua", async () => {
    const request: PasswordResetRequest = {
      id: "r-1",
      user_id: "u-lan",
      full_name: "Lê Thị Lan",
      staff_id: "s-lan",
      requested_at: `${today}T07:05:00Z`,
      expires_at: `${today}T23:05:00Z`,
    };
    const spy = serve(OWNER, { resetRequests: [request] });
    at("/nhan-vien?tab=profiles");
    const notice = await screen.findByRole("status", { name: "Yêu cầu đặt lại mật khẩu" });
    expect(within(notice).getByText(/xin đặt lại mật khẩu · 14:05/)).toBeInTheDocument();
    await userEvent.click(within(notice).getByRole("button", { name: "Đặt lại" }));
    const shown = await screen.findByRole("dialog", { name: "Mật khẩu tạm của Lê Thị Lan" });
    expect(within(shown).getByLabelText("Mật khẩu tạm")).toHaveTextContent("NEWT-EMPP-ASSW");
    await userEvent.click(within(notice).getByRole("button", { name: "Bỏ qua" }));
    await waitFor(() => {
      expect(requests(spy).some((r) => r.url.endsWith("/staff/password-reset-requests/r-1/dismiss"))).toBe(true);
    });
  });

  it("manager: never asks for the forgotten-password requests", async () => {
    const spy = serve(MANAGER);
    at("/nhan-vien?tab=profiles");
    await screen.findByRole("table", { name: "Danh sách nhân sự" });
    expect(requests(spy).some((r) => r.url.includes("/password-reset-requests"))).toBe(false);
  });
});
