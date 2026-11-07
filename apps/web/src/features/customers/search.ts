/** URL state of /khach-hang (lib/searchState.ts): the search box, so Back returns to the same hits. */
import type { HistoryKind } from "@vpyd/api-client";
import { param, searchSchema } from "../../lib/searchState";

/** Mirrors the server's bound on `q` (crm::service::MAX_QUERY_CHARS). */
export const customersSearch = searchSchema({
  q: param.text(100),
});

export const HISTORY_KINDS = ["visit", "purchase", "visit_voided", "purchase_voided", "package_cancelled", "package_extended"] as const satisfies readonly HistoryKind[];

/**
 * /khach-hang/:id — the usage history filters (EM round 3 item 6): search service, category,
 * package or staff; one event kind ("" = all); a date range. Back from a package page restores them.
 */
export const historySearch = searchSchema({
  hq: param.text(100),
  hkind: param.oneOf<HistoryKind | "">(["", ...HISTORY_KINDS], ""),
  hfrom: param.day(),
  hto: param.day(),
});
