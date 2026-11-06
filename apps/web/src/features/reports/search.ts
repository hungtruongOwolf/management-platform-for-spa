/**
 * URL state of the management screens (lib/searchState.ts). Imported by router.tsx, so it stays tiny
 * and free of the screens' code: the screens themselves are lazy chunks.
 */
import type { CustomerKindFilter, FollowUpFilter, InactiveScope } from "@vpyd/api-client";
import { param, searchSchema } from "../../lib/searchState";
import type { PackageFilter } from "./api";

/** Report periods (features/reports/period.ts resolves them); here so the router stays light. */
export const PRESETS = ["today", "7d", "30d", "this_month", "last_month", "this_year", "12m", "3y", "5y", "custom"] as const;
export type Preset = (typeof PRESETS)[number];

export const PACKAGE_FILTERS = ["expiring", "active", "expired", "exhausted", "cancelled", "all"] as const satisfies readonly PackageFilter[];

/** Bound on free-text search (the API takes ≤ 100 characters). */
const SEARCH_MAX = 80;

/** /goi-lieu-trinh?status=…&q=… (q: customer name, package name or phone). */
export const packagesSearch = searchSchema({
  status: param.oneOf<PackageFilter>(PACKAGE_FILTERS, "expiring"),
  q: param.text(SEARCH_MAX),
});

/** Absence thresholds offered (EM 2026-11-05: 7 days is the default). */
export const THRESHOLDS = [7, 14, 30, 60, 90] as const;
export const INACTIVE_SCOPES = ["all", "with_sessions"] as const satisfies readonly InactiveScope[];
export const CUSTOMER_KINDS = ["all", "package", "one_off"] as const satisfies readonly CustomerKindFilter[];
/** The three tabs of the absent list (EM 2026-11-05): who to call now, who is planned, everyone. */
export const FOLLOW_UP_FILTERS = ["due", "scheduled", "all"] as const satisfies readonly FollowUpFilter[];

/**
 * /khach-lau-khong-quay-lai: threshold, scope (every customer by default: EM round 3 item 4),
 * kind, follow-up state ("Cần liên hệ" by default: who to call today) and a search.
 */
export const inactiveSearch = searchSchema({
  days: param.oneOfNumber(THRESHOLDS, 7),
  scope: param.oneOf<InactiveScope>(INACTIVE_SCOPES, "all"),
  kind: param.oneOf<CustomerKindFilter>(CUSTOMER_KINDS, "all"),
  follow: param.oneOf<FollowUpFilter>(FOLLOW_UP_FILTERS, "due"),
  q: param.text(SEARCH_MAX),
});

export const REPORT_TABS = ["overview", "ledger"] as const;
export const REPORT_METRICS = ["revenue", "customers", "visits"] as const;
export type ReportMetric = (typeof REPORT_METRICS)[number];

/** /bao-cao?tab=…&period=…&from=…&to=…&metric=… (from/to only matter for `custom`). */
export const reportsSearch = searchSchema({
  tab: param.oneOf(REPORT_TABS, "overview"),
  period: param.oneOf<Preset>(PRESETS, "7d"),
  from: param.day(),
  to: param.day(),
  metric: param.oneOf<ReportMetric>(REPORT_METRICS, "revenue"),
});
