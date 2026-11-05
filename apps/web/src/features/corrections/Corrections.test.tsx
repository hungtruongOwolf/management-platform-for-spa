import type { CustomerPackage, Visit } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { staffByUse } from "../front-desk/api";
import { addMonths, isUnused, spaToday } from "./api";

const OWNER = [
  "customer.read",
  "customer.write",
  "package.sell",
  "package.extend_expiry",
  "package.cancel_refund",
  "record.void_same_day",
  "record.void_closed_day",
  "visit.record",
  "visit.read",
  "catalog.read",
  "staff.read_basic",
];
const MANAGER = OWNER.filter(
  (p) =>
    p !== "package.extend_expiry" &&
    p !== "package.cancel_refund" &&
    p !== "record.void_closed_day",
);

const me = (permissions: string[]) => ({
  user: { id: "u1", full_name: "Chủ Spa", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});

const HA = {
  id: "c1",
  full_name: "Nguyễn Thị Hà",
  phone: "0905 111 222",
  phone_e164: "+84905111222",
  gender: "female",
  birth_year: null,
  address: null,
  source: null,
  note: null,
  version: 1,
  created_at: "2026-11-01T00:00:00Z",
};

function pkg(
  remaining = 12,
  extra: Partial<CustomerPackage> = {},
): CustomerPackage {
  return {
    id: "cp1",
    customer_id: "c1",
    plan_id: "p1",
    name: "Gội đầu 10 + tặng 2",
    gift_note: null,
    sold_price: 1_500_000,
    sale_id: "sale1",
    closer_staff_id: "st1",
    closer_name: "KTV Lan",
    starts_on: "2026-11-04",
    expires_on: "2027-05-04",
    cancelled_at: null,
    cancel_reason: null,
    refund: null,
    status: "active",
    sessions_total: 12,
    sessions_remaining: remaining,
    version: 3,
    created_at: "2026-11-04T00:00:00Z",
    items: [
      {
        id: "i1",
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
    ...extra,
  };
}

function visit(extra: Partial<Visit> = {}): Visit {
  return {
    id: "v1",
    customer_id: "c1",
    customer_name: "Nguyễn Thị Hà",
    business_date: spaToday(),
    check_in_at: "2026-11-04T02:00:00Z",
    status: "completed",
    void_reason: null,
    voided_at: null,
    sale_id: null,
    subtotal: 0,
    discount_amount: 0,
    discount_reason: null,
    total: 0,
    payments: [],
    version: 1,
    created_at: "2026-11-04T02:00:00Z",
    lines: [
      {
        id: "l1",
        service_id: "s1",
        service_name: "Gội đầu",
        category_name: "Gội đầu dưỡng sinh",
        source: "package",
        customer_package_id: "cp1",
        customer_package_item_id: "i1",
        package_name: "Gội đầu 10 + tặng 2",
        sessions_left_after: 11,
        sessions_total: 12,
        staff_id: "st1",
        staff_name: "KTV Lan",
        qty: 1,
        price_variant: null,
        unit_price: 150_000,
        line_value: 150_000,
        discount_amount: 0,
        expiry_override_reason: null,
      },
    ],
    ...extra,
  };
}

interface Call {
  method: string;
  url: string;
  body: unknown;
  headers: Record<string, string>;
}

type Handler = (call: Call) => Response | undefined;

function serve(permissions: string[], handler: Handler = () => undefined) {
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
    const custom = handler(call);
    if (custom) return Promise.resolve(custom);
    const json = (v: unknown) => Promise.resolve(Response.json(v));
    if (url === "/api/v1/me") return json(me(permissions));
    if (url === "/api/v1/customers/c1") return json(HA);
    if (url === "/api/v1/customers/c1/packages") return json([pkg()]);
    if (url === "/api/v1/staff") return json([]);
    if (url === "/api/v1/services") return json([]);
    if (url === "/api/v1/service-categories") return json([]);
    if (url.startsWith("/api/v1/visits") && call.method === "GET")
      return json([visit()]);
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

const problem = (status: number, code: string, errors: unknown[] = []) =>
  Response.json({ status, code, errors }, { status });

const at = (path: string) =>
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

const posts = (calls: Call[], path: string) =>
  calls.filter((c) => c.method === "POST" && c.url === `/api/v1${path}`);

describe("correction helpers", () => {
  it("adds months without spilling into the next month", () => {
    expect(addMonths("2026-01-31", 1)).toBe("2026-02-28");
    expect(addMonths("2026-11-04", 2)).toBe("2027-01-04");
    expect(addMonths("bad", 1)).toBe("bad");
  });

  it("does not rank technicians by voided visits", () => {
    const staff = [
      { id: "st1", full_name: "KTV Lan", position: "KTV", status: "active" },
      { id: "st2", full_name: "KTV Mai", position: "KTV", status: "active" },
    ] as const;
    const mai = visit({
      lines: visit().lines.map((l) => ({ ...l, staff_id: "st2" })),
    });
    const voided = [visit({ status: "voided" }), visit({ status: "voided" })];
    expect(staffByUse([...staff], [mai, ...voided]).map((s) => s.id)).toEqual([
      "st2",
      "st1",
    ]);
  });

  it("knows an unused package and the spa's day", () => {
    expect(isUnused(pkg(12))).toBe(true);
    expect(isUnused(pkg(11))).toBe(false);
    // 2026-11-04 20:00 UTC is already the 5th in Viet Nam.
    expect(spaToday(new Date("2026-11-04T20:00:00Z"))).toBe("2026-11-05");
  });
});

describe("package actions on the customer profile", () => {
  it("shows owners every action and managers only the sale void", async () => {
    serve(MANAGER);
    at("/khach-hang/c1");
    expect(
      await screen.findByRole("button", { name: "Hủy hóa đơn bán gói" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Gia hạn" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Hủy gói và hoàn tiền" }),
    ).toBeNull();
  });

  it("offers no sale void once a session was used", async () => {
    serve(OWNER, (c) =>
      c.url === "/api/v1/customers/c1/packages"
        ? Response.json([pkg(11)])
        : undefined,
    );
    at("/khach-hang/c1");
    expect(
      await screen.findByRole("button", { name: "Gia hạn" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Hủy hóa đơn bán gói" }),
    ).toBeNull();
  });

  it("shows a cancelled package with its reason and refund, without actions", async () => {
    serve(OWNER, (c) =>
      c.url === "/api/v1/customers/c1/packages"
        ? Response.json([
            pkg(0, {
              status: "cancelled",
              cancelled_at: "2026-11-04T03:00:00Z",
              cancel_reason: "Khách chuyển nhà",
              refund: {
                id: "r1",
                amount: 300_000,
                method: "cash",
                business_date: "2026-11-04",
                created_at: "2026-11-04T03:00:00Z",
              },
            }),
          ])
        : undefined,
    );
    at("/khach-hang/c1");
    expect(
      await screen.findByText(/Đã hủy: Khách chuyển nhà · Hoàn 300\.000/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Gia hạn" })).toBeNull();
  });

  it("extends with a reason, If-Match and one idempotency key", async () => {
    const calls = serve(OWNER, (c) =>
      c.url === "/api/v1/customer-packages/cp1/extend"
        ? Response.json(pkg(12, { expires_on: "2027-07-04", version: 4 }))
        : undefined,
    );
    const user = userEvent.setup();
    at("/khach-hang/c1");
    await user.click(await screen.findByRole("button", { name: "Gia hạn" }));
    const dialog = await screen.findByRole("dialog", { name: /Gia hạn/ });
    await user.click(within(dialog).getByRole("button", { name: "+2 tháng" }));
    await user.click(within(dialog).getByRole("button", { name: "Gia hạn" }));
    expect(within(dialog).getByText("Bắt buộc ghi lý do.")).toBeInTheDocument();
    expect(posts(calls, "/customer-packages/cp1/extend")).toHaveLength(0);

    await user.type(
      within(dialog).getByLabelText("Lý do gia hạn"),
      "Khách nằm viện",
    );
    await user.click(within(dialog).getByRole("button", { name: "Gia hạn" }));
    expect(
      await screen.findByText(
        'Đã gia hạn "Gội đầu 10 + tặng 2" tới 04/07/2027.',
      ),
    ).toBeInTheDocument();
    const [sent] = posts(calls, "/customer-packages/cp1/extend");
    expect(sent?.body).toEqual({
      expires_on: "2027-07-04",
      reason: "Khách nằm viện",
    });
    expect(sent?.headers["If-Match"]).toBe('"3"');
    expect(sent?.headers["Idempotency-Key"]).toMatch(/^[0-9a-f-]{36}$/);
  });

  it("cancels with a refund in one request, without asking the owner for the password", async () => {
    const calls = serve(OWNER, (c) =>
      c.url === "/api/v1/customer-packages/cp1/cancel"
        ? Response.json(
            pkg(0, {
              status: "cancelled",
              refund: {
                id: "r1",
                amount: 300_000,
                method: "cash",
                business_date: "2026-11-04",
                created_at: "2026-11-04T03:00:00Z",
              },
            }),
          )
        : undefined,
    );
    const user = userEvent.setup();
    at("/khach-hang/c1");
    await user.click(
      await screen.findByRole("button", { name: "Hủy gói và hoàn tiền" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: /Hủy gói/,
    });
    const amount = within(dialog).getByLabelText(/Số tiền hoàn/);
    await user.type(amount, "1.600.000");
    expect(
      within(dialog).getByText(/Không hoàn quá số khách đã trả/),
    ).toBeInTheDocument();
    await user.clear(amount);
    await user.type(amount, "300.000");
    expect(within(dialog).queryByRole("checkbox")).not.toBeInTheDocument();
    await user.type(
      within(dialog).getByLabelText("Lý do hủy"),
      "Khách chuyển nhà",
    );
    await user.click(
      within(dialog).getByRole("button", { name: /Hủy và hoàn 300\.000/ }),
    );

    expect(
      await screen.findByText(/Đã hủy "Gội đầu 10 \+ tặng 2" và hoàn 300\.000/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("dialog", { name: "Xác nhận lại mật khẩu" }),
    ).not.toBeInTheDocument();
    const cancels = posts(calls, "/customer-packages/cp1/cancel");
    expect(cancels).toHaveLength(1);
    expect(cancels[0]?.headers["Idempotency-Key"]).toMatch(/^[0-9a-f-]{36}$/);
    expect(cancels[0]?.body).toEqual({
      reason: "Khách chuyển nhà",
      refund_amount: 300_000,
      refund_method: "cash",
    });
    expect(posts(calls, "/auth/reauth")).toHaveLength(0);
  });

  it("shows the server's 409 when the sale's sessions were used meanwhile", async () => {
    serve(MANAGER, (c) =>
      c.url === "/api/v1/sales/sale1/void"
        ? problem(409, "sale.has_used_sessions")
        : undefined,
    );
    const user = userEvent.setup();
    at("/khach-hang/c1");
    await user.click(
      await screen.findByRole("button", { name: "Hủy hóa đơn bán gói" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: /Hủy hóa đơn bán/,
    });
    await user.type(within(dialog).getByLabelText("Lý do hủy"), "Bán nhầm gói");
    await user.click(
      within(dialog).getByRole("button", { name: "Hủy hóa đơn" }),
    );
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Hủy gói và hoàn tiền",
    );
  });
});

describe("voids in the /quay visit list", () => {
  it("voids a visit with a reason and shows it as voided", async () => {
    let voided = false;
    const calls = serve(MANAGER, (c) => {
      if (c.url === "/api/v1/visits/v1/void") {
        voided = true;
        return Response.json(
          visit({
            status: "voided",
            void_reason: "Ghi nhầm khách",
            voided_at: "2026-11-04T03:00:00Z",
          }),
        );
      }
      if (c.url.startsWith("/api/v1/visits") && c.method === "GET")
        return Response.json([
          voided
            ? visit({ status: "voided", void_reason: "Ghi nhầm khách" })
            : visit(),
        ]);
      return undefined;
    });
    const user = userEvent.setup();
    at("/quay");
    const list = await screen.findByRole("list", {
      name: "Lượt khách hôm nay",
    });
    await user.click(
      await within(list).findByRole("button", {
        name: /Hủy lượt .* của Nguyễn Thị Hà/,
      }),
    );
    const dialog = await screen.findByRole("dialog", { name: /Hủy lượt/ });
    await waitFor(() => {
      expect(within(dialog).getByLabelText("Lý do hủy")).toHaveFocus();
    });
    await user.click(within(dialog).getByRole("button", { name: "Hủy lượt" }));
    expect(within(dialog).getByText("Bắt buộc ghi lý do.")).toBeInTheDocument();
    expect(posts(calls, "/visits/v1/void")).toHaveLength(0);

    await user.type(
      within(dialog).getByLabelText("Lý do hủy"),
      "Ghi nhầm khách",
    );
    await user.click(within(dialog).getByRole("button", { name: "Hủy lượt" }));
    expect(
      await screen.findByText("Đã hủy lượt của Nguyễn Thị Hà."),
    ).toBeInTheDocument();
    expect(
      await within(list).findByText("Lý do hủy: Ghi nhầm khách"),
    ).toBeInTheDocument();
    expect(within(list).getByText("Đã hủy")).toBeInTheDocument();
    expect(within(list).queryByRole("button", { name: /Hủy lượt/ })).toBeNull();
    expect(posts(calls, "/visits/v1/void")[0]?.body).toEqual({
      reason: "Ghi nhầm khách",
    });
  });

  it("hides the void of a closed day from managers", async () => {
    serve(MANAGER);
    at("/quay");
    await screen.findByRole("list", { name: "Lượt khách hôm nay" });
    // DateField: always dd/mm/yyyy (round 4), committed on blur.
    const dayField = screen.getByLabelText("Xem ngày");
    fireEvent.change(dayField, { target: { value: "01/11/2026" } });
    fireEvent.blur(dayField);
    expect(
      await screen.findByText(
        "Ngày đã chốt: chỉ chủ spa hủy được lượt của ngày này.",
      ),
    ).toBeInTheDocument();
    const list = await screen.findByRole("list", {
      name: "Lượt khách trong ngày",
    });
    expect(within(list).queryByRole("button", { name: /Hủy lượt/ })).toBeNull();
  });
});
