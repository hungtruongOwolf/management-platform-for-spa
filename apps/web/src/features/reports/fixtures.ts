/**
 * Contract-shaped sample data for tests (and nothing else: no app module imports this file, so it
 * never ships). Deterministic per date / customer through a seeded PRNG, consistent with the
 * contract's own sums (net = collected − voids − refunds, technicians add up to visits).
 */
import type {
  CustomerStats,
  DailyReport,
  DayCounts,
  HistoryEvent,
  HistoryPage,
  InactivePage,
  MethodSplit,
  PackageOverviewPage,
  PackageStatus,
  RangeReport,
  Revenue,
} from "./contract";

function rng(seedText: string): () => number {
  let h = 2166136261;
  for (const ch of seedText) h = Math.imul(h ^ ch.charCodeAt(0), 16777619);
  return () => {
    h = Math.imul(h ^ (h >>> 15), 2246822507);
    h = Math.imul(h ^ (h >>> 13), 3266489909);
    return ((h ^= h >>> 16) >>> 0) / 4294967296;
  };
}

const int = (r: () => number, lo: number, hi: number) => lo + Math.floor(r() * (hi - lo + 1));
const thousand = (n: number) => Math.round(n / 1000) * 1000;

function addDays(iso: string, days: number): string {
  const d = new Date(`${iso}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

const TECHNICIANS = ["Nguyễn Thị Lan", "Lê Thị Hoa", "Võ Thị Hằng", "Phạm Văn Nam", "Trần Thị Mai"];
const CUSTOMERS = [
  ["Phan Thị Thùy Dung", "0399000002"],
  ["Lê Quang Minh", "0912345678"],
  ["Bùi Thị Ngọc Hân", "0987111222"],
  ["Trần Thị Bích Ngọc", "0905333444"],
  ["Hoàng Thị Mai", "0935666777"],
  ["Nguyễn Văn Tài", "0978888999"],
  ["Huỳnh Thị Như Quỳnh", "0868123123"],
  ["Trương Thị Kim Liên", "0903456789"],
] as const;
const PACKAGES = ["Gội đầu dưỡng sinh 10 buổi", "Massage body Thân Tâm An Định 10+2", "Xông chân thảo dược 15 buổi", "Bổ Thận An 8 buổi"];
const SERVICES = ["Gội đầu dưỡng sinh", "Thân Tâm An Định", "Xông chân thảo dược", "Chuyên Sâu Đặc Biệt", "Bổ Thận An"];

function split(total: number, r: () => number): MethodSplit {
  const cash = thousand(total * (0.35 + r() * 0.3));
  return { cash, bank_transfer: total - cash, total };
}

function counts(r: () => number, weekend: boolean): DayCounts {
  const visits = int(r, weekend ? 22 : 12, weekend ? 38 : 28);
  return {
    customers: visits - int(r, 0, 3),
    visits,
    voided_visits: int(r, 0, 1),
    sessions_used: Math.round(visits * (0.55 + r() * 0.2)),
    one_off_services: Math.round(visits * (0.3 + r() * 0.2)),
    packages_sold: int(r, 0, 4),
  };
}

function revenue(c: DayCounts, r: () => number): Revenue {
  const pkg = thousand(c.packages_sold * (1_500_000 + r() * 1_800_000));
  const oneOff = thousand(c.one_off_services * (150_000 + r() * 120_000));
  const voidAmt = c.voided_visits ? thousand(100_000 + r() * 150_000) : 0;
  const collected = split(pkg + oneOff, r);
  const voids = split(voidAmt, r);
  const zero = { cash: 0, bank_transfer: 0, total: 0 };
  return {
    collected,
    collected_by_kind: { package: pkg, one_off: oneOff },
    voids,
    voids_by_kind: { package: 0, one_off: voidAmt },
    refunds: zero,
    net: {
      cash: collected.cash - voids.cash,
      bank_transfer: collected.bank_transfer - voids.bank_transfer,
      total: collected.total - voids.total,
    },
    net_by_kind: { package: pkg, one_off: oneOff - voidAmt },
  };
}

function day(date: string) {
  const r = rng(`day-${date}`);
  const weekday = new Date(`${date}T00:00:00Z`).getUTCDay();
  const c = counts(r, weekday === 0 || weekday === 6);
  return { c, rev: revenue(c, r), r };
}

export function mockDaily(date: string): DailyReport {
  const { c, rev, r } = day(date);
  const headline = (d: string) => {
    const o = day(d);
    return {
      date: d,
      customers: o.c.customers,
      visits: o.c.visits,
      sessions_used: o.c.sessions_used,
      packages_sold: o.c.packages_sold,
      net_revenue: o.rev.net.total,
    };
  };
  let left = c.visits;
  const technicians = TECHNICIANS.map((name, i) => {
    const visits = i === TECHNICIANS.length - 1 ? left : Math.min(left, int(r, 2, 8));
    left -= visits;
    const sessions = Math.round(visits * 0.6);
    return { staff_id: `mock-staff-${String(i)}`, name, visits, sessions_used: sessions, one_off_services: visits - sessions };
  })
    .filter((t) => t.visits > 0)
    .sort((a, b) => b.visits - a.visits);
  return {
    date,
    counts: c,
    revenue: rev,
    technicians,
    compare: { yesterday: headline(addDays(date, -1)), same_weekday_last_week: headline(addDays(date, -7)) },
  };
}

export function mockRange(from: string, to: string): RangeReport {
  const series: RangeReport["series"] = [];
  for (let d = from; d <= to; d = addDays(d, 1)) {
    const { c, rev } = day(d);
    series.push({ start: d, end: d, counts: c, revenue: rev });
  }
  const sum = (f: (c: DayCounts) => number) => series.reduce((s, b) => s + f(b.counts), 0);
  const totals: DayCounts = {
    customers: sum((c) => c.customers),
    visits: sum((c) => c.visits),
    voided_visits: sum((c) => c.voided_visits),
    sessions_used: sum((c) => c.sessions_used),
    one_off_services: sum((c) => c.one_off_services),
    packages_sold: sum((c) => c.packages_sold),
  };
  const r = rng(`range-${from}-${to}`);
  return {
    from,
    to,
    granularity: "day",
    totals,
    series,
    top_services: SERVICES.map((name, i) => ({
      service_id: `mock-service-${String(i)}`,
      name,
      sessions_used: int(r, 10, 60),
      one_off_services: int(r, 2, 20),
    })).sort((a, b) => b.sessions_used + b.one_off_services - a.sessions_used - a.one_off_services),
    top_packages: PACKAGES.map((name, i) => ({ plan_id: `mock-plan-${String(i)}`, name, sold: int(r, 1, 9) })),
  };
}

function iso(date: string, hour: number, minute: number): string {
  // Spa time is UTC+7.
  return new Date(Date.UTC(+date.slice(0, 4), +date.slice(5, 7) - 1, +date.slice(8, 10), hour - 7, minute)).toISOString();
}

export function mockHistory(customerId: string, today: string): HistoryPage {
  const r = rng(`history-${customerId}`);
  const pkg = PACKAGES[int(r, 0, PACKAGES.length - 1)] ?? "Gói liệu trình";
  const events: HistoryEvent[] = [];
  let date = addDays(today, -int(r, 0, 3));
  const left = int(r, 2, 8);
  for (let i = 0; i < 9; i++) {
    const at = iso(date, int(r, 9, 19), int(r, 0, 59));
    const staff = TECHNICIANS[int(r, 0, TECHNICIANS.length - 1)] ?? "KTV";
    const oneOff = r() < 0.3;
    const voided = i === 4;
    const amount = oneOff ? thousand(150_000 + r() * 100_000) : 0;
    events.push({
      kind: voided ? "visit_voided" : "visit",
      at,
      business_date: date,
      visit: {
        id: `mock-visit-${String(i)}`,
        voided,
        check_in_at: at,
        lines: [
          {
            service_name: pkg.replace(/ \d.*$/, ""),
            source: "package",
            customer_package_id: "mock-package",
            package_name: pkg,
            qty: 1,
            sessions_left_after: left + i,
            staff_id: "mock-staff",
            staff_name: staff,
            amount: 0,
          },
          ...(oneOff
            ? [
                {
                  service_name: SERVICES[int(r, 0, SERVICES.length - 1)] ?? "Dịch vụ lẻ",
                  source: "one_off" as const,
                  customer_package_id: null,
                  package_name: null,
                  qty: 1,
                  sessions_left_after: null,
                  staff_id: "mock-staff",
                  staff_name: staff,
                  amount,
                },
              ]
            : []),
        ],
        paid: split(amount, r),
        void_reason: voided ? "Ghi nhầm khách" : null,
        voided_at: voided ? at : null,
      },
    });
    date = addDays(date, -int(r, 4, 12));
  }
  const total = thousand(2_000_000 + r() * 2_500_000);
  events.push({
    kind: "purchase",
    at: iso(date, 10, 15),
    business_date: date,
    purchase: {
      sale_id: "mock-sale",
      voided: false,
      lines: [{ description: pkg, qty: 1, line_total: total, customer_package_id: "mock-package" }],
      subtotal: total,
      discount_amount: 0,
      discount_reason: null,
      total,
      paid: split(total, r),
      closer_name: "Nguyễn Thị Thu Hà",
      void_reason: null,
      voided_at: null,
    },
  });
  return { customer_id: customerId, events, next_cursor: null };
}

export function mockCustomerStats(customerId: string, history: HistoryPage): CustomerStats {
  const visits = history.events.filter((e) => e.kind === "visit");
  const paid = history.events.reduce((s, e) => s + (e.purchase?.paid.total ?? 0) + (e.visit?.paid.total ?? 0), 0);
  const voided = history.events.reduce((s, e) => s + (e.kind === "visit_voided" ? (e.visit?.paid.total ?? 0) : 0), 0);
  const last = visits[0]?.business_date ?? null;
  const today = new Date().toISOString().slice(0, 10);
  return {
    customer_id: customerId,
    first_visit_on: visits.at(-1)?.business_date ?? null,
    last_visit_on: last,
    days_since_last_visit: last ? Math.round((Date.parse(today) - Date.parse(last)) / 86_400_000) : null,
    visit_count: visits.length,
    voided_visit_count: history.events.length - visits.length - 1,
    sessions_used: visits.length,
    paid,
    voided,
    refunded: 0,
    net_spent: paid - voided,
    packages: { active: 1, expired: 0, exhausted: 1, cancelled: 0 },
    sessions_remaining: visits[0]?.visit?.lines[0]?.sessions_left_after ?? 0,
  };
}

const STATUSES: PackageStatus[] = ["active", "active", "active", "active", "expired", "exhausted", "cancelled"];

export function mockPackages(today: string, status: PackageStatus | "expiring" | "all"): PackageOverviewPage {
  const r = rng("packages");
  const all = Array.from({ length: 28 }, (_, i) => {
    const [name, phone] = CUSTOMERS[i % CUSTOMERS.length] ?? ["Khách", "0900000000"];
    const st = STATUSES[int(r, 0, STATUSES.length - 1)] ?? "active";
    const total = [10, 12, 15, 8][i % 4] ?? 10;
    const remaining = st === "exhausted" ? 0 : int(r, 1, total - 1);
    const days = st === "expired" ? -int(r, 1, 40) : int(r, 3, 200);
    return {
      id: `mock-package-${String(i)}`,
      customer_id: `mock-customer-${String(i % CUSTOMERS.length)}`,
      customer_name: name,
      customer_phone: phone,
      name: PACKAGES[i % PACKAGES.length] ?? "Gói",
      status: st,
      sessions_total: total,
      sessions_remaining: remaining,
      starts_on: addDays(today, days - 365),
      expires_on: addDays(today, days),
      days_to_expiry: days,
      last_used_on: addDays(today, -int(r, 0, 60)),
      cancelled_at: st === "cancelled" ? iso(addDays(today, -int(r, 5, 30)), 15, 0) : null,
    };
  });
  const expiring = (p: (typeof all)[number]) => p.status === "active" && p.days_to_expiry <= 30;
  const items =
    status === "all"
      ? all
      : status === "expiring"
        ? all.filter(expiring).sort((a, b) => a.days_to_expiry - b.days_to_expiry)
        : all.filter((p) => p.status === status);
  const n = (s: PackageStatus) => all.filter((p) => p.status === s).length;
  return {
    items,
    counts: { active: n("active"), expiring_soon: all.filter(expiring).length, expired: n("expired"), exhausted: n("exhausted"), cancelled: n("cancelled") },
    expiring_within_days: 30,
    next_cursor: null,
  };
}

export function mockInactive(today: string, days: number): InactivePage {
  const r = rng(`inactive-${String(days)}`);
  const items = CUSTOMERS.map(([full_name, phone], i) => {
    const absent = days + int(r, 0, 60);
    const last = addDays(today, -absent);
    return {
      customer_id: `mock-customer-${String(i)}`,
      full_name,
      phone,
      last_visit_on: last,
      last_seen_on: last,
      days_absent: absent,
      active_packages: int(r, 1, 2),
      sessions_remaining: int(r, 1, 9),
    };
  }).sort((a, b) => b.days_absent - a.days_absent);
  return { days, items, next_cursor: null };
}
