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

  it("adds a service group from the page header, the form at the top of the page", async () => {
    const calls = serve(OWNER, (c) =>
      c.method === "POST" && c.path === "/api/v1/service-categories"
        ? Response.json({ id: "c9", name: "Chăm sóc tay chân", sort_order: 9 }, { status: 201 })
        : undefined,
    );
    at("/bang-gia");
    const user = userEvent.setup();
    // Beside "Mẫu gói liệu trình", like "Tạo mẫu gói" on the plans page (EM 2026-11-06).
    await user.click(await screen.findByRole("button", { name: "Thêm nhóm dịch vụ" }));
    const form = screen.getByRole("form", { name: "Thêm nhóm dịch vụ" });
    expect(screen.queryByRole("button", { name: "Thêm nhóm dịch vụ" })).not.toBeInTheDocument();
    await user.click(within(form).getByRole("button", { name: "Lưu nhóm dịch vụ" }));
    expect(within(form).getByText("Nhập tên nhóm.")).toBeInTheDocument();
    expect(calls.some((c) => c.method === "POST")).toBe(false);
    await user.type(within(form).getByLabelText("Tên nhóm dịch vụ mới"), "Chăm sóc tay chân");
    await user.click(within(form).getByRole("button", { name: "Lưu nhóm dịch vụ" }));
    expect(calls.find((c) => c.method === "POST")?.body).toEqual({ name: "Chăm sóc tay chân" });
    expect(await screen.findByRole("button", { name: "Thêm nhóm dịch vụ" })).toBeInTheDocument();
  });

  it("renames a service group from 'Sửa nhóm' on its heading: the name only, its place kept (K48)", async () => {
    let renamed = false;
    const calls = serve(OWNER, (c) => {
      if (c.method === "PUT" && c.path === "/api/v1/service-categories/c1") {
        renamed = true;
        return Response.json({ id: "c1", name: "Gội đầu thảo dược", sort_order: 0 });
      }
      if (c.method === "GET" && c.path === "/api/v1/service-categories" && renamed)
        return Response.json([{ id: "c1", name: "Gội đầu thảo dược", sort_order: 0 }]);
      return undefined;
    });
    at("/bang-gia");
    const user = userEvent.setup();
    const open = await screen.findByRole("button", { name: "Sửa nhóm Gội đầu dưỡng sinh" });
    expect(open).toHaveTextContent("Sửa nhóm");
    await user.click(open);
    expect(open).toHaveAttribute("aria-expanded", "true");
    const form = screen.getByRole("form", { name: "Sửa nhóm: Gội đầu dưỡng sinh" });
    // Under the group's heading, before its services.
    expect(form.closest("section")).toBe(screen.getByRole("region", { name: "Gội đầu dưỡng sinh" }));
    const name = within(form).getByLabelText("Tên nhóm dịch vụ");
    expect(name).toHaveFocus();
    expect(name).toHaveValue("Gội đầu dưỡng sinh");
    await user.clear(name);
    await user.type(name, "  Gội đầu thảo dược ");
    await user.click(within(form).getByRole("button", { name: "Lưu nhóm dịch vụ" }));

    const put = calls.find((c) => c.method === "PUT");
    expect(put?.body).toEqual({ name: "Gội đầu thảo dược", sort_order: 0 });
    // The list and every picker read the new name (["catalog"] invalidated), the form closes.
    expect(await screen.findByRole("heading", { name: "Gội đầu thảo dược" })).toBeInTheDocument();
    expect(screen.queryByRole("form", { name: /^Sửa nhóm/ })).toBeNull();
    expect(calls.filter((c) => c.path === "/api/v1/service-categories" && c.method === "GET").length).toBe(2);
    expect(calls.filter((c) => c.path === "/api/v1/services" && c.method === "GET").length).toBe(2);
  });

  it("a group's name taken: said under the field; empty: said before any request; unchanged: closes without one", async () => {
    const calls = serve(OWNER, (c) =>
      c.method === "PUT"
        ? Response.json({ status: 422, code: "validation.failed", errors: [{ field: "name", code: "taken" }] }, { status: 422 })
        : undefined,
    );
    at("/bang-gia");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Sửa nhóm Gội đầu dưỡng sinh" }));
    const form = screen.getByRole("form", { name: "Sửa nhóm: Gội đầu dưỡng sinh" });
    const name = within(form).getByLabelText("Tên nhóm dịch vụ");
    const save = within(form).getByRole("button", { name: "Lưu nhóm dịch vụ" });

    await user.clear(name);
    await user.click(save);
    expect(within(form).getByText("Nhập tên nhóm.")).toBeInTheDocument();
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(calls.some((c) => c.method === "PUT")).toBe(false);

    await user.type(name, "Massage body");
    await user.click(save);
    expect(await within(form).findByText("Tên này đã được dùng.")).toBeInTheDocument();
    expect(name).toHaveAccessibleDescription("Tên này đã được dùng.");
    expect(within(form).queryByRole("alert")).toBeNull();
    expect(calls.filter((c) => c.method === "PUT")).toHaveLength(1);

    await user.clear(name);
    await user.type(name, "Gội đầu dưỡng sinh");
    await user.click(save);
    expect(screen.queryByRole("form", { name: /^Sửa nhóm/ })).toBeNull();
    expect(calls.filter((c) => c.method === "PUT")).toHaveLength(1);
  });

  describe("'Xóa nhóm': a group with no service (K68)", () => {
    const MISTAKE = { id: "c2", name: "Nhóm thêm nhầm", sort_order: 1 };
    const FEET = { id: "c3", name: "Chăm sóc chân", sort_order: 2 };
    /** Off sale ("Ngừng bán"): still a service of its group. */
    const OFF_SALE = { ...CO_BAN, id: "s3", category_id: "c3", name: "Ngâm chân cũ", active: false };

    /** The price list with `groups`, `MISTAKE` gone once deleted; `refuse` answers the delete instead. */
    function spa(groups: unknown[], refuse?: Response) {
      let gone = false;
      let filled = false;
      const calls = serve(OWNER, (c) => {
        if (c.method === "DELETE" && c.path === "/api/v1/service-categories/c2") {
          if (refuse) {
            filled = true;
            return refuse;
          }
          gone = true;
          return new Response(null, { status: 204 });
        }
        if (c.method === "GET" && c.path === "/api/v1/service-categories")
          return Response.json(gone ? groups.filter((g) => g !== MISTAKE) : groups);
        if (c.method === "GET" && c.path === "/api/v1/services")
          // Someone added a service to the group from another device.
          return Response.json([THU_GIAN, CO_BAN, OFF_SALE, ...(filled ? [{ ...CO_BAN, id: "s4", category_id: "c2", name: "Vừa thêm" }] : [])]);
        return undefined;
      });
      return calls;
    }

    async function openDelete(user: ReturnType<typeof userEvent.setup>) {
      await user.click(await screen.findByRole("button", { name: "Sửa nhóm Nhóm thêm nhầm" }));
      const form = screen.getByRole("form", { name: "Sửa nhóm: Nhóm thêm nhầm" });
      await user.click(within(form).getByRole("button", { name: "Xóa nhóm Nhóm thêm nhầm" }));
      return { form, sheet: await screen.findByRole("dialog", { name: "Xóa nhóm Nhóm thêm nhầm?" }) };
    }

    it("only on a group with no service: one on sale or off sale keeps the button away", async () => {
      spa([CATEGORIES[0], MISTAKE, FEET]);
      at("/bang-gia");
      const user = userEvent.setup();
      for (const group of ["Gội đầu dưỡng sinh", "Chăm sóc chân"]) {
        await user.click(await screen.findByRole("button", { name: `Sửa nhóm ${group}` }));
        const form = screen.getByRole("form", { name: `Sửa nhóm: ${group}` });
        expect(within(form).queryByRole("button", { name: /^Xóa nhóm/ })).toBeNull();
        await user.click(within(form).getByRole("button", { name: "Hủy" }));
      }
      await user.click(screen.getByRole("button", { name: "Sửa nhóm Nhóm thêm nhầm" }));
      const form = screen.getByRole("form", { name: "Sửa nhóm: Nhóm thêm nhầm" });
      expect(within(form).getByRole("button", { name: "Xóa nhóm Nhóm thêm nhầm" })).toHaveTextContent("Xóa nhóm");
    });

    it("asks first, in a sheet; 'Thôi' sends nothing and keeps the form", async () => {
      const calls = spa([CATEGORIES[0], MISTAKE, FEET]);
      at("/bang-gia");
      const user = userEvent.setup();
      const { form, sheet } = await openDelete(user);
      expect(sheet).toHaveTextContent("Nhóm này chưa có dịch vụ nào. Xóa xong, nhóm không còn trên bảng giá.");
      await user.click(within(sheet).getByRole("button", { name: "Thôi" }));
      expect(screen.queryByRole("dialog", { name: /^Xóa nhóm/ })).toBeNull();
      expect(form).toBeInTheDocument();
      expect(calls.some((c) => c.method === "DELETE")).toBe(false);
    });

    it("deletes, refetches the catalogue and focuses the next group's heading", async () => {
      const calls = spa([CATEGORIES[0], MISTAKE, FEET]);
      at("/bang-gia");
      const user = userEvent.setup();
      const { sheet } = await openDelete(user);
      await user.click(within(sheet).getByRole("button", { name: "Xóa nhóm" }));

      expect(calls.filter((c) => c.method === "DELETE").map((c) => c.path)).toEqual(["/api/v1/service-categories/c2"]);
      const next = await screen.findByRole("heading", { level: 2, name: "Chăm sóc chân" });
      await vi.waitFor(() => {
        expect(next).toHaveFocus();
      });
      expect(screen.queryByRole("heading", { name: "Nhóm thêm nhầm" })).toBeNull();
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(screen.queryByRole("form", { name: /^Sửa nhóm/ })).toBeNull();
      // ["catalog"] invalidated: every picker drops the group too.
      expect(calls.filter((c) => c.path === "/api/v1/service-categories" && c.method === "GET").length).toBe(2);
      // A heading takes focus from script only: never a tab stop.
      expect(next).toHaveAttribute("tabindex", "-1");
    });

    it("the last group: focus goes back to the page title", async () => {
      spa([CATEGORIES[0], MISTAKE]);
      at("/bang-gia");
      const user = userEvent.setup();
      const { sheet } = await openDelete(user);
      await user.click(within(sheet).getByRole("button", { name: "Xóa nhóm" }));
      await vi.waitFor(() => {
        expect(screen.getByRole("heading", { level: 1, name: "Bảng giá dịch vụ" })).toHaveFocus();
      });
      expect(screen.queryByRole("heading", { name: "Nhóm thêm nhầm" })).toBeNull();
    });

    it("a group filled from another device: the sheet says why in Vietnamese, the list refetches, the button goes", async () => {
      spa(
        [CATEGORIES[0], MISTAKE, FEET],
        Response.json({ status: 409, code: "catalog.category_in_use" }, { status: 409 }),
      );
      at("/bang-gia");
      const user = userEvent.setup();
      const { form, sheet } = await openDelete(user);
      await user.click(within(sheet).getByRole("button", { name: "Xóa nhóm" }));
      expect(await within(sheet).findByRole("alert")).toHaveTextContent(
        "Không xóa được: nhóm này đang có dịch vụ (có thể vừa được thêm ở máy khác). Chỉ xóa được nhóm chưa có dịch vụ nào.",
      );
      expect(await screen.findByText("Vừa thêm")).toBeInTheDocument();
      expect(within(form).queryByRole("button", { name: /^Xóa nhóm/ })).toBeNull();
      expect(screen.getByRole("heading", { level: 2, name: "Nhóm thêm nhầm" })).toBeInTheDocument();
    });
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
