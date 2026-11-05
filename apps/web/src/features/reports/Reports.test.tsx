import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { addDays, compactVnd, dailyReportQuery, relativeDay } from "./api";
import { mockDaily, mockRange } from "./mock";

// Management views run on the typed mock in tests (app/features.ts: dev/test → "mock").
const MANAGER = ["visit.record", "customer.read", "catalog.read", "sale.read"];

function serve(permissions: string[]) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (url === "/api/v1/me")
      return Promise.resolve(
        Response.json({
          user: { id: "u1", full_name: "Quản lý Hà", phone: "0905 123 456" },
          active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
          tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
          permissions,
        }),
      );
    if (url === "/api/v1/customers/c1")
      return Promise.resolve(
        Response.json({ id: "c1", full_name: "Nguyễn Thị Hà", phone: "0905 111 222", gender: "female", version: 1 }),
      );
    return Promise.resolve(Response.json([]));
  });
}

const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

describe("formatting helpers", () => {
  it("names recent days and compacts money for chart labels", () => {
    expect(relativeDay("2026-11-05", "2026-11-05")).toBe("Hôm nay");
    expect(relativeDay("2026-11-04", "2026-11-05")).toBe("Hôm qua");
    expect(relativeDay("2026-10-30", "2026-11-05")).toBe("30/10/2026");
    expect(compactVnd(3_250_000)).toBe("3,3 tr");
    expect(compactVnd(850_000)).toBe("850 k");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
  });

  it("keeps the mock consistent with the contract's own sums", () => {
    const d = mockDaily("2026-11-05");
    const rev = d.revenue;
    expect(rev).toBeDefined();
    if (!rev) return;
    expect(rev.net.total).toBe(rev.collected.total - rev.voids.total - rev.refunds.total);
    expect(rev.net.cash + rev.net.bank_transfer).toBe(rev.net.total);
    expect(d.technicians.reduce((s, t) => s + t.visits, 0)).toBe(d.counts.visits);
    expect(mockRange("2026-10-30", "2026-11-05").series).toHaveLength(7);
  });

  it("asks the API for the day when the real source is on", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(Response.json(mockDaily("2026-11-05")));
    const opts = dailyReportQuery("2026-11-05", "api");
    await opts.queryFn?.({} as never);
    expect(fetchSpy).toHaveBeenCalledWith("/api/v1/reports/daily?date=2026-11-05", expect.anything());
  });
});

describe("/bao-cao", () => {
  it("shows today's numbers, the 7-day chart and each technician", async () => {
    serve(MANAGER);
    at("/bao-cao");
    expect(await screen.findByRole("heading", { level: 1, name: "Tổng quan hôm nay" })).toBeInTheDocument();
    expect(screen.getByText(/Dữ liệu mẫu/)).toBeInTheDocument();
    expect(await screen.findByText("Thực thu", { selector: "p" })).toBeInTheDocument();
    const chart = await screen.findByRole("list", { name: "Thực thu theo ngày" });
    expect(within(chart).getAllByRole("listitem")).toHaveLength(7);
    const table = await screen.findByRole("table");
    expect(within(table).getAllByRole("row").length).toBeGreaterThan(1);
  });

  it("steps to the previous day and back to today", async () => {
    serve(MANAGER);
    const user = userEvent.setup();
    at("/bao-cao");
    await user.click(await screen.findByRole("button", { name: "Ngày trước" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Tổng quan hôm qua" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hôm nay" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Tổng quan hôm nay" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ngày sau" })).toBeDisabled();
  });

  it("is closed to users without the permission (UX gate; the API decides)", async () => {
    serve(["catalog.read"]);
    at("/bao-cao");
    expect(await screen.findByText("Màn hình này chưa mở cho tài khoản của bạn.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Hôm nay" })).not.toBeInTheDocument();
  });
});

describe("management lists", () => {
  it("filters packages by status", async () => {
    serve(MANAGER);
    const user = userEvent.setup();
    at("/goi-lieu-trinh");
    const table = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    expect(within(table).getAllByRole("row").length).toBeGreaterThan(1);
    await user.click(screen.getByRole("button", { name: /Đã hủy/ }));
    const filtered = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    for (const row of within(filtered).getAllByRole("row").slice(1)) expect(row).toHaveTextContent("Đã hủy");
  });

  it("lists absent customers, longest first", async () => {
    serve(MANAGER);
    at("/nhac-khach");
    const table = await screen.findByRole("table", { name: "Khách lâu chưa quay lại" });
    const days = within(table)
      .getAllByRole("row")
      .slice(1)
      .map((r) => Number(/(\d+) ngày/.exec(r.textContent)?.[1]));
    expect(days).toEqual([...days].sort((a, b) => b - a));
    expect(days.every((d) => d >= 30)).toBe(true);
  });
});

describe("customer history", () => {
  it("shows the summary and the usage timeline on the profile", async () => {
    serve(MANAGER);
    at("/khach-hang/c1");
    const section = await screen.findByRole("region", { name: "Lịch sử sử dụng" });
    expect(await within(section).findByText("Buổi còn lại")).toBeInTheDocument();
    const timeline = await within(section).findByRole("list", { name: "Lịch sử sử dụng" });
    expect(within(timeline).getAllByText("Lượt khách").length).toBeGreaterThan(0);
    expect(within(timeline).getByText("Mua gói liệu trình")).toBeInTheDocument();
    expect(within(timeline).getByText(/Lý do hủy/)).toBeInTheDocument();
  });

  it("is not shown without the management permission", async () => {
    serve(["customer.read"]);
    at("/khach-hang/c1");
    expect(await screen.findByRole("heading", { level: 1, name: "Nguyễn Thị Hà" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Lịch sử sử dụng" })).not.toBeInTheDocument();
  });
});
