// Management-view types come from the API contract (packages/api-client, generated from the
// reporting crate's DTOs). Money fields are optional: the server leaves them out for callers without
// `report.revenue.read`, and screens render nothing in their place.
import type { Schemas } from "@vpyd/api-client";

export type {
  CustomerStats,
  DailyReport,
  HistoryEvent,
  HistoryPage,
  InactivePage,
  PackageOverviewPage,
  PackageOverviewRow,
  RangeReport,
  Revenue,
} from "@vpyd/api-client";

export type PackageStatus = Schemas["PackageStatusFilter"];
export type DayCounts = Schemas["DayCounts"];
export type MethodSplit = Schemas["MethodSplit"];
