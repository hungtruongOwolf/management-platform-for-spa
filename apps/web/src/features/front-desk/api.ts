import type {
  CustomerPackage,
  PaymentCreate,
  Service,
  ServiceDiscountCreate,
  Staff,
  Visit,
  VisitCreate,
} from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api } from "../../lib/api";

export type Gender = "female" | "male";

/** Visits the day list shows before "Xem tất cả". */
export const VISITS_PAGE = 30;
/** Asked for: one more than shown tells whether the day has more (no count header needed). */
const VISITS_PEEK = VISITS_PAGE + 1;

/**
 * Today's newest visits (business day of the spa, decided by the server). A busy day's full list
 * is large; the peek keeps /quay's first screen light (Lighthouse LCP budget, testing.md §4).
 */
export const todayVisitsQuery = queryOptions({
  queryKey: ["visits", "today"],
  queryFn: async () =>
    (await api(`/visits?limit=${String(VISITS_PEEK)}`)) as Visit[],
  staleTime: 15_000,
});

/** Every visit of today, once the user asks for all of them. */
export const todayAllVisitsQuery = queryOptions({
  queryKey: ["visits", "today", "all"],
  queryFn: async () => (await api("/visits")) as Visit[],
  staleTime: 15_000,
});

/** Visits of one business day ("YYYY-MM-DD"), e.g. to void a record of an earlier day. */
export function visitsOfDayQuery(date: string, all = false) {
  const day = `date=${encodeURIComponent(date)}`;
  return queryOptions({
    queryKey: ["visits", date, all ? "all" : "peek"],
    queryFn: async () =>
      (await api(
        all ? `/visits?${day}` : `/visits?${day}&limit=${String(VISITS_PEEK)}`,
      )) as Visit[],
    staleTime: 15_000,
  });
}

/** One key per visit being recorded: a double click or a retry after a network error replays it. */
export async function recordVisit(
  body: VisitCreate,
  idempotencyKey: string,
): Promise<Visit> {
  return (await api("/visits", {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as Visit;
}

/** A line in the visit being recorded. Prices here are previews: the server prices the visit. */
export type CartLine =
  | {
      key: number;
      source: "package";
      serviceId: string;
      serviceName: string;
      /** Menu category shown before the name ("Thư Giãn · Gội đầu dưỡng sinh"). */
      categoryName?: string;
      itemId: string;
      packageName: string;
      staffId: string;
      expiryOverrideReason?: string;
    }
  | {
      key: number;
      source: "one_off";
      serviceId: string;
      serviceName: string;
      categoryName?: string;
      /** Only sent when the customer has no recorded gender. */
      priceVariant: Gender | null;
      price: number;
      staffId: string;
    };

export type PriceResult =
  | { ok: true; price: number; variant: "all" | Gender }
  | { ok: false; reason: "need_gender" | "not_offered" | "no_price" };

/** Mirrors the server rule (AC4.2b/c): flat price for everyone, otherwise the gender's price. */
export function priceFor(service: Service, gender: Gender | null): PriceResult {
  const { prices } = service;
  if (prices.all != null)
    return { ok: true, price: prices.all, variant: "all" };
  if (prices.female == null && prices.male == null)
    return { ok: false, reason: "no_price" };
  if (!gender) return { ok: false, reason: "need_gender" };
  const price = prices[gender];
  return price == null
    ? { ok: false, reason: "not_offered" }
    : { ok: true, price, variant: gender };
}

/** Sum of the one-off lines: what the customer pays now. */
export function amountDue(lines: CartLine[]): number {
  return lines.reduce(
    (sum, l) => sum + (l.source === "one_off" ? l.price : 0),
    0,
  );
}

/** Usable for a deduction today: not cancelled, not expired, sessions left. */
export function isUsable(p: CustomerPackage): boolean {
  return p.status === "active";
}

/** Sessions of an item already put in the cart (not saved yet). */
export function inCart(lines: CartLine[], itemId: string): number {
  return lines.filter((l) => l.source === "package" && l.itemId === itemId)
    .length;
}

/**
 * Packages in the order the desk should use them (AC4.3): usable first, soonest expiry first
 * (no expiry last), then oldest purchase. The server applies the same rule when no item is sent.
 */
export function byUseOrder(packages: CustomerPackage[]): CustomerPackage[] {
  const expiry = (p: CustomerPackage) => p.expires_on ?? "9999-12-31";
  return [...packages].sort((a, b) => {
    if (isUsable(a) !== isUsable(b)) return isUsable(a) ? -1 : 1;
    if (expiry(a) !== expiry(b)) return expiry(a) < expiry(b) ? -1 : 1;
    return a.created_at < b.created_at
      ? -1
      : a.created_at > b.created_at
        ? 1
        : 0;
  });
}

/** Technicians who worked most today come first, so the usual pick is at the top. Voided visits do not count. */
export function staffByUse(staff: Staff[], today: Visit[]): Staff[] {
  const uses = new Map<string, number>();
  for (const v of today.filter((v) => v.status !== "voided"))
    for (const l of v.lines)
      uses.set(l.staff_id, (uses.get(l.staff_id) ?? 0) + 1);
  return [...staff].sort(
    (a, b) =>
      (uses.get(b.id) ?? 0) - (uses.get(a.id) ?? 0) ||
      a.full_name.localeCompare(b.full_name, "vi"),
  );
}

export type PaymentMode = "cash" | "bank_transfer" | "mixed";

/** Full collection (no debt in R1): one method, or a cash part + the rest by transfer. */
export function paymentsFor(
  total: number,
  mode: PaymentMode,
  cashPart: number,
): PaymentCreate[] {
  if (total === 0) return [];
  if (mode === "mixed")
    return [
      { method: "cash", amount: cashPart },
      { method: "bank_transfer", amount: total - cashPart },
    ];
  return [{ method: mode, amount: total }];
}

/** A discount typed at the desk: percent or amount of the one-off lines (D10, server decides). */
export interface DiscountDraft {
  kind: "percent" | "amount";
  /** Raw text of the number field. */
  value: string;
  reason: string;
}

/** The discount this draft asks for, as the server computes it (percent rounds down); 0 = none. */
export function discountPreview(due: number, d: DiscountDraft | null): number {
  if (!d) return 0;
  const n = Number(d.value.replace(/[.\s,đ]/g, ""));
  if (!Number.isInteger(n) || n <= 0) return 0;
  if (d.kind === "percent") return n > 100 ? 0 : Math.floor((due * n) / 100);
  return n > due ? 0 : n;
}

/** The wire form of a valid draft (`null` = no discount). */
export function discountBody(
  d: DiscountDraft | null,
): ServiceDiscountCreate | null {
  if (!d) return null;
  const n = Number(d.value.replace(/[.\s,đ]/g, ""));
  return d.kind === "percent"
    ? { percent: n, amount: null, reason: d.reason.trim() }
    : { percent: null, amount: n, reason: d.reason.trim() };
}

export interface VisitExtras {
  discount?: DiscountDraft | null;
  /** When the service happened (ISO instant); omitted = now. */
  checkInAt?: string | null;
}

export function visitBody(
  customerId: string,
  lines: CartLine[],
  payments: PaymentCreate[],
  note: string,
  extras: VisitExtras = {},
): VisitCreate {
  return {
    customer_id: customerId,
    ...(extras.checkInAt ? { check_in_at: extras.checkInAt } : {}),
    ...(extras.discount ? { discount: discountBody(extras.discount) } : {}),
    note: note.trim() === "" ? null : note.trim(),
    lines: lines.map((l) =>
      l.source === "package"
        ? {
            service_id: l.serviceId,
            source: "package",
            customer_package_item_id: l.itemId,
            staff_id: l.staffId,
            expiry_override_reason: l.expiryOverrideReason ?? null,
          }
        : {
            service_id: l.serviceId,
            source: "one_off",
            staff_id: l.staffId,
            price_variant: l.priceVariant,
          },
    ),
    payments,
  };
}

/** "Đã lưu lượt của Hà · Gội đầu còn 4/12" from the saved visit. */
export function savedSummary(visit: Visit): string {
  const left = visit.lines
    .filter((l) => l.source === "package" && l.sessions_left_after != null)
    .map(
      (l) =>
        `${l.service_name} còn ${String(l.sessions_left_after)}/${String(l.sessions_total ?? "?")}`,
    );
  return [`Đã lưu lượt của ${visit.customer_name}`, ...left].join(" · ");
}

/** "Đã thanh toán 150.000đ (TM)" / "Không phát sinh tiền" / "Đã hủy" for a saved visit. */
export function paymentText(v: Visit, money: (n: number) => string): string {
  if (v.status === "voided") return "Đã hủy, đã hoàn tiền";
  if (v.total === 0) return "Trừ liệu trình, không phát sinh tiền";
  const methods = [
    ...new Set(
      v.payments
        .filter((p) => p.kind === "payment")
        .map((p) => (p.method === "cash" ? "TM" : "CK")),
    ),
  ].join(" + ");
  return `Đã thanh toán ${money(v.total)}${methods ? ` (${methods})` : ""}`;
}

/**
 * The instant for a "giờ làm" picked at the desk: `day` (YYYY-MM-DD, spa calendar) at `time`
 * (HH:MM) in the spa's zone (UTC+7, no DST). "" time → null (= now, server clock).
 */
export function serviceInstant(day: string, time: string): string | null {
  if (!/^\d{2}:\d{2}$/.test(time) || !/^\d{4}-\d{2}-\d{2}$/.test(day))
    return null;
  return new Date(`${day}T${time}:00+07:00`).toISOString();
}

/** "2026-11-05T02:12:00Z" → "09:12" in the spa's time zone. */
export function timeOf(iso: string): string {
  return new Intl.DateTimeFormat("vi-VN", {
    hour: "2-digit",
    minute: "2-digit",
    timeZone: "Asia/Ho_Chi_Minh",
  }).format(new Date(iso));
}
