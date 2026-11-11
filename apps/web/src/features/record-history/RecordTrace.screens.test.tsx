// "Ai ghi, ai sửa" on the management screens (ADR-0022, design record-history §2.3–2.6): Sổ doanh
// thu (time → sheet), Lịch sử khách (time → history at the card's foot), the customer's packages
// ("Lịch sử") and profile ("Tạo bởi …"). The API is the contract-shaped fake of reports/testServer.
import type { CustomerPackage, HistoryEvent, LedgerRow } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { spaToday } from "../../lib/format";
import { relativeDay } from "../reports/api";
import { serveManagement } from "../reports/testServer";

const MANAGER = ["visit.record", "visit.read", "customer.read", "catalog.read", "report.read", "report.revenue.read", "sale.read"];
const OWNER = [...MANAGER, "record.history.read"];
const today = spaToday();

const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);
const urls = (spy: ReturnType<typeof serveManagement>) =>
  spy.mock.calls.map(([input]) => (typeof input === "string" ? input : input instanceof URL ? input.href : input.url));

afterEach(() => {
  vi.unstubAllGlobals();
});

const row = (over: Partial<LedgerRow> & Pick<LedgerRow, "id">): LedgerRow => ({
  at: `${today}T03:07:00Z`,
  business_date: today,
  customer_id: "c1",
  customer_name: "Lê Thị Mai",
  customer_gender: "female",
  entry: "sale",
  kind: "one_off",
  one_off: "Gội đầu dưỡng sinh",
  package: null,
  cash: 300_000,
  bank_transfer: 0,
  unpaid: 0,
  note: null,
  upsell_by: null,
  visit_id: "v1",
  voided: false,
  ...over,
});

/** A standing edited one-off sale, the void row of an edited visit, a package sale, a refund, a paper-book sale. */
const ROWS: LedgerRow[] = [
  row({ id: "r1", edited: true }),
  row({ id: "r2", entry: "void", at: `${today}T04:00:00Z`, cash: -300_000, edited: true, note: "Sửa lượt" }),
  row({ id: "r3", kind: "package", one_off: null, package: "Gội 10 buổi", at: `${today}T05:00:00Z`, cash: 1_500_000, visit_id: null }),
  row({ id: "r4", entry: "refund", kind: "package", one_off: null, package: "Gội 10 buổi", at: `${today}T06:00:00Z`, cash: -500_000, visit_id: null }),
  row({ id: "r5", customer_name: "Khách Sổ Giấy", at: `${today}T07:00:00Z`, visit_id: null }),
];
const ledger = { ledger: () => ({ from: today, to: today, days: [{ date: today, cash: 0, bank_transfer: 0, total: 0, unpaid: 0, entries: 5 }], rows: ROWS, next_cursor: null }) };

describe("Sổ doanh thu", () => {
  it("the time opens the row's history in a sheet, open from the start; 'Đã sửa' only on a standing one-off sale", async () => {
    const spy = serveManagement({ permissions: OWNER, ...ledger });
    const user = userEvent.setup();
    at("/bao-cao?tab=ledger");
    const table = await screen.findByRole("table", { name: /Sổ doanh thu/ });
    const edited = within(table).getAllByText("Đã sửa");
    expect(edited).toHaveLength(1);
    expect(edited[0]?.closest("tr")).toBe(within(table).getByRole("button", { name: "10:07, xem hóa đơn của chị Lê Thị Mai" }).closest("tr"));
    expect(within(table).getByRole("button", { name: "11:00, xem hủy hóa đơn của chị Lê Thị Mai" })).toBeInTheDocument();
    expect(within(table).getByRole("button", { name: "13:00, xem hoàn tiền của chị Lê Thị Mai" })).toBeInTheDocument();
    // A one-off sale outside any visit (a paper-book entry) has no history to open.
    expect(within(table).queryByRole("button", { name: /^14:00, xem/ })).toBeNull();

    await user.click(within(table).getByRole("button", { name: "10:07, xem hóa đơn của chị Lê Thị Mai" }));
    const sheet = await screen.findByRole("dialog", { name: "Hóa đơn dịch vụ · chị Lê Thị Mai" });
    const trace = await within(sheet).findByRole("button", { name: /^Ghi bởi Hà · / });
    expect(trace).toHaveAttribute("aria-expanded", "true");
    expect(within(sheet).getByText("Tiền mặt 300.000đ")).toBeInTheDocument();
    expect(urls(spy)).toContain("/api/v1/record-history/sale/r1");
    // The footer button (the header has its own "Đóng" icon).
    await user.click(within(sheet).getAllByRole("button", { name: "Đóng" }).at(-1) as HTMLElement);

    await user.click(within(table).getByRole("button", { name: "13:00, xem hoàn tiền của chị Lê Thị Mai" }));
    expect(await screen.findByRole("dialog", { name: "Hoàn tiền gói · chị Lê Thị Mai" })).toBeInTheDocument();
    await waitFor(() => {
      expect(urls(spy)).toContain("/api/v1/record-history/refund/r4");
    });
  });

  it("refused (the switch went off): the sheet closes and focus goes back to the row", async () => {
    serveManagement({ permissions: OWNER, ...ledger, recordHistory: () => ({ status: 403, json: { status: 403, code: "auth.forbidden" } }) });
    const user = userEvent.setup();
    at("/bao-cao?tab=ledger");
    const table = await screen.findByRole("table", { name: /Sổ doanh thu/ });
    const time = within(table).getByRole("button", { name: "10:07, xem hóa đơn của chị Lê Thị Mai" });
    await user.click(time);
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: /^Hóa đơn dịch vụ/ })).toBeNull();
    });
    await waitFor(() => {
      expect(within(time.closest("tr") as HTMLElement).getByRole("link", { name: /Lê Thị Mai/ })).toHaveFocus();
    });
  });

  it("phones: the card's time is a real button, a tap on the card opens the sheet too, the name still opens the profile", async () => {
    vi.stubGlobal(
      "matchMedia",
      (query: string) =>
        ({ matches: query.includes("max-width"), media: query, addEventListener: () => undefined, removeEventListener: () => undefined }) as unknown as MediaQueryList,
    );
    serveManagement({ permissions: OWNER, ...ledger });
    const user = userEvent.setup();
    at("/bao-cao?tab=ledger");
    const button = await screen.findByRole("button", { name: "13:00, xem hoàn tiền của chị Lê Thị Mai" });
    const card = button.closest("article") as HTMLElement;
    await user.click(within(card).getByText("Liệu trình"));
    expect(await screen.findByRole("dialog", { name: "Hoàn tiền gói · chị Lê Thị Mai" })).toBeInTheDocument();
  });

  it("without the history right: plain times, no sheet, nothing asked", async () => {
    const spy = serveManagement({ permissions: MANAGER, ...ledger });
    at("/bao-cao?tab=ledger");
    const table = await screen.findByRole("table", { name: /Sổ doanh thu/ });
    expect(within(table).getByText("10:07")).toBeInTheDocument();
    expect(within(table).queryByRole("button", { name: /, xem / })).toBeNull();
    expect(urls(spy).some((u) => u.includes("/record-history/"))).toBe(false);
  });
});

const visitEvent = (id: string, over: Partial<HistoryEvent> = {}): HistoryEvent => ({
  kind: "visit",
  at: `${today}T03:07:00Z`,
  business_date: today,
  visit: {
    id,
    voided: false,
    check_in_at: `${today}T03:07:00Z`,
    lines: [],
    paid: { cash: 0, bank_transfer: 0 },
  },
  ...over,
});
const EVENTS: HistoryEvent[] = [
  visitEvent("vb", { edited: true, visit: { ...visitEvent("vb").visit!, corrects_visit_id: "va" } }),
  visitEvent("va", {
    kind: "visit_voided",
    at: `${today}T02:00:00Z`,
    edited: true,
    visit: { ...visitEvent("va").visit!, voided: true, corrected_by_visit_id: "vb", void_reason: "Sửa lượt: nhầm" },
  }),
  {
    kind: "package_extended",
    at: `${today}T01:00:00Z`,
    business_date: today,
    extension: { by_name: "Lan", customer_package_id: "cp1", expires_on: "2027-05-01", previous_expires_on: "2027-02-01", package_name: "Gội 10 buổi", reason: "Khách ốm" },
  },
];
const timeline = { history: () => ({ customer_id: "c1", events: EVENTS, next_cursor: null }) };

describe("Lịch sử khách", () => {
  it("one chip per visit (D9), and the card's time toggles its history at the card's foot", async () => {
    const spy = serveManagement({ permissions: OWNER, ...timeline });
    const user = userEvent.setup();
    at("/khach-hang/c1");
    const list = await screen.findByRole("list", { name: "Lịch sử sử dụng" });
    const cards = Array.from(list.children) as HTMLElement[];
    expect(within(cards[0] as HTMLElement).getByText("Đã sửa")).toBeInTheDocument();
    expect(within(cards[0] as HTMLElement).queryByText("Lượt sửa lại")).toBeNull();
    expect(within(cards[1] as HTMLElement).getByText("Đã thay")).toBeInTheDocument();
    expect(within(cards[1] as HTMLElement).getByText("Đã thay bằng lượt mới. Sửa lượt: nhầm")).toBeInTheDocument();

    const when = `${relativeDay(today)} · 10:07`;
    const time = within(cards[0] as HTMLElement).getByRole("button", { name: `${when}, ai ghi, ai sửa` });
    expect(time).toHaveTextContent(when);
    expect(time).toHaveAttribute("aria-expanded", "false");
    await user.click(time);
    expect(time).toHaveAttribute("aria-expanded", "true");
    const region = document.getElementById(time.getAttribute("aria-controls") ?? "");
    expect(region).not.toBeNull();
    expect(await within(region as HTMLElement).findByText(/^Ghi bởi Hà · 10:07/)).toBeInTheDocument();
    expect(urls(spy)).toContain("/api/v1/record-history/visit/vb");
    await user.click(time);
    expect(time).toHaveAttribute("aria-expanded", "false");
    expect(within(cards[0] as HTMLElement).queryByText(/^Ghi bởi Hà · 10:07/)).toBeNull();

    await user.click(within(cards[2] as HTMLElement).getByRole("button", { name: /, ai ghi, ai sửa$/ }));
    await waitFor(() => {
      expect(urls(spy)).toContain("/api/v1/record-history/customer-package/cp1");
    });
  });

  it("without the right: the time is plain text and the new visit of 'Sửa lượt' keeps 'Lượt sửa lại'", async () => {
    // The server sends no `edited` without the right.
    const plain = EVENTS.map(({ edited: _, ...e }) => e);
    serveManagement({ permissions: MANAGER, history: () => ({ customer_id: "c1", events: plain, next_cursor: null }) });
    at("/khach-hang/c1");
    const list = await screen.findByRole("list", { name: "Lịch sử sử dụng" });
    const cards = Array.from(list.children) as HTMLElement[];
    expect(within(cards[0] as HTMLElement).getByText("Lượt sửa lại")).toBeInTheDocument();
    expect(within(cards[1] as HTMLElement).getByText("Đã thay")).toBeInTheDocument();
    expect(within(list).queryByRole("button", { name: /ai ghi, ai sửa/ })).toBeNull();
  });
});

const PKG: CustomerPackage = {
  id: "cp1",
  customer_id: "c1",
  plan_id: "p1",
  name: "Gội 10 buổi",
  gift_note: null,
  sold_price: 1_500_000,
  sale_id: "sale1",
  closer_staff_id: "st1",
  closer_name: "KTV Lan",
  starts_on: "2026-11-01",
  expires_on: "2027-05-01",
  status: "active",
  sessions_total: 12,
  sessions_remaining: 6,
  version: 1,
  created_at: "2026-11-01T00:00:00Z",
  items: [],
};

describe("Hồ sơ khách", () => {
  it("'Tạo bởi …' is the last element of the contact details; a package's 'Lịch sử' opens its own", async () => {
    const spy = serveManagement({ permissions: OWNER, packages: [PKG] });
    const user = userEvent.setup();
    at("/khach-hang/c1");
    const info = await screen.findByRole("article", { name: "Thông tin khách" });
    const trace = await within(info).findByRole("button", { name: "Tạo bởi Hà · 05/11/2026" });
    // After the contact details, before the health note section.
    expect(info.querySelector("dl.detail-grid")?.compareDocumentPosition(trace)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(urls(spy)).toContain("/api/v1/record-history/customer/c1");

    const open = await screen.findByRole("button", { name: "Lịch sử gói Gội 10 buổi" });
    expect(open).toHaveTextContent(/^Lịch sử$/);
    await user.click(open);
    expect(open).toHaveAttribute("aria-expanded", "true");
    const region = document.getElementById(open.getAttribute("aria-controls") ?? "");
    expect(await within(region as HTMLElement).findByText("Ghi bởi Hà · 05/11/2026")).toBeInTheDocument();
    expect(urls(spy)).toContain("/api/v1/record-history/customer-package/cp1");
  });

  it("without the right: neither, and nothing asked", async () => {
    const spy = serveManagement({ permissions: MANAGER, packages: [PKG] });
    at("/khach-hang/c1");
    await screen.findByRole("article", { name: "Thông tin khách" });
    expect(await screen.findByText("Gội 10 buổi")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Tạo bởi|^Lịch sử gói/ })).toBeNull();
    expect(urls(spy).some((u) => u.includes("/record-history/"))).toBe(false);
  });
});
