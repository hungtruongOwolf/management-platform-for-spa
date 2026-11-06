// Round 6 at /quay (spec R1-1 §10, plan r1-6-upsell B1–B5): sell a package in the cart and use
// today's session from it, a package-only sale, the referrer of a one-off line, the draft that
// survives a reload, and the ⋯ actions on a saved visit (additions, convert-to-package).
import type {
  CustomerPackage,
  PackagePlan,
  Service,
  Visit,
} from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { spaToday } from "../corrections/api";

const PERMS = [
  "customer.read",
  "customer.write",
  "visit.record",
  "visit.read",
  "payment.collect",
  "catalog.read",
  "staff.read_basic",
  "package.sell",
  "sale.discount",
  "record.void_same_day",
];

const HA = {
  id: "c1",
  full_name: "Nguyễn Thị Hà",
  phone: "0905 111 222",
  gender: "female",
};
const DTL: Service = {
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
const GAC_HOI: Service = {
  ...DTL,
  id: "s10",
  name: "Giác hơi",
  recommended: false,
  prices: { all: 100_000 },
};
const PLAN: PackagePlan = {
  id: "pl1",
  name: "Gói Đại Thông Lạc 10 + 2",
  price: 3_500_000,
  active: true,
  gift_note: "Dầu thảo dược",
  validity_months: 12,
  version: 1,
  items: [{ service_id: "s5", sessions: 10, bonus_sessions: 2 }],
};
const EXHAUSTED: CustomerPackage = {
  id: "cp9",
  customer_id: "c1",
  plan_id: "pl1",
  name: PLAN.name,
  gift_note: null,
  sold_price: 3_500_000,
  sale_id: "sale9",
  closer_staff_id: null,
  closer_name: null,
  starts_on: "2026-01-01",
  expires_on: "2027-01-01",
  status: "exhausted",
  sessions_total: 12,
  sessions_remaining: 0,
  version: 1,
  created_at: "2026-01-01T00:00:00Z",
  items: [
    {
      id: "cp9-i1",
      service_id: "s5",
      name: "Đại Thông Lạc",
      category_name: "Cổ vai gáy",
      sessions_purchased: 10,
      sessions_bonus: 2,
      sessions_total: 12,
      sessions_remaining: 0,
      per_session_value: 290_000,
    },
  ],
};

/** A saved one-off visit of today: Đại Thông Lạc 349.000đ in cash, KTV Lan. */
const ONE_OFF_VISIT: Visit = {
  id: "v1",
  customer_id: "c1",
  customer_name: "Nguyễn Thị Hà",
  customer_gender: "female",
  business_date: spaToday(),
  check_in_at: "2026-11-05T02:12:00Z",
  status: "completed",
  sale_id: "sale1",
  subtotal: 349_000,
  discount_amount: 0,
  discount_reason: null,
  total: 349_000,
  payments: [
    {
      id: "pay1",
      kind: "payment",
      method: "cash",
      amount: 349_000,
      paid_at: "2026-11-05T02:12:00Z",
      reference: null,
    },
  ],
  version: 3,
  created_at: "2026-11-05T02:12:00Z",
  companions: [],
  package_sales: [],
  lines: [
    {
      id: "l1",
      service_id: "s5",
      service_name: "Đại Thông Lạc",
      category_name: "Cổ vai gáy",
      source: "one_off",
      customer_package_id: null,
      customer_package_item_id: null,
      package_name: null,
      sessions_left_after: null,
      sessions_total: null,
      staff_id: "st1",
      staff_name: "KTV Lan",
      qty: 1,
      price_variant: null,
      unit_price: 349_000,
      line_value: 349_000,
      discount_amount: 0,
      expiry_override_reason: null,
      referrer_staff_id: null,
      referrer_name: null,
    },
  ],
};

interface Call {
  method: string;
  url: string;
  body: unknown;
  headers: Record<string, string>;
}

function serve(dayVisits: Visit[] = [], packages: CustomerPackage[] = []) {
  const calls: Call[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    const call: Call = {
      method: init?.method ?? "GET",
      url,
      body:
        typeof init?.body === "string"
          ? (JSON.parse(init.body) as unknown)
          : undefined,
      headers: (init?.headers ?? {}) as Record<string, string>,
    };
    calls.push(call);
    const json = (v: unknown, status = 200) =>
      Promise.resolve(Response.json(v, { status }));
    if (url === "/api/v1/me")
      return json({
        user: { id: "u1", full_name: "Quản lý Ngọc", phone: "0905" },
        active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
        tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
        permissions: PERMS,
      });
    if (url.startsWith("/api/v1/customers?q=")) return json([HA]);
    if (url === "/api/v1/customers/c1")
      return json({
        ...HA,
        phone_e164: "+84905111222",
        birth_year: null,
        address: null,
        source: null,
        note: null,
        version: 1,
        created_at: "2026-01-01T00:00:00Z",
      });
    if (url === "/api/v1/customers/c1/packages") return json(packages);
    if (url === "/api/v1/staff")
      return json([
        {
          id: "st1",
          full_name: "KTV Lan",
          position: "Kỹ thuật viên",
          status: "active",
        },
        {
          id: "st2",
          full_name: "Chủ spa Hưng",
          position: "Chủ spa",
          status: "active",
        },
      ]);
    if (url === "/api/v1/services") return json([DTL, GAC_HOI]);
    if (url === "/api/v1/service-categories")
      return json([{ id: "cat1", name: "Cổ vai gáy", sort_order: 0 }]);
    if (url === "/api/v1/package-plans") return json([PLAN]);
    if (url.startsWith("/api/v1/visits") && call.method === "GET")
      return json(dayVisits);
    if (url === "/api/v1/visits" && call.method === "POST")
      return json(
        {
          ...ONE_OFF_VISIT,
          id: "v9",
          package_sales: [
            {
              sale_id: "s9",
              business_date: spaToday(),
              customer_package_ids: ["cpn"],
              description: PLAN.name,
              payments: [],
              total: 3_500_000,
              voided: false,
            },
          ],
        },
        201,
      );
    if (url === "/api/v1/sales" && call.method === "POST")
      return json(
        {
          id: "sale9",
          lines: [
            {
              id: "sl",
              description: PLAN.name,
              line_total: 3_500_000,
              line_type: "package",
              qty: 1,
              unit_price: 3_500_000,
            },
          ],
        },
        201,
      );
    if (url.endsWith("/additions")) return json(ONE_OFF_VISIT, 201);
    if (url.endsWith("/convert-to-package"))
      return json(
        {
          voided: { ...ONE_OFF_VISIT, status: "voided" },
          visit: { ...ONE_OFF_VISIT, id: "v2" },
          difference: 3_151_000,
        },
        201,
      );
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

const posted = (calls: Call[], url: string) =>
  calls.filter((c) => c.method === "POST" && c.url === url);

async function pickHa(user: ReturnType<typeof userEvent.setup>) {
  const search = await screen.findByRole("combobox", {
    name: "Tìm khách theo SĐT hoặc tên",
  });
  await user.type(search, "0905");
  await user.click(
    await screen.findByRole("option", { name: /Nguyễn Thị Hà/ }),
  );
}

/** Pick a one-off service once the price list has loaded (the select adds on pick). */
async function pickService(
  user: ReturnType<typeof userEvent.setup>,
  label: string,
  id: string,
) {
  const select = await screen.findByLabelText(label);
  await waitFor(() => {
    expect(select.querySelector(`option[value="${id}"]`)).not.toBeNull();
  });
  await user.selectOptions(select, id);
}
const addOneOff = (user: ReturnType<typeof userEvent.setup>, id: string) =>
  pickService(user, "Thêm dịch vụ lẻ", id);

describe("/quay round 6", () => {
  it("sells a package in the cart and takes today's session from it in one request (AC9.1)", async () => {
    const calls = serve();
    const user = userEvent.setup();
    render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    await pickHa(user);
    await addOneOff(user, "s5");
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "KTV" }),
      "st1",
    );
    await user.click(
      await screen.findByRole("button", { name: "Bán gói liệu trình" }),
    );
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Mẫu gói" }),
      "pl1",
    );
    // "Dùng gói này cho buổi hôm nay" turns on by itself: the one-off line becomes 0đ.
    expect(
      screen.getByRole("checkbox", { name: /Dùng gói này cho buổi hôm nay/ }),
    ).toBeChecked();
    expect(screen.getByText(/Trừ gói mua hôm nay/)).toBeInTheDocument();
    // The closer defaults to the technician of the covered line (Q-U2).
    expect(
      screen.getByRole("combobox", { name: "Người giới thiệu / chốt" }),
    ).toHaveValue("st1");
    const total = screen.getByRole("group", { name: "Lưu lượt" });
    expect(within(total).getByText("3.500.000đ")).toBeInTheDocument();
    await user.click(
      within(total).getByRole("button", { name: "Lưu lượt khách" }),
    );
    await waitFor(() => {
      expect(posted(calls, "/api/v1/visits")).toHaveLength(1);
    });
    const body = posted(calls, "/api/v1/visits")[0]?.body as Record<
      string,
      unknown
    >;
    expect(body["lines"]).toEqual([
      {
        service_id: "s5",
        source: "package",
        package_ref: "p1",
        staff_id: "st1",
      },
    ]);
    expect(body["package_sales"]).toEqual([
      expect.objectContaining({
        ref: "p1",
        package_plan_id: "pl1",
        closer_staff_id: "st1",
        discount: null,
      }),
    ]);
    expect(body["payments"]).toEqual([{ method: "cash", amount: 3_500_000 }]);
    expect(
      await screen.findByText(/bán Gói Đại Thông Lạc/),
    ).toBeInTheDocument();
  });

  it("sells only a package with POST /sales from “Mua lại gói này” (US10, AC10.2)", async () => {
    const calls = serve([], [EXHAUSTED]);
    const user = userEvent.setup();
    render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    await pickHa(user);
    await user.click(await screen.findByText(/Gói đã hết \/ đã hủy/));
    await user.click(
      screen.getByRole("button", { name: `Mua lại gói này: ${PLAN.name}` }),
    );
    expect(
      await screen.findByRole("combobox", { name: "Mẫu gói" }),
    ).toHaveValue("pl1");
    // No service: one sale, no visit; nobody is credited by default without a technician.
    await user.click(screen.getByRole("button", { name: "Lưu hóa đơn gói" }));
    await waitFor(() => {
      expect(posted(calls, "/api/v1/sales")).toHaveLength(1);
    });
    expect(posted(calls, "/api/v1/visits")).toHaveLength(0);
    expect(posted(calls, "/api/v1/sales")[0]?.body).toMatchObject({
      customer_id: "c1",
      closer_staff_id: null,
      lines: [{ package_plan_id: "pl1", gift_note: "Dầu thảo dược" }],
      payments: [{ method: "cash", amount: 3_500_000 }],
    });
  });

  it("records who recommended a one-off line; khách tự chọn by default (Q-U2)", async () => {
    const calls = serve();
    const user = userEvent.setup();
    render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    await pickHa(user);
    await addOneOff(user, "s10");
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "KTV" }),
      "st1",
    );
    const ref = await screen.findByRole("combobox", {
      name: "Người giới thiệu Giác hơi",
    });
    expect(ref).toHaveValue("");
    await user.selectOptions(ref, "st2");
    await user.click(screen.getByRole("button", { name: "Lưu lượt khách" }));
    await waitFor(() => {
      expect(posted(calls, "/api/v1/visits")).toHaveLength(1);
    });
    expect(
      (posted(calls, "/api/v1/visits")[0]?.body as { lines: unknown[] }).lines,
    ).toEqual([
      {
        service_id: "s10",
        source: "one_off",
        staff_id: "st1",
        price_variant: null,
        referrer_staff_id: "st2",
      },
    ]);
  });

  it("brings the unsaved cart back after a reload, and “Bỏ giỏ” drops it (Q-U8)", async () => {
    serve();
    const user = userEvent.setup();
    const first = render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    await pickHa(user);
    await addOneOff(user, "s10");
    await waitFor(() => {
      expect(sessionStorage.getItem("vpyd.cart.v1:u1:c1")).not.toBeNull();
    });
    first.unmount();
    render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    expect(
      await screen.findByText(/Đã khôi phục giỏ chưa lưu của Nguyễn Thị Hà/),
    ).toBeInTheDocument();
    expect(
      within(
        screen.getByRole("list", { name: "Dịch vụ trong lượt" }),
      ).getByText(/Giác hơi/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Bỏ giỏ" }));
    await waitFor(() => {
      expect(
        screen.queryByRole("list", { name: "Dịch vụ trong lượt" }),
      ).toBeNull();
    });
    await waitFor(() => {
      expect(sessionStorage.getItem("vpyd.cart.v1:u1:c1")).toBeNull();
    });
  });

  it("adds a service to a saved visit with its own sale; the technician is its referrer (US12)", async () => {
    const calls = serve([ONE_OFF_VISIT]);
    const user = userEvent.setup();
    render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    const table = await screen.findByRole("table", {
      name: "Lượt khách hôm nay",
    });
    await user.click(
      within(table).getByRole("button", {
        name: /^Thao tác lượt .* của Nguyễn Thị Hà$/,
      }),
    );
    await user.click(
      screen.getByRole("menuitem", { name: "Thêm dịch vụ / bán gói" }),
    );
    await pickService(user, "Thêm dịch vụ vào lượt", "s10");
    await user.click(screen.getByRole("button", { name: "Lưu phần thêm" }));
    await waitFor(() => {
      expect(posted(calls, "/api/v1/visits/v1/additions")).toHaveLength(1);
    });
    const call = posted(calls, "/api/v1/visits/v1/additions")[0];
    expect(call?.headers["Idempotency-Key"]).toBeTruthy();
    expect(call?.body).toEqual({
      lines: [
        {
          service_id: "s10",
          source: "one_off",
          staff_id: "st1",
          price_variant: null,
          referrer_staff_id: "st1",
        },
      ],
      payments: [{ method: "cash", amount: 100_000 }],
    });
  });

  it("“Chuyển sang gói” sells a new package by default; “Tính buổi hôm nay vào gói” converts (Q-U1)", async () => {
    const calls = serve([ONE_OFF_VISIT]);
    const user = userEvent.setup();
    render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    const table = await screen.findByRole("table", {
      name: "Lượt khách hôm nay",
    });
    await user.click(
      within(table).getByRole("button", {
        name: /^Thao tác lượt .* của Nguyễn Thị Hà$/,
      }),
    );
    await user.click(
      screen.getByRole("menuitem", { name: "Chuyển sang gói liệu trình" }),
    );
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Mẫu gói" }),
      "pl1",
    );
    const count = screen.getByRole("checkbox", {
      name: /Tính buổi hôm nay vào gói/,
    });
    expect(count).not.toBeChecked();
    await user.click(count);
    // Paid 349.000đ today: the customer hands over the rest.
    expect(screen.getByText("Khách đưa thêm")).toBeInTheDocument();
    expect(screen.getByText("3.151.000đ")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Chuyển sang gói" }));
    await waitFor(() => {
      expect(
        posted(calls, "/api/v1/visits/v1/convert-to-package"),
      ).toHaveLength(1);
    });
    const call = posted(calls, "/api/v1/visits/v1/convert-to-package")[0];
    expect(call?.headers["If-Match"]).toBe('"3"');
    expect(call?.body).toMatchObject({
      package_sale: { package_plan_id: "pl1", closer_staff_id: "st1" },
      payments: [{ method: "cash", amount: 3_500_000 }],
    });
    expect(posted(calls, "/api/v1/visits/v1/additions")).toHaveLength(0);
  });

  it("does not offer round 6 actions without the rights (the server still decides)", async () => {
    serve([{ ...ONE_OFF_VISIT, status: "voided" }]);
    const user = userEvent.setup();
    render(
      <App history={createMemoryHistory({ initialEntries: ["/quay"] })} />,
    );
    const table = await screen.findByRole("table", {
      name: "Lượt khách hôm nay",
    });
    await user.click(
      within(table).getByRole("button", {
        name: /^Thao tác lượt .* của Nguyễn Thị Hà$/,
      }),
    );
    const items = within(screen.getByRole("menu"))
      .getAllByRole("menuitem")
      .map((i) => i.textContent);
    expect(items).not.toContain("Thêm dịch vụ / bán gói");
    expect(items).not.toContain("Chuyển sang gói liệu trình");
  });
});
