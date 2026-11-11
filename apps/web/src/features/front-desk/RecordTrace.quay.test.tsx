// "Ai ghi, ai sửa" at Quầy (ADR-0022, design record-history §2.1–2.2): the one chip of a visit,
// the history as the last element of an opened visit, and "Xem ai ghi, ai sửa" of a package sale.
import type { DaySale, RecordHistory, Visit } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { spaToday } from "../../lib/format";

const DESK = [
  "customer.read",
  "customer.health_notes.read",
  "visit.record",
  "visit.read",
  "payment.collect",
  "catalog.read",
  "staff.read_basic",
  "sale.read",
];
/** The owner's right by default; a manager has it only when the operator switches it on. */
const OWNER = [...DESK, "record.history.read"];

const today = spaToday();
const visit = (over: Partial<Visit>): Visit => ({
  companions: [],
  package_sales: [],
  id: "v1",
  customer_id: "c1",
  customer_name: "Khách Thường",
  business_date: today,
  check_in_at: "2026-11-05T02:12:00Z",
  status: "completed",
  sale_id: null,
  subtotal: 150_000,
  discount_amount: 0,
  discount_reason: null,
  total: 150_000,
  payments: [],
  version: 1,
  created_at: "2026-11-05T02:12:00Z",
  lines: [
    {
      id: "l1",
      service_id: "s1",
      service_name: "Gội đầu",
      category_name: "Gội đầu dưỡng sinh",
      source: "one_off",
      customer_package_id: null,
      customer_package_item_id: null,
      package_name: null,
      sessions_left_after: null,
      sessions_total: null,
      staff_id: "st2",
      staff_name: "KTV Hoa",
      qty: 1,
      price_variant: null,
      unit_price: 150_000,
      line_value: 150_000,
      discount_amount: 0,
      expiry_override_reason: null,
    },
  ],
  ...over,
});

/** A "Sửa lượt" pair (the old one replaced, the new one edited), an untouched visit, a back-filled one. */
const REPLACED = visit({ id: "va", customer_name: "Khách Thay", status: "voided", corrected_by_visit_id: "vb", void_reason: "Sửa lượt: nhầm KTV", edited: true });
const CORRECTED = visit({ id: "vb", customer_name: "Khách Mới", corrects_visit_id: "va", edited: true });
const PLAIN = visit({ id: "vc", customer_name: "Khách Thường", edited: false });
const BACKFILLED = visit({
  id: "vd",
  customer_name: "Khách Nhập Bù",
  business_date: "2026-11-07",
  check_in_at: "2026-11-07T08:00:00Z",
  created_at: "2026-11-08T12:27:27Z",
});

const HA = { kind: "user" as const, user_id: "u9", name: "Hà" };
function historyOf(id: string): RecordHistory {
  const v = [REPLACED, CORRECTED, PLAIN, BACKFILLED].find((x) => x.id === id);
  const at = v?.created_at ?? "2026-11-05T02:12:00Z";
  const day = id === "vd" ? "2026-11-08" : today;
  return {
    record: { type: "visit", id },
    summary: { created: { actor: HA, at, day, source: "audit" }, edits: id === "vc" || id === "vd" ? 0 : 1, state: "active" },
    items: [
      {
        id: `a-${id}`,
        action: "visit.create",
        kind: "front_desk",
        at,
        day,
        actor: HA,
        changes: [],
        packages: [],
        folded: [],
        detail: id === "vd" ? "backdated" : null,
        amount: 150_000,
      },
    ],
    omitted: 0,
  };
}

const SALE: DaySale = {
  customer_name: "Khách Mua Gói",
  customer_gender: "female",
  sale: {
    id: "s1",
    customer_id: "c2",
    business_date: today,
    subtotal: 1_500_000,
    discount_amount: 0,
    total: 1_500_000,
    amount_paid: 1_500_000,
    status: "open",
    closer_name: "KTV Lan",
    created_at: "2026-11-05T03:30:00Z",
    version: 2,
    lines: [{ id: "sl1", line_type: "package", description: "Gội đầu 10 buổi", qty: 1, unit_price: 1_500_000, line_total: 1_500_000 }],
    payments: [],
    customer_packages: [],
  },
};

function serve(permissions: string[], visits: Visit[], sales: DaySale[] = []) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const json = (v: unknown, status = 200) => Promise.resolve(Response.json(v, { status }));
    if (url === "/api/v1/me")
      return json({
        user: { id: "u1", full_name: "Chủ Spa", phone: "0905 123 456" },
        active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
        tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
        permissions,
      });
    if (url.startsWith("/api/v1/visits")) return json(visits);
    if (url.startsWith("/api/v1/sales")) return json(sales);
    if (url === "/api/v1/staff") return json([]);
    if (url.startsWith("/api/v1/customers/c")) return json({ id: "c1", full_name: "Khách", phone: null, named: true, version: 1 });
    const trace = /^\/api\/v1\/record-history\/(visit|sale)\/(.+)$/.exec(url);
    if (trace) {
      if (!permissions.includes("record.history.read")) return json({ code: "auth.forbidden" }, 403);
      return json(trace[1] === "sale" ? { ...historyOf("vc"), record: { type: "customer_package", id: "cp9" } } : historyOf(trace[2] ?? ""));
    }
    return json([]);
  });
}

const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);
const rowOf = (list: HTMLElement, name: string) => within(list).getByRole("button", { name: new RegExp(`^${name}`) }).closest("tr") as HTMLElement;

describe("/quay: ai ghi, ai sửa (ADR-0022)", () => {
  it("one chip a visit: the replaced one 'Đã thay', the edited one 'Đã sửa' (owner), the history last in the opened visit", async () => {
    const fetch = serve(OWNER, [REPLACED, CORRECTED, PLAIN]);
    const user = userEvent.setup();
    at("/quay");
    const list = await screen.findByRole("table", { name: "Lượt khách hôm nay" });
    const replaced = rowOf(list, "Khách Thay");
    expect(within(replaced).getByText("Đã thay")).toBeInTheDocument();
    expect(within(replaced).queryByText("Đã sửa")).toBeNull();
    expect(within(replaced).queryByText("Đã hủy")).toBeNull();
    expect(within(rowOf(list, "Khách Mới")).getByText("Đã sửa")).toBeInTheDocument();
    expect(within(rowOf(list, "Khách Thường")).queryByText(/^Đã (sửa|thay|hủy)$/)).toBeNull();

    await user.click(within(list).getByRole("button", { name: /^Khách Mới/ }));
    const details = await screen.findByTestId("visit-details");
    const trace = await within(details).findByRole("button", { name: "Ghi bởi Hà · 09:12 · đã sửa 1 lần" });
    // The last element of the details (design §1.2).
    expect(details.lastElementChild?.contains(trace)).toBe(true);
    expect(fetch).toHaveBeenCalledWith("/api/v1/record-history/visit/vb", expect.anything());
    await user.click(trace);
    expect(within(details).getByText("Hà ghi lượt · Thu 150.000đ")).toBeInTheDocument();
    // A click in the history is no click on the row: the visit stays open.
    await user.click(within(details).getByText("Hà ghi lượt · Thu 150.000đ"));
    expect(screen.getByTestId("visit-details")).toBeInTheDocument();
  });

  it("the replaced visit reads 'Đã thay bằng lượt mới.' with the same story", async () => {
    serve(OWNER, [REPLACED]);
    const user = userEvent.setup();
    at("/quay");
    const list = await screen.findByRole("table", { name: "Lượt khách hôm nay" });
    await user.click(within(list).getByRole("button", { name: /^Khách Thay/ }));
    const details = await screen.findByTestId("visit-details");
    expect(within(details).getByText("Đã thay bằng lượt mới. Sửa lượt: nhầm KTV")).toBeInTheDocument();
    expect(await within(details).findByRole("button", { name: /^Ghi bởi Hà/ })).toBeInTheDocument();
  });

  it("a back-filled visit: the meta line says when it was recorded, so 'Nhập bù · Ghi lúc' is not repeated", async () => {
    serve(OWNER, [BACKFILLED]);
    const user = userEvent.setup();
    at("/quay");
    await user.click(await screen.findByRole("button", { name: /^Khách Nhập Bù/ }));
    const details = await screen.findByTestId("visit-details");
    expect(await within(details).findByRole("button", { name: "Ghi bởi Hà · 19:27 08/11" })).toBeInTheDocument();
    expect(within(details).queryByText(/^Ghi lúc/)).toBeNull();
  });

  it("without the history right (a manager by default): no 'Đã sửa', no history, nothing asked; 'Đã thay' and 'Ghi lúc' stay", async () => {
    // The server sends no `edited` to them (AC-H8).
    const strip = (v: Visit): Visit => {
      const rest = { ...v };
      delete rest.edited;
      return rest;
    };
    const fetch = serve(DESK, [REPLACED, CORRECTED, BACKFILLED].map(strip));
    const user = userEvent.setup();
    at("/quay");
    const list = await screen.findByRole("table", { name: "Lượt khách hôm nay" });
    expect(within(rowOf(list, "Khách Thay")).getByText("Đã thay")).toBeInTheDocument();
    expect(within(rowOf(list, "Khách Mới")).queryByText(/^Đã (sửa|thay|hủy)$/)).toBeNull();
    await user.click(within(list).getByRole("button", { name: /^Khách Nhập Bù/ }));
    const details = await screen.findByTestId("visit-details");
    expect(within(details).getByText("Ghi lúc 19:27 08/11/2026")).toBeInTheDocument();
    expect(within(details).queryByRole("button", { name: /^Ghi bởi|^Đang tải ai ghi/ })).toBeNull();
    expect(fetch.mock.calls.some(([u]) => typeof u === "string" && u.includes("/record-history/"))).toBe(false);
  });

  it("a package sale without a visit: ⋯ 'Xem ai ghi, ai sửa' opens its history under the row, focused", async () => {
    const fetch = serve(OWNER, [], [SALE]);
    const user = userEvent.setup();
    at("/quay");
    const table = await screen.findByRole("table", { name: "Bán gói liệu trình không kèm lượt khách" });
    // A package's lifecycle is no edit (D9): no chip on the row.
    expect(within(table).queryByText("Đã sửa")).toBeNull();
    await user.click(within(table).getByRole("button", { name: /^Thao tác hóa đơn 10:30/ }));
    await user.click(screen.getByRole("menuitem", { name: "Xem ai ghi, ai sửa" }));
    const trace = await within(table).findByRole("button", { name: /^Ghi bởi Hà/ });
    expect(trace).toHaveAttribute("aria-expanded", "true");
    await waitFor(() => {
      expect(trace).toHaveFocus();
    });
    expect(fetch).toHaveBeenCalledWith("/api/v1/record-history/sale/s1", expect.anything());
    await user.click(within(table).getByRole("button", { name: /^Thao tác hóa đơn 10:30/ }));
    await user.click(screen.getByRole("menuitem", { name: "Ẩn ai ghi, ai sửa" }));
    expect(within(table).queryByRole("button", { name: /^Ghi bởi/ })).toBeNull();
  });

  it("a manager without the right gets no such menu item", async () => {
    serve(DESK, [], [SALE]);
    const user = userEvent.setup();
    at("/quay");
    const table = await screen.findByRole("table", { name: "Bán gói liệu trình không kèm lượt khách" });
    await user.click(within(table).getByRole("button", { name: /^Thao tác hóa đơn 10:30/ }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /ai ghi, ai sửa/ })).toBeNull();
  });
});
