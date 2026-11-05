import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "../../app/App";
import { serveManagement } from "./testServer";

// The server leaves money out for callers without `report.revenue.read` (entitled data only).
// The screen must then show the counts and no money at all, never an empty or zero amount.
describe("/bao-cao without revenue in the reply", () => {
  it("shows counts only", async () => {
    serveManagement({ permissions: ["report.read"], withoutRevenue: true });
    render(<App history={createMemoryHistory({ initialEntries: ["/bao-cao"] })} />);
    expect(await screen.findByText("Gói bán ra", { selector: "p" })).toBeInTheDocument();
    expect(await screen.findByRole("list", { name: "Lượt khách theo ngày" })).toBeInTheDocument();
    expect(screen.queryByText("Thực thu", { selector: "p" })).not.toBeInTheDocument();
    expect(screen.queryByText(/\d\.\d{3}đ/)).not.toBeInTheDocument();
  });
});
