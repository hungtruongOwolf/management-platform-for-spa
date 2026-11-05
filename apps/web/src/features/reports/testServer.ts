import { vi } from "vitest";
import { spaToday } from "../corrections/api";
import type { PackageFilter } from "./api";
import { mockCustomerStats, mockDaily, mockHistory, mockInactive, mockPackages, mockRange } from "./fixtures";

/** Test-only fake of the API for the management views, contract-shaped (fixtures.ts). */
export interface FakeOptions {
  permissions: string[];
  /** Drop every money field, as the server does without `report.revenue.read`. */
  withoutRevenue?: boolean;
  /** Reply of `GET /customers?q=` (customer search). */
  searchHits?: unknown[];
}

export const ME = (permissions: string[]) => ({
  user: { id: "u1", full_name: "Quản lý Hà", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});

function route(url: URL, { permissions, withoutRevenue = false, searchHits = [] }: FakeOptions): unknown {
  const q = url.searchParams;
  const p = url.pathname.replace(/^\/api\/v1/, "");
  if (p === "/me") return ME(permissions);
  if (p === "/customers" && q.has("q")) return searchHits;
  if (p === "/reports/daily") {
    const d = mockDaily(q.get("date") ?? spaToday());
    if (withoutRevenue) {
      delete d.revenue;
      delete d.compare.yesterday.net_revenue;
      delete d.compare.same_weekday_last_week.net_revenue;
    }
    return d;
  }
  if (p === "/reports/range") {
    const r = mockRange(q.get("from") ?? spaToday(), q.get("to") ?? spaToday());
    if (withoutRevenue) for (const b of r.series) delete b.revenue;
    return r;
  }
  if (p === "/customers/c1")
    return { id: "c1", full_name: "Nguyễn Thị Hà", phone: "0905 111 222", gender: "female", version: 1 };
  if (p === "/customers/c1/history") return mockHistory("c1", spaToday());
  if (p === "/customers/c1/summary") return mockCustomerStats("c1", mockHistory("c1", spaToday()));
  if (p === "/packages") {
    const filter: PackageFilter = q.get("expiring_within_days")
      ? "expiring"
      : ((q.get("status") as PackageFilter | null) ?? "all");
    return mockPackages(spaToday(), filter);
  }
  if (p === "/customers/inactive") {
    const page = mockInactive(spaToday(), Number(q.get("days") ?? "7"));
    // Two pages: the first half, then the rest behind a cursor.
    const half = Math.ceil(page.items.length / 2);
    return q.get("cursor")
      ? { ...page, items: page.items.slice(half), next_cursor: null }
      : { ...page, items: page.items.slice(0, half), next_cursor: "page-2" };
  }
  return [];
}

/** Replaces `fetch`; returns the spy so a test can inspect the requests. */
export function serveManagement(options: FakeOptions) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return Promise.resolve(Response.json(route(new URL(raw, "http://app.test"), options)));
  });
}
