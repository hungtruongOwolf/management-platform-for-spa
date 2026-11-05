/**
 * TODO(management-views): temporary mirror of the management-views API contract, copied from the
 * DTOs on branch feat/management-views (apps/api/crates/reporting/src/dto.rs). Once that branch is
 * merged and `pnpm api:gen` emits these schemas, replace this file with re-exports from
 * `@vpyd/api-client` (frontend.md: the frontend never owns DTO types) and let typecheck flag drift.
 * Money is integer đồng; fields behind `report.revenue.read` are optional because the server leaves
 * them out for callers without it.
 */

export interface MethodSplit {
  cash: number;
  bank_transfer: number;
  total: number;
}

export interface KindSplit {
  package: number;
  one_off: number;
}

export interface Revenue {
  collected: MethodSplit;
  collected_by_kind: KindSplit;
  voids: MethodSplit;
  voids_by_kind: KindSplit;
  refunds: MethodSplit;
  net: MethodSplit;
  net_by_kind: KindSplit;
}

export interface DayCounts {
  customers: number;
  visits: number;
  voided_visits: number;
  sessions_used: number;
  one_off_services: number;
  packages_sold: number;
}

export interface TechnicianDay {
  staff_id: string;
  name: string;
  visits: number;
  sessions_used: number;
  one_off_services: number;
}

export interface DayHeadline {
  date: string;
  customers: number;
  visits: number;
  sessions_used: number;
  packages_sold: number;
  net_revenue?: number;
}

export interface DailyReport {
  date: string;
  counts: DayCounts;
  revenue?: Revenue;
  technicians: TechnicianDay[];
  compare: { yesterday: DayHeadline; same_weekday_last_week: DayHeadline };
}

export interface RangeBucket {
  start: string;
  end: string;
  counts: DayCounts;
  revenue?: Revenue;
}

export interface TopService {
  service_id: string;
  name: string;
  sessions_used: number;
  one_off_services: number;
  one_off_revenue?: number;
}

export interface RangeReport {
  from: string;
  to: string;
  granularity: "day" | "week" | "month";
  totals: DayCounts;
  revenue?: Revenue;
  series: RangeBucket[];
  top_services: TopService[];
  top_packages: { plan_id: string | null; name: string; sold: number; revenue?: number }[];
}

export type HistoryKind = "visit" | "purchase" | "visit_voided" | "purchase_voided" | "package_cancelled";

export interface HistoryVisitLine {
  service_name: string;
  source: "package" | "one_off";
  customer_package_id: string | null;
  package_name: string | null;
  qty: number;
  sessions_left_after: number | null;
  staff_id: string;
  staff_name: string;
  amount: number;
}

export interface HistoryEvent {
  kind: HistoryKind;
  at: string;
  business_date: string;
  visit?: {
    id: string;
    voided: boolean;
    check_in_at: string;
    lines: HistoryVisitLine[];
    paid: MethodSplit;
    void_reason: string | null;
    voided_at: string | null;
  };
  purchase?: {
    sale_id: string;
    voided: boolean;
    lines: { description: string; qty: number; line_total: number; customer_package_id: string | null }[];
    subtotal: number;
    discount_amount: number;
    discount_reason: string | null;
    total: number;
    paid: MethodSplit;
    closer_name: string | null;
    void_reason: string | null;
    voided_at: string | null;
  };
  void?: { id: string; reason: string; reversed: MethodSplit; sessions_restored: number };
  cancellation?: {
    customer_package_id: string;
    package_name: string;
    reason: string;
    refund: { amount: number; method: string; business_date: string } | null;
  };
}

export interface HistoryPage {
  customer_id: string;
  events: HistoryEvent[];
  next_cursor: string | null;
}

export interface CustomerStats {
  customer_id: string;
  first_visit_on: string | null;
  last_visit_on: string | null;
  days_since_last_visit: number | null;
  visit_count: number;
  voided_visit_count: number;
  sessions_used: number;
  paid: number;
  voided: number;
  refunded: number;
  net_spent: number;
  packages: { active: number; expired: number; exhausted: number; cancelled: number };
  sessions_remaining: number;
}

export type PackageStatus = "active" | "expired" | "exhausted" | "cancelled";

export interface PackageOverviewRow {
  id: string;
  customer_id: string;
  customer_name: string;
  customer_phone: string;
  name: string;
  status: PackageStatus;
  sessions_total: number;
  sessions_remaining: number;
  starts_on: string;
  expires_on: string | null;
  days_to_expiry: number | null;
  last_used_on: string | null;
  cancelled_at: string | null;
}

export interface PackageOverviewPage {
  items: PackageOverviewRow[];
  counts: { active: number; expiring_soon: number; expired: number; exhausted: number; cancelled: number };
  expiring_within_days: number;
  next_cursor: string | null;
}

export interface InactiveCustomer {
  customer_id: string;
  full_name: string;
  phone: string;
  last_visit_on: string | null;
  last_seen_on: string;
  days_absent: number;
  active_packages: number;
  sessions_remaining: number;
}

export interface InactivePage {
  days: number;
  items: InactiveCustomer[];
  next_cursor: string | null;
}
