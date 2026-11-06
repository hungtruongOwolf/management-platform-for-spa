import { createMemoryHistory } from "@tanstack/react-router";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { isoToDmy } from "../../components/DateField";
import { spaToday } from "../corrections/api";
import { addDays, compactVnd, packagesPath, relativeDay } from "./api";
import { niceTicks } from "./LineChart";
import { daysBetween, rangeLabel, resolvePeriod } from "./period";
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
    expect(screen.getByRole("region", { name: /Mẫu gói bán chạy/ })).toBeInTheDocument();
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
    // The field is dd/mm/yyyy text whatever the browser locale (DateField); Enter commits it.
    await user.type(from, `${isoToDmy(addDays(today, -2))}{Enter}`);
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
    // Round 6: who sold it (người giới thiệu / chốt) sits with the note.
    expect(within(table).getAllByText("Giới thiệu: KTV Lan").length).toBeGreaterThan(0);
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
    expect(rangeLabel("2026-10-30", "2026-11-05")).toBe("30/10 – 05/11/2026");
    expect(rangeLabel("2025-12-30", "2026-01-02")).toBe("30/12/2025 – 02/01/2026");
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
  it("filters packages by status and searches them; both live in the URL", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    const { search } = at("/goi-lieu-trinh");
    const table = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    expect(within(table).getAllByRole("row").length).toBeGreaterThan(1);
    await user.click(screen.getByRole("button", { name: /Đã hủy/ }));
    const filtered = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    for (const row of within(filtered).getAllByRole("row").slice(1)) expect(row).toHaveTextContent("Đã hủy");
    expect(urls(spy)).toContain("/api/v1/packages?status=cancelled&limit=50");

    await user.click(screen.getByRole("button", { name: "Tất cả" }));
    await user.type(screen.getByRole("searchbox", { name: /Tìm theo tên khách/ }), "Bùi");
    await waitFor(() => {
      expect(search()).toEqual({ status: "all", q: "Bùi" });
    });
    await waitFor(() => {
      expect(urls(spy)).toContain("/api/v1/packages?q=B%C3%B9i&limit=50");
    });
    await waitFor(() => {
      const rows = within(screen.getByRole("table", { name: "Gói liệu trình của khách" })).getAllByRole("row").slice(1);
      expect(rows.length).toBeGreaterThan(0);
      for (const row of rows) expect(row).toHaveTextContent("Bùi Thị Ngọc Hân");
    });
  });

  it("shows no-expiry packages as “Không hạn”, never among the expiring ones", async () => {
    serveManagement({ permissions: MANAGER });
    at("/goi-lieu-trinh?status=all");
    const table = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    const unlimited = within(table)
      .getAllByRole("row")
      .filter((r) => within(r).queryByText("Không hạn"));
    expect(unlimited.length).toBeGreaterThan(0);
    for (const row of unlimited) {
      expect(row).not.toHaveTextContent("Hết hạn");
      expect(row).not.toHaveTextContent(/còn \d+ ngày/);
    }
    cleanup();
    serveManagement({ permissions: MANAGER });
    at("/goi-lieu-trinh");
    const expiring = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    expect(within(expiring).queryByText("Không hạn")).not.toBeInTheDocument();
  });

  it("lists overdue package and one-off customers, “Cần liên hệ” first, with paging and filters in the URL", async () => {
    const spy = serveManagement({ permissions: [...MANAGER, "customer.follow_up"] });
    const user = userEvent.setup();
    const { search } = at("/khach-lau-khong-quay-lai");
    expect(await screen.findByRole("heading", { level: 1, name: "Khách lâu chưa quay lại" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cần liên hệ" })).toHaveAttribute("aria-pressed", "true");
    const table = await screen.findByRole("table", { name: "Khách lâu chưa quay lại" });
    expect(urls(spy)).toContain("/api/v1/customers/inactive?days=7&follow_up=due&limit=20");
    // Both kinds of customer, each named with gender (EM round 3 items 4 and 7).
    expect(within(table).getAllByText("Khách lẻ").length).toBeGreaterThan(0);
    expect(within(table).getAllByText(/^Liệu trình · còn/).length).toBeGreaterThan(0);
    expect(within(table).getAllByRole("link", { name: /, (Nữ|Nam)$/ }).length).toBeGreaterThan(0);

    await user.click(screen.getByRole("button", { name: "Xem thêm" }));
    await waitFor(() => {
      expect(urls(spy)).toContain("/api/v1/customers/inactive?days=7&follow_up=due&limit=20&cursor=page-2");
    });
    // The seeded customer called 3 days ago with no answer is due again today.
    // (The badge is in the row twice: its column from 640px, and inside the first cell on phones.)
    expect((await within(screen.getByRole("table", { name: "Khách lâu chưa quay lại" })).findAllByText(/Đến hạn/)).length).toBeGreaterThan(0);

    expect(screen.getAllByRole("button", { name: /^(Cần liên hệ|Đã hẹn|Tất cả)$/ })).toHaveLength(3);
    expect(screen.queryByRole("button", { name: "Chưa liên hệ" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đã hẹn" }));
    await user.selectOptions(screen.getByLabelText("Loại khách"), "package");
    await waitFor(() => {
      expect(search()).toEqual({ follow: "scheduled", kind: "package" });
    });
    await waitFor(() => {
      expect(urls(spy)).toContain("/api/v1/customers/inactive?days=7&kind=package&follow_up=scheduled&limit=20");
    });
    const scheduled = await screen.findByRole("table", { name: "Khách lâu chưa quay lại" });
    await waitFor(() => {
      expect(within(scheduled).getAllByRole("row")).toHaveLength(2);
    });
    expect(within(scheduled).getAllByText(/^Hẹn \d{2}\/\d{2}\/\d{4}$/).length).toBeGreaterThan(0);
  });

  it("lists every overdue customer longest absence first under “Tất cả”", async () => {
    serveManagement({ permissions: MANAGER });
    at("/khach-lau-khong-quay-lai?follow=all");
    const table = await screen.findByRole("table", { name: "Khách lâu chưa quay lại" });
    const days = within(table)
      .getAllByRole("row")
      .slice(1)
      .map((r) => Number(/(\d+) ngày/.exec(within(r).getAllByRole("cell")[1]?.textContent ?? "")?.[1]));
    expect(days).toEqual([...days].sort((a, b) => b - a));
    expect(days.every((d) => d >= 7)).toBe(true);
  });

  it("“Đã liên hệ” records a contact with a suggested next date, then refreshes the list", async () => {
    const spy = serveManagement({ permissions: [...MANAGER, "customer.follow_up"] });
    const user = userEvent.setup();
    at("/khach-lau-khong-quay-lai");
    const table = await screen.findByRole("table", { name: "Khách lâu chưa quay lại" });
    const [first] = within(table).getAllByRole("button", { name: /^Đã liên hệ với / });
    // EM round 5 item 8: marking the contact is the only row action (no call button).
    expect(within(table).queryByRole("link", { name: /^Gọi / })).not.toBeInTheDocument();
    const name = (first?.getAttribute("aria-label") ?? "").replace("Đã liên hệ với ", "");
    await user.click(first as HTMLElement);
    const form = await screen.findByRole("form", { name: /^Ghi lần liên hệ với / });
    expect(within(form).getByLabelText("Ngày liên hệ")).toHaveValue(isoToDmy(spaToday()));
    // "Không nghe máy" is the default outcome: try again in 3 days (the server's rule too).
    expect(within(form).getByLabelText("Hẹn liên hệ lại")).toHaveValue(isoToDmy(addDays(spaToday(), 3)));
    // Each outcome pre-fills its rule before saving (lead decision): other → empty, not interested → hint.
    await user.click(within(form).getByLabelText("Khác"));
    expect(within(form).getByLabelText("Hẹn liên hệ lại")).toHaveValue("");
    await user.click(within(form).getByLabelText("Chưa có nhu cầu"));
    expect(within(form).getByLabelText("Hẹn liên hệ lại")).toHaveValue("");
    expect(within(form).getByText("Không nhắc lại trong 90 ngày.")).toBeInTheDocument();
    await user.click(within(form).getByLabelText("Hẹn sẽ quay lại"));
    expect(within(form).getByLabelText("Hẹn liên hệ lại")).toHaveValue(isoToDmy(addDays(spaToday(), 7)));
    await user.selectOptions(within(form).getByLabelText("Cách liên hệ"), "zalo");
    await user.type(within(form).getByLabelText(/Ghi chú/), "Hẹn thứ bảy");
    await user.click(within(form).getByRole("button", { name: "Lưu lần liên hệ" }));

    await waitFor(() => {
      expect(screen.queryByRole("form", { name: /^Ghi lần liên hệ/ })).not.toBeInTheDocument();
    });
    const post = spy.mock.calls.find(([, init]) => init?.method === "POST");
    expect(post?.[0]).toMatch(/\/api\/v1\/customers\/mock-customer-\d+\/follow-ups$/);
    expect(new Headers(post?.[1]?.headers).get("Idempotency-Key")).toMatch(/^[0-9a-f-]{36}$/);
    expect(JSON.parse(post?.[1]?.body as string)).toEqual({
      contacted_on: spaToday(),
      channel: "zalo",
      outcome: "will_come",
      note: "Hẹn thứ bảy",
      next_follow_up_on: addDays(spaToday(), 7),
    });
    // The list was invalidated: planned in 7 days, the customer left "Cần liên hệ".
    await waitFor(() => {
      expect(within(screen.getByRole("table", { name: "Khách lâu chưa quay lại" })).queryByRole("button", { name: `Đã liên hệ với ${name}` })).not.toBeInTheDocument();
    });
    expect(screen.getByRole("status", { name: "" })).toHaveTextContent(/Đã ghi liên hệ với .*, hẹn lại/);
  });

  it("hides “Đã liên hệ” without customer.follow_up (the API decides)", async () => {
    serveManagement({ permissions: MANAGER });
    at("/khach-lau-khong-quay-lai");
    const table = await screen.findByRole("table", { name: "Khách lâu chưa quay lại" });
    expect(within(table).queryByRole("button", { name: /^Đã liên hệ/ })).not.toBeInTheDocument();
    // EM round 5 item 8: no call button; with nothing left to do, no actions column either.
    expect(within(table).queryByRole("link", { name: /^Gọi / })).not.toBeInTheDocument();
    expect(within(table).queryByRole("columnheader", { name: "Thao tác" })).not.toBeInTheDocument();
  });
});

describe("customer history", () => {
  it("shows the summary and the usage timeline with service categories", async () => {
    serveManagement({ permissions: MANAGER });
    at("/khach-hang/c1");
    const section = await screen.findByRole("region", { name: "Lịch sử sử dụng" });
    expect(await within(section).findByText("Buổi còn lại")).toBeInTheDocument();
    const timeline = await within(section).findByRole("list", { name: "Lịch sử sử dụng" });
    expect(within(timeline).getAllByText("Lượt khách").length).toBeGreaterThan(0);
    expect(within(timeline).getByText("Mua gói liệu trình")).toBeInTheDocument();
    expect(within(timeline).getByText(/Lý do hủy/)).toBeInTheDocument();
    expect(within(timeline).getAllByText(/^Gội đầu dưỡng sinh · /).length).toBeGreaterThan(0);
  });

  it("searches the history by kind and text; the filters stay in the profile URL", async () => {
    const spy = serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    const { search } = at("/khach-hang/c1");
    const section = await screen.findByRole("region", { name: "Lịch sử sử dụng" });
    await within(section).findByRole("list", { name: "Lịch sử sử dụng" });
    await user.selectOptions(within(section).getByLabelText("Loại"), "purchase");
    await waitFor(() => {
      expect(urls(spy)).toContain("/api/v1/customers/c1/history?kind=purchase&limit=20");
    });
    await waitFor(() => {
      expect(within(within(section).getByRole("list", { name: "Lịch sử sử dụng" })).queryByText("Lượt khách")).not.toBeInTheDocument();
    });
    await user.selectOptions(within(section).getByLabelText("Loại"), "");
    await user.type(within(section).getByRole("searchbox", { name: /Tìm dịch vụ/ }), "zzz");
    await waitFor(() => {
      expect(search()).toEqual({ hq: "zzz" });
    });
    expect(await within(section).findByText("Không thấy mục nào")).toBeInTheDocument();
    await user.click(within(section).getByRole("button", { name: "Xóa bộ lọc" }));
    await waitFor(() => {
      expect(search()).toEqual({});
    });
  });

  it("shows the follow-up log and records a contact from the profile", async () => {
    const spy = serveManagement({ permissions: [...MANAGER, "customer.follow_up"] });
    const user = userEvent.setup();
    at("/khach-hang/c1");
    const card = await screen.findByRole("region", { name: "Chăm sóc lại" });
    expect(await within(card).findByText(/Chưa liên hệ lần nào./)).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Ghi lần liên hệ" }));
    await user.click(within(card).getByLabelText("Chưa có nhu cầu"));
    // No next try for "not interested": nothing suggested, nothing sent.
    expect(within(card).getByLabelText("Hẹn liên hệ lại")).toHaveValue("");
    await user.click(within(card).getByRole("button", { name: "Lưu lần liên hệ" }));
    const log = await within(card).findByRole("list", { name: "Các lần liên hệ" });
    expect(within(log).getByText("Chưa có nhu cầu")).toBeInTheDocument();
    const post = spy.mock.calls.find(([, init]) => init?.method === "POST");
    expect(JSON.parse(post?.[1]?.body as string)).toMatchObject({ outcome: "not_interested", next_follow_up_on: null });
  });

  it("is not shown without report.read", async () => {
    serveManagement({ permissions: ["customer.read"] });
    at("/khach-hang/c1");
    expect(await screen.findByRole("heading", { level: 1, name: /^Nguyễn Thị Hà/ })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Lịch sử sử dụng" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Chăm sóc lại" })).not.toBeInTheDocument();
  });
});
