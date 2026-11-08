import type {
  CustomerPackage,
  PackagePlan,
  PaymentCreate,
  Service,
  ServiceDiscountCreate,
  Staff,
  Visit,
  VisitCreate,
  VisitPackageSaleCreate,
} from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { spaTime } from "../../lib/format";

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

/**
 * The day list's order (F26): newest service time (`check_in_at`) first, as `GET /visits` sorts.
 * A visit saved with an earlier "giờ làm" lands at its time instead of on top of the list.
 */
export function byServiceTime<T extends { check_in_at: string; id: string }>(
  rows: T[],
): T[] {
  return [...rows].sort(
    (a, b) =>
      Date.parse(b.check_in_at) - Date.parse(a.check_in_at) ||
      b.id.localeCompare(a.id),
  );
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
      /** "Người giới thiệu" (upsell credit, spec §10.8 Q-U2); absent or "" = khách tự chọn. */
      referrerId?: string;
    }
  | {
      key: number;
      /**
       * "Làm 1 buổi hôm nay" on a package sold in this checkout (EM 2026-11-07): today's session
       * comes from that package (`package_ref` on the wire), never charged.
       */
      source: "new_package";
      serviceId: string;
      serviceName: string;
      categoryName?: string;
      /** `ref` of the package sold today. */
      saleRef: string;
      staffId: string;
    };


/**
 * A package sold in this checkout ("Gói bán hôm nay", spec §10.4.1). `plan` is a snapshot of the
 * catalog entry for the preview; the server prices the sale from the catalog.
 */
export interface PackageSaleDraft {
  /** `ref` on the wire: the lines that use today's session point at it. */
  ref: string;
  plan: Pick<PackagePlan, "id" | "name" | "price" | "items"> | null;
  /** "" = not chosen yet, "self" = khách tự chọn (nobody credited), else a staff id. */
  closer: string;
  giftNote: string;
  /** Raw text of the amount field ("" = no discount). */
  discountText: string;
  discountReason: string;
  /** "Dùng gói này cho buổi hôm nay": covered one-off lines become today's session. */
  useToday: boolean;
  /** "Mua cho người khác" (US13): the package belongs to this customer, never used today. */
  beneficiary: { id: string; full_name: string } | null;
  /** "Mua cho người khác" is ticked (the beneficiary may not be picked yet). */
  forOther?: boolean;
  /**
   * "Ghi chú gói" (F4): health data like the session note, sent only by callers holding
   * `customer.health_notes.write` and never kept in the draft cart.
   */
  note?: string;
}

/** Amount typed as a package discount: 0 = none, NaN = not a number. */
export function packageDiscount(p: PackageSaleDraft): number {
  const t = p.discountText.replace(/[.\s,đ]/g, "");
  if (t === "") return 0;
  return /^\d+$/.test(t) ? Number(t) : Number.NaN;
}

/** What the packages of this checkout cost after their own discounts (previews). */
export function packagesDue(sales: PackageSaleDraft[]): number {
  return sales.reduce((s, p) => {
    const d = packageDiscount(p);
    return p.plan ? s + p.plan.price - (Number.isNaN(d) ? 0 : d) : s;
  }, 0);
}

/**
 * Which one-off lines a package sold today covers ("Dùng gói này cho buổi hôm nay"): line key →
 * the package's `ref`. A package covers lines of its services in cart order, up to its paid +
 * bonus sessions; a package bought for someone else covers nothing (packages are not shared, D5).
 */
export function coverage(
  lines: CartLine[],
  sales: PackageSaleDraft[],
): Map<number, string> {
  const covered = new Map<number, string>();
  for (const p of sales) {
    // "Làm 1 buổi hôm nay" lines belong to their package (pruned when it cannot hold them).
    for (const l of lines)
      if (l.source === "new_package" && l.saleRef === p.ref)
        covered.set(l.key, p.ref);
    if (!p.useToday || !p.plan || p.beneficiary || p.forOther) continue;
    const left = new Map(
      p.plan.items.map((i) => [i.service_id, i.sessions + i.bonus_sessions]),
    );
    for (const l of lines) {
      const n = left.get(l.serviceId) ?? 0;
      if (l.source !== "one_off" || covered.has(l.key) || n <= 0) continue;
      covered.set(l.key, p.ref);
      left.set(l.serviceId, n - 1);
    }
  }
  return covered;
}

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

/** Sum of the one-off lines not covered by a package sold today: what they cost now. */
export function amountDue(
  lines: CartLine[],
  covered: ReadonlyMap<number, string> = new Map(),
): number {
  return lines.reduce(
    (sum, l) =>
      sum + (l.source === "one_off" && !covered.has(l.key) ? l.price : 0),
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
  /**
   * Packages sold in this checkout, in wire form (`packageSalesBody`, lazy `checkout.ts`), and
   * which lines they cover (`coverage`): those lines use today's session from them.
   */
  packageSales?: VisitPackageSaleCreate[];
  covered?: ReadonlyMap<number, string>;
  /** "Người đi cùng" notes (0–5); blanks are dropped, none = omitted. */
  companions?: string[];
}

export function visitBody(
  customerId: string,
  lines: CartLine[],
  payments: PaymentCreate[],
  note: string,
  extras: VisitExtras = {},
): VisitCreate {
  const covered = extras.covered ?? new Map<number, string>();
  const companions = (extras.companions ?? [])
    .map((c) => c.trim())
    .filter((c) => c !== "");
  return {
    customer_id: customerId,
    ...(extras.checkInAt ? { check_in_at: extras.checkInAt } : {}),
    ...(extras.discount ? { discount: discountBody(extras.discount) } : {}),
    ...(extras.packageSales?.length
      ? { package_sales: extras.packageSales }
      : {}),
    ...(companions.length > 0 ? { companions } : {}),
    note: note.trim() === "" ? null : note.trim(),
    lines: lines.map((l) =>
      covered.has(l.key) || l.source === "new_package"
        ? {
            service_id: l.serviceId,
            source: "package",
            package_ref: covered.get(l.key) ?? null,
            staff_id: l.staffId,
          }
        : l.source === "package"
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
              ...(l.referrerId ? { referrer_staff_id: l.referrerId } : {}),
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
export const timeOf = spaTime;
