import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { exceedsCap, formatDate, parseAmount, paymentsFor } from "./api";

const MANAGER = [
  "customer.read",
  "customer.write",
  "package.sell",
  "sale.discount",
  "catalog.read",
  "staff.read_basic",
];
const me = (permissions: string[]) => ({
  user: { id: "u1", full_name: "Quản lý Ngọc", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});
const HA = {
  id: "c1",
  full_name: "Nguyễn Thị Hà",
  phone: "0905 111 222",
  named: true,
  phone_e164: "+84905111222",
  gender: "female",
  birth_year: 1985,
  address: null,
  source: null,
  note: null,
  version: 1,
  created_at: "2026-11-04T00:00:00Z",
};
/** Hà's husband: a package she pays for can sit on his profile ("Mua cho người khác"). */
const NAM = {
  id: "c2",
  full_name: "Trần Văn Nam",
  phone: "0905 333 444",
  named: true,
  gender: "male",
};
const PLAN = {
  id: "p1",
  name: "Gội đầu 10 + tặng 2",
  price: 1_500_000,
  validity_months: 6,
  gift_note: "Tặng dầu gội",
  active: true,
  version: 1,
  items: [{ service_id: "s1", sessions: 10, bonus_sessions: 2 }],
};
const PACKAGE = {
  id: "cp1",
  customer_id: "c1",
  plan_id: "p1",
  name: "Gội đầu 10 + tặng 2",
  gift_note: "Tặng dầu gội",
  sold_price: 1_500_000,
  origin: "sale",
  sale_id: "sale1",
  closer_staff_id: "st1",
  closer_name: "KTV Lan",
  starts_on: "2026-11-04",
  expires_on: "2027-05-04",
  status: "active",
  sessions_total: 12,
  sessions_remaining: 12,
  version: 1,
  created_at: "2026-11-04T00:00:00Z",
  items: [
    {
      id: "i1",
      service_id: "s1",
      name: "Gội đầu",
      sessions_purchased: 10,
      sessions_bonus: 2,
      sessions_total: 12,
      sessions_remaining: 12,
      per_session_value: 150_000,
    },
  ],
};

interface Call {
  method: string;
  url: string;
  body: unknown;
  key: string | undefined;
}

function serve(
  permissions: string[],
  onSale: () => Response = () =>
    Response.json({ customer_packages: [PACKAGE] }, { status: 201 }),
) {
  const calls: Call[] = [];
  let sold = false;
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
    if (url === "/api/v1/me")
      return Promise.resolve(Response.json(me(permissions)));
    if (url === "/api/v1/customers/c1")
      return Promise.resolve(Response.json(HA));
    if (url === "/api/v1/customers/c1/packages")
      return Promise.resolve(Response.json(sold ? [PACKAGE] : []));
    if (url.startsWith("/api/v1/customers?q="))
      return Promise.resolve(Response.json([NAM, { ...HA, id: "c1" }]));
    if (url === "/api/v1/package-plans")
      return Promise.resolve(Response.json([PLAN]));
    if (url === "/api/v1/staff")
      return Promise.resolve(
        Response.json([
          {
            id: "st1",
            full_name: "KTV Lan",
            position: "Kỹ thuật viên",
            status: "active",
          },
        ]),
      );
    if (url === "/api/v1/sales" && call.method === "POST") {
      const reply = onSale();
      if (reply.ok) sold = true;
      return Promise.resolve(reply);
    }
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

const at = (path: string) =>
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

async function openDialog() {
  const user = userEvent.setup();
  at("/khach-hang/c1");
  await user.click(
    await screen.findByRole("button", { name: "Bán gói liệu trình" }),
  );
  const dialog = screen.getByRole("dialog", { name: "Bán gói liệu trình" });
  // The same package block as the front desk's cart (EM 2026-11-07).
  await within(dialog).findByRole("option", {
    name: /Gội đầu 10 \+ tặng 2 · 1\.500\.000/,
  });
  await within(dialog).findByRole("option", { name: /KTV Lan/ });
  const planSelect = within(dialog).getByRole("combobox", { name: "Mẫu gói" });
  const closerSelect = within(dialog).getByRole("combobox", {
    name: "Người giới thiệu / chốt",
  });
  const save = () =>
    user.click(within(dialog).getByRole("button", { name: "Lưu & thu tiền" }));
  return { user, dialog, planSelect, closerSelect, save };
}
const salePosts = (calls: Call[]) =>
  calls.filter((c) => c.method === "POST" && c.url === "/api/v1/sales");

describe("sale helpers", () => {
  it("splits a mixed payment so it always adds up to the total", () => {
    expect(paymentsFor(1_500_000, "mixed", 500_000)).toEqual([
      { method: "cash", amount: 500_000 },
      { method: "bank_transfer", amount: 1_000_000 },
    ]);
    expect(paymentsFor(1_500_000, "bank_transfer", 0)).toEqual([
      { method: "bank_transfer", amount: 1_500_000 },
    ]);
    expect(paymentsFor(0, "cash", 0)).toEqual([]);
  });

  it("knows the manager cap, dates and typed amounts", () => {
    expect(exceedsCap(1_500_000, 150_000, false)).toBe(false);
    expect(exceedsCap(1_500_000, 150_001, false)).toBe(true);
    expect(exceedsCap(1_500_000, 600_000, true)).toBe(false);
    expect(formatDate("2027-05-04")).toBe("04/05/2027");
    expect(parseAmount("1.500.000")).toBe(1_500_000);
    expect(parseAmount("abc")).toBeNaN();
  });
});

describe("sell package dialog", () => {
  it("sells a package once even when Lưu is clicked twice, then lists 12/12", async () => {
    const calls = serve(MANAGER);
    const { user, dialog, planSelect, closerSelect } = await openDialog();
    expect(
      await screen.findByText(/^Chưa có gói liệu trình nào./),
    ).toBeInTheDocument();
    await user.selectOptions(planSelect, "p1");
    await user.selectOptions(closerSelect, "st1");
    expect(within(dialog).getByLabelText("Quà tặng")).toHaveValue(
      "Tặng dầu gội",
    );
    expect(
      within(dialog).getByText("Phải trả: 1.500.000đ"),
    ).toBeInTheDocument();
    const save = within(dialog).getByRole("button", { name: "Lưu & thu tiền" });
    await user.dblClick(save);

    const posts = salePosts(calls);
    expect(posts.length).toBeGreaterThanOrEqual(1);
    expect(new Set(posts.map((c) => c.key)).size).toBe(1);
    expect(posts[0]?.key).toMatch(/^[0-9a-f-]{36}$/);
    expect(posts[0]?.body).toEqual({
      customer_id: "c1",
      closer_staff_id: "st1",
      beneficiary_customer_id: null,
      lines: [{ package_plan_id: "p1", gift_note: "Tặng dầu gội" }],
      discount: null,
      payments: [{ method: "cash", amount: 1_500_000 }],
    });
    expect(await screen.findByRole("status")).toHaveTextContent(
      "còn 12/12 buổi",
    );
    const list = await screen.findByRole("list", {
      name: "Gói liệu trình của khách",
    });
    expect(within(list).getByText("Còn 12/12 buổi")).toBeInTheDocument();
    expect(within(list).getByText(/HSD 04\/05\/2027/)).toBeInTheDocument();
  });

  it("records a package the customer chose themselves with nobody credited (Q-U2)", async () => {
    const calls = serve(MANAGER);
    const { user, planSelect, closerSelect, save } = await openDialog();
    await user.selectOptions(planSelect, "p1");
    // No technician to default to on the profile: "Khách tự chọn" until someone is picked.
    expect(closerSelect).toHaveValue("");
    expect(
      within(closerSelect).getByRole("option", { name: "Khách tự chọn" }),
    ).toBeInTheDocument();
    await save();
    expect(salePosts(calls)[0]?.body).toMatchObject({ closer_staff_id: null });
  });

  it("asks for a package before saving", async () => {
    const calls = serve(MANAGER);
    const { dialog, save } = await openDialog();
    await save();
    expect(
      within(dialog).getByText("Chọn mẫu gói liệu trình."),
    ).toBeInTheDocument();
    expect(salePosts(calls)).toHaveLength(0);
  });

  it("requires a reason for a discount and blocks a manager above 10%", async () => {
    const calls = serve(MANAGER);
    const { user, dialog, planSelect, closerSelect, save } = await openDialog();
    await user.selectOptions(planSelect, "p1");
    await user.selectOptions(closerSelect, "st1");
    await user.click(
      within(dialog).getByRole("button", { name: "Thêm giảm giá gói" }),
    );
    const amount = within(dialog).getByLabelText("Giảm giá gói (đ)");
    await user.type(amount, "200000");
    await save();
    expect(
      within(dialog).getByText("Giảm giá gói phải ghi lý do."),
    ).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Lý do giảm giá"), "Khách quen");
    await save();
    expect(
      within(dialog).getByText("Vượt 10 % giá gói, cần chủ spa."),
    ).toBeInTheDocument();
    expect(salePosts(calls)).toHaveLength(0);

    await user.clear(amount);
    await user.type(amount, "150000");
    await user.click(
      within(dialog).getByRole("radio", { name: "Tiền mặt + CK" }),
    );
    await user.type(within(dialog).getByLabelText("Tiền mặt (đ)"), "350000");
    expect(
      within(dialog).getByText("Chuyển khoản: 1.000.000đ"),
    ).toBeInTheDocument();
    await save();
    expect(salePosts(calls)[0]?.body).toMatchObject({
      discount: { amount: 150_000, reason: "Khách quen" },
      payments: [
        { method: "cash", amount: 350_000 },
        { method: "bank_transfer", amount: 1_000_000 },
      ],
    });
  });

  it("lets the owner discount above 10%", async () => {
    const calls = serve([...MANAGER, "sale.discount_unlimited"]);
    const { user, dialog, planSelect, save } = await openDialog();
    await user.selectOptions(planSelect, "p1");
    await user.click(
      within(dialog).getByRole("button", { name: "Thêm giảm giá gói" }),
    );
    await user.type(within(dialog).getByLabelText("Giảm giá gói (đ)"), "500000");
    await user.type(within(dialog).getByLabelText("Lý do giảm giá"), "Khách VIP");
    await save();
    expect(salePosts(calls)[0]?.body).toMatchObject({
      discount: { amount: 500_000, reason: "Khách VIP" },
      payments: [{ method: "cash", amount: 1_000_000 }],
    });
  });

  it("buys a package for someone else: it goes on their profile, the toast names both (US13)", async () => {
    const calls = serve(MANAGER);
    const { user, dialog, planSelect, save } = await openDialog();
    await user.selectOptions(planSelect, "p1");
    await user.click(
      within(dialog).getByRole("checkbox", { name: /Mua cho người khác/ }),
    );
    await save();
    expect(within(dialog).getByText("Chọn khách sẽ dùng gói.")).toBeInTheDocument();
    expect(salePosts(calls)).toHaveLength(0);

    await user.type(
      within(dialog).getByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" }),
      "Nam",
    );
    await user.click(
      await within(dialog).findByRole("option", { name: /Trần Văn Nam/ }),
    );
    expect(within(dialog).getByText("Người dùng gói")).toBeInTheDocument();
    const packagesBefore = calls.filter(
      (c) => c.url === "/api/v1/customers/c1/packages",
    ).length;
    await save();
    expect(salePosts(calls)[0]?.body).toMatchObject({
      customer_id: "c1",
      beneficiary_customer_id: "c2",
      lines: [{ package_plan_id: "p1" }],
    });
    expect(await screen.findByRole("status")).toHaveTextContent(
      'Đã bán "Gội đầu 10 + tặng 2" cho Trần Văn Nam (Nguyễn Thị Hà trả tiền).',
    );
    // The payer's list is refetched too (the beneficiary's key is invalidated the same way).
    expect(
      calls.filter((c) => c.url === "/api/v1/customers/c1/packages").length,
    ).toBeGreaterThan(packagesBefore);
  });

  it("explains a server rejection in Vietnamese and keeps the form", async () => {
    serve(
      MANAGER,
      () =>
        new Response(JSON.stringify({ status: 422, code: "staff.inactive" }), {
          status: 422,
        }),
    );
    const { user, dialog, planSelect, closerSelect, save } = await openDialog();
    await user.selectOptions(planSelect, "p1");
    await user.selectOptions(closerSelect, "st1");
    await save();
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "không còn làm việc",
    );
    expect(planSelect).toHaveValue("p1");
  });

  it("hides selling and discounts from people without those permissions", async () => {
    serve(["customer.read"]);
    at("/khach-hang/c1");
    expect(
      await screen.findByRole("heading", { name: /^Nguyễn Thị Hà/ }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(/^Chưa có gói liệu trình nào./),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Bán gói liệu trình" }),
    ).not.toBeInTheDocument();
  });
});
