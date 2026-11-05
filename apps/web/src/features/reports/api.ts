import { queryOptions } from "@tanstack/react-query";
import { MANAGEMENT_DATA, type ManagementData } from "../../app/features";
import { api } from "../../lib/api";
import { spaToday } from "../corrections/api";
import type {
  CustomerStats,
  DailyReport,
  HistoryPage,
  InactivePage,
  PackageOverviewPage,
  PackageStatus,
  RangeReport,
} from "./contract";

/**
 * Data source of the management views: the API, or the typed mock behind the feature flag
 * (app/features.ts). Screens only ever see these query options, so switching to the real API is a
 * flag flip. The mock is a separate chunk, only downloaded when the flag is "mock".
 */
async function fetchOr<T>(path: string, mock: (m: typeof import("./mock")) => T, data: ManagementData): Promise<T> {
  if (data === "api") return (await api(path)) as T;
  const m = await import("./mock");
  // A short wait so the skeletons are visible in the mock, like a real request.
  await new Promise((r) => setTimeout(r, import.meta.env.MODE === "test" ? 0 : 350));
  return mock(m);
}

export const isMock = MANAGEMENT_DATA === "mock";

export function addDays(iso: string, days: number): string {
  const d = new Date(`${iso}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

export function dailyReportQuery(date: string, data: ManagementData = MANAGEMENT_DATA) {
  return queryOptions({
    queryKey: ["reports", "daily", date],
    queryFn: () => fetchOr<DailyReport>(`/reports/daily?date=${date}`, (m) => m.mockDaily(date), data),
    staleTime: 30_000,
  });
}

export function rangeReportQuery(from: string, to: string, data: ManagementData = MANAGEMENT_DATA) {
  return queryOptions({
    queryKey: ["reports", "range", from, to],
    queryFn: () =>
      fetchOr<RangeReport>(`/reports/range?from=${from}&to=${to}&granularity=day`, (m) => m.mockRange(from, to), data),
    staleTime: 60_000,
  });
}

export function customerHistoryQuery(id: string, data: ManagementData = MANAGEMENT_DATA) {
  return queryOptions({
    queryKey: ["customers", "history", id],
    queryFn: () =>
      fetchOr<HistoryPage>(`/customers/${id}/history?limit=20`, (m) => m.mockHistory(id, spaToday()), data),
  });
}

export function customerStatsQuery(id: string, data: ManagementData = MANAGEMENT_DATA) {
  return queryOptions({
    queryKey: ["customers", "summary", id],
    queryFn: () =>
      fetchOr<CustomerStats>(
        `/customers/${id}/summary`,
        (m) => m.mockCustomerStats(id, m.mockHistory(id, spaToday())),
        data,
      ),
  });
}

export type PackageFilter = PackageStatus | "expiring" | "all";

export function packagesOverviewQuery(filter: PackageFilter, data: ManagementData = MANAGEMENT_DATA) {
  const qs =
    filter === "all" ? "" : filter === "expiring" ? "?expiring_within_days=30" : `?status=${filter}`;
  return queryOptions({
    queryKey: ["packages", "overview", filter],
    queryFn: () => fetchOr<PackageOverviewPage>(`/packages${qs}`, (m) => m.mockPackages(spaToday(), filter), data),
    staleTime: 30_000,
  });
}

export function inactiveCustomersQuery(days: number, data: ManagementData = MANAGEMENT_DATA) {
  return queryOptions({
    queryKey: ["customers", "inactive", days],
    queryFn: () =>
      fetchOr<InactivePage>(`/customers/inactive?days=${String(days)}`, (m) => m.mockInactive(spaToday(), days), data),
    staleTime: 60_000,
  });
}

const dayMonth = new Intl.DateTimeFormat("vi-VN", { day: "2-digit", month: "2-digit", timeZone: "UTC" });
const fullDate = new Intl.DateTimeFormat("vi-VN", { day: "2-digit", month: "2-digit", year: "numeric", timeZone: "UTC" });
const weekday = new Intl.DateTimeFormat("vi-VN", { weekday: "short", timeZone: "UTC" });

/** "05/11" for a business day `YYYY-MM-DD`. */
export const shortDay = (iso: string) => dayMonth.format(new Date(`${iso}T00:00:00Z`));
/** "05/11/2026". */
export const longDay = (iso: string) => fullDate.format(new Date(`${iso}T00:00:00Z`));
/** "T5" (Thứ 5) / "CN". */
export const weekdayOf = (iso: string) => weekday.format(new Date(`${iso}T00:00:00Z`));

/** "Hôm nay", "Hôm qua" or the date (design-system.md "Định dạng"). */
export function relativeDay(iso: string, today: string = spaToday()): string {
  if (iso === today) return "Hôm nay";
  if (iso === addDays(today, -1)) return "Hôm qua";
  return longDay(iso);
}

/** Compact money for chart axes: 1,2 tr · 850 k. */
export function compactVnd(n: number): string {
  if (Math.abs(n) >= 1_000_000) return `${(n / 1_000_000).toLocaleString("vi-VN", { maximumFractionDigits: 1 })} tr`;
  if (Math.abs(n) >= 1000) return `${String(Math.round(n / 1000))} k`;
  return String(n);
}
