import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { spaToday } from "../corrections/api";
import { addDays, compactVnd, packagesPath, relativeDay } from "./api";
import { niceTicks } from "./LineChart";
import { daysBetween, resolvePeriod } from "./period";
import { inactiveSearch, reportsSearch } from "./search";
import { mockDaily, mockRange } from "./fixtures";
import { displayPhone } from "./PackagesOverviewPage";
import { serveManagement } from "./testServer";

const MANAGER = ["visit.record", "customer.read", "catalog.read", "report.read", "report.revenue.read"];

const at = (path: string) => {
  const history = createMemoryHistory({ initialEntries: [path] });
  render(<App history={history} />);
  /** Current URL search as plain strings (what the back button would restore). */
  const search = () => Object.fromEntries(new URLSearchParams(history.location.search));
  return { search };
};
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
    expect(packagesPath("cancelled", "Hà")).toBe("/packages?status=cancelled&q=H%C3%A0&limit=50");
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
  const today = spaToday();

  it("opens on the last 7 days against the 7 before: KPIs, line chart, breakdown, top lists", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    at("/bao-cao");
    expect(await screen.findByRole("heading", { level: 1, name: "Báo cáo kinh doanh" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "7 ngày" })).toHaveAttribute("aria-pressed", "true");
    expect(await screen.findByText("Thực thu", { selector: "p" })).toBeInTheDocument();
    expect(await screen.findByRole("img", { name: /^Thực thu theo ngày/ })).toBeInTheDocument();
    const table = await screen.findByRole("table", { name: /Chi tiết theo ngày/ });
    // 7 days + header + the "Cả kỳ" footer.
    expect(within(table).getAllByRole("row")).toHaveLength(9);
    expect(screen.getAllByText(/so với 7 ngày trước đó/).length).toBeGreaterThan(0);
    expect(screen.getByRole("region", { name: /Gói bán chạy/ })).toBeInTheDocument();
    expect(urls(spy)).toContain(`/api/v1/reports/range?from=${addDays(today, -6)}&to=${today}&granularity=day`);
    expect(urls(spy)).toContain(`/api/v1/reports/range?from=${addDays(today, -13)}&to=${addDays(today, -7)}&granularity=day`);
  });

  it("keeps the period in the URL: presets, the year by month, a custom range", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    const { search } = at("/bao-cao");
    await user.click(await screen.findByRole("button", { name: "30 ngày" }));
    await waitFor(() => {
      expect(search()).toEqual({ period: "30d" });
    });
    expect(urls(spy)).toContain(`/api/v1/reports/range?from=${addDays(today, -29)}&to=${today}&granularity=day`);

    await user.click(screen.getByRole("button", { name: "Năm nay" }));
    await waitFor(() => {
      expect(urls(spy)).toContain(`/api/v1/reports/range?from=${today.slice(0, 4)}-01-01&to=${today}&granularity=month`);
    });
    expect(await screen.findByRole("table", { name: /Chi tiết theo tháng/ })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Tùy chọn" }));
    const from = screen.getByLabelText("Từ ngày");
    await user.clear(from);
    await user.type(from, addDays(today, -2));
    await waitFor(() => {
      expect(search()).toMatchObject({ period: "custom", from: addDays(today, -2), to: today });
    });
    expect(urls(spy)).toContain(`/api/v1/reports/range?from=${addDays(today, -2)}&to=${today}&granularity=day`);
  });

  it("restores the period and the metric from the URL (back button)", async () => {
    serveManagement({ permissions: MANAGER });
    at("/bao-cao?period=this_month&metric=visits");
    expect(await screen.findByRole("button", { name: "Tháng này" })).toHaveAttribute("aria-pressed", "true");
    expect(await screen.findByRole("img", { name: /^Lượt khách theo ngày/ })).toBeInTheDocument();
  });

  it("shows the ledger tab: the paper book's columns with day totals", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    at("/bao-cao");
    await user.click(await screen.findByRole("tab", { name: "Sổ doanh thu" }));
    const table = await screen.findByRole("table", { name: /Sổ doanh thu/ });
    expect(within(table).getByRole("columnheader", { name: "Dịch vụ 1 lần" })).toBeInTheDocument();
    expect(within(table).getAllByText(/^Cộng /)).toHaveLength(7);
    expect(within(table).getAllByText("Hủy hóa đơn").length).toBeGreaterThan(0);
    expect(urls(spy).some((u) => u.startsWith(`/api/v1/reports/ledger?from=${addDays(today, -6)}&to=${today}&limit=100`))).toBe(true);
  });

  it("does not ask the ledger for more than the API serves (92 days)", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    at("/bao-cao?tab=ledger&period=12m");
    expect(await screen.findByText(/xem tối đa 3 tháng/)).toBeInTheDocument();
    expect(urls(spy).some((u) => u.includes("/reports/ledger"))).toBe(false);
  });

  it("is closed to users without report.read (UX gate; the API decides)", async () => {
    serveManagement({ permissions: ["catalog.read", "sale.read"] });
    at("/bao-cao");
    expect(await screen.findByText("Màn hình này chưa mở cho tài khoản của bạn.")).toBeInTheDocument();
  });
});

describe("report periods", () => {
  it("resolves presets and the previous period of equal length", () => {
    expect(resolvePeriod("today", "2026-11-05")).toMatchObject({ from: "2026-11-05", to: "2026-11-05", previous: { from: "2026-11-04", to: "2026-11-04" } });
    expect(resolvePeriod("this_month", "2026-03-31")).toMatchObject({ from: "2026-03-01", previous: { from: "2026-02-01", to: "2026-02-28" } });
    expect(resolvePeriod("last_month", "2026-01-15")).toMatchObject({ from: "2025-12-01", to: "2025-12-31", previous: { from: "2025-11-01", to: "2025-11-30" } });
    expect(resolvePeriod("this_year", "2026-11-05")).toMatchObject({ from: "2026-01-01", bucket: "month", previous: { from: "2025-01-01", to: "2025-11-05" } });
    expect(resolvePeriod("12m", "2026-11-05")).toMatchObject({ from: "2025-12-01", bucket: "month", previous: { from: "2024-12-01", to: "2025-11-05" } });
    expect(resolvePeriod("custom", "2026-11-05", { from: "2026-10-01", to: "2026-11-05" })).toMatchObject({ bucket: "week", previous: { from: "2026-08-26", to: "2026-09-30" } });
    // A broken or future custom range falls back to 7 days instead of breaking the screen.
    expect(resolvePeriod("custom", "2026-11-05", { from: "2026-11-09", to: "2026-11-01" })).toMatchObject({ from: "2026-10-30", to: "2026-11-05" });
    expect(daysBetween("2026-01-01", "2026-12-31")).toBe(365);
    expect(niceTicks(7_300_000)).toEqual([0, 2_500_000, 5_000_000, 7_500_000]);
  });

  it("parses search params and drops defaults and junk", () => {
    expect(reportsSearch.validate({ period: "7d", metric: "revenue", tab: "x" })).toEqual({});
    expect(reportsSearch.validate({ period: "custom", from: "2026-10-01", to: "nope" })).toEqual({ period: "custom", from: "2026-10-01" });
    expect(inactiveSearch.validate({ days: "14", follow: "due", kind: "one_off", q: "Hà" })).toEqual({ days: 14, kind: "one_off", q: "Hà" });
    expect(inactiveSearch.validate({ days: "13", follow: "all", scope: "nope" })).toEqual({ follow: "all" });
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
    expect(await screen.findByRole("heading", { level: 1, name: /^Nguyễn Thị Hà/ })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Lịch sử sử dụng" })).not.toBeInTheDocument();
  });
});
