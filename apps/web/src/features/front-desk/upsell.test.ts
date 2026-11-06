import type { PackagePlan, Visit } from "@vpyd/api-client";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  addToVisit,
  canCountToday,
  conversionMoney,
  convertToPackage,
  coveredLines,
  defaultCloser,
} from "./upsell";

type Line = Visit["lines"][number];

function line(over: Partial<Line>): Line {
  return {
    id: "l1",
    service_id: "shampoo",
    service_name: "Gội đầu",
    category_name: "Gội đầu dưỡng sinh",
    source: "one_off",
    staff_id: "lan",
    staff_name: "KTV Lan",
    qty: 1,
    unit_price: 49_000,
    discount_amount: 0,
    line_value: 49_000,
    ...over,
  };
}

const VISIT: Visit = {
  id: "v1",
  customer_id: "c1",
  customer_name: "Nguyễn Thị Hà",
  business_date: "2026-11-06",
  check_in_at: "2026-11-06T02:00:00Z",
  status: "completed",
  sale_id: "s1",
  subtotal: 88_000,
  discount_amount: 0,
  total: 88_000,
  payments: [],
  package_sales: [],
  companions: [],
  lines: [
    line({}),
    line({
      id: "l2",
      service_id: "foot",
      service_name: "Ngâm chân",
      unit_price: 39_000,
      line_value: 39_000,
    }),
  ],
  version: 1,
  created_at: "2026-11-06T02:00:00Z",
};

const PLAN = {
  id: "p1",
  name: "Gội đầu 10 + 2",
  price: 1_500_000,
  items: [{ service_id: "shampoo" }],
} as unknown as PackagePlan;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("Tính buổi hôm nay vào gói (Q-U1)", () => {
  it("is offered only when the plan covers a one-off service done today", () => {
    expect(coveredLines(VISIT, PLAN).map((l) => l.id)).toEqual(["l1"]);
    expect(canCountToday(VISIT, PLAN)).toBe(true);
    const unrelated = {
      ...PLAN,
      items: [{ service_id: "body" }],
    } as unknown as PackagePlan;
    expect(canCountToday(VISIT, unrelated)).toBe(false);
    expect(canCountToday({ ...VISIT, status: "voided" }, PLAN)).toBe(false);
  });

  it("asks for the package plus the kept lines, and shows what the customer hands over", () => {
    expect(conversionMoney(VISIT, PLAN)).toEqual({
      due: 1_500_000 + 39_000,
      handOver: 1_500_000 + 39_000 - 88_000,
    });
    expect(conversionMoney(VISIT, PLAN, 150_000).due).toBe(1_350_000 + 39_000);
  });
});

describe("người giới thiệu / chốt (Q-U2)", () => {
  it("defaults to the technician of the matching line, else the first, else khách tự chọn", () => {
    const lines = [
      { serviceId: "foot", staffId: "mai" },
      { serviceId: "shampoo", staffId: "lan" },
    ];
    expect(defaultCloser(lines, PLAN)).toBe("lan");
    expect(defaultCloser(lines, null)).toBe("mai");
    expect(defaultCloser([], PLAN)).toBeNull();
  });
});

describe("API calls", () => {
  it("sends the idempotency key, and If-Match for a conversion", async () => {
    const fetchMock = vi.fn(() =>
      Promise.resolve(new Response(JSON.stringify(VISIT), { status: 201 })),
    );
    vi.stubGlobal("fetch", fetchMock);
    await addToVisit("v1", { lines: [], package_sales: [] }, "k1");
    await convertToPackage(
      VISIT,
      { package_sale: { package_plan_id: "p1" } },
      "k2",
    );
    const calls = fetchMock.mock.calls as unknown as [string, RequestInit][];
    expect(calls[0]?.[0]).toBe("/api/v1/visits/v1/additions");
    expect(new Headers(calls[0]?.[1].headers).get("Idempotency-Key")).toBe(
      "k1",
    );
    expect(calls[1]?.[0]).toBe("/api/v1/visits/v1/convert-to-package");
    const h = new Headers(calls[1]?.[1].headers);
    expect(h.get("If-Match")).toBe('"1"');
    expect(h.get("Idempotency-Key")).toBe("k2");
  });
});
