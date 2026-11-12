// What the front desk's first screen needs from this feature's API: today's visits and how many
// sessions of a package item the cart holds. The cart's pricing and wire forms (api.ts) load with
// the checkout after the first paint, off /quay's critical path (frontend.md "Perf budget").
// api.ts re-exports these, so every other module keeps importing from there.
import type { Visit } from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api } from "../../lib/api";
import type { CartLine } from "./api";

/** Visits the day list shows before "Xem tất cả". */
export const VISITS_PAGE = 30;
/** Asked for: one more than shown tells whether the day has more (no count header needed). */
export const VISITS_PEEK = VISITS_PAGE + 1;

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

/** Sessions of an item already put in the cart (not saved yet). */
export function inCart(lines: CartLine[], itemId: string): number {
  return lines.filter((l) => l.source === "package" && l.itemId === itemId)
    .length;
}
