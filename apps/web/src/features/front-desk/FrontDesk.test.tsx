import type { CustomerPackage, Service, Staff, Visit } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import {
  amountDue,
  byUseOrder,
  priceFor,
  savedSummary,
  staffByUse,
  visitBody,
} from "./api";

const MANAGER = [
  "customer.read",
  "customer.write",
  "customer.health_notes.read",
  "customer.health_notes.write",
  "visit.record",
  "visit.read",
  "payment.collect",
  "package.use_expired_override",
  "catalog.read",
  "staff.read_basic",
];
const me = (permissions: string[]) => ({
  user: { id: "u1", full_name: "Quản lý Ngọc", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});

const DAI_THONG_LAC: Service = {
  id: "s5",
  category_id: "cat1",
  name: "Đại Thông Lạc",
  description: null,
  duration_min: 90,
  recommended: true,
  active: true,
  version: 1,
  prices: { female: 349_000, male: 399_000 },
};
const ON_AM: Service = {
  ...DAI_THONG_LAC,
  id: "s6",
  name: "Ôn ấm tử cung",
  prices: { female: 399_000 },
};
const GAC_HOI: Service = {
  ...DAI_THONG_LAC,
  id: "s10",
  name: "Giác hơi",
  recommended: false,
  prices: { all: 100_000 },
};

function pkg(
  id: string,
  over: Partial<CustomerPackage> = {},
  remaining = 5,
): CustomerPackage {
  return {
    id,
    customer_id: "c1",
    plan_id: "p1",
    name: `Gói ${id}`,
    gift_note: null,
    sold_price: 1_500_000,
    sale_id: "sale1",
    closer_staff_id: "st1",
    closer_name: "KTV Lan",
    starts_on: "2026-11-01",
    expires_on: "2027-05-01",
    status: "active",
    sessions_total: 12,
    sessions_remaining: remaining,
    version: 1,
    created_at: "2026-11-01T00:00:00Z",
    items: [
      {
        id: `${id}-i1`,
        service_id: "s1",
        name: "Gội đầu",
        category_name: "Gội đầu dưỡng sinh",
        sessions_purchased: 10,
        sessions_bonus: 2,
        sessions_total: 12,
        sessions_remaining: remaining,
        per_session_value: 150_000,
      },
    ],
    ...over,
  };
}

const staff = (id: string, name: string): Staff => ({
  id,
  full_name: name,
  position: "KTV",
  status: "active",
});

const VISIT: Visit = {
  id: "v1",
  customer_id: "c1",
  customer_name: "Nguyễn Thị Hà",
  business_date: "2026-11-05",
  check_in_at: "2026-11-05T02:12:00Z",
  status: "completed",
  sale_id: null,
  total: 0,
  payments: [],
  version: 1,
  created_at: "2026-11-05T02:12:00Z",
  lines: [
    {
      id: "l1",
      service_id: "s1",
      service_name: "Gội đầu",
      category_name: "Gội đầu dưỡng sinh",
      source: "package",
      customer_package_id: "cp1",
      customer_package_item_id: "cp1-i1",
      package_name: "Gói cp1",
      sessions_left_after: 4,
      sessions_total: 12,
      staff_id: "st2",
      staff_name: "KTV Hoa",
      qty: 1,
      price_variant: null,
      unit_price: 150_000,
      line_value: 150_000,
      expiry_override_reason: null,
    },
  ],
};

describe("front desk rules", () => {
  it("prices one-off services by gender like the server (AC4.2b/c)", () => {
    expect(priceFor(DAI_THONG_LAC, "male")).toEqual({
      ok: true,
      price: 399_000,
      variant: "male",
    });
    expect(priceFor(DAI_THONG_LAC, "female")).toMatchObject({ price: 349_000 });
    expect(priceFor(DAI_THONG_LAC, null)).toEqual({
      ok: false,
      reason: "need_gender",
    });
    expect(priceFor(ON_AM, "male")).toEqual({
      ok: false,
      reason: "not_offered",
    });
    expect(priceFor(GAC_HOI, null)).toMatchObject({ ok: true, price: 100_000 });
  });

  it("orders packages: usable first, soonest expiry, then oldest (AC4.3)", () => {
    const late = pkg("late", { expires_on: "2027-12-01" });
    const soon = pkg("soon", { expires_on: "2027-01-01" });
    const never = pkg("never", { expires_on: null });
    const used = pkg(
      "used",
      { expires_on: "2026-12-01", status: "exhausted" },
      0,
    );
    expect(byUseOrder([never, late, used, soon]).map((p) => p.id)).toEqual([
      "soon",
      "late",
      "never",
      "used",
    ]);
  });

  it("puts today's busiest technician first and sums only one-off lines", () => {
    const sorted = staffByUse(
      [staff("st1", "KTV Lan"), staff("st2", "KTV Hoa")],
      [VISIT],
    );
    expect(sorted.map((s) => s.id)).toEqual(["st2", "st1"]);
    expect(
      amountDue([
        {
          key: 1,
          source: "package",
          serviceId: "s1",
          serviceName: "Gội",
          itemId: "i1",
          packageName: "Gói",
          staffId: "st1",
        },
        {
          key: 2,
          source: "one_off",
          serviceId: "s5",
          serviceName: "Đại Thông Lạc",
          priceVariant: null,
          price: 399_000,
          staffId: "st1",
        },
      ]),
    ).toBe(399_000);
  });

  it("never sends a price to the server and summarises the saved visit", () => {
    const body = visitBody(
      "c1",
      [
        {
          key: 2,
          source: "one_off",
          serviceId: "s5",
          serviceName: "Đại Thông Lạc",
          priceVariant: "male",
          price: 399_000,
          staffId: "st1",
        },
      ],
      [{ method: "cash", amount: 399_000 }],
      "  ",
    );
    expect(body.lines[0]).toEqual({
      service_id: "s5",
      source: "one_off",
      staff_id: "st1",
      price_variant: "male",
    });
    expect(body.note).toBeNull();
    expect(savedSummary(VISIT)).toBe(
      "Đã lưu lượt của Nguyễn Thị Hà · Gội đầu còn 4/12",
    );
  });
});

// ------------------------------------------------------------------ screen

const HA = {
  id: "c1",
  full_name: "Nguyễn Thị Hà",
  phone: "0905 111 222",
  gender: "female",
};

interface Call {
  method: string;
  url: string;
  body: unknown;
  key: string | undefined;
}

function serve(
  permissions: string[],
  onVisit: () => Response = () => Response.json(VISIT, { status: 201 }),
  packages: CustomerPackage[] = [pkg("cp1")],
  dayVisits: Visit[] = [],
) {
  const calls: Call[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    const headers = (init?.headers ?? {}) as Record<string, string>;
    const call: Call = {
      method: init?.method ?? "GET",
      url,
      body:
        typeof init?.body === "string"
          ? (JSON.parse(init.body) as unknown)
          : undefined,
      key: headers["Idempotency-Key"],
    };
    calls.push(call);
    const json = (v: unknown) => Promise.resolve(Response.json(v));
    if (url === "/api/v1/me") return json(me(permissions));
    if (url.startsWith("/api/v1/customers?q=")) return json([HA]);
    if (url === "/api/v1/customers/c1")
      return json({
        ...HA,
        phone_e164: "+84905111222",
        birth_year: null,
        address: null,
        source: null,
        note: null,
        health_note: "Huyết áp cao — lực nhẹ vùng cổ",
        version: 1,
        created_at: "2026-11-01T00:00:00Z",
      });
    if (url === "/api/v1/customers/c1/packages") return json(packages);
    if (url === "/api/v1/staff")
      return json([staff("st1", "KTV Lan"), staff("st2", "KTV Hoa")]);
    if (url === "/api/v1/services") return json([DAI_THONG_LAC, GAC_HOI]);
    if (url === "/api/v1/service-categories")
      return json([{ id: "cat1", name: "Cổ vai gáy", sort_order: 0 }]);
    if (url.startsWith("/api/v1/visits") && call.method === "GET") {
      const limit = new URL(url, "http://x").searchParams.get("limit");
      return json(limit ? dayVisits.slice(0, Number(limit)) : dayVisits);
    }
    if (url === "/api/v1/visits" && call.method === "POST")
      return Promise.resolve(onVisit());
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

const at = (path: string) =>
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

describe("/quay", () => {
  it("records a package session in 4 actions: Enter, Trừ 1 buổi, KTV, Ctrl+Enter", async () => {
    const calls = serve(MANAGER);
    const user = userEvent.setup();
    at("/quay");
    const search = await screen.findByRole("combobox", {
      name: "Tìm khách theo SĐT hoặc tên",
    });
    await waitFor(() => {
      expect(search).toHaveFocus();
    });
    await user.type(search, "0905111");
    await screen.findByRole("option", { name: /Nguyễn Thị Hà/ });
    await user.keyboard("{Enter}"); // 1
    expect(
      await screen.findByText("Huyết áp cao — lực nhẹ vùng cổ"),
    ).toBeInTheDocument();
    const deduct = await screen.findByRole("button", {
      name: "Trừ 1 buổi Gói cp1",
    });
    await waitFor(() => {
      expect(deduct).toHaveFocus();
    });
    expect(screen.getByText("5/12")).toBeInTheDocument();
    await user.keyboard("{Enter}"); // 2
    const tech = await screen.findByRole("combobox", { name: "KTV" });
    await waitFor(() => {
      expect(tech).toHaveFocus();
    });
    expect(screen.getByText("4/12")).toBeInTheDocument();
    await user.selectOptions(tech, "st1"); // 3
    await user.keyboard("{Control>}{Enter}{/Control}"); // 4

    const posts = calls.filter(
      (c) => c.method === "POST" && c.url === "/api/v1/visits",
    );
    await waitFor(() => {
      expect(posts.length).toBe(1);
    });
    expect(posts[0]?.key).toMatch(/^[0-9a-f-]{36}$/);
    expect(posts[0]?.body).toEqual({
      customer_id: "c1",
      note: null,
      lines: [
        {
          service_id: "s1",
          source: "package",
          customer_package_item_id: "cp1-i1",
          staff_id: "st1",
          expiry_override_reason: null,
        },
      ],
      payments: [],
    });
    expect(
      await screen.findByText(
        "Đã lưu lượt của Nguyễn Thị Hà · Gội đầu còn 4/12",
      ),
    ).toBeInTheDocument();
    const today = await screen.findByRole("list", {
      name: "Lượt khách hôm nay",
    });
    expect(within(today).getByText(/KTV Hoa/)).toBeInTheDocument();
    await waitFor(() => {
      expect(
        screen.getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" }),
      ).toHaveFocus();
    });
  });

  it("renders the latest 30 visits of a busy day, the rest on request", async () => {
    const many = Array.from({ length: 65 }, (_, i) => ({
      ...VISIT,
      id: `v${String(i)}`,
      customer_name: `Khách ${String(i)}`,
    }));
    const calls = serve(MANAGER, undefined, undefined, many);
    const user = userEvent.setup();
    at("/quay");
    const list = await screen.findByRole("list", {
      name: "Lượt khách hôm nay",
    });
    expect(within(list).getAllByRole("listitem")).toHaveLength(30);
    // Only the newest 31 are fetched first: one more than shown says the day has more.
    expect(
      calls.some(
        (c) => c.method === "GET" && c.url === "/api/v1/visits?limit=31",
      ),
    ).toBe(true);
    expect(
      screen.getByRole("heading", { name: /Lượt khách hôm nay \(30\+\)/ }),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Xem tất cả lượt trong ngày" }),
    );
    await waitFor(() => {
      expect(within(list).getAllByRole("listitem")).toHaveLength(60);
    });
    expect(
      screen.getByRole("heading", { name: /Lượt khách hôm nay \(65\)/ }),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Xem thêm (5 lượt nữa)" }),
    );
    expect(within(list).getAllByRole("listitem")).toHaveLength(65);
    expect(
      screen.queryByRole("button", { name: /Xem thêm/ }),
    ).not.toBeInTheDocument();
  });

  it("will not save a line without a technician", async () => {
    const calls = serve(MANAGER);
    const user = userEvent.setup();
    at("/quay");
    await user.type(
      await screen.findByRole("combobox", {
        name: "Tìm khách theo SĐT hoặc tên",
      }),
      "0905111",
    );
    await user.click(
      await screen.findByRole("option", { name: /Nguyễn Thị Hà/ }),
    );
    await user.click(
      await screen.findByRole("button", { name: "Trừ 1 buổi Gói cp1" }),
    );
    await user.click(screen.getByRole("button", { name: "Lưu lượt khách" }));
    expect(await screen.findByText("Cần chọn KTV")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "KTV" })).toHaveFocus();
    expect(calls.some((c) => c.method === "POST")).toBe(false);
  });

  it("collects a one-off service in full and shows the server's conflict", async () => {
    const calls = serve(MANAGER, () =>
      Response.json(
        { code: "package.no_sessions_left", status: 409 },
        { status: 409 },
      ),
    );
    const user = userEvent.setup();
    at("/quay");
    await user.type(
      await screen.findByRole("combobox", {
        name: "Tìm khách theo SĐT hoặc tên",
      }),
      "ha",
    );
    await user.click(
      await screen.findByRole("option", { name: /Nguyễn Thị Hà/ }),
    );
    await screen.findByRole("option", { name: /Đại Thông Lạc/ });
    await user.selectOptions(screen.getByLabelText("Dịch vụ lẻ"), "s5");
    await user.click(screen.getByRole("button", { name: "Thêm" }));
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "KTV" }),
      "st2",
    );
    expect(screen.getAllByText("349.000đ").length).toBeGreaterThan(0);
    await user.click(screen.getByLabelText("Chuyển khoản"));
    await user.click(screen.getByRole("button", { name: "Lưu lượt khách" }));
    const post = await waitFor(() => {
      const p = calls.find((c) => c.method === "POST");
      if (!p) throw new Error("no POST yet");
      return p;
    });
    expect(post.body).toMatchObject({
      lines: [
        {
          service_id: "s5",
          source: "one_off",
          staff_id: "st2",
          price_variant: null,
        },
      ],
      payments: [{ method: "bank_transfer", amount: 349_000 }],
    });
    expect(
      await screen.findByText(/Liệu trình vừa hết buổi/),
    ).toBeInTheDocument();
    // The cart is kept for a retry.
    expect(
      screen.getByRole("list", { name: "Dịch vụ trong lượt" }),
    ).toBeInTheDocument();
  });

  it("shows a package migrated from the paper books like any other (EM 2026-11-05)", async () => {
    serve(MANAGER, undefined, [pkg("cp1"), pkg("cp2", { sale_id: null })]);
    const user = userEvent.setup();
    at("/quay");
    const search = await screen.findByRole("combobox", {
      name: "Tìm khách theo SĐT hoặc tên",
    });
    await user.type(search, "0905111");
    await screen.findByRole("option", { name: /Nguyễn Thị Hà/ });
    await user.keyboard("{Enter}");
    const list = await screen.findByRole("list", {
      name: "Liệu trình của khách",
    });
    const rows = within(list).getAllByRole("listitem");
    const imported = rows.find((li) => li.textContent.includes("Gói cp2"));
    const sold = rows.find((li) => li.textContent.includes("Gói cp1"));
    // No "sổ cũ" label: the migration is a one-time data fill, not a kind of package.
    expect(imported).toBeDefined();
    expect(sold).toBeDefined();
    expect(list).not.toHaveTextContent(/sổ cũ/i);
    expect(
      within(imported as HTMLElement).getByRole("button", {
        name: /Trừ 1 buổi/,
      }),
    ).toBeInTheDocument();
  });

  it("hides the Quầy entry without visit.record", async () => {
    serve(["catalog.read"]);
    at("/");
    await screen.findByRole("navigation", { name: "Chính" });
    expect(screen.queryByRole("link", { name: "Ghi lượt khách" })).toBeNull();
  });
});
