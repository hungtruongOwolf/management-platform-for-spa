import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { navFor, tabsFor } from "../../app/nav";
import { spaToday } from "../corrections/api";
import { ME } from "../reports/testServer";
import type { Adjustment, AdjustmentLog, AttendanceSheet, EvaluationSheet, MyStaff, MyWork, Payslip, PayrollMonth, ReviewSheet } from "./api";
import { daysOf, monthLabel, percentOfBp, shiftMonth, weekdayShort } from "./labels";
import { evaluationDueMonth, incompleteDays, missingOn } from "./progress";
import { staffSearch } from "./search";
import { PayslipView } from "./PayrollTab";

const MANAGER = ["catalog.read", "staff.read_basic", "attendance.record", "evaluation.record", "staff.read_own", "report.read"];
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
    staff: [
      {
        staff_id: "s-lan",
        full_name: "Lê Thị Lan",
        position: "Kỹ thuật viên",
        shift_start: "09:00",
        joined_on: null,
        left_on: null,
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
        days: [],
        totals,
        can_record: true,
        visit_days: [],
      },
      // The signed-in manager's own row (decision C): the owner's to write.
      {
        staff_id: "s-hoa",
        full_name: "Phạm Thị Hoa",
        position: "Quản lý",
        shift_start: "09:00",
        joined_on: null,
        left_on: null,
        days: [],
        totals,
        can_record: false,
        visit_days: [],
      },
    ],
  };
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

function work(): MyWork {
  return {
    linked: true,
    data: {
      month,
      today,
      served: [
        {
          visit_id: "v-1",
          business_date: today,
          check_in_at: `${today}T03:00:00Z`,
          customer_name: "Nguyễn Thị Hằng",
          services: [{ service_name: "Gội đầu thảo dược", qty: 2, from_package: true }],
        },
      ],
      upcoming: [
        {
          appointment_id: "a-1",
          business_date: today,
          starts_at: `${today}T07:00:00Z`,
          ends_at: `${today}T08:00:00Z`,
          status: "booked",
          customer_name: "Đỗ Văn Bình",
          services: ["Massage cổ vai gáy"],
        },
      ],
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
}

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
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://app.test");
    const p = url.pathname.replace(/^\/api\/v1/, "");
    const method = (init?.method ?? "GET").toUpperCase();
    const ok = (json: unknown, status = 200) => Promise.resolve(Response.json(json, { status }));
    if (p === "/me") return ok(ME(permissions));
    if (p === "/attendance" && method === "PUT") return options.attendancePut ? options.attendancePut() : ok([]);
    if (p === "/attendance") return ok({ ...sheet(options), closed: options.closed ?? false });
    if (p === "/payroll") return permissions.includes("payroll.read") ? ok(payroll()) : ok({ code: "auth.forbidden" }, 403);
    if (p === "/me/staff") return ok({ linked: true, data: { month, closed: false, attendance: lan(), reviews: [], evaluation: null, adjustments: [], payslip: slip() } } satisfies MyStaff);
    if (p === "/me/work") return ok(work());
    if (p === "/daily-reviews" && method === "PUT") return ok([]);
    if (p === "/daily-reviews") return ok(reviews());
    if (p === "/adjustments" && method === "GET") return ok({ month, closed: false, items: options.adjustments ?? [] } satisfies AdjustmentLog);
    if (p.startsWith("/adjustments/") && method === "DELETE") return Promise.resolve(new Response(null, { status: 204 }));
    if (p === "/evaluations" && method === "PUT") return ok({});
    if (p === "/evaluations") return ok(evaluations(url.searchParams.get("month") ?? month));
    if (p === "/staff") return ok([]);
    return ok([]);
  });
}

const at = (path: string) => {
  const history = createMemoryHistory({ initialEntries: [path] });
  render(<App history={history} />);
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
  });

  it("names the nav entries by what the user may do; technicians get 'Khách của tôi' in the tab bar", () => {
    const items = (perms: string[]) => navFor(ME(perms)).flatMap((g) => g.items);
    const item = (perms: string[]) => items(perms).find((i) => i.to === "/nhan-vien");
    expect(item(MANAGER)?.label).toBe("Nhân viên");
    expect(item(TECH)?.label).toBe("Công và lương của tôi");
    expect(item(["catalog.read"])).toBeUndefined();
    const mine = (perms: string[]) => items(perms).find((i) => i.to === "/khach-cua-toi");
    expect(mine(TECH)?.label).toBe("Khách của tôi");
    expect(mine([...MANAGER, "visit.read"])).toBeUndefined();
    expect(tabsFor(navFor(ME(TECH))).map((t) => t.to)).toEqual(["/", "/nhan-vien", "/khach-cua-toi"]);
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
    expect(screen.queryByRole("tab", { name: "Hồ sơ" })).not.toBeInTheDocument();

    const progress = await screen.findByRole("status", { name: "Tiến độ chấm công" });
    expect(progress).toHaveTextContent("Đã chấm công 1/3 người");
    expect(screen.getByRole("button", { name: "Tới Trần Thị Mai" })).toBeInTheDocument();
    expect(card("s-lan").getByText(/muộn 12 phút/)).toBeInTheDocument();
    expect(card("s-lan").getByText(/Khách khen tay nghề/)).toBeInTheDocument();
    expect(card("s-mai").getByText("Chưa chấm công")).toBeInTheDocument();

    // Attendance in words, one radiogroup per person; the chosen option says it, no extra chip.
    const lanChoice = card("s-lan").getByRole("radiogroup", { name: "Chấm công Lê Thị Lan" });
    expect(within(lanChoice).getAllByRole("radio").map((r) => r.textContent)).toEqual(["Đi làm", "Nghỉ có phépP", "Nghỉ đột xuấtĐX", "Nghỉ không phépK"]);
    expect(within(lanChoice).getByRole("radio", { name: "Đi làm" })).toHaveAttribute("aria-checked", "true");
    expect(card("s-lan").getAllByText("Đi làm")).toHaveLength(1);
    expect(card("s-lan").getByText("Đi muộn 12 phút")).toBeInTheDocument();
    expect(within(card("s-mai").getByRole("radiogroup")).queryByRole("radio", { checked: true })).not.toBeInTheDocument();

    await userEvent.click(card("s-mai").getByRole("radio", { name: "Nghỉ có phép (P)" }));
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
    await userEvent.click(within(group).getByRole("radio", { name: "Nghỉ đột xuất (ĐX)" }));
    // Saving: the tapped option shows chosen at once, the others ignore taps but keep focus.
    expect(within(group).getByRole("radio", { name: "Nghỉ đột xuất (ĐX)" })).toHaveAttribute("aria-checked", "true");
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
    // Decision C: the manager's own row is read-only, with the reason.
    expect(within(grid).getByRole("rowheader", { name: /Phạm Thị Hoa/ })).toHaveTextContent("Chủ spa chấm");
    expect(within(grid).queryByRole("button", { name: /Phạm Thị Hoa/ })).not.toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Sổ thưởng, phạt của tháng" })).toBeInTheDocument();
  });

  it("decision C: a manager's own row has no write controls, only the reason", async () => {
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
    await userEvent.click(within(group).getByRole("radio", { name: "Nghỉ không phép (K)" }));
    const ask = await screen.findByRole("dialog", { name: "Lê Thị Lan có lượt khách ngày này" });
    await userEvent.click(within(ask).getByRole("button", { name: "Thôi" }));
    expect(puts(spy, "/attendance")).toHaveLength(0);
    await userEvent.click(within(group).getByRole("radio", { name: "Nghỉ không phép (K)" }));
    const again = await screen.findByRole("dialog", { name: "Lê Thị Lan có lượt khách ngày này" });
    await userEvent.click(within(again).getByRole("button", { name: "Vẫn chấm Nghỉ không phép" }));
    await waitFor(() => {
      expect(puts(spy, "/attendance")).toHaveLength(1);
    });
    expect(puts(spy, "/attendance")[0]?.body).toEqual({ entries: [{ staff_id: "s-lan", date: today, status: "absent_k" }] });
    // Someone without visits that day saves at once.
    await userEvent.click(card("s-mai").getByRole("radio", { name: "Nghỉ có phép (P)" }));
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
    expect(within(region).getAllByText("Dịch vụ lẻ").length).toBeGreaterThan(0);
    expect(screen.getByText(/gói 1\.500\.000đ · dịch vụ lẻ 39\.000đ/)).toBeInTheDocument();
  });

  it("technician: own month as a calendar with totals, never the management APIs", async () => {
    const spy = serve(TECH);
    at("/nhan-vien");
    expect(await screen.findByRole("heading", { level: 1, name: "Công và lương của tôi" })).toBeInTheDocument();
    expect(await screen.findByText(/5\.315\.000/)).toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    const calendar = screen.getByRole("list", { name: /Lịch công/ });
    expect(within(calendar).getAllByRole("listitem").filter((li) => li.getAttribute("aria-label"))).toHaveLength(daysOf(month).length);
    expect(within(calendar).getByRole("listitem", { name: /Đi làm, muộn 12 phút/ })).toBeInTheDocument();
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname);
    expect(paths.some((p) => p.endsWith("/attendance") || p.endsWith("/payroll"))).toBe(false);
  });
});

describe("/khach-cua-toi", () => {
  it("technician: upcoming bookings and customers served, from /me/work only", async () => {
    const spy = serve(TECH);
    at("/khach-cua-toi");
    expect(await screen.findByRole("heading", { level: 1, name: "Khách của tôi" })).toBeInTheDocument();
    expect(await screen.findByText("Đỗ Văn Bình")).toBeInTheDocument();
    expect(screen.getByText("Massage cổ vai gáy")).toBeInTheDocument();
    expect(screen.getByText("14:00–15:00")).toBeInTheDocument();
    expect(screen.getByText("Nguyễn Thị Hằng")).toBeInTheDocument();
    expect(screen.getByText("Gội đầu thảo dược ×2 (gói)")).toBeInTheDocument();
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname.replace(/^\/api\/v1/, ""));
    expect(paths.filter((p) => p !== "/me")).toEqual(expect.arrayContaining(["/me/work"]));
    expect(paths.some((p) => /^\/(visits|customers|appointments)/.test(p))).toBe(false);
  });
});
