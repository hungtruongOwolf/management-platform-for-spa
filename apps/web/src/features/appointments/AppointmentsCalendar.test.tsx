import type { Calendar } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";

const OWNER = [
  "visit.record",
  "visit.read",
  "appointment.manage",
  "record.void_closed_day",
  "customer.read",
  "catalog.read",
  "staff.read_basic",
];

const CAL: Calendar = {
  from: "2026-11-06",
  to: "2026-11-06",
  today: "2026-11-06",
  opens_at: "09:00",
  closes_at: "21:00",
  slot_minutes: 15,
  time_zone: "Asia/Ho_Chi_Minh",
  technicians: [{ id: "st1", full_name: "KTV Lan" }],
  max_per_slot: null,
  appointments: [],
  visits: [],
};

function serve(permissions: string[]) {
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    const json = (v: unknown) => Promise.resolve(Response.json(v));
    if (url === "/api/v1/me")
      return json({
        user: { id: "u1", full_name: "Chủ spa Hưng", phone: "0905" },
        active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
        tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
        permissions,
      });
    if (url.startsWith("/api/v1/appointments?")) {
      const q = new URL(url, "http://x").searchParams;
      return json({ ...CAL, from: q.get("from"), to: q.get("to") });
    }
    if (url === "/api/v1/staff") return json([]);
    if (url === "/api/v1/services") return json([]);
    if (url === "/api/v1/service-categories") return json([]);
    return Promise.resolve(new Response(null, { status: 404 }));
  });
}

const at = (path: string) =>
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Lịch hẹn: booking only where the server takes it", () => {
  it("offers no booking on a past day, the owner included (check_day refuses everyone)", async () => {
    // 2026-11-06 10:00 in the spa.
    vi.setSystemTime(new Date("2026-11-06T03:00:00Z"));
    serve(OWNER);
    at("/quay?view=lich&ngay=2026-11-05");
    expect(
      await screen.findByText(/Chưa có lịch hẹn ngày này\./),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Đặt lịch” để thêm/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Đặt lịch" }),
    ).not.toBeInTheDocument();
  });

  it("still offers it today", async () => {
    vi.setSystemTime(new Date("2026-11-06T03:00:00Z"));
    serve(OWNER);
    at("/quay?view=lich");
    expect(
      await screen.findByRole("button", { name: "Đặt lịch" }),
    ).toBeInTheDocument();
  });

  it("starts a late booking on the last slot of the day, not after closing", async () => {
    // 20:55 in the spa: the next slot would be 21:00, the closing time.
    vi.setSystemTime(new Date("2026-11-06T13:55:00Z"));
    serve(OWNER);
    const user = userEvent.setup();
    at("/quay?view=lich");
    await user.click(await screen.findByRole("button", { name: "Đặt lịch" }));
    expect(await screen.findByLabelText("Giờ hẹn")).toHaveValue(
      String(20 * 60 + 45),
    );
    // What is sent follows the state, not the select's fallback pick: the sheet's own summary.
    expect(screen.getByText(/20:45–21:45/)).toBeInTheDocument();
  });
});
