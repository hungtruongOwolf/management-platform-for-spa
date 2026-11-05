/**
 * URL state of the management lists (EM round 3: "back restores it"). Each route's
 * `validateSearch` parses with these, so a hand-edited or stale URL falls back to defaults instead
 * of breaking the screen. Defaults are left out of the URL. Tiny and dependency-free on purpose:
 * the router (startup bundle) imports this file, the screens stay lazy.
 */

type Raw = Record<string, unknown>;

const PRESETS = ["today", "7d", "30d", "this_month", "last_month", "this_year", "12m", "custom"] as const;
const PACKAGE_FILTERS = ["expiring", "active", "expired", "exhausted", "cancelled", "all"] as const;
const FOLLOW_UPS = ["due", "not_contacted", "scheduled", "all"] as const;
const KINDS = ["package", "one_off"] as const;
const HISTORY_KINDS = ["visit", "purchase", "visit_voided", "purchase_voided", "package_cancelled"] as const;
const METRICS = ["revenue", "customers", "visits"] as const;

function oneOf<T extends string>(values: readonly T[], v: unknown): T | undefined {
  return typeof v === "string" && (values as readonly string[]).includes(v) ? (v as T) : undefined;
}

function day(v: unknown): string | undefined {
  return typeof v === "string" && /^\d{4}-\d{2}-\d{2}$/.test(v) ? v : undefined;
}

/** Free-text search, trimmed and capped (it ends up in an API query string). */
function text(v: unknown): string | undefined {
  if (typeof v !== "string" && typeof v !== "number") return undefined;
  const t = String(v).trim().slice(0, 80);
  return t === "" ? undefined : t;
}

/** Drops undefined keys so defaults never show up in the URL. */
function compact<T extends object>(o: { [K in keyof Required<T>]: T[K] | undefined }): T {
  return Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)) as T;
}

export interface ReportsSearch {
  tab?: "ledger";
  period?: (typeof PRESETS)[number];
  from?: string;
  to?: string;
  metric?: (typeof METRICS)[number];
}

export function parseReportsSearch(s: Raw): ReportsSearch {
  const period = oneOf(PRESETS, s["period"]);
  return compact<ReportsSearch>({
    tab: s["tab"] === "ledger" ? "ledger" : undefined,
    period: period === "7d" ? undefined : period,
    from: period === "custom" ? day(s["from"]) : undefined,
    to: period === "custom" ? day(s["to"]) : undefined,
    metric: oneOf(METRICS, s["metric"]) === "revenue" ? undefined : oneOf(METRICS, s["metric"]),
  });
}

export interface PackagesSearch {
  status?: (typeof PACKAGE_FILTERS)[number];
  q?: string;
}

export function parsePackagesSearch(s: Raw): PackagesSearch {
  const status = oneOf(PACKAGE_FILTERS, s["status"]);
  return compact<PackagesSearch>({ status: status === "expiring" ? undefined : status, q: text(s["q"]) });
}

export const INACTIVE_THRESHOLDS = [7, 14, 30, 60, 90] as const;

export interface InactiveSearch {
  days?: number;
  /** Default: every customer seen before (EM round 3 item 4); "with_sessions" narrows to packages with sessions left. */
  scope?: "with_sessions";
  kind?: (typeof KINDS)[number];
  follow?: (typeof FOLLOW_UPS)[number];
  q?: string;
}

export function parseInactiveSearch(s: Raw): InactiveSearch {
  const days = Number(s["days"]);
  const follow = oneOf(FOLLOW_UPS, s["follow"]);
  return compact<InactiveSearch>({
    days: (INACTIVE_THRESHOLDS as readonly number[]).includes(days) && days !== 7 ? days : undefined,
    scope: s["scope"] === "with_sessions" ? "with_sessions" : undefined,
    kind: oneOf(KINDS, s["kind"]),
    // Default "due": the list opens on who to call today (EM round 3), "all" is explicit.
    follow: follow === "due" ? undefined : follow,
    q: text(s["q"]),
  });
}

export interface HistorySearch {
  hq?: string;
  hkind?: (typeof HISTORY_KINDS)[number];
  hfrom?: string;
  hto?: string;
}

export function parseHistorySearch(s: Raw): HistorySearch {
  return compact<HistorySearch>({
    hq: text(s["hq"]),
    hkind: oneOf(HISTORY_KINDS, s["hkind"]),
    hfrom: day(s["hfrom"]),
    hto: day(s["hto"]),
  });
}

export interface CatalogSearch {
  q?: string;
}

export function parseCatalogSearch(s: Raw): CatalogSearch {
  return compact<CatalogSearch>({ q: text(s["q"]) });
}
