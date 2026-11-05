/**
 * URL state of the management screens (lib/searchState.ts). Imported by router.tsx, so it stays tiny
 * and free of the screens' code: the screens themselves are lazy chunks.
 */
import type { InactiveScope } from "@vpyd/api-client";
import { param, searchSchema } from "../../lib/searchState";
import type { PackageFilter } from "./api";

export const PACKAGE_FILTERS = ["expiring", "active", "expired", "exhausted", "cancelled", "all"] as const satisfies readonly PackageFilter[];

/** /goi-lieu-trinh?status=… */
export const packagesSearch = searchSchema({
  status: param.oneOf<PackageFilter>(PACKAGE_FILTERS, "expiring"),
});

/** Absence thresholds offered (EM 2026-11-05: 7 days is the default). */
export const THRESHOLDS = [7, 14, 30] as const;
export const INACTIVE_SCOPES = ["with_sessions", "all"] as const satisfies readonly InactiveScope[];

/** /khach-lau-khong-quay-lai?days=…&scope=… */
export const inactiveSearch = searchSchema({
  days: param.oneOfNumber(THRESHOLDS, 7),
  scope: param.oneOf<InactiveScope>(INACTIVE_SCOPES, "with_sessions"),
});

/** /bao-cao?date=YYYY-MM-DD ("" = today, so the plain link always opens today). */
export const reportsSearch = searchSchema({
  date: param.day(),
});
