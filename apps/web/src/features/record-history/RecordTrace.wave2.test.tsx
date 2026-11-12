// "Ai ghi, ai sửa" on the wave-2 screens (ADR-0022, design record-history §2.7–2.9): the booking's
// sheet (last element) and "Đã sửa" in the calendar's list view only; the person-day sheet (last
// element of each fieldset, one request per book); the bonus / penalty log (⋯ → under the row).
// The owner sees them; a manager by default does not, and sends no request for them.
import type { Appointment, Calendar, RecordHistory } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { spaToday } from "../../lib/format";
import { ME } from "../reports/testServer";
import type { Adjustment, AdjustmentLog, AttendanceSheet, ReviewSheet } from "../staff/api";

const today = spaToday();
const month = today.slice(0, 7);
const DESK = ["visit.record", "visit.read", "appointment.manage", "customer.read", "catalog.read", "staff.read_basic"];
const BOOKS = ["catalog.read", "staff.read_basic", "staff.read", "attendance.record", "evaluation.record", "staff.read_own", "report.read"];
const HISTORY = "record.history.read";

const HA = { kind: "user" as const, user_id: "u2", name: "Hà" };

function story(record: RecordHistory["record"], action: string, edits = 0): RecordHistory {
  return {
    record,
    summary: { created: { actor: HA, at: `${today}T01:15:00Z`, day: today, source: "audit" }, edits, state: "active" },
    items: [{ id: `audit-${action}`, action, kind: "staff", at: `${today}T01:15:00Z`, day: today, actor: HA, changes: [], packages: [], folded: [] }],
    omitted: 0,
  };
}

const booking = (over: Partial<Appointment>): Appointment => ({
  id: "a1",
  status: "booked",
  phase: "waiting",
  group_id: null,
  companions: [],
  business_date: today,
  starts_at: `${today}T03:00:00Z`,
  ends_at: `${today}T04:00:00Z`,
  duration_min: 60,
  customer_id: "c1",
  customer_name: "Lê Thị Mai",
  customer_phone: "+84905111222",
  customer_gender: "female",
  is_guest: false,
  staff_id: null,
  staff_name: null,
  services: [{ service_id: "sv1", service_name: "Gội đầu dưỡng sinh", category_name: "Gội đầu", duration_min: 60 }],
  note: null,
  status_reason: null,
  arrived_at: null,
  closed_at: null,
  visit_id: null,
  payment_state: "unpaid",
  paid_amount: 0,
  version: 2,
  created_at: `${today}T01:15:00Z`,
  ...over,
});

/**
 * The fake API: `/me`, the calendar (marks only for the history right, as the server), the books, the histories.
 * `closed`: the month is closed; `lanLocked`: Lan is a person only the owner records (another manager, for a manager).
 */
function serve(permissions: string[], { switchedOff = false, closed = false, lanLocked = false }: { switchedOff?: boolean; closed?: boolean; lanLocked?: boolean } = {}) {
  const shown = permissions.includes(HISTORY);
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://app.test");
    const p = url.pathname.replace(/^\/api\/v1/, "");
    const ok = (json: unknown, status = 200) => Promise.resolve(Response.json(json, { status }));
    if (p === "/me") return ok(ME(permissions));
    if (p === "/appointments") {
      const marks = shown && url.searchParams.get("marks") === "true";
      const mark = (a: Appointment, edited: boolean): Appointment => (marks ? { ...a, edited } : a);
      const cal: Calendar = {
        from: url.searchParams.get("from") ?? today,
        to: url.searchParams.get("to") ?? today,
        today,
        opens_at: "09:00",
        closes_at: "21:00",
        slot_minutes: 15,
        time_zone: "Asia/Ho_Chi_Minh",
        technicians: [],
        max_per_slot: null,
        appointments: [mark(booking({}), true), mark(booking({ id: "a2", customer_id: "c2", customer_name: "Trần Thị Hoa", starts_at: `${today}T05:00:00Z`, ends_at: `${today}T06:00:00Z` }), false)],
        visits: [],
      };
      return ok(cal);
    }
    if (p === "/attendance") {
      const sheet: AttendanceSheet = {
        month,
        days_in_month: 30,
        today,
        closed,
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
            days: [{ staff_id: "s-lan", date: today, status: "present", check_in: "09:12", check_out: null, late_minutes: 12, note: "Kẹt xe" }],
            totals: { present: 1, late_count: 1, late_minutes: 12, leave_p: 0, leave_dx: 0, absent_k: 0, unrecorded: 0 },
            can_record: !lanLocked,
            visit_days: [],
          },
        ],
      };
      return ok(sheet);
    }
    if (p === "/daily-reviews") {
      const reviews: ReviewSheet = {
        month,
        today,
        closed,
        staff: [{ staff_id: "s-lan", full_name: "Lê Thị Lan", position: "Kỹ thuật viên", compliments: 1, complaints: 0, days: [{ staff_id: "s-lan", date: today, compliments: 1, complaints: 0, note: null }], can_record: !lanLocked }],
      };
      return ok(reviews);
    }
    if (p === "/adjustments") {
      const entry: Adjustment = {
        id: "adj-1",
        staff_id: "s-lan",
        occurred_on: today,
        kind: "bonus",
        amount: 300_000,
        points: null,
        reason: "Khách khen đích danh",
        corrects_month: null,
        by_name: "Quản lý Hà",
        created_at: `${today}T03:00:00Z`,
        can_edit: true,
      };
      return ok({ month, closed, items: [entry] } satisfies AdjustmentLog);
    }
    if (p.startsWith("/record-history/")) {
      // `switchedOff`: the operator hid it mid-session; `/me` still says it until refetched.
      if (!shown || switchedOff) return ok({ code: "auth.forbidden" }, 403);
      const [, , kind, id] = p.split("/");
      if (kind === "appointment") return ok(story({ type: "appointment", id: id ?? "" }, "appointment.create", 1));
      if (kind === "adjustment") return ok(story({ type: "adjustment", id: id ?? "" }, "staff.adjustment", 1));
      const book = url.searchParams.get("book") === "daily_review" ? "daily_review" : "attendance";
      return ok(story({ type: book }, book === "attendance" ? "attendance.record" : "evaluation.daily_review", book === "attendance" ? 2 : 0));
    }
    return ok([]);
  });
}

const urls = (spy: ReturnType<typeof serve>) => spy.mock.calls.map(([input]) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url));
/** Every history request, repeats included. */
const traces = (spy: ReturnType<typeof serve>) => urls(spy).filter((u) => u.includes("/record-history/"));
const ATTENDANCE = `/api/v1/record-history/staff-day/s-lan/${today}?book=attendance`;
const REVIEW = `/api/v1/record-history/staff-day/s-lan/${today}?book=daily_review`;
/** No control to type, pick or count in: read-only. */
const inputs = (sheet: HTMLElement) =>
  ["textbox", "radio", "checkbox", "spinbutton", "combobox"].flatMap((role) => within(sheet).queryAllByRole(role));
const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Lịch hẹn", () => {
  it("the list view asks for the marks and shows 'Đã sửa' on an edited booking; its sheet ends with who booked it", async () => {
    const spy = serve([...DESK, HISTORY]);
    const user = userEvent.setup();
    at("/quay?view=lich&kieu=ds");
    const list = await screen.findByRole("list", { name: /^Lịch hẹn ngày/ });
    await waitFor(() => {
      expect(within(list).getAllByText("Đã sửa")).toHaveLength(1);
    });
    expect(within(list).getByRole("button", { name: /Lê Thị Mai.* Đã sửa/ })).toBeInTheDocument();
    expect(within(list).getByRole("button", { name: /Trần Thị Hoa/ })).not.toHaveTextContent("Đã sửa");
    expect(urls(spy).some((u) => u.includes("/appointments?") && u.includes("marks=true"))).toBe(true);

    await user.click(within(list).getByRole("button", { name: /Lê Thị Mai/ }));
    const sheet = await screen.findByRole("dialog");
    const trace = await within(sheet).findByRole("button", { name: "Ghi bởi Hà · 08:15 · đã sửa 1 lần" });
    expect(trace).toHaveAttribute("aria-expanded", "false");
    // The last element of the sheet's body.
    expect(sheet.querySelector(".appt-sheet")?.lastElementChild).toBe(trace.closest(".record-trace"));
    expect(urls(spy)).toContain("/api/v1/record-history/appointment/a1");
  });

  it("the day grid never asks for the marks", async () => {
    const spy = serve([...DESK, HISTORY]);
    at("/quay?view=lich&kieu=ngay");
    await waitFor(() => {
      expect(urls(spy).some((u) => u.startsWith("/api/v1/appointments?"))).toBe(true);
    });
    expect(urls(spy).some((u) => u.includes("marks=true"))).toBe(false);
  });

  it("a manager by default: no marks asked, no chip, no history in the sheet", async () => {
    const spy = serve(DESK);
    const user = userEvent.setup();
    at("/quay?view=lich&kieu=ds");
    const list = await screen.findByRole("list", { name: /^Lịch hẹn ngày/ });
    expect(within(list).queryByText("Đã sửa")).toBeNull();
    await user.click(within(list).getByRole("button", { name: /Lê Thị Mai/ }));
    const sheet = await screen.findByRole("dialog");
    expect(within(sheet).queryByRole("button", { name: /^Ghi bởi/ })).toBeNull();
    expect(urls(spy).some((u) => u.includes("marks=true") || u.includes("/record-history/"))).toBe(false);
  });
});

describe("Sổ hằng ngày → Giờ & ghi chú", () => {
  it("each fieldset ends with its own book's cell history (a button, never a submit)", async () => {
    const spy = serve([...BOOKS, HISTORY]);
    const user = userEvent.setup();
    at("/nhan-vien");
    await user.click(await screen.findByRole("button", { name: "Giờ & ghi chú" }));
    const sheet = await screen.findByRole("dialog", { name: "Lê Thị Lan" });
    const attendance = within(sheet).getByRole("group", { name: "Chấm công" });
    const review = within(sheet).getByRole("group", { name: /^Khen, phàn nàn/ });
    const own = await within(attendance).findByRole("button", { name: "Ghi bởi Hà · 08:15 · đã sửa 2 lần" });
    const theirs = await within(review).findByRole("button", { name: "Ghi bởi Hà · 08:15" });
    for (const [fieldset, button] of [
      [attendance, own],
      [review, theirs],
    ] as const) {
      expect(button).toHaveAttribute("type", "button");
      expect(fieldset.lastElementChild).toBe(button.closest(".record-trace"));
    }
    // One request per book for this open (the prefetch's reply is not asked again on mount).
    expect(traces(spy).sort()).toEqual([ATTENDANCE, REVIEW]);
    // Opening it saves nothing.
    await user.click(own);
    expect(own).toHaveAttribute("aria-expanded", "true");
    expect(spy.mock.calls.some(([, init]) => (init?.method ?? "GET") !== "GET")).toBe(false);
  });

  it("a manager by default: no history, no request", async () => {
    const spy = serve(BOOKS);
    const user = userEvent.setup();
    at("/nhan-vien");
    await user.click(await screen.findByRole("button", { name: "Giờ & ghi chú" }));
    const sheet = await screen.findByRole("dialog", { name: "Lê Thị Lan" });
    expect(within(sheet).queryByRole("button", { name: /^Ghi bởi/ })).toBeNull();
    expect(urls(spy).some((u) => u.includes("/record-history/"))).toBe(false);
  });
});

describe("Sổ hằng ngày, Bảng công tháng → chỉ xem (K30, K49)", () => {
  /** The read-only day: its values, no input, no save; each book's part ends with its history where shown. */
  async function viewLan(user: ReturnType<typeof userEvent.setup>) {
    await user.click(await screen.findByRole("button", { name: "Xem giờ & ghi chú" }));
    const sheet = await screen.findByRole("dialog", { name: "Lê Thị Lan" });
    expect(sheet).toHaveTextContent("chỉ xem");
    expect(inputs(sheet)).toEqual([]);
    expect(within(sheet).queryByRole("button", { name: /^Lưu|^Xóa chấm công/ })).toBeNull();
    const attendance = within(sheet).getByRole("region", { name: "Chấm công" });
    expect(attendance).toHaveTextContent("Đi làm");
    expect(attendance).toHaveTextContent("Muộn 12 phút");
    expect(attendance).toHaveTextContent("đến 09:12");
    expect(attendance).toHaveTextContent("Kẹt xe");
    const review = within(sheet).getByRole("region", { name: "Khen, phàn nàn" });
    expect(review).toHaveTextContent("1 lời khen · 0 phàn nàn");
    return { sheet, attendance, review };
  }

  it("a closed month, with the history right: the card opens read-only, each part ending with its history, one request each", async () => {
    const spy = serve([...BOOKS, HISTORY], { closed: true });
    const user = userEvent.setup();
    at("/nhan-vien");
    expect(await screen.findByRole("button", { name: "Xem giờ & ghi chú" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Giờ & ghi chú" })).toBeNull();
    const { attendance, review } = await viewLan(user);
    const own = await within(attendance).findByRole("button", { name: "Ghi bởi Hà · 08:15 · đã sửa 2 lần" });
    const theirs = await within(review).findByRole("button", { name: "Ghi bởi Hà · 08:15" });
    expect(attendance.lastElementChild).toBe(own.closest(".record-trace"));
    expect(review.lastElementChild).toBe(theirs.closest(".record-trace"));
    expect(traces(spy).sort()).toEqual([ATTENDANCE, REVIEW]);
    await user.click(own);
    expect(own).toHaveAttribute("aria-expanded", "true");
    expect(spy.mock.calls.some(([, init]) => (init?.method ?? "GET") !== "GET")).toBe(false);
  });

  it("a closed month without the history right: the same values, no history, no request", async () => {
    const spy = serve(BOOKS, { closed: true });
    const user = userEvent.setup();
    at("/nhan-vien");
    const { sheet } = await viewLan(user);
    expect(within(sheet).queryByRole("button", { name: /^Ghi bởi|^Chưa có ai ghi|^Đang tải ai ghi/ })).toBeNull();
    expect(traces(spy)).toEqual([]);
  });

  it("an open month, a manager shown the history: another manager's card (owner's to write) opens read-only with its history", async () => {
    const spy = serve([...BOOKS, HISTORY], { lanLocked: true });
    const user = userEvent.setup();
    at("/nhan-vien");
    expect(await screen.findByText("Chỉ chủ spa chấm công, ghi khen/phàn nàn và thưởng/phạt cho quản lý.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Giờ & ghi chú" })).toBeNull();
    const { attendance } = await viewLan(user);
    expect(await within(attendance).findByRole("button", { name: "Ghi bởi Hà · 08:15 · đã sửa 2 lần" })).toBeInTheDocument();
    expect(traces(spy).sort()).toEqual([ATTENDANCE, REVIEW]);
  });

  it("an open month, a manager by default: another manager's card has nothing to open, no request", async () => {
    const spy = serve(BOOKS, { lanLocked: true });
    at("/nhan-vien");
    expect(await screen.findByText("Chỉ chủ spa chấm công, ghi khen/phàn nàn và thưởng/phạt cho quản lý.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /giờ & ghi chú$/i })).toBeNull();
    expect(traces(spy)).toEqual([]);
  });

  it("a manager's own profile is not in the book the server sends: no card, so nothing of theirs is offered or asked", async () => {
    // The server leaves the caller's own profile out of the books (hr::scope `lists`): its cell's history would be 404.
    const spy = serve([...BOOKS, HISTORY], { lanLocked: true });
    at("/nhan-vien");
    const book = await screen.findByRole("list", { name: /^Sổ ngày/ });
    expect(within(book).getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Lê Thị Lan"]);
    expect(traces(spy).every((u) => u.includes("/staff-day/s-lan/"))).toBe(true);
  });

  it("the month grid: a closed month's cell opens read-only; an open month's locked row only with the history right", async () => {
    const lanToday = new RegExp(`^Lê Thị Lan, .*${today.slice(8, 10)}/${today.slice(5, 7)}/${today.slice(0, 4)}: Đi làm`);
    for (const [permissions, options, opens, hint] of [
      [[...BOOKS, HISTORY], { closed: true }, true, "Bấm một ô để xem."],
      [BOOKS, { closed: true }, true, "Bấm một ô để xem."],
      [[...BOOKS, HISTORY], { lanLocked: true }, true, "Bấm một ô để xem."],
      // Nothing opens: no hint to click.
      [BOOKS, { lanLocked: true }, false, null],
    ] as const) {
      const spy = serve([...permissions], options);
      const user = userEvent.setup();
      const view = at("/nhan-vien?tab=month");
      expect(await screen.findByRole("region", { name: /^Bảng công/ })).toBeInTheDocument();
      if (hint) expect(screen.getByText(hint)).toBeInTheDocument();
      else expect(screen.queryByText(/^Bấm một ô/)).toBeNull();
      const cell = screen.queryByRole("button", { name: lanToday });
      expect(cell !== null).toBe(opens);
      if (cell) {
        await user.click(cell);
        const sheet = await screen.findByRole("dialog", { name: "Lê Thị Lan" });
        expect(sheet).toHaveTextContent("chỉ xem");
        expect(inputs(sheet)).toEqual([]);
        const traced = permissions.includes(HISTORY);
        if (traced) expect(await within(sheet).findByRole("button", { name: "Ghi bởi Hà · 08:15 · đã sửa 2 lần" })).toBeInTheDocument();
        await waitFor(() => {
          expect(traces(spy).filter((u) => u.includes("/staff-day/")).sort()).toEqual(traced ? [ATTENDANCE, REVIEW] : []);
        });
      }
      view.unmount();
      vi.restoreAllMocks();
    }
  });
});

describe("Sổ thưởng, phạt", () => {
  it("⋯ → 'Xem ai ghi, ai sửa' opens the entry's history under its row, open; again hides it", async () => {
    const spy = serve([...BOOKS, HISTORY]);
    const user = userEvent.setup();
    at("/nhan-vien?tab=month");
    await user.click(await screen.findByRole("button", { name: "Thao tác Thưởng 300.000đ của Lê Thị Lan" }));
    await user.click(screen.getByRole("menuitem", { name: "Xem ai ghi, ai sửa" }));
    const trace = await screen.findByRole("button", { name: "Ghi bởi Hà · 08:15 · đã sửa 1 lần" });
    expect(trace).toHaveAttribute("aria-expanded", "true");
    await waitFor(() => {
      expect(trace).toHaveFocus();
    });
    expect(urls(spy)).toContain("/api/v1/record-history/adjustment/adj-1");
    await user.click(screen.getByRole("button", { name: "Thao tác Thưởng 300.000đ của Lê Thị Lan" }));
    await user.click(screen.getByRole("menuitem", { name: "Ẩn ai ghi, ai sửa" }));
    expect(screen.queryByRole("button", { name: /^Ghi bởi Hà/ })).toBeNull();
  });

  it("switched off mid-session (403): the history hides and focus goes to the log's heading", async () => {
    serve([...BOOKS, HISTORY], { switchedOff: true });
    const user = userEvent.setup();
    at("/nhan-vien?tab=month");
    await user.click(await screen.findByRole("button", { name: "Thao tác Thưởng 300.000đ của Lê Thị Lan" }));
    await user.click(screen.getByRole("menuitem", { name: "Xem ai ghi, ai sửa" }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Sổ thưởng, phạt của tháng" })).toHaveFocus();
    });
    expect(screen.queryByRole("button", { name: /^Ghi bởi|^Đang tải ai ghi/ })).toBeNull();
  });

  it("a manager by default: no ⋯ for it", async () => {
    serve(BOOKS);
    at("/nhan-vien?tab=month");
    expect(await screen.findByRole("table", { name: "Sổ thưởng phạt" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Thao tác Thưởng/ })).toBeNull();
  });
});
