import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { serveManagement } from "./testServer";

// The server leaves money out for callers without `report.revenue.read` (entitled data only).
// The screen must then show the counts and no money at all, never an empty or zero amount.
describe("/bao-cao without revenue in the reply", () => {
  it("shows counts only: no money KPI, chart metric, breakdown columns, mix card or ledger tab", async () => {
    serveManagement({ permissions: ["report.read"], withoutRevenue: true });
    render(<App history={createMemoryHistory({ initialEntries: ["/bao-cao"] })} />);
    expect(await screen.findByText("Gói bán ra", { selector: "p" })).toBeInTheDocument();
    // The URL default metric is money; without it the chart falls back to visits.
    expect(await screen.findByRole("img", { name: /^Lượt khách theo ngày/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Thực thu" })).not.toBeInTheDocument();
    const table = await screen.findByRole("table", { name: /Chi tiết theo ngày/ });
    expect(within(table).queryByRole("columnheader", { name: "Thực thu" })).not.toBeInTheDocument();
    expect(screen.queryByText("Thực thu", { selector: "p" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Sổ doanh thu" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /Cơ cấu thu/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/\d\.\d{3}đ/)).not.toBeInTheDocument();
  });

  it("ignores a ledger tab in the URL", async () => {
    const spy = serveManagement({ permissions: ["report.read"], withoutRevenue: true });
    render(<App history={createMemoryHistory({ initialEntries: ["/bao-cao?tab=ledger"] })} />);
    expect(await screen.findByRole("heading", { level: 1, name: "Báo cáo kinh doanh" })).toBeInTheDocument();
    expect(spy.mock.calls.some(([u]) => String(u instanceof Request ? u.url : u).includes("/reports/ledger"))).toBe(false);
  });
});
