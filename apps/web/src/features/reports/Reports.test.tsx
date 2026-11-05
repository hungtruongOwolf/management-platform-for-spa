import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { addDays, compactVnd, packagesPath, relativeDay } from "./api";
import { mockDaily, mockRange } from "./fixtures";
import { displayPhone } from "./PackagesOverviewPage";
import { serveManagement } from "./testServer";

const MANAGER = ["visit.record", "customer.read", "catalog.read", "report.read", "report.revenue.read"];

const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);
const urls = (spy: ReturnType<typeof serveManagement>) =>
  spy.mock.calls.map(([input]) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url));

describe("formatting helpers", () => {
  it("names recent days and compacts money for chart labels", () => {
    expect(relativeDay("2026-11-05", "2026-11-05")).toBe("Hôm nay");
    expect(relativeDay("2026-11-04", "2026-11-05")).toBe("Hôm qua");
    expect(relativeDay("2026-10-30", "2026-11-05")).toBe("30/10/2026");
    expect(compactVnd(3_250_000)).toBe("3,3 tr");
    expect(compactVnd(850_000)).toBe("850 k");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
    expect(packagesPath("expiring")).toBe("/packages?expiring_within_days=30&limit=50");
    expect(packagesPath("cancelled")).toBe("/packages?status=cancelled&limit=50");
    expect(displayPhone("+84399000024")).toBe("0399 000 024");
    expect(displayPhone("0905 111 222")).toBe("0905 111 222");
  });

  it("keeps the test fixtures consistent with the contract's own sums", () => {
    const d = mockDaily("2026-11-05");
    const rev = d.revenue;
    expect(rev).toBeDefined();
    if (!rev) return;
    expect(rev.net.total).toBe(rev.collected.total - rev.voids.total - rev.refunds.total);
    expect(rev.net.cash + rev.net.bank_transfer).toBe(rev.net.total);
    expect(d.technicians.reduce((s, t) => s + t.visits, 0)).toBe(d.counts.visits);
    expect(mockRange("2026-10-30", "2026-11-05").series).toHaveLength(7);
  });
});

describe("/bao-cao", () => {
  it("asks the API for the day and the last 7 days, then shows numbers, chart and technicians", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    at("/bao-cao");
    expect(await screen.findByRole("heading", { level: 1, name: "Tổng quan hôm nay" })).toBeInTheDocument();
    expect(await screen.findByText("Thực thu", { selector: "p" })).toBeInTheDocument();
    const chart = await screen.findByRole("list", { name: "Thực thu theo ngày" });
    expect(within(chart).getAllByRole("listitem")).toHaveLength(7);
    const table = await screen.findByRole("table");
    expect(within(table).getAllByRole("row").length).toBeGreaterThan(1);
    expect(urls(spy).some((u) => u.startsWith("/api/v1/reports/daily?date="))).toBe(true);
    expect(urls(spy).some((u) => /\/api\/v1\/reports\/range\?from=.*&to=.*&granularity=day/.test(u))).toBe(true);
  });

  it("steps to the previous day and back to today", async () => {
    serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    at("/bao-cao");
    await user.click(await screen.findByRole("button", { name: "Ngày trước" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Tổng quan hôm qua" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hôm nay" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Tổng quan hôm nay" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ngày sau" })).toBeDisabled();
  });

  it("is closed to users without report.read (UX gate; the API decides)", async () => {
    serveManagement({ permissions: ["catalog.read", "sale.read"] });
    at("/bao-cao");
    expect(await screen.findByText("Màn hình này chưa mở cho tài khoản của bạn.")).toBeInTheDocument();
  });
});

describe("management lists", () => {
  it("filters packages by status", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    at("/goi-lieu-trinh");
    const table = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    expect(within(table).getAllByRole("row").length).toBeGreaterThan(1);
    await user.click(screen.getByRole("button", { name: /Đã hủy/ }));
    const filtered = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    for (const row of within(filtered).getAllByRole("row").slice(1)) expect(row).toHaveTextContent("Đã hủy");
    expect(urls(spy)).toContain("/api/v1/packages?status=cancelled&limit=50");
  });

  it("lists overdue customers from 7 days by default, longest first, with paging, threshold and scope", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    at("/khach-lau-khong-quay-lai");
    expect(await screen.findByRole("heading", { level: 1, name: "Khách lâu không quay lại" })).toBeInTheDocument();
    const table = await screen.findByRole("table", { name: "Khách lâu không quay lại" });
    const firstPage = within(table).getAllByRole("row").length - 1;
    const days = within(table)
      .getAllByRole("row")
      .slice(1)
      .map((r) => Number(/(\d+) ngày/.exec(within(r).getAllByRole("cell")[1]?.textContent ?? "")?.[1]));
    expect(days).toEqual([...days].sort((a, b) => b - a));
    expect(days.every((d) => d >= 7)).toBe(true);
    expect(within(table).getAllByRole("link")[0]).toHaveAttribute("href", expect.stringMatching(/^\/khach-hang\//));
    expect(urls(spy)).toContain("/api/v1/customers/inactive?days=7&scope=with_sessions&limit=20");

    await user.click(screen.getByRole("button", { name: "Xem thêm" }));
    await screen.findByRole("table", { name: "Khách lâu không quay lại" });
    expect(await screen.findAllByRole("row")).toHaveLength(firstPage * 2 + 1);
    expect(urls(spy)).toContain("/api/v1/customers/inactive?days=7&scope=with_sessions&limit=20&cursor=page-2");
    expect(screen.queryByRole("button", { name: "Xem thêm" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "≥ 30 ngày" }));
    await user.click(screen.getByRole("button", { name: "Mọi khách đã từng đến" }));
    await screen.findByRole("table", { name: "Khách lâu không quay lại" });
    expect(urls(spy)).toContain("/api/v1/customers/inactive?days=30&scope=all&limit=20");
  });
});

describe("customer history", () => {
  it("shows the summary and the usage timeline on the profile", async () => {
    serveManagement({ permissions: MANAGER });
    at("/khach-hang/c1");
    const section = await screen.findByRole("region", { name: "Lịch sử sử dụng" });
    expect(await within(section).findByText("Buổi còn lại")).toBeInTheDocument();
    const timeline = await within(section).findByRole("list", { name: "Lịch sử sử dụng" });
    expect(within(timeline).getAllByText("Lượt khách").length).toBeGreaterThan(0);
    expect(within(timeline).getByText("Mua gói liệu trình")).toBeInTheDocument();
    expect(within(timeline).getByText(/Lý do hủy/)).toBeInTheDocument();
  });

  it("is not shown without report.read", async () => {
    serveManagement({ permissions: ["customer.read"] });
    at("/khach-hang/c1");
    expect(await screen.findByRole("heading", { level: 1, name: "Nguyễn Thị Hà" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Lịch sử sử dụng" })).not.toBeInTheDocument();
  });
});
