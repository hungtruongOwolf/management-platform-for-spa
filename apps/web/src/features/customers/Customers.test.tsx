import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { isSearchable } from "./api";
import { prefillFrom } from "./CustomerForm";

const ALL = [
  "customer.read",
  "customer.write",
  "customer.health_notes.read",
  "customer.health_notes.write",
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
  gender: "female",
};
const HA_FULL = {
  ...HA,
  phone_e164: "+84905111222",
  birth_year: 1985,
  address: null,
  source: null,
  note: null,
  health_note: "Huyết áp cao — tránh đá nóng",
  version: 1,
  created_at: "2026-11-04T00:00:00Z",
};

interface Call {
  method: string;
  url: string;
  body: unknown;
}

function serve(
  permissions: string[],
  handle: (c: Call) => Response | undefined = () => undefined,
) {
  const calls: Call[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    const call = {
      method: init?.method ?? "GET",
      url,
      body:
        typeof init?.body === "string"
          ? (JSON.parse(init.body) as unknown)
          : undefined,
    };
    calls.push(call);
    const custom = handle(call);
    if (custom) return Promise.resolve(custom);
    if (url === "/api/v1/me")
      return Promise.resolve(Response.json(me(permissions)));
    if (url === "/api/v1/customers") return Promise.resolve(Response.json([]));
    if (url.startsWith("/api/v1/customers?q="))
      return Promise.resolve(Response.json([HA]));
    if (url === "/api/v1/customers/c1")
      return Promise.resolve(Response.json(HA_FULL));
    if (url === "/api/v1/customers/c1/packages")
      return Promise.resolve(Response.json([]));
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

const at = (path: string) =>
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

describe("search rules", () => {
  it("mirrors the server: ≥ 3 phone digits or ≥ 2 letters", () => {
    expect(isSearchable("09")).toBe(false);
    expect(isSearchable("090")).toBe(false); // leading 0 does not count
    expect(isSearchable("0905")).toBe(true);
    // "+84" is the country code, not digits of the number (crm search_customers).
    expect(isSearchable("+84 9")).toBe(false);
    expect(isSearchable("+84 90")).toBe(false);
    expect(isSearchable("+84 905")).toBe(true);
    expect(isSearchable("849")).toBe(true);
    expect(isSearchable("h")).toBe(false);
    expect(isSearchable("ha")).toBe(true);
  });

  it("prefills the new-customer form from what was typed", () => {
    expect(prefillFrom("0905 111")).toEqual({
      full_name: "",
      phone: "0905 111",
    });
    expect(prefillFrom("Nguyễn Hà")).toEqual({
      full_name: "Nguyễn Hà",
      phone: "",
    });
  });
});

describe("customers page", () => {
  it("searches without accents and opens the profile with the health warning", async () => {
    const calls = serve(ALL);
    at("/khach-hang");
    const user = userEvent.setup();
    await user.type(
      await screen.findByRole("combobox", {
        name: "Tìm khách theo SĐT hoặc tên",
      }),
      "nguyen thi ha",
    );
    const option = await screen.findByRole("option", { name: /Nguyễn Thị Hà/ });
    expect(
      calls.some((c) => c.url === "/api/v1/customers?q=nguyen%20thi%20ha"),
    ).toBe(true);
    await user.click(option);
    expect(
      await screen.findByRole("heading", { name: /^Nguyễn Thị Hà/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("note")).toHaveTextContent(
      "Huyết áp cao — tránh đá nóng",
    );
  });

  it("keyboard: arrow + Enter selects a result", async () => {
    serve(ALL);
    at("/khach-hang");
    const user = userEvent.setup();
    const box = await screen.findByRole("combobox");
    await user.type(box, "0905");
    await screen.findByRole("option", { name: /Nguyễn Thị Hà/ });
    await user.keyboard("{Enter}");
    expect(
      await screen.findByRole("heading", { name: /^Nguyễn Thị Hà/ }),
    ).toBeInTheDocument();
  });

  it("shows no health section when the API omits the field", async () => {
    const withoutNote = Object.fromEntries(
      Object.entries(HA_FULL).filter(([key]) => key !== "health_note"),
    );
    serve(["customer.read"], (c) =>
      c.url === "/api/v1/customers/c1" ? Response.json(withoutNote) : undefined,
    );
    at("/khach-hang/c1");
    expect(
      await screen.findByRole("heading", { name: /^Nguyễn Thị Hà/ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Sửa hồ sơ" }),
    ).not.toBeInTheDocument();
  });

  it("offers 'Khách mới' only to a role that may create customers (customer.write)", async () => {
    serve(["customer.read"]);
    at("/khach-hang");
    expect(await screen.findByRole("combobox")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Khách mới" })).not.toBeInTheDocument();
  });

  it("creating a duplicate phone offers to open the existing profile", async () => {
    const calls = serve(ALL, (c) =>
      c.method === "POST"
        ? new Response(
            JSON.stringify({
              status: 409,
              code: "customer.phone_taken",
              existing_id: "c1",
            }),
            { status: 409 },
          )
        : undefined,
    );
    at("/khach-hang");
    const user = userEvent.setup();
    await user.type(await screen.findByRole("combobox"), "0905111222");
    await user.click(screen.getByRole("button", { name: "Khách mới" }));
    const form = screen.getByRole("form", { name: "Khách mới" });
    expect(within(form).getByLabelText("Số điện thoại")).toHaveValue(
      "0905111222",
    );
    await user.type(within(form).getByLabelText("Họ tên"), "Hà");
    await user.click(within(form).getByRole("button", { name: "Lưu khách" }));
    expect(calls.find((c) => c.method === "POST")?.body).toMatchObject({
      full_name: "Hà",
      phone: "0905111222",
      health_note: null,
    });
    await user.click(
      await within(form).findByRole("button", { name: "Mở hồ sơ đó" }),
    );
    expect(
      await screen.findByRole("heading", { name: /^Nguyễn Thị Hà/ }),
    ).toBeInTheDocument();
  });

  it("warns about a duplicate phone while typing, before submit, with one debounced check", async () => {
    const calls = serve(ALL, (c) =>
      c.url.startsWith("/api/v1/customers/phone-check")
        ? Response.json({ valid: true, taken: true, existing_id: "c1", full_name: "Nguyễn Thị Hà" })
        : undefined,
    );
    at("/khach-hang");
    const user = userEvent.setup();
    await user.type(await screen.findByRole("combobox"), "Hà");
    await user.click(screen.getByRole("button", { name: "Khách mới" }));
    const form = screen.getByRole("form", { name: "Khách mới" });
    await user.type(within(form).getByLabelText("Số điện thoại"), "0905 111 222");
    const notice = await within(form).findByText(/Số này đã có hồ sơ:/, {}, { timeout: 3000 });
    expect(notice).toHaveTextContent("Số này đã có hồ sơ: Nguyễn Thị Hà");
    const checks = calls.filter((c) => c.url.startsWith("/api/v1/customers/phone-check"));
    expect(checks.map((c) => c.url)).toEqual(["/api/v1/customers/phone-check?phone=0905111222"]);
    expect(calls.some((c) => c.method === "POST")).toBe(false);
    await user.click(within(form).getByRole("button", { name: "Mở hồ sơ" }));
    expect(await screen.findByRole("heading", { name: /^Nguyễn Thị Hà/ })).toBeInTheDocument();
  });

  it("does not check an incomplete number, nor flag a customer's own number when editing", async () => {
    const calls = serve(ALL, (c) =>
      c.url.startsWith("/api/v1/customers/phone-check")
        ? Response.json({ valid: true, taken: true, existing_id: "c1", full_name: "Nguyễn Thị Hà" })
        : c.url === "/api/v1/customers/c1"
          ? Response.json(HA_FULL)
          : undefined,
    );
    at("/khach-hang/c1");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Sửa hồ sơ" }));
    const form = screen.getByRole("form", { name: "Sửa hồ sơ khách" });
    const phone = within(form).getByLabelText("Số điện thoại");
    await user.clear(phone);
    await user.type(phone, "0905 111");
    await new Promise((r) => setTimeout(r, 600));
    expect(calls.some((c) => c.url.includes("phone-check?phone=0905111&") || c.url.endsWith("phone-check?phone=0905111"))).toBe(false);
    await user.type(phone, " 222");
    await new Promise((r) => setTimeout(r, 600));
    expect(calls.some((c) => c.url.endsWith("phone-check?phone=0905111222"))).toBe(true);
    // The number belongs to the profile being edited: no warning.
    expect(within(form).queryByText(/Số này đã có hồ sơ/)).not.toBeInTheDocument();
  });

  it("rejects a malformed phone before calling the API", async () => {
    const calls = serve(ALL);
    at("/khach-hang");
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole("button", { name: "Khách mới" }),
    );
    const form = screen.getByRole("form", { name: "Khách mới" });
    await user.type(within(form).getByLabelText("Họ tên"), "Ai đó");
    await user.type(within(form).getByLabelText("Số điện thoại"), "12345");
    await user.click(within(form).getByRole("button", { name: "Lưu khách" }));
    expect(
      await within(form).findByText(/Số điện thoại chưa đúng/),
    ).toBeInTheDocument();
    expect(calls.some((c) => c.method === "POST")).toBe(false);
  });
});

describe("customer identity (EM 2026-11-07, ADR-0011)", () => {
  const WALK_IN = {
    ...HA_FULL,
    id: "w1",
    full_name: "Khách vãng lai",
    named: false,
    phone: null,
    phone_e164: null,
    gender: null,
    birth_year: null,
    birth_date: null,
  };

  it("saves a profile with neither name nor phone (anonymous walk-in)", async () => {
    const calls = serve(ALL, (c) =>
      c.method === "POST" && c.url === "/api/v1/customers"
        ? Response.json(WALK_IN, { status: 201 })
        : c.url === "/api/v1/customers/w1"
          ? Response.json(WALK_IN)
          : undefined,
    );
    at("/khach-hang");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Khách mới" }));
    const form = screen.getByRole("form", { name: "Khách mới" });
    expect(within(form).getByLabelText("Họ tên")).toHaveAccessibleDescription(/Không bắt buộc/);
    // The server's limits (crm validate: address 500, note 2000, health note 2000).
    expect(within(form).getByLabelText("Địa chỉ")).toHaveAttribute("maxLength", "500");
    expect(within(form).getByLabelText("Ghi chú chung")).toHaveAttribute("maxLength", "2000");
    expect(within(form).getByLabelText(/Lưu ý sức khỏe/)).toHaveAttribute("maxLength", "2000");
    await user.click(within(form).getByRole("button", { name: "Lưu khách" }));
    await vi.waitFor(() => {
      expect(calls.some((c) => c.method === "POST")).toBe(true);
    });
    const post = calls.find((c) => c.method === "POST");
    expect(post?.body).toMatchObject({ full_name: null, phone: null, birth_date: null });
  });

  it("sends a date of birth (and lets the server take the year from it)", async () => {
    const calls = serve(ALL, (c) =>
      c.method === "POST" && c.url === "/api/v1/customers"
        ? Response.json({ ...HA_FULL, named: true, birth_date: "1990-03-14", birth_year: 1990 }, { status: 201 })
        : undefined,
    );
    at("/khach-hang");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Khách mới" }));
    const form = screen.getByRole("form", { name: "Khách mới" });
    await user.type(within(form).getByLabelText("Họ tên"), "Lê Thị Mai");
    const birth = within(form).getByLabelText("Ngày sinh");
    await user.type(birth, "14031990");
    await user.tab();
    // Year field is for those who only know the year: hidden once a date is given.
    expect(within(form).queryByLabelText(/Năm sinh/)).not.toBeInTheDocument();
    await user.click(within(form).getByRole("button", { name: "Lưu khách" }));
    await vi.waitFor(() => {
      expect(calls.some((c) => c.method === "POST")).toBe(true);
    });
    expect(calls.find((c) => c.method === "POST")?.body).toMatchObject({
      full_name: "Lê Thị Mai",
      phone: null,
      birth_date: "1990-03-14",
      birth_year: null,
    });
  });

  it("refuses a date of birth in the future before calling the API", async () => {
    const calls = serve(ALL);
    at("/khach-hang");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Khách mới" }));
    const form = screen.getByRole("form", { name: "Khách mới" });
    await user.type(within(form).getByLabelText("Ngày sinh"), "01012999");
    await user.tab();
    await user.click(within(form).getByRole("button", { name: "Lưu khách" }));
    expect(calls.some((c) => c.method === "POST")).toBe(false);
  });

  it("shows a phone-less profile as “Chưa có SĐT” and its date of birth", async () => {
    serve(ALL, (c) =>
      c.url === "/api/v1/customers/c1"
        ? Response.json({ ...HA_FULL, named: true, phone: null, phone_e164: null, birth_date: "1990-03-14", birth_year: 1990 })
        : undefined,
    );
    at("/khach-hang/c1");
    expect(await screen.findByText("Chưa có SĐT")).toBeInTheDocument();
    expect(screen.getByText("14/03/1990")).toBeInTheDocument();
  });

  it("edits an anonymous walk-in starting from an empty name", async () => {
    serve(ALL, (c) => (c.url === "/api/v1/customers/w1" ? Response.json(WALK_IN) : undefined));
    at("/khach-hang/w1");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Sửa hồ sơ" }));
    const form = await screen.findByRole("form", { name: "Sửa hồ sơ khách" });
    expect(within(form).getByLabelText("Họ tên")).toHaveValue("");
    expect(within(form).getByLabelText("Số điện thoại")).toHaveValue("");
  });
});

