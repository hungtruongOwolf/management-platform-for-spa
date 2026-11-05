import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { navFor } from "../../app/nav";
import { spaToday } from "../corrections/api";
import { ME } from "../reports/testServer";
import type { AttendanceSheet, MyStaff, Payslip, PayrollMonth } from "./api";
import { daysOf, monthLabel, percentOfBp, shiftMonth, weekdayShort } from "./labels";
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

  it("names the nav entry by what the user may do", () => {
    const item = (perms: string[]) => navFor(ME(perms)).flatMap((g) => g.items).find((i) => i.to === "/nhan-vien");
    expect(item(MANAGER)?.label).toBe("Nhân viên");
    expect(item(TECH)?.label).toBe("Công và lương của tôi");
    expect(item(["catalog.read"])).toBeUndefined();
  });
});

describe("/nhan-vien", () => {
  it("manager: attendance and evaluation tabs, no payroll; one tap records a day", async () => {
    const spy = serve(MANAGER);
    at("/nhan-vien");
    expect(await screen.findByRole("tab", { name: "Chấm công" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Đánh giá" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Bảng lương" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Hồ sơ" })).not.toBeInTheDocument();
    const lan = await screen.findByText("Lê Thị Lan");
    expect(within(lan.closest("li") ?? document.body).getByText(/muộn 12 phút/)).toBeInTheDocument();

    const mai = within(screen.getByText("Trần Thị Mai").closest("li") ?? document.body);
    await userEvent.click(mai.getByRole("button", { name: "Nghỉ có phép (P)" }));
    await waitFor(() => {
      expect(requests(spy).some((r) => r.method === "PUT")).toBe(true);
    });
    const put = requests(spy).find((r) => r.method === "PUT");
    expect(put?.body).toEqual({ entries: [{ staff_id: "s-mai", date: today, status: "leave_p" }] });

    await userEvent.click(screen.getByRole("button", { name: "Cả spa đi làm" }));
    await waitFor(() => {
      expect(requests(spy).filter((r) => r.method === "PUT")).toHaveLength(2);
    });
    expect(requests(spy).filter((r) => r.method === "PUT")[1]?.body).toEqual({
      entries: [{ staff_id: "s-mai", date: today, status: "present" }],
    });
    expect(requests(spy).some((r) => r.url.includes("/payroll"))).toBe(false);
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

  it("technician: only their own month, never the management APIs", async () => {
    const spy = serve(TECH);
    at("/nhan-vien");
    expect(await screen.findByRole("heading", { level: 1, name: "Công và lương của tôi" })).toBeInTheDocument();
    expect(await screen.findByText(/5\.315\.000/)).toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    const paths = requests(spy).map((r) => new URL(r.url, "http://app.test").pathname);
    expect(paths.some((p) => p.endsWith("/attendance") || p.endsWith("/payroll"))).toBe(false);
  });
});
