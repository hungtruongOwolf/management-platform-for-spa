import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";

// The server leaves money out for callers without `report.revenue.read` (entitled data only).
// The screen must then show the counts and no money at all — never an empty or zero amount.
vi.mock("./mock", async (importOriginal) => {
  const real = await importOriginal<typeof import("./mock")>();
  return {
    ...real,
    mockDaily: (date: string) => {
      const report = real.mockDaily(date);
      delete report.revenue;
      return report;
    },
    mockRange: (from: string, to: string) => {
      const r = real.mockRange(from, to);
      for (const b of r.series) delete b.revenue;
      return r;
    },
  };
});

describe("/bao-cao without revenue in the reply", () => {
  it("shows counts only", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation((input) =>
      Promise.resolve(
        input === "/api/v1/me"
          ? Response.json({
              user: { id: "u1", full_name: "Quản lý Hà", phone: "0905 123 456" },
              active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
              tenants: [],
              permissions: ["sale.read"],
            })
          : Response.json([]),
      ),
    );
    render(<App history={createMemoryHistory({ initialEntries: ["/bao-cao"] })} />);
    expect(await screen.findByText("Gói bán ra", { selector: "p" })).toBeInTheDocument();
    expect(await screen.findByRole("list", { name: "Lượt khách theo ngày" })).toBeInTheDocument();
    expect(screen.queryByText("Thực thu", { selector: "p" })).not.toBeInTheDocument();
    expect(screen.queryByText(/\d\.\d{3}đ/)).not.toBeInTheDocument();
  });
});
