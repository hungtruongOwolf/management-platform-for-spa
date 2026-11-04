import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import {
  emptyPackage,
  fromPlan,
  itemProblem,
  packageProblems,
  toRequest,
  todayInVietnam,
} from "./api";

const MANAGER = [
  "customer.read",
  "customer.write",
  "opening_balance.import",
  "catalog.read",
];
const me = (permissions: string[]) => ({
  user: { id: "u1", full_name: "Quản lý Ngọc", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});
const PLAN = {
  id: "p1",
  name: "Gội đầu 10 + tặng 2",
  price: 1_500_000,
  validity_months: 6,
  gift_note: null,
  active: true,
  version: 1,
  items: [{ service_id: "s1", sessions: 10, bonus_sessions: 2 }],
};
const SERVICE = {
  id: "s1",
  category_id: "k1",
  name: "Gội đầu dưỡng sinh",
  description: null,
  duration_min: 60,
  recommended: false,
  active: true,
  version: 1,
  prices: { all: 150_000 },
};
const HA = {
  id: "c1",
  full_name: "Nguyễn Thị Hà",
  phone: "0905 111 222",
  gender: "female",
};
const IMPORTED = {
  customer_id: "c1",
  customer_created: false,
  customer_packages: [
    {
      id: "cp1",
      name: "Gội đầu 10 + tặng 2",
      origin: "opening_balance",
      sessions_remaining: 4,
      sessions_total: 12,
    },
  ],
};

const OPENING_PACKAGE = {
  id: "cp1",
  customer_id: "c1",
  plan_id: null,
  name: "Combo cũ",
  gift_note: null,
  sold_price: 500_000,
  origin: "opening_balance",
  sale_id: null,
  closer_staff_id: null,
  closer_name: null,
  starts_on: "2026-01-15",
  expires_on: null,
  status: "active",
  sessions_total: 5,
  sessions_remaining: 2,
  version: 1,
  created_at: "2026-11-04T00:00:00Z",
  items: [],
};

interface Call {
  method: string;
  url: string;
  body: unknown;
  key: string | undefined;
}

function serve(
  permissions: string[],
  onImport: () => Response = () => Response.json(IMPORTED, { status: 201 }),
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
    if (url === "/api/v1/me")
      return Promise.resolve(Response.json(me(permissions)));
    if (url === "/api/v1/package-plans?active=false")
      return Promise.resolve(Response.json([PLAN]));
    if (url === "/api/v1/services?active=false")
      return Promise.resolve(Response.json([SERVICE]));
    if (url.startsWith("/api/v1/customers?q="))
      return Promise.resolve(Response.json([HA]));
    if (url === "/api/v1/opening-balances" && call.method === "POST")
      return Promise.resolve(onImport());
    if (url === "/api/v1/customers/c1")
      return Promise.resolve(
        Response.json({
          ...HA,
          phone_e164: "+84905111222",
          birth_year: null,
          address: null,
          source: null,
          note: null,
          version: 1,
          created_at: "2026-11-04T00:00:00Z",
        }),
      );
    if (url === "/api/v1/customers/c1/packages")
      return Promise.resolve(Response.json([OPENING_PACKAGE]));
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

const at = (path: string) =>
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

describe("opening balance helpers", () => {
  it("rejects more sessions left than the original total (AC8.4)", () => {
    const item = {
      serviceId: "s1",
      purchased: "10",
      bonus: "0",
      remaining: "11",
    };
    expect(itemProblem(item)).toBe("Còn lại không được lớn hơn tổng 10 buổi.");
    expect(itemProblem({ ...item, remaining: "10" })).toBeUndefined();
    expect(
      itemProblem({ ...item, bonus: "2", remaining: "12" }),
    ).toBeUndefined();
    expect(itemProblem({ ...item, remaining: "" })).toBe(
      "Nhập số buổi còn lại.",
    );
  });

  it("copies a plan, then sends what the book says", () => {
    const draft = fromPlan(
      { ...emptyPackage(), startsOn: "2026-06-01" },
      PLAN,
      () => "Gội đầu dưỡng sinh",
    );
    expect(draft.items[0]).toMatchObject({
      serviceName: "Gội đầu dưỡng sinh",
      purchased: "10",
      bonus: "2",
    });
    const filled = {
      ...draft,
      price: "1.400.000",
      items: draft.items.map((i) => ({ ...i, remaining: "4" })),
    };
    expect(toRequest(filled)).toEqual({
      package_plan_id: "p1",
      name: "Gội đầu 10 + tặng 2",
      gift_note: null,
      sold_price: 1_400_000,
      starts_on: "2026-06-01",
      expires_on: null,
      items: [
        {
          service_id: "s1",
          sessions_purchased: 10,
          sessions_bonus: 2,
          sessions_remaining: 4,
        },
      ],
    });
  });

  it("knows the business day and refuses future start dates", () => {
    expect(todayInVietnam(new Date("2026-11-04T18:00:00Z"))).toBe("2026-11-05");
    const p = {
      ...emptyPackage(),
      name: "X",
      price: "0",
      startsOn: "2026-11-06",
    };
    expect(packageProblems(p, "2026-11-05").startsOn).toBe(
      "Ngày đăng ký không được ở tương lai.",
    );
    expect(
      packageProblems(
        { ...p, startsOn: "2026-11-05", expiresOn: "2026-11-01" },
        "2026-11-05",
      ).expiresOn,
    ).toBe("Hạn dùng trước ngày đăng ký.");
  });
});

describe("/nhap-so-cu", () => {
  async function pickCustomerAndPlan() {
    const user = userEvent.setup();
    at("/nhap-so-cu");
    const search = await screen.findByRole("combobox", {
      name: "Tìm khách theo SĐT hoặc tên",
    });
    expect(search).toHaveFocus();
    await user.type(search, "0905");
    await screen.findByRole("option", { name: /Nguyễn Thị Hà/ });
    await user.keyboard("{Enter}");
    expect(
      screen.getByText("Nguyễn Thị Hà · 0905 111 222"),
    ).toBeInTheDocument();
    const form = screen.getByRole("form", { name: "Nhập liệu trình từ sổ cũ" });
    await within(form).findByRole("option", { name: /Gội đầu 10 \+ tặng 2/ });
    await user.selectOptions(within(form).getByLabelText("Gói"), "p1");
    return { user, form };
  }

  it("imports 4 left of a plan once even on a double click, then clears for the next customer", async () => {
    const calls = serve(MANAGER);
    const { user, form } = await pickCustomerAndPlan();
    expect(
      screen.getByText("Không tính vào doanh thu hôm nay."),
    ).toBeInTheDocument();
    await user.type(within(form).getByLabelText("Còn lại"), "4");
    await user.type(
      within(form).getByLabelText("Ngày đăng ký gốc"),
      "2026-06-01",
    );
    await user.dblClick(
      within(form).getByRole("button", { name: "Lưu và nhập tiếp" }),
    );

    const posts = calls.filter((c) => c.method === "POST");
    expect(posts.length).toBeGreaterThanOrEqual(1);
    expect(new Set(posts.map((c) => c.key)).size).toBe(1);
    expect(posts[0]?.body).toEqual({
      customer_id: "c1",
      customer: null,
      packages: [
        {
          package_plan_id: "p1",
          name: "Gội đầu 10 + tặng 2",
          gift_note: null,
          sold_price: 1_500_000,
          starts_on: "2026-06-01",
          expires_on: null,
          items: [
            {
              service_id: "s1",
              sessions_purchased: 10,
              sessions_bonus: 2,
              sessions_remaining: 4,
            },
          ],
        },
      ],
    });
    expect(await screen.findByText(/còn 4\/12 buổi/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Đã nhập 1 liệu trình",
    );
    expect(
      await screen.findByRole("combobox", {
        name: "Tìm khách theo SĐT hoặc tên",
      }),
    ).toBeInTheDocument();
  });

  it("'Lưu và nhập tiếp' puts the cursor back in the customer search", async () => {
    serve(MANAGER);
    const { user, form } = await pickCustomerAndPlan();
    await user.type(within(form).getByLabelText("Còn lại"), "4");
    await user.type(
      within(form).getByLabelText("Ngày đăng ký gốc"),
      "2026-06-01",
    );
    await user.click(
      within(form).getByRole("button", { name: "Lưu và nhập tiếp" }),
    );
    expect(
      await screen.findByRole("combobox", {
        name: "Tìm khách theo SĐT hoặc tên",
      }),
    ).toHaveFocus();
    expect(within(form).getByLabelText("Gói")).toHaveValue("");
  });

  it("blocks more left than total before sending", async () => {
    const calls = serve(MANAGER);
    const { user, form } = await pickCustomerAndPlan();
    await user.type(within(form).getByLabelText("Còn lại"), "13");
    await user.type(
      within(form).getByLabelText("Ngày đăng ký gốc"),
      "2026-06-01",
    );
    await user.click(
      within(form).getByRole("button", { name: "Lưu và nhập tiếp" }),
    );
    expect(
      within(form).getByText("Còn lại không được lớn hơn tổng 12 buổi."),
    ).toBeInTheDocument();
    expect(calls.some((c) => c.method === "POST")).toBe(false);
  });

  it("creates a new customer in the same form and offers the existing profile on a duplicate phone", async () => {
    const calls = serve(
      MANAGER,
      () =>
        new Response(
          JSON.stringify({
            status: 409,
            code: "customer.phone_taken",
            existing_id: "c1",
          }),
          {
            status: 409,
          },
        ),
    );
    const user = userEvent.setup();
    at("/nhap-so-cu");
    await user.type(
      await screen.findByRole("combobox", {
        name: "Tìm khách theo SĐT hoặc tên",
      }),
      "Bình",
    );
    await user.click(screen.getByRole("button", { name: "+ Khách mới" }));
    const form = screen.getByRole("form", { name: "Nhập liệu trình từ sổ cũ" });
    expect(within(form).getByLabelText("Họ tên")).toHaveValue("Bình");
    await user.type(within(form).getByLabelText("Số điện thoại"), "0905111222");
    await user.type(within(form).getByLabelText("Tên gói"), "Combo cũ");
    await within(form).findByRole("option", { name: "Gội đầu dưỡng sinh" });
    await user.selectOptions(within(form).getByLabelText("Dịch vụ"), "s1");
    await user.type(within(form).getByLabelText("Buổi mua"), "5");
    await user.type(within(form).getByLabelText("Còn lại"), "2");
    await user.type(within(form).getByLabelText("Giá đã bán (đ)"), "500000");
    await user.type(
      within(form).getByLabelText("Ngày đăng ký gốc"),
      "2026-01-15",
    );
    await user.click(
      within(form).getByRole("button", { name: "Lưu và nhập tiếp" }),
    );

    const post = calls.find((c) => c.method === "POST");
    expect(post?.body).toMatchObject({
      customer_id: null,
      customer: { full_name: "Bình", phone: "0905111222", gender: null },
      packages: [
        { package_plan_id: null, name: "Combo cũ", sold_price: 500_000 },
      ],
    });
    await user.click(
      await within(form).findByRole("button", { name: "Dùng hồ sơ đó" }),
    );
    expect(
      screen.getByText("Khách đã có (SĐT 0905111222)"),
    ).toBeInTheDocument();
  });

  it("is in the menu only for people who may import", async () => {
    serve(["customer.read", "catalog.read"]);
    at("/nhap-so-cu");
    expect(
      await screen.findByText("Bạn không có quyền nhập liệu trình từ sổ cũ."),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("link", { name: "Nhập sổ cũ" }),
    ).not.toBeInTheDocument();
  });

  it("shows the menu entry to managers", async () => {
    serve(MANAGER);
    at("/");
    expect(
      await screen.findByRole("link", { name: "Nhập sổ cũ" }),
    ).toBeInTheDocument();
  });
});

describe("customer profile", () => {
  it("labels imported packages 'Nhập từ sổ cũ' (AC8.5)", async () => {
    serve(MANAGER);
    at("/khach-hang/c1");
    const list = await screen.findByRole("list", {
      name: "Liệu trình của khách",
    });
    expect(within(list).getByText("Nhập từ sổ cũ")).toBeInTheDocument();
    expect(within(list).getByText("còn 2/5 buổi")).toBeInTheDocument();
  });
});
