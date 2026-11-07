import type { NoteEntry, NotesPage } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { navFor } from "../../app/nav";
import { spaToday } from "../corrections/api";
import { ME } from "../reports/testServer";
import { notesParams } from "./api";
import { notesSearch } from "./search";

const MANAGER = ["visit.record", "visit.read", "sale.read", "customer.read", "customer.health_notes.read", "attendance.record", "evaluation.record", "report.read", "staff.read_basic"];
const TECH = ["staff.read_own", "catalog.read"];
const CUSTOMER = "0193a5b0-0000-7000-8000-0000000000c1";
const LAN = "0193a5b0-0000-7000-8000-0000000000a1";
const VISIT = "0193a5b0-0000-7000-8000-0000000000b1";
const today = spaToday();

const NOTES: NoteEntry[] = [
  {
    kind: "visit_note", at: `${today}T07:30:00Z`, business_date: today, text: "Đau vai gáy, ấn nhẹ",
    customer_id: CUSTOMER, customer_name: "Nguyễn Thị Hoa", author_name: "Quản lý Hà", visit_id: VISIT,
  },
  {
    kind: "adjustment", at: `${today}T06:00:00Z`, business_date: today, text: "Bán được gói lớn",
    staff_id: LAN, staff_name: "KTV Lan", author_name: "Quản lý Hà", detail: "bonus",
  },
  {
    kind: "void", at: "2026-11-01T03:00:00Z", business_date: "2026-11-01", text: "Ghi nhầm khách",
    customer_id: CUSTOMER, customer_name: "Nguyễn Thị Hoa", author_name: "Chủ Spa", visit_id: VISIT, detail: "visit_corrected",
  },
];

function serve(permissions: string[], page: Partial<NotesPage> = {}) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://app.test");
    const p = url.pathname.replace(/^\/api\/v1/, "");
    const json = (body: unknown, status = 200) => Promise.resolve(Response.json(body, { status }));
    if (p === "/me") return json(ME(permissions));
    if (p === "/staff") return json([{ id: LAN, full_name: "KTV Lan", position: "Kỹ thuật viên", status: "active" }]);
    if (p === "/notes") {
      if (!permissions.includes("report.read")) return json({ status: 403, code: "auth.forbidden" }, 403);
      return json({
        from: "2026-10-09", to: today, kinds: ["visit_note", "adjustment", "void"],
        items: NOTES, next_cursor: null, scan_capped: false, ...page,
      } satisfies NotesPage);
    }
    return json({ status: 404, code: "not_found" }, 404);
  });
}

const at = (path: string) => {
  const history = createMemoryHistory({ initialEntries: [path] });
  render(<App history={history} />);
  return { search: () => Object.fromEntries(new URLSearchParams(history.location.search)) };
};

const notesCalls = (spy: ReturnType<typeof serve>) =>
  spy.mock.calls
    .map(([input]) => new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://app.test"))
    .filter((u) => u.pathname.endsWith("/notes"));

describe("notes search state", () => {
  it("keeps only valid filters in the URL and maps them to the API", () => {
    expect(notesSearch.validate({ kind: "nope", staff: "x", customer: CUSTOMER.toUpperCase(), from: "2026-13-40", q: "vai" })).toEqual({
      customer: CUSTOMER,
      q: "vai",
    });
    const params = notesParams(notesSearch.parse({ kind: "void", staff: LAN, from: "2026-11-01", to: "2026-11-07", q: "gáy" }));
    expect(Object.fromEntries(params)).toEqual({ kind: "void", staff_id: LAN, from: "2026-11-01", to: "2026-11-07", q: "gáy", limit: "30" });
  });

  it("puts 'Nhật ký ghi chú' under Báo cáo for report readers only", () => {
    const report = (perms: string[]) => navFor(ME(perms)).find((g) => g.label === "Báo cáo")?.items.map((i) => i.label);
    expect(report(MANAGER)).toEqual(["Báo cáo kinh doanh", "Nhật ký ghi chú"]);
    expect(report(TECH)).toBeUndefined();
  });
});

describe("/nhat-ky-ghi-chu", () => {
  it("lists notes by day with type, time, author, subject and a link back to the source", async () => {
    serve(MANAGER);
    at("/nhat-ky-ghi-chu");
    expect(await screen.findByRole("heading", { level: 1, name: "Nhật ký ghi chú" })).toBeInTheDocument();
    const day = await screen.findByRole("region", { name: new RegExp(today.split("-").reverse().join("/")) });
    const rows = within(day).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    const [visit, bonus] = rows;
    if (!visit || !bonus) throw new Error("rows");
    expect(within(visit).getByText("Ghi chú buổi")).toBeInTheDocument();
    expect(within(visit).getByText("Đau vai gáy, ấn nhẹ")).toBeInTheDocument();
    expect(within(visit).getByText("· Quản lý Hà")).toBeInTheDocument();
    expect(within(visit).getByRole("link", { name: "Nguyễn Thị Hoa" })).toHaveAttribute("href", `/khach-hang/${CUSTOMER}`);
    expect(within(visit).getByRole("link", { name: "Xem lượt khách" }).getAttribute("href")).toContain(`hfrom=${today}`);
    expect(within(bonus).getByText("Thưởng")).toBeInTheDocument();
    expect(within(bonus).getByRole("link", { name: "Mở sổ ngày" }).getAttribute("href")).toContain(`day=${today}`);
    // A correction says so; its link opens the voided visit in the customer's history.
    expect(screen.getByText("Sửa lượt")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Xem trong lịch sử khách" }).getAttribute("href")).toContain("hkind=visit_voided");
    // The type filter offers only the kinds the server says this caller may read.
    const kind = screen.getByRole("combobox", { name: "Loại" });
    expect(within(kind).getAllByRole("option").map((o) => o.textContent)).toEqual(["Mọi loại", "Ghi chú buổi", "Thưởng / phạt", "Hủy / sửa"]);
  });

  it("keeps filters in the URL and sends them to the API", async () => {
    const spy = serve(MANAGER);
    const { search } = at("/nhat-ky-ghi-chu");
    await screen.findByText("Đau vai gáy, ấn nhẹ");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Loại" }), "void");
    await waitFor(() => {
      expect(search()).toMatchObject({ kind: "void" });
    });
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Nhân viên" }), LAN);
    await waitFor(() => {
      expect(notesCalls(spy).at(-1)?.searchParams.get("staff_id")).toBe(LAN);
    });
    expect(notesCalls(spy).at(-1)?.searchParams.get("kind")).toBe("void");
    // One customer: from the note, as a chip that removes the filter.
    await userEvent.click(screen.getAllByRole("button", { name: "Chỉ xem khách này" })[0] as HTMLElement);
    await waitFor(() => {
      expect(search()).toMatchObject({ customer: CUSTOMER });
    });
    expect(notesCalls(spy).at(-1)?.searchParams.get("customer_id")).toBe(CUSTOMER);
    await userEvent.click(screen.getByRole("button", { name: "Bỏ lọc theo khách Nguyễn Thị Hoa" }));
    await waitFor(() => {
      expect(search()).not.toHaveProperty("customer");
    });
  });

  it("says when a search stopped at its cap and offers to search further", async () => {
    serve(MANAGER, { items: [], next_cursor: "abc", scan_capped: true });
    at("/nhat-ky-ghi-chu?q=vai");
    expect(await screen.findByText(/Đã tìm trong 500 ghi chú gần nhất/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tìm tiếp" })).toBeInTheDocument();
  });

  it("technicians get 'Không có quyền' instead of the screen", async () => {
    const spy = serve(TECH);
    at("/nhat-ky-ghi-chu");
    expect(await screen.findByText(/Không có quyền/)).toBeInTheDocument();
    expect(notesCalls(spy)).toHaveLength(0);
  });
});
