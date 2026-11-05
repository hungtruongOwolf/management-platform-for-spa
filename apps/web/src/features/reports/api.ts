import type { InactiveScope } from "@vpyd/api-client";
import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
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

/** Query options of the management views; screens only ever use these (frontend.md). */

export function addDays(iso: string, days: number): string {
  const d = new Date(`${iso}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

export function dailyReportQuery(date: string) {
  return queryOptions({
    queryKey: ["reports", "daily", date],
    queryFn: async () => (await api(`/reports/daily?date=${date}`)) as DailyReport,
    staleTime: 30_000,
  });
}

export function rangeReportQuery(from: string, to: string) {
  return queryOptions({
    queryKey: ["reports", "range", from, to],
    queryFn: async () => (await api(`/reports/range?from=${from}&to=${to}&granularity=day`)) as RangeReport,
    staleTime: 60_000,
  });
}

export function customerHistoryQuery(id: string) {
  return queryOptions({
    queryKey: ["customers", "history", id],
    queryFn: async () => (await api(`/customers/${id}/history?limit=20`)) as HistoryPage,
  });
}

export function customerStatsQuery(id: string) {
  return queryOptions({
    queryKey: ["customers", "summary", id],
    queryFn: async () => (await api(`/customers/${id}/summary`)) as CustomerStats,
  });
}

export type PackageFilter = PackageStatus | "expiring" | "all";

/** Query string of `GET /packages` for a filter chip. */
export function packagesPath(filter: PackageFilter): string {
  if (filter === "all") return "/packages?limit=50";
  if (filter === "expiring") return "/packages?expiring_within_days=30&limit=50";
  return `/packages?status=${filter}&limit=50`;
}

export function packagesOverviewQuery(filter: PackageFilter) {
  return queryOptions({
    queryKey: ["packages", "overview", filter],
    queryFn: async () => (await api(packagesPath(filter))) as PackageOverviewPage,
    staleTime: 30_000,
  });
}

/** `GET /customers/inactive`, 20 a page; `next_cursor` drives "Xem thêm". */
export function inactiveCustomersQuery(days: number, scope: InactiveScope = "with_sessions") {
  return infiniteQueryOptions({
    queryKey: ["customers", "inactive", days, scope],
    queryFn: async ({ pageParam }) =>
      (await api(
        `/customers/inactive?days=${String(days)}&scope=${scope}&limit=20${pageParam ? `&cursor=${encodeURIComponent(pageParam)}` : ""}`,
      )) as InactivePage,
    initialPageParam: "",
    getNextPageParam: (last) => last.next_cursor ?? undefined,
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
