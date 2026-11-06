import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { toPayload } from "./ServiceForm";
import { toPlanPayload } from "./PlanForm";

const OWNER = {
  user: { id: "u1", full_name: "Chủ Spa", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions: ["catalog.read", "catalog.manage"],
};
const MANAGER = { ...OWNER, permissions: ["catalog.read"] };
const CATEGORIES = [{ id: "c1", name: "Gội đầu dưỡng sinh", sort_order: 0 }];
const THU_GIAN = {
  id: "s1",
  category_id: "c1",
  name: "Thư Giãn",
  description: "+ Cạo gió đầu, đá nóng",
  duration_min: 45,
  recommended: false,
  active: true,
  version: 3,
  prices: { female: 79_000, male: 99_000 },
};
const CO_BAN = {
  id: "s2",
  category_id: "c1",
  name: "Cơ Bản – Dầu thảo dược",
  duration_min: 30,
  recommended: false,
  active: true,
  version: 1,
  prices: { all: 49_000 },
};

interface Call {
  method: string;
  path: string;
  headers: Record<string, string>;
  body: unknown;
}

function serve(
  me: unknown,
  handle: (call: Call) => Response | undefined = () => undefined,
) {
  const calls: Call[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    const path = url.replace(/\?.*$/, "");
    const call: Call = {
      method: init?.method ?? "GET",
      path,
      headers: (init?.headers ?? {}) as Record<string, string>,
      body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
    };
    calls.push(call);
    const custom = handle(call);
    if (custom) return Promise.resolve(custom);
    if (path === "/api/v1/me") return Promise.resolve(Response.json(me));
    if (path === "/api/v1/service-categories")
      return Promise.resolve(Response.json(CATEGORIES));
    if (path === "/api/v1/services")
      return Promise.resolve(Response.json([THU_GIAN, CO_BAN]));
    if (path === "/api/v1/package-plans")
      return Promise.resolve(Response.json([]));
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

const at = (path: string) =>
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

describe("payload mapping", () => {
  it("maps gender prices, consult duration and empty description", () => {
    expect(
      toPayload({
        name: " Thư Giãn ",
        category_id: "c1",
        description: " ",
        duration: "",
        mode: "gender",
        all: "",
        female: "79.000",
        male: "",
        recommended: true,
        active: true,
      }),
    ).toEqual({
      category_id: "c1",
      name: "Thư Giãn",
      description: null,
      duration_min: null,
      recommended: true,
      active: true,
      prices: { female: 79_000, male: null },
    });
  });

  it("maps plan items to numbers", () => {
    expect(
      toPlanPayload({
        name: "Gói 10",
        price: "1.500.000",
        validity: "6",
        gift_note: "",
        active: true,
        items: [{ service_id: "s2", sessions: "10", bonus: "2" }],
      }),
    ).toEqual({
      name: "Gói 10",
      price: 1_500_000,
      validity_months: 6,
      gift_note: null,
      active: true,
      items: [{ service_id: "s2", sessions: 10, bonus_sessions: 2 }],
    });
  });
});

describe("owner edits the price list", () => {
  it("updates a gender price with If-Match of the version read", async () => {
    const calls = serve(OWNER, (c) =>
      c.method === "PUT"
        ? Response.json({
            ...THU_GIAN,
            version: 4,
            prices: { female: 89_000, male: 99_000 },
          })
        : undefined,
    );
    at("/bang-gia");
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole("button", { name: "Sửa Thư Giãn" }),
    );
    const form = screen.getByRole("form", { name: "Sửa: Thư Giãn" });
    const female = within(form).getByLabelText("Giá Nữ (đ)");
    await user.clear(female);
    await user.type(female, "89000");
    await user.click(within(form).getByRole("button", { name: "Lưu dịch vụ" }));

    const put = calls.find((c) => c.method === "PUT");
    expect(put?.path).toBe("/api/v1/services/s1");
    expect(put?.headers["If-Match"]).toBe('"3"');
    expect(put?.body).toMatchObject({
      prices: { female: 89_000, male: 99_000 },
      duration_min: 45,
    });
  });

  it("explains a conflict when someone else saved first", async () => {
    serve(OWNER, (c) =>
      c.method === "PUT"
        ? new Response(
            JSON.stringify({ status: 412, code: "conflict.stale_version" }),
            { status: 412 },
          )
        : undefined,
    );
    at("/bang-gia");
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole("button", { name: "Sửa Cơ Bản – Dầu thảo dược" }),
    );
    await user.click(
      within(
        screen.getByRole("form", { name: "Sửa: Cơ Bản – Dầu thảo dược" }),
      ).getByRole("button", { name: "Lưu dịch vụ" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Dữ liệu vừa được người khác sửa",
    );
  });

  it("blocks an active service without a price before calling the API", async () => {
    const calls = serve(OWNER);
    at("/bang-gia");
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole("button", { name: "Thêm dịch vụ" }),
    );
    const form = screen.getByRole("form", { name: "Thêm dịch vụ" });
    await user.type(
      within(form).getByLabelText("Tên dịch vụ"),
      "Ngâm chân muối hồng",
    );
    await user.click(within(form).getByRole("button", { name: "Thêm dịch vụ vào bảng giá" }));
    expect(
      await within(form).findByText("Nhập giá để bán dịch vụ."),
    ).toBeInTheDocument();
    expect(calls.some((c) => c.method === "POST")).toBe(false);
  });

  it("creates a combo package plan", async () => {
    const calls = serve(OWNER, (c) =>
      c.method === "POST" && c.path === "/api/v1/package-plans"
        ? Response.json({ id: "p1" }, { status: 201 })
        : undefined,
    );
    at("/bang-gia/goi");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Tạo mẫu gói" }));
    const form = screen.getByRole("form", { name: "Tạo mẫu gói liệu trình" });
    await user.type(within(form).getByLabelText("Tên gói"), "Gội 10 + tặng 2");
    await user.type(within(form).getByLabelText("Giá gói (đ)"), "1500000");
    await user.selectOptions(within(form).getByLabelText("Dịch vụ 1"), "s2");
    await user.type(within(form).getByLabelText("Buổi tặng 1"), "{backspace}2");
    await user.click(within(form).getByRole("button", { name: "Lưu mẫu gói" }));

    const post = calls.find((c) => c.method === "POST");
    expect(post?.body).toEqual({
      name: "Gội 10 + tặng 2",
      price: 1_500_000,
      validity_months: null,
      gift_note: null,
      active: true,
      items: [{ service_id: "s2", sessions: 10, bonus_sessions: 2 }],
    });
  });
});

describe("non-owners", () => {
  it("see the menu without any edit controls (UX only; the API also refuses)", async () => {
    serve(MANAGER);
    at("/bang-gia");
    expect(await screen.findByText("Thư Giãn")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Sửa/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Thêm dịch vụ" }),
    ).not.toBeInTheDocument();
  });
});
