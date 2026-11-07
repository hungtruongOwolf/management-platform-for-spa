// "Sửa lượt" (spec R1-1 US6b): how the cart decides between a direct edit and a correction, and
// the edit flow on /quay end to end against a mocked API.
import type { Service, Staff, Visit } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { mayCorrectVisit, spaToday } from "../corrections/api";
import {
  cartFromVisit,
  differenceText,
  paymentModeOf,
  planCorrection,
  type CorrectionDraft,
} from "./correction";

const GAC_HOI: Service = {
  id: "s10",
  category_id: "cat1",
  name: "Giác hơi",
  description: null,
  duration_min: 30,
  recommended: false,
  active: true,
  version: 1,
  prices: { all: 100_000 },
};
const DAI_THONG_LAC: Service = {
  ...GAC_HOI,
  id: "s5",
  name: "Đại Thông Lạc",
  recommended: true,
  prices: { female: 349_000, male: 399_000 },
};

const VISIT: Visit = {
  companions: [],
  package_sales: [],
  id: "v1",
  customer_id: "c1",
  customer_name: "Nguyễn Thị Hà",
  customer_gender: "female",
  business_date: spaToday(),
  check_in_at: "2026-11-05T02:12:30Z",
  status: "completed",
  sale_id: "sale9",
  subtotal: 100_000,
  discount_amount: 0,
  discount_reason: null,
  total: 100_000,
  payments: [
    {
      id: "p1",
      kind: "payment",
      method: "cash",
      amount: 100_000,
      paid_at: "2026-11-05T02:13:00Z",
      reversal_of: null,
    },
  ],
  version: 1,
  created_at: "2026-11-05T02:13:00Z",
  lines: [
    {
      id: "l1",
      service_id: "s10",
      service_name: "Giác hơi",
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
      price_variant: "all",
      unit_price: 100_000,
      line_value: 100_000,
      discount_amount: 0,
      expiry_override_reason: null,
    },
  ],
};

function first<T>(xs: T[]): T {
  const x = xs[0];
  if (x === undefined) throw new Error("empty list");
  return x;
}

function draft(over: Partial<CorrectionDraft> = {}): CorrectionDraft {
  return {
    customerId: "c1",
    lines: cartFromVisit(VISIT),
    payments: [{ method: "cash", amount: 100_000 }],
    discount: null,
    checkInAt: "2026-11-05T02:12:00Z",
    note: "",
    canWriteNote: true,
    reason: "",
    ...over,
  };
}

describe("Sửa lượt: how the cart is saved", () => {
  it("opens the visit as it was recorded", () => {
    const lines = cartFromVisit({
      ...VISIT,
      lines: [{ ...first(VISIT.lines), qty: 2 }],
    });
    expect(lines).toHaveLength(2);
    expect(lines[0]).toMatchObject({
      source: "one_off",
      price: 100_000,
      staffId: "st1",
      priceVariant: null,
    });
    expect(paymentModeOf(VISIT)).toEqual({ mode: "cash", cashText: "" });
  });

  it("saves nothing when nothing changed (seconds of the recorded time ignored)", () => {
    expect(planCorrection(VISIT, "", draft())).toEqual({ kind: "none" });
  });

  it("edits technician, time and note in place, without money", () => {
    const line = first(cartFromVisit(VISIT));
    const plan = planCorrection(
      VISIT,
      "",
      draft({
        lines: [{ ...line, staffId: "st2" }],
        checkInAt: "2026-11-05T03:00:00Z",
        note: " lực nhẹ ",
      }),
    );
    expect(plan).toEqual({
      kind: "direct",
      patch: {
        lines: [{ id: "l1", staff_id: "st2" }],
        check_in_at: "2026-11-05T03:00:00Z",
        note: "lực nhẹ",
      },
    });
  });

  it("corrects anything that touches money: service, payment method, customer", () => {
    const dearer = planCorrection(
      VISIT,
      "old",
      draft({
        lines: [
          {
            key: 1,
            source: "one_off",
            serviceId: "s5",
            serviceName: "Đại Thông Lạc",
            priceVariant: null,
            price: 349_000,
            staffId: "st1",
          },
        ],
        payments: [{ method: "cash", amount: 349_000 }],
        note: "old",
        reason: " chọn nhầm ",
      }),
    );
    expect(dearer.kind).toBe("correct");
    if (dearer.kind !== "correct") return;
    expect(dearer.difference).toBe(249_000);
    expect(dearer.body.reason).toBe("chọn nhầm");
    // The note is unchanged: absent, so the server keeps it without it ever reaching the browser.
    expect("note" in dearer.body).toBe(false);
    expect(dearer.body.lines).toEqual([
      {
        service_id: "s5",
        source: "one_off",
        staff_id: "st1",
        price_variant: null,
      },
    ]);

    const transfer = planCorrection(
      VISIT,
      "",
      draft({ payments: [{ method: "bank_transfer", amount: 100_000 }] }),
    );
    expect(transfer).toMatchObject({ kind: "correct", difference: 0 });
    expect(planCorrection(VISIT, "", draft({ customerId: "c2" })).kind).toBe(
      "correct",
    );
    expect(planCorrection(VISIT, "", draft({ lines: [] })).kind).toBe(
      "correct",
    );
  });

  it("says the difference in money words", () => {
    expect(differenceText(50_000)).toBe("Thu thêm 50.000đ");
    expect(differenceText(-50_000)).toBe("Hoàn 50.000đ");
    expect(differenceText(0)).toBe("Không đổi tiền");
  });

  it("offers Sửa lượt only where the server would allow it", () => {
    const today = spaToday();
    const manager = ["visit.record", "record.void_same_day"];
    const owner = [...manager, "record.void_closed_day"];
    expect(
      mayCorrectVisit(
        manager,
        { business_date: today, status: "completed" },
        today,
      ),
    ).toBe(true);
    expect(
      mayCorrectVisit(
        manager,
        { business_date: "2020-01-01", status: "completed" },
        today,
      ),
    ).toBe(false);
    expect(
      mayCorrectVisit(
        owner,
        { business_date: "2020-01-01", status: "completed" },
        today,
      ),
    ).toBe(true);
    expect(
      mayCorrectVisit(owner, { business_date: today, status: "voided" }, today),
    ).toBe(false);
    expect(
      mayCorrectVisit(
        ["visit.record"],
        { business_date: today, status: "completed" },
        today,
      ),
    ).toBe(false);
    expect(
      mayCorrectVisit([], { business_date: today, voided: false }, today),
    ).toBe(false);
  });
});

// ------------------------------------------------------------------ screen

const PERMISSIONS = [
  "customer.read",
  "customer.write",
  "customer.health_notes.read",
  "customer.health_notes.write",
  "visit.record",
  "visit.read",
  "payment.collect",
  "catalog.read",
  "staff.read_basic",
  "record.void_same_day",
];

const staff = (id: string, name: string): Staff => ({
  id,
  full_name: name,
  position: "KTV",
  status: "active",
});

interface Call {
  method: string;
  url: string;
  body: unknown;
  headers: Record<string, string>;
}

function serve() {
  const calls: Call[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    const method = init?.method ?? "GET";
    const headers = (init?.headers ?? {}) as Record<string, string>;
    const body =
      typeof init?.body === "string"
        ? (JSON.parse(init.body) as unknown)
        : undefined;
    calls.push({ method, url, body, headers });
    const json = (v: unknown, status = 200) =>
      Promise.resolve(Response.json(v, { status }));
    if (url === "/api/v1/me")
      return json({
        user: { id: "u1", full_name: "Quản lý Ngọc", phone: "0905 123 456" },
        active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
        tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
        permissions: PERMISSIONS,
      });
    if (url === "/api/v1/customers/c1")
      return json({
        id: "c1",
        full_name: "Nguyễn Thị Hà",
        phone: "0905 111 222",
        named: true,
        phone_e164: "+84905111222",
        gender: "female",
        birth_year: null,
        address: null,
        source: null,
        note: null,
        health_note: null,
        version: 1,
        created_at: "2026-11-01T00:00:00Z",
      });
    if (url === "/api/v1/customers/c1/packages") return json([]);
    if (url === "/api/v1/staff")
      return json([staff("st1", "KTV Lan"), staff("st2", "KTV Hoa")]);
    if (url === "/api/v1/services") return json([GAC_HOI, DAI_THONG_LAC]);
    if (url === "/api/v1/service-categories")
      return json([{ id: "cat1", name: "Cổ vai gáy", sort_order: 0 }]);
    if (url === "/api/v1/visits/v1" && method === "GET")
      return json({ ...VISIT, note: "Đau vai" });
    if (url === "/api/v1/visits/v1" && method === "PATCH")
      return json({
        ...VISIT,
        version: 2,
        lines: [
          { ...first(VISIT.lines), staff_id: "st2", staff_name: "KTV Hoa" },
        ],
      });
    if (url === "/api/v1/visits/v1/correct")
      return json(
        {
          voided: { ...VISIT, status: "voided", corrected_by_visit_id: "v2" },
          visit: {
            ...VISIT,
            id: "v2",
            total: 349_000,
            corrects_visit_id: "v1",
          },
          difference: 249_000,
        },
        201,
      );
    if (url.startsWith("/api/v1/visits")) return json([VISIT]);
    return Promise.resolve(new Response(null, { status: 404 }));
  });
  return calls;
}

async function openCorrection() {
  const user = userEvent.setup();
  render(<App history={createMemoryHistory({ initialEntries: ["/quay"] })} />);
  const table = await screen.findByRole("table", {
    name: "Lượt khách hôm nay",
  });
  const more = within(table).getByRole("button", {
    name: /^Thao tác lượt .* của Nguyễn Thị Hà$/,
  });
  await user.click(more);
  const menu = screen.getByRole("menu");
  expect(
    within(menu)
      .getAllByRole("menuitem")
      .map((i) => i.textContent),
  ).toEqual([
    "Mở hồ sơ khách",
    "Sửa lượt",
    "Thêm dịch vụ / bán gói",
    "Hủy lượt",
  ]);
  await user.click(within(menu).getByRole("menuitem", { name: "Sửa lượt" }));
  await screen.findByRole("heading", { name: "Sửa lượt khách" });
  return user;
}

describe("/quay Sửa lượt", () => {
  it("changes only the technician with a direct edit (If-Match, no money)", async () => {
    const calls = serve();
    const user = await openCorrection();
    const cart = await screen.findByRole("list", { name: "Dịch vụ trong lượt" });
    expect(within(cart).getByText(/Giác hơi/)).toBeInTheDocument();
    // The note was read with the visit and is in the cart.
    expect(screen.getByLabelText("Ghi chú buổi (không bắt buộc)")).toHaveValue(
      "Đau vai",
    );
    expect(
      await screen.findByText("Chưa có gì thay đổi so với lượt đã ghi."),
    ).toBeInTheDocument();
    // The recorded line is complete, so it starts folded (EM 2026-11-07): Sửa opens it.
    await user.click(within(cart).getByRole("button", { name: "Sửa dòng Giác hơi" }));
    await user.selectOptions(
      within(cart).getByRole("combobox", { name: "KTV" }),
      "st2",
    );
    expect(
      await screen.findByText(/Chỉ đổi KTV, giờ làm hoặc ghi chú/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Lưu sửa lượt" }));
    await waitFor(() => {
      expect(calls.some((c) => c.method === "PATCH")).toBe(true);
    });
    const patch = calls.find((c) => c.method === "PATCH");
    expect(patch?.url).toBe("/api/v1/visits/v1");
    expect(patch?.headers["If-Match"]).toBe('"1"');
    expect(patch?.body).toEqual({ lines: [{ id: "l1", staff_id: "st2" }] });
    expect(
      await screen.findByText(/Đã sửa lượt của Nguyễn Thị Hà · không đổi tiền/),
    ).toBeInTheDocument();
    expect(calls.some((c) => c.url.endsWith("/correct"))).toBe(false);
  });

  it("corrects a wrong service: shows the difference, needs a reason, one keyed request", async () => {
    const calls = serve();
    const user = await openCorrection();
    await user.click(screen.getByRole("button", { name: "Bỏ dòng Giác hơi" }));
    // Picking is adding: no separate "Thêm".
    await user.selectOptions(screen.getByLabelText("Thêm dịch vụ lẻ"), "s5");
    const cart = screen.getByRole("list", { name: "Dịch vụ trong lượt" });
    await user.selectOptions(
      within(cart).getByRole("combobox", { name: "KTV" }),
      "st1",
    );
    expect(
      await screen.findByTestId("correction-difference"),
    ).toHaveTextContent(/Thu thêm\s*249\.000đ/);
    // The amount to collect is the difference; the new total is only context (EM live test).
    expect(
      screen.getByText("Tổng lượt sau sửa 349.000đ · đã thu 100.000đ"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "Lưu lượt" })).queryByText(
        "Phải trả",
      ),
    ).toBeNull();

    await user.click(screen.getByRole("button", { name: "Lưu sửa lượt" }));
    expect(
      await screen.findByText("Cần ghi lý do sửa lượt."),
    ).toBeInTheDocument();
    expect(calls.some((c) => c.url.endsWith("/correct"))).toBe(false);

    const reason = screen.getByRole("textbox", {
      name: /^Lý do sửa \(bắt buộc\)/,
    });
    expect(reason).toHaveAttribute("aria-invalid", "true");
    await user.type(reason, "chọn nhầm dịch vụ");
    await user.click(screen.getByRole("button", { name: "Lưu sửa lượt" }));
    await waitFor(() => {
      expect(calls.some((c) => c.url.endsWith("/correct"))).toBe(true);
    });
    const post = calls.find((c) => c.url.endsWith("/correct"));
    expect(post?.method).toBe("POST");
    expect(post?.headers["If-Match"]).toBe('"1"');
    expect(post?.headers["Idempotency-Key"]).toMatch(/^[0-9a-f-]{36}$/);
    expect(post?.body).toMatchObject({
      reason: "chọn nhầm dịch vụ",
      customer_id: "c1",
      lines: [{ service_id: "s5", source: "one_off", staff_id: "st1" }],
      payments: [{ method: "cash", amount: 349_000 }],
    });
    expect(post?.body).not.toHaveProperty("note");
    expect(
      await screen.findByText(
        /Đã sửa lượt của Nguyễn Thị Hà · Thu thêm 249.000đ/,
      ),
    ).toBeInTheDocument();
  });
});
