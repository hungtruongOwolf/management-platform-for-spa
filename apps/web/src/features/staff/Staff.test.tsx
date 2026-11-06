import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { navFor, tabsFor } from "../../app/nav";
import { spaToday } from "../corrections/api";
import { ME } from "../reports/testServer";
import type { AdjustmentLog, AttendanceSheet, EvaluationSheet, MyStaff, MyWork, Payslip, PayrollMonth, ReviewSheet } from "./api";
import { daysOf, monthLabel, percentOfBp, shiftMonth, weekdayShort } from "./labels";
import { evaluationDueMonth, incompleteDays, missingOn } from "./progress";
import { staffSearch } from "./search";

const MANAGER = ["catalog.read", "staff.read_basic", "attendance.record", "evaluation.record", "staff.read_own", "report.read"];
const OWNER = [...MANAGER, "staff.manage", "payroll.read", "payroll.close"];
const TECH = ["catalog.read", "staff.read_basic", "staff.read_own"];

const today = spaToday();
const month = today.slice(0, 7);

const totals = { present: 0, late_count: 0, late_minutes: 0, leave_p: 0, leave_dx: 0, absent_k: 0, unrecorded: 0 };

function sheet(): AttendanceSheet {
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
      },
      { staff_id: "s-mai", full_name: "Trần Thị Mai", position: "Kỹ thuật viên", shift_start: "09:00", joined_on: null, left_on: null, days: [], totals },
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
      },
      { staff_id: "s-mai", full_name: "Trần Thị Mai", position: "Kỹ thuật viên", compliments: 0, complaints: 0, days: [] },
    ],
  };
}

function evaluations(m: string): EvaluationSheet {
  return {
    month: m,
    closed: false,
    criteria: [],
    staff: [{ staff_id: "s-mai", full_name: "Trần Thị Mai", position: "Kỹ thuật viên", compliments: 0, complaints: 0, attendance: totals, evaluation: null }],
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

function serve(permissions: string[]) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://app.test");
    const p = url.pathname.replace(/^\/api\/v1/, "");
    const method = (init?.method ?? "GET").toUpperCase();
    const ok = (json: unknown, status = 200) => Promise.resolve(Response.json(json, { status }));
    if (p === "/me") return ok(ME(permissions));
    if (p === "/attendance" && method === "PUT") return ok([]);
    if (p === "/attendance") return ok(sheet());
    if (p === "/payroll") return permissions.includes("payroll.read") ? ok(payroll()) : ok({ code: "auth.forbidden" }, 403);
    if (p === "/me/staff") return ok({ linked: true, data: { month, closed: false, attendance: lan(), reviews: [], evaluation: null, adjustments: [], payslip: slip() } } satisfies MyStaff);
    if (p === "/me/work") return ok(work());
    if (p === "/daily-reviews" && method === "PUT") return ok([]);
    if (p === "/daily-reviews") return ok(reviews());
    if (p === "/adjustments" && method === "GET") return ok({ month, closed: false, items: [] } satisfies AdjustmentLog);
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
    expect(missingOn(s.staff, today).map((p) => p.staff_id)).toEqual(["s-mai"]);
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
    expect(progress).toHaveTextContent("Đã chấm công 1/2 người");
    expect(screen.getByRole("button", { name: "Tới Trần Thị Mai" })).toBeInTheDocument();
    expect(card("s-lan").getByText(/muộn 12 phút/)).toBeInTheDocument();
    expect(card("s-lan").getByText(/Khách khen tay nghề/)).toBeInTheDocument();
    expect(card("s-mai").getByText("Chưa chấm công")).toBeInTheDocument();

    await userEvent.click(card("s-mai").getByRole("button", { name: "P Nghỉ có phép (P)" }));
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
    await userEvent.click(card("s-mai").getByRole("button", { name: "+ Thưởng / phạt" }));
    const dialog = await screen.findByRole("dialog", { name: "Thưởng / phạt · Trần Thị Mai" });
    expect(within(dialog).queryByLabelText("Nhân viên")).not.toBeInTheDocument();
    expect(requests(spy).some((r) => r.url.includes("/payroll"))).toBe(false);
  });

  it("manager: the month sheet is a person × day grid with totals and the bonus log", async () => {
    serve(MANAGER);
    at("/nhan-vien?tab=month");
    const grid = await screen.findByRole("table", { name: /Bảng chấm công/ });
    expect(within(grid).getByRole("rowheader", { name: "Lê Thị Lan" })).toBeInTheDocument();
    expect(within(grid).getAllByRole("columnheader")).toHaveLength(1 + daysOf(month).length + 5);
    expect(within(grid).getByRole("button", { name: /Lê Thị Lan, .*: Đi làm, muộn 12 phút/ })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Sổ thưởng, phạt của tháng" })).toBeInTheDocument();
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
