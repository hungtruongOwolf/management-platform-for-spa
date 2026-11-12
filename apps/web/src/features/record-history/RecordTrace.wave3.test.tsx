// "Ai ghi, ai sửa" on the wave-3 screens (ADR-0022, design record-history §2.10): the last element
// of a service's and a plan's edit form, of "Quy tắc lương" and of "Số khách tối đa", above their
// buttons; "Sửa lần cuối bởi …" (K37). Never on a create form, never on the lists. The owner sees
// it; a manager cannot open these screens and sends no request for it.
import type { Calendar, RecordHistory } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { spaToday } from "../../lib/format";
import { ME } from "../reports/testServer";
import type { PayrollMonth } from "../staff/api";
import { fullDay } from "./text";

const today = spaToday();
/** A day of last year: a line on it says its full date. */
const earlier = `${String(Number(today.slice(0, 4)) - 1)}-02-03`;
const HISTORY = "record.history.read";
const CATALOG = ["catalog.read", "catalog.manage"];
const PAYROLL = ["catalog.read", "staff.read_basic", "staff.read", "staff.manage", "payroll.read", "payroll.close"];
const CALENDAR = ["visit.record", "visit.read", "appointment.manage", "appointment.settings", "customer.read", "catalog.read", "staff.read_basic"];

const HA = { kind: "user" as const, user_id: "u2", name: "Hà" };
const LAN = { kind: "user" as const, user_id: "u3", name: "Lan" };

const CATEGORIES = [{ id: "c1", name: "Gội đầu dưỡng sinh", sort_order: 0 }];
const THU_GIAN = {
  id: "s1",
  category_id: "c1",
  name: "Thư Giãn",
  description: null,
  duration_min: 45,
  recommended: false,
  active: true,
  version: 3,
  prices: { female: 170_000, male: 150_000 },
};
const CO_BAN = { id: "s2", category_id: "c1", name: "Cơ Bản", duration_min: 30, recommended: false, active: true, version: 1, prices: { all: 49_000 } };
const PLAN = {
  id: "p1",
  name: "Gội đầu 10 buổi",
  price: 1_500_000,
  validity_months: 6,
  gift_note: null,
  active: true,
  version: 1,
  items: [{ service_id: "s1", sessions: 10, bonus_sessions: 2 }],
};

/** Added by Hà long ago; `edits` saves by Lan today (the newest one changing Giá Nữ). */
function story(record: RecordHistory["record"], made: string, edit: string | null, edits: number): RecordHistory {
  const items: RecordHistory["items"] = [
    { id: "a-made", action: made, kind: "settings", at: `${earlier}T02:00:00Z`, day: earlier, actor: HA, changes: [], packages: [], folded: [] },
  ];
  if (edit)
    items.push({
      id: "a-edit",
      action: edit,
      kind: "settings",
      at: `${today}T03:30:00Z`,
      day: today,
      actor: LAN,
      changes: [{ field: "price_female", format: "money", old: "150000", new: "170000" }],
      packages: [],
      folded: [],
    });
  return {
    record,
    summary: {
      created: { actor: HA, at: `${earlier}T02:00:00Z`, day: earlier, source: "audit" },
      ...(edit ? { last: { actor: LAN, at: `${today}T03:30:00Z`, day: today } } : {}),
      edits,
      state: "active",
    },
    items,
    omitted: 0,
  };
}

const nothing = (record: RecordHistory["record"]): RecordHistory => ({ record, summary: { edits: 0, state: "active" }, items: [], omitted: 0 });

const payroll = (): PayrollMonth => ({
  month: today.slice(0, 7),
  closed: false,
  closed_at: null,
  closed_by_name: null,
  ended: false,
  payslips: [],
  totals: { staff: 0, base_salary: 0, deductions: 0, commission: 0, bonuses: 0, penalties: 0, meal_allowance: 0, net_pay_shortfall: 0, net_pay: 0 },
  missing: [],
  leaver_refunds: [],
});

const RULES = {
  version: 2,
  rules: {
    closing_commission: true,
    commission_on_bonus_sessions: false,
    default_shift_start: "09:00",
    free_leave_codes: ["leave_p", "leave_dx", "absent_k"],
    free_leave_days: 2,
    late_grace_minutes: 5,
    late_penalty_amount: 20_000,
    loyalty_partial_month: "fifteen_days",
    meal_allowance_basis: "monthly",
    tet_level_amounts: [500_000, 400_000, 300_000],
  },
};

const calendar = (url: URL): Calendar => ({
  from: url.searchParams.get("from") ?? today,
  to: url.searchParams.get("to") ?? today,
  today,
  opens_at: "09:00",
  closes_at: "21:00",
  slot_minutes: 15,
  time_zone: "Asia/Ho_Chi_Minh",
  technicians: [],
  max_per_slot: 4,
  appointments: [],
  visits: [],
});

/** The fake API. `forbidden`: the operator hid the history mid-session; `neverSaved`: the settings are still the defaults. */
function serve(permissions: string[], { forbidden = false, neverSaved = false }: { forbidden?: boolean; neverSaved?: boolean } = {}) {
  const shown = permissions.includes(HISTORY);
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://app.test");
    const p = url.pathname.replace(/^\/api\/v1/, "");
    const ok = (json: unknown, status = 200) => Promise.resolve(Response.json(json, { status }));
    if (p === "/me") return ok(ME(permissions));
    if (p === "/service-categories") return ok(CATEGORIES);
    if (p === "/services") return ok([THU_GIAN, CO_BAN]);
    if (p === "/package-plans") return ok([PLAN]);
    if (p === "/payroll") return ok(payroll());
    if (p === "/payroll/rules") return ok(RULES);
    if (p === "/appointments") return ok(calendar(url));
    if (p.startsWith("/record-history/")) {
      if (!shown || forbidden) return ok({ code: "auth.forbidden" }, 403);
      if (p === "/record-history/service/s1") return ok(story({ type: "service", id: "s1" }, "catalog.service_create", "catalog.service_update", 2));
      if (p === "/record-history/service/s2") return ok(nothing({ type: "service", id: "s2" }));
      if (p === "/record-history/package-plan/p1") return ok(story({ type: "package_plan", id: "p1" }, "catalog.plan_create", null, 0));
      if (p === "/record-history/service-category/c1") {
        const renamed = story({ type: "service_category", id: "c1" }, "catalog.category_create", "catalog.category_update", 1);
        const edit = renamed.items[1];
        if (edit) edit.changes = [{ field: "name", format: "text", old: "Gội đầu", new: "Gội đầu dưỡng sinh" }];
        return ok(renamed);
      }
      if (p === "/record-history/settings/payroll-rules")
        return ok(neverSaved ? nothing({ type: "payroll_rules" }) : story({ type: "payroll_rules" }, "payroll.rules_update", "payroll.rules_update", 1));
      if (p === "/record-history/settings/appointment-settings") return ok(story({ type: "appointment_settings" }, "appointment.settings", null, 0));
      return ok({ code: "not_found" }, 404);
    }
    return ok([]);
  });
}

const urls = (spy: ReturnType<typeof serve>) => spy.mock.calls.map(([input]) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url));
/** The histories asked for, every request: one per open (the prefetch's reply is not asked again on mount). */
const traces = (spy: ReturnType<typeof serve>) => urls(spy).filter((u) => u.includes("/record-history/"));
const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);
const lastEdit = `Sửa lần cuối bởi Lan · ${fullDay(today)}`;

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Bảng giá dịch vụ", () => {
  it("a service's edit form ends with who edited it last, above its buttons; opened, Giá Nữ old → new", async () => {
    const spy = serve([...CATALOG, HISTORY]);
    const user = userEvent.setup();
    at("/bang-gia");
    await user.click(await screen.findByRole("button", { name: "Sửa Thư Giãn" }));
    const form = screen.getByRole("form", { name: "Sửa: Thư Giãn" });
    const trace = await within(form).findByRole("button", { name: `${lastEdit} · đã sửa 2 lần` });
    expect(trace).toHaveAttribute("type", "button");
    expect(trace).toHaveAttribute("aria-expanded", "false");
    // The form's last element above its buttons.
    expect(trace.closest(".span-2")?.nextElementSibling).toBe(within(form).getByRole("button", { name: "Lưu dịch vụ" }).closest(".button-row"));
    expect(traces(spy)).toEqual(["/api/v1/record-history/service/s1"]);

    await user.click(trace);
    const list = within(form).getByRole("list", { name: "Lịch sử dịch vụ Thư Giãn" });
    expect(within(list).getByText("Hà thêm dịch vụ")).toBeInTheDocument();
    expect(within(list).getByText("Lan sửa dịch vụ")).toBeInTheDocument();
    expect(within(list).getByText("Giá Nữ: 150.000đ → 170.000đ")).toBeInTheDocument();
    // A line of another day says its date (of another year: with it); today's only its time.
    const times = within(list).getAllByRole("time");
    expect(times.map((t) => t.textContent)).toEqual([expect.stringMatching(new RegExp(` ${fullDay(earlier)}$`)), expect.stringMatching(/^\d\d:\d\d$/)]);
    // Opening it saves nothing.
    expect(spy.mock.calls.some(([, init]) => (init?.method ?? "GET") !== "GET")).toBe(false);
  });

  it("a service seeded with the spa: 'Chưa rõ ai ghi', opened 'Chưa có lịch sử thay đổi.'", async () => {
    serve([...CATALOG, HISTORY]);
    const user = userEvent.setup();
    at("/bang-gia");
    await user.click(await screen.findByRole("button", { name: "Sửa Cơ Bản" }));
    const form = screen.getByRole("form", { name: "Sửa: Cơ Bản" });
    await user.click(await within(form).findByRole("button", { name: "Chưa rõ ai ghi" }));
    expect(within(form).getByText("Chưa có lịch sử thay đổi.")).toBeInTheDocument();
  });

  it("a group's rename form ('Sửa nhóm', K39) ends with who renamed it last, above its buttons; opened, Tên old → new", async () => {
    const spy = serve([...CATALOG, HISTORY]);
    const user = userEvent.setup();
    at("/bang-gia");
    await user.click(await screen.findByRole("button", { name: "Sửa nhóm Gội đầu dưỡng sinh" }));
    const form = screen.getByRole("form", { name: "Sửa nhóm: Gội đầu dưỡng sinh" });
    const trace = await within(form).findByRole("button", { name: `${lastEdit} · đã sửa 1 lần` });
    expect(trace).toHaveAttribute("type", "button");
    expect(trace.closest(".span-2")?.nextElementSibling).toBe(within(form).getByRole("button", { name: "Lưu nhóm dịch vụ" }).closest(".button-row"));
    // One open, one request.
    expect(traces(spy)).toEqual(["/api/v1/record-history/service-category/c1"]);

    await user.click(trace);
    const list = within(form).getByRole("list", { name: "Lịch sử nhóm dịch vụ Gội đầu dưỡng sinh" });
    expect(within(list).getByText("Hà thêm nhóm dịch vụ")).toBeInTheDocument();
    expect(within(list).getByText("Lan sửa nhóm dịch vụ")).toBeInTheDocument();
    expect(within(list).getByText("Tên: Gội đầu → Gội đầu dưỡng sinh")).toBeInTheDocument();
    expect(spy.mock.calls.some(([, init]) => (init?.method ?? "GET") !== "GET")).toBe(false);

    // Closed and opened again: asked again, once.
    await user.click(within(form).getByRole("button", { name: "Hủy" }));
    await user.click(screen.getByRole("button", { name: "Sửa nhóm Gội đầu dưỡng sinh" }));
    expect(await screen.findByRole("button", { name: `${lastEdit} · đã sửa 1 lần` })).toBeInTheDocument();
    await waitFor(() => {
      expect(traces(spy)).toHaveLength(2);
    });
  });

  it("a group's rename form, switched off mid-session (403): the history hides, the form and its focus stay", async () => {
    const spy = serve([...CATALOG, HISTORY], { forbidden: true });
    const user = userEvent.setup();
    at("/bang-gia");
    await user.click(await screen.findByRole("button", { name: "Sửa nhóm Gội đầu dưỡng sinh" }));
    const form = screen.getByRole("form", { name: "Sửa nhóm: Gội đầu dưỡng sinh" });
    await waitFor(() => {
      expect(urls(spy).filter((u) => u === "/api/v1/me").length).toBeGreaterThan(1);
    });
    expect(within(form).queryByRole("button", { name: /^Sửa lần cuối|^Đang tải ai ghi/ })).toBeNull();
    expect(within(form).getByLabelText("Tên nhóm dịch vụ")).toHaveFocus();
    expect(within(form).getByRole("button", { name: "Lưu nhóm dịch vụ" })).toBeInTheDocument();
  });

  it("never on a create form: 'Thêm dịch vụ', 'Thêm nhóm dịch vụ'", async () => {
    const spy = serve([...CATALOG, HISTORY]);
    const user = userEvent.setup();
    at("/bang-gia");
    await user.click(await screen.findByRole("button", { name: "Thêm dịch vụ" }));
    expect(screen.getByRole("form", { name: "Thêm dịch vụ" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Thêm nhóm dịch vụ" }));
    expect(screen.getByRole("form", { name: "Thêm nhóm dịch vụ" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Sửa lần cuối|^Ghi bởi|^Chưa rõ ai ghi|^Đang tải ai ghi/ })).toBeNull();
    expect(traces(spy)).toEqual([]);
  });

  it("switched off mid-session (403): the history hides, the form stays, /me is asked again", async () => {
    const spy = serve([...CATALOG, HISTORY], { forbidden: true });
    const user = userEvent.setup();
    at("/bang-gia");
    await user.click(await screen.findByRole("button", { name: "Sửa Thư Giãn" }));
    const form = screen.getByRole("form", { name: "Sửa: Thư Giãn" });
    await waitFor(() => {
      expect(urls(spy).filter((u) => u === "/api/v1/me").length).toBeGreaterThan(1);
    });
    expect(within(form).queryByRole("button", { name: /^Sửa lần cuối|^Đang tải ai ghi/ })).toBeNull();
    expect(within(form).getByRole("button", { name: "Lưu dịch vụ" })).toBeInTheDocument();
  });

  it("the owner without the switch, a manager (no edit at all): no history, no request", async () => {
    for (const permissions of [CATALOG, ["catalog.read", HISTORY]]) {
      const spy = serve(permissions);
      const user = userEvent.setup();
      const view = at("/bang-gia");
      expect(await screen.findByRole("heading", { name: "Gội đầu dưỡng sinh" })).toBeInTheDocument();
      const edit = screen.queryByRole("button", { name: "Sửa Thư Giãn" });
      if (edit) {
        await user.click(edit);
        expect(screen.getByRole("form", { name: "Sửa: Thư Giãn" })).toBeInTheDocument();
      }
      expect(screen.queryByRole("button", { name: /^Sửa lần cuối|^Đang tải ai ghi/ })).toBeNull();
      // "Sửa nhóm" only with catalog.manage; renaming without the switch shows no history either.
      const group = screen.queryByRole("button", { name: "Sửa nhóm Gội đầu dưỡng sinh" });
      expect(group !== null).toBe(permissions.includes("catalog.manage"));
      if (group) {
        await user.click(group);
        expect(screen.getByRole("form", { name: "Sửa nhóm: Gội đầu dưỡng sinh" })).toBeInTheDocument();
      }
      expect(screen.queryByRole("button", { name: /^Sửa lần cuối|^Ghi bởi|^Chưa rõ ai ghi|^Đang tải ai ghi/ })).toBeNull();
      expect(traces(spy)).toEqual([]);
      view.unmount();
      vi.restoreAllMocks();
    }
  });
});

describe("Mẫu gói liệu trình", () => {
  it("a plan's edit form ends with its history; only added: 'Ghi bởi Hà · 03/02/…'; the create form has none", async () => {
    const spy = serve([...CATALOG, HISTORY]);
    const user = userEvent.setup();
    at("/bang-gia/goi");
    await user.click(await screen.findByRole("button", { name: "Sửa mẫu gói Gội đầu 10 buổi" }));
    const form = screen.getByRole("form", { name: "Sửa mẫu gói: Gội đầu 10 buổi" });
    const trace = await within(form).findByRole("button", { name: `Ghi bởi Hà · ${fullDay(earlier)}` });
    expect(trace.closest(".span-2")?.nextElementSibling).toBe(within(form).getByRole("button", { name: "Lưu mẫu gói" }).closest(".button-row"));
    expect(traces(spy)).toEqual(["/api/v1/record-history/package-plan/p1"]);

    await user.click(within(form).getByRole("button", { name: "Hủy" }));
    await user.click(screen.getByRole("button", { name: "Tạo mẫu gói" }));
    expect(screen.getByRole("form", { name: "Tạo mẫu gói liệu trình" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Ghi bởi|^Đang tải ai ghi/ })).toBeNull();
    expect(traces(spy)).toHaveLength(1);
  });
});

describe("Quy tắc lương", () => {
  it("the sheet ends with who changed the rules last, above 'Lưu quy tắc'", async () => {
    const spy = serve([...PAYROLL, HISTORY]);
    const user = userEvent.setup();
    at("/nhan-vien?tab=payroll");
    await user.click(await screen.findByRole("button", { name: "Quy tắc lương" }));
    const sheet = await screen.findByRole("dialog", { name: "Quy tắc lương của spa" });
    const trace = await within(sheet).findByRole("button", { name: `${lastEdit} · đã sửa 1 lần` });
    expect(trace).toHaveAttribute("type", "button");
    expect(trace.closest(".record-trace")?.nextElementSibling).toContainElement(within(sheet).getByRole("button", { name: "Lưu quy tắc" }));
    expect(traces(spy)).toEqual(["/api/v1/record-history/settings/payroll-rules"]);
    await user.click(trace);
    expect(within(sheet).getByRole("list", { name: "Lịch sử quy tắc lương" })).toHaveTextContent("Lan sửa quy tắc lương");
  });

  it("never saved (still the defaults): 'Chưa có ai sửa'", async () => {
    serve([...PAYROLL, HISTORY], { neverSaved: true });
    const user = userEvent.setup();
    at("/nhan-vien?tab=payroll");
    await user.click(await screen.findByRole("button", { name: "Quy tắc lương" }));
    const sheet = await screen.findByRole("dialog", { name: "Quy tắc lương của spa" });
    expect(await within(sheet).findByRole("button", { name: "Chưa có ai sửa" })).toBeInTheDocument();
  });

  it("without the switch: the sheet, no history, no request", async () => {
    const spy = serve(PAYROLL);
    const user = userEvent.setup();
    at("/nhan-vien?tab=payroll");
    await user.click(await screen.findByRole("button", { name: "Quy tắc lương" }));
    const sheet = await screen.findByRole("dialog", { name: "Quy tắc lương của spa" });
    expect(await within(sheet).findByRole("button", { name: "Lưu quy tắc" })).toBeInTheDocument();
    expect(within(sheet).queryByRole("button", { name: /^Sửa lần cuối|^Đang tải ai ghi/ })).toBeNull();
    expect(traces(spy)).toEqual([]);
  });
});

describe("Số khách tối đa", () => {
  it("the sheet's body ends with who set it, above its footer", async () => {
    const spy = serve([...CALENDAR, HISTORY]);
    const user = userEvent.setup();
    at("/quay?view=lich");
    await user.click(await screen.findByRole("button", { name: "Số khách tối đa: 4" }));
    const sheet = await screen.findByRole("dialog", { name: "Số khách tối đa mỗi khung giờ" });
    const trace = await within(sheet).findByRole("button", { name: `Sửa lần cuối bởi Hà · ${fullDay(earlier)}` });
    const block = trace.closest(".record-trace");
    expect(block?.previousElementSibling).toBe(sheet.querySelector("#slot-limit-form"));
    expect(block?.nextElementSibling).toBeNull();
    expect(traces(spy)).toEqual(["/api/v1/record-history/settings/appointment-settings"]);
  });

  it("a manager by default: no 'Số khách tối đa' to open, no request", async () => {
    const spy = serve(CALENDAR.filter((p) => p !== "appointment.settings"));
    at("/quay?view=lich");
    await waitFor(() => {
      expect(urls(spy).some((u) => u.startsWith("/api/v1/appointments?"))).toBe(true);
    });
    expect(screen.queryByRole("button", { name: /^Số khách tối đa/ })).toBeNull();
    expect(traces(spy)).toEqual([]);
  });
});
