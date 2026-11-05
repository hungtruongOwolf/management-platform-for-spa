import type { Granularity, InactiveScope } from "@vpyd/api-client";
import { infiniteQueryOptions, keepPreviousData, queryOptions } from "@tanstack/react-query";
import type { Bucket } from "./period";
import { api } from "../../lib/api";
import { spaToday } from "../corrections/api";
import type {
  CustomerStats,
  DailyReport,
  HistoryPage,
  InactivePage,
  LedgerPage,
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

/** API bucket of a period's chart (`/reports/range` `granularity`). */
export function granularityOf(bucket: Bucket): Granularity {
  // Adapter until the contract has `year` (feat/followup-and-reports-api).
  return bucket === "year" ? "month" : bucket;
}

/**
 * `GET /reports/range` for a period. The previous render stays on screen while the next period
 * loads (dataviz: refetch keeps the frame); the caller dims it through `isPlaceholderData`.
 */
export function rangeReportQuery(from: string, to: string, bucket: Bucket = "day") {
  const granularity = granularityOf(bucket);
  return queryOptions({
    queryKey: ["reports", "range", from, to, granularity],
    queryFn: async () => (await api(`/reports/range?from=${from}&to=${to}&granularity=${granularity}`)) as RangeReport,
    staleTime: 60_000,
    placeholderData: keepPreviousData,
  });
}

/** `GET /reports/ledger` (sổ doanh thu): 100 rows a page; `days[]` carries every day's totals. */
export function ledgerQuery(from: string, to: string) {
  return infiniteQueryOptions({
    queryKey: ["reports", "ledger", from, to],
    queryFn: async ({ pageParam }) =>
      (await api(
        `/reports/ledger?from=${from}&to=${to}&limit=100${pageParam ? `&cursor=${encodeURIComponent(pageParam)}` : ""}`,
      )) as LedgerPage,
    initialPageParam: "",
    getNextPageParam: (last) => last.next_cursor ?? undefined,
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

/** Query string of `GET /packages` for a filter chip and a search (customer name or phone). */
export function packagesPath(filter: PackageFilter, q = "", cursor = ""): string {
  const params = new URLSearchParams();
  if (filter === "expiring") params.set("expiring_within_days", "30");
  else if (filter !== "all") params.set("status", filter);
  if (q) params.set("q", q);
  params.set("limit", "50");
  if (cursor) params.set("cursor", cursor);
  return `/packages?${params.toString()}`;
}

/** `GET /packages`, 50 a page; `counts` (first page) count the whole spa for the same `q`. */
export function packagesOverviewQuery(filter: PackageFilter, q = "") {
  return infiniteQueryOptions({
    queryKey: ["packages", "overview", filter, q],
    queryFn: async ({ pageParam }) => (await api(packagesPath(filter, q, pageParam))) as PackageOverviewPage,
    initialPageParam: "",
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    staleTime: 30_000,
    placeholderData: keepPreviousData,
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
