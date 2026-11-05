import { useQuery, useSuspenseQuery, type UseQueryResult } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { Icon } from "../../app/Icon";
import { DateField } from "../../components/DateField";
import { ResponsiveTable } from "../../components/ResponsiveTable";
import { dayLabel } from "../../lib/format";
import { serviceLabel } from "../../lib/names";
import { useSearchState } from "../../lib/searchState";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import { compactVnd, rangeReportQuery, shortDay } from "./api";
import { SplitBar } from "./charts";
import type { RangeBucket, RangeReport, Revenue } from "./contract";
import { LedgerTab } from "./LedgerTab";
import { LineChart } from "./LineChart";
import {
  MAX_RANGE_DAYS,
  PRESETS,
  PRESET_LABEL,
  bucketLabel,
  daysBetween,
  rangeLabel,
  resolvePeriod,
  type Bucket,
  type Period,
  type Preset,
} from "./period";
import { reportsSearch, type ReportMetric } from "./search";
import { CountUp, Delta, EmptyState, ErrorState, Loading, ManagementOnly, PageHeader, RowsSkeleton, StatCard, StatSkeleton, money } from "./ui";

type Metric = ReportMetric;

/**
 * /bao-cao: a period (preset or custom, in the URL) → headline numbers against the previous period
 * of the same length, the trend as a line chart, the breakdown per day/week/month, technicians and
 * what sold most; and the "Sổ doanh thu" tab (the paper revenue book). Money appears only when the
 * API sends it (`report.revenue.read`); the counts need `report.read`.
 */
export function ReportsPage() {
  return (
    <ManagementOnly>
      <ReportsPageBody />
    </ManagementOnly>
  );
}

function ReportsPageBody() {
  const { data: me } = useSuspenseQuery(meQuery);
  const [search, patch] = useSearchState(reportsSearch);
  const today = spaToday();
  const period = resolvePeriod(search.period, today, { from: search.from, to: search.to });
  const canLedger = can(me, "report.revenue.read");
  const tab = search.tab === "ledger" && canLedger ? "ledger" : "overview";

  return (
    <section className="grid content-start gap-5">
      <PageHeader eyebrow="Báo cáo" title={tab === "ledger" ? "Sổ doanh thu" : "Báo cáo kinh doanh"} />
      {canLedger && (
        <div role="tablist" aria-label="Loại báo cáo" className="tabs">
          {(
            [
              ["overview", "Tổng quan"],
              ["ledger", "Sổ doanh thu"],
            ] as const
          ).map(([t, label]) => (
            <button
              key={t}
              type="button"
              role="tab"
              id={`tab-${t}`}
              aria-selected={tab === t}
              aria-controls="report-panel"
              onClick={() => {
                patch({ tab: t });
              }}
            >
              {label}
            </button>
          ))}
        </div>
      )}
      <PeriodPicker
        period={period}
        today={today}
        onPreset={(p) => {
          patch(p === "custom" ? { period: p, from: period.from, to: period.to } : { period: p, from: "", to: "" });
        }}
        onCustom={(from, to) => {
          patch({ period: "custom", from, to });
        }}
      />
      <div id="report-panel" role={canLedger ? "tabpanel" : undefined} aria-labelledby={canLedger ? `tab-${tab}` : undefined} className="grid content-start gap-5">
        {tab === "ledger" ? (
          <LedgerTab period={period} />
        ) : (
          <Overview
            period={period}
            metric={search.metric}
            onMetric={(m) => {
              patch({ metric: m });
            }}
          />
        )}
      </div>
    </section>
  );
}

/** Presets as one chip row (nobody fights a calendar for "30 ngày"); a custom range behind "Tùy chọn". */
function PeriodPicker({
  period,
  today,
  onPreset,
  onCustom,
}: {
  period: Period;
  today: string;
  onPreset: (p: Preset) => void;
  onCustom: (from: string, to: string) => void;
}) {
  const [draft, setDraft] = useState({ from: period.from, to: period.to });
  const [error, setError] = useState<string | null>(null);
  const change = (next: { from: string; to: string }) => {
    setDraft(next);
    if (!next.from || !next.to) return;
    const problem =
      next.from > next.to ? "Ngày bắt đầu phải trước ngày kết thúc." : daysBetween(next.from, next.to) > MAX_RANGE_DAYS ? "Chọn tối đa 2 năm." : null;
    if (problem) {
      setError(problem);
      return;
    }
    setError(null);
    onCustom(next.from, next.to);
  };
  return (
    <div className="card grid content-start gap-3 p-4">
      <div className="segmented chip-row" role="group" aria-label="Khoảng thời gian">
        {PRESETS.map((p) => (
          <button
            key={p}
            type="button"
            aria-pressed={period.preset === p}
            onClick={() => {
              setDraft({ from: period.from, to: period.to });
              setError(null);
              onPreset(p);
            }}
          >
            {PRESET_LABEL[p]}
          </button>
        ))}
      </div>
      {period.preset === "custom" && (
        <div className="filter-bar">
          <DateField
            label="Từ ngày"
            value={draft.from}
            max={today}
            onChange={(from) => {
              change({ ...draft, from });
            }}
          />
          <DateField
            label="Đến ngày"
            value={draft.to}
            max={today}
            onChange={(to) => {
              change({ ...draft, to });
            }}
          />
          {error && (
            <p role="alert" className="pb-3 text-sm text-danger">
              {error}
            </p>
          )}
        </div>
      )}
      <p className="flex flex-wrap items-center gap-x-2 text-sm text-text-muted">
        <Icon name="calendar" className="size-4 text-accent" />
        <span className="font-semibold text-text">{period.label}</span>
        {period.label !== rangeLabel(period.from, period.to) && <span className="tabular">({rangeLabel(period.from, period.to)})</span>}
        <span>
          · {period.compareLabel}: <span className="tabular">{rangeLabel(period.previous.from, period.previous.to)}</span>
        </span>
      </p>
    </div>
  );
}

function bucketTitle(b: RangeBucket, bucket: Bucket): string {
  if (bucket === "day") return dayLabel(b.start);
  if (bucket === "week") return `Tuần ${shortDay(b.start)} – ${shortDay(b.end)}`;
  if (bucket === "month") return `Tháng ${String(Number(b.start.slice(5, 7)))}/${b.start.slice(0, 4)}`;
  return `Năm ${b.start.slice(0, 4)}`;
}

function Overview({ period, metric, onMetric }: { period: Period; metric: Metric; onMetric: (m: Metric) => void }) {
  const range = useQuery(rangeReportQuery(period.from, period.to, period.bucket));
  const previous = useQuery(rangeReportQuery(period.previous.from, period.previous.to, period.bucket));
  const refreshing = range.isPlaceholderData;

  if (range.isError) return <ErrorState error={range.error} onRetry={() => void range.refetch()} />;
  const data = range.data;
  return (
    <div className={`grid content-start gap-5 ${refreshing ? "is-refreshing" : ""}`} aria-busy={refreshing || range.isPending}>
      <div className={`grid grid-cols-2 gap-3 sm:gap-4 ${data?.revenue ? "lg:grid-cols-5" : "lg:grid-cols-4"}`}>
        {data ? <Kpis range={data} previous={previous.data} compareLabel={period.compareLabel} /> : <StatSkeleton />}
      </div>
      <div className="grid gap-4 lg:grid-cols-3">
        <TrendCard range={range} previous={previous.data} period={period} metric={metric} onMetric={onMetric} />
        <MoneyMix revenue={data?.revenue} pending={range.isPending} title={`Cơ cấu thu · ${period.label}`} />
      </div>
      {data && <Breakdown range={data} bucket={period.bucket} />}
      <div className="grid gap-4 lg:grid-cols-3">
        <TechniciansCard range={data} period={period} />
        <TopServices range={data} label={period.label} />
        <TopPackages range={data} label={period.label} />
      </div>
    </div>
  );
}

function Kpis({ range, previous, compareLabel }: { range: RangeReport; previous: RangeReport | undefined; compareLabel: string }) {
  const c = range.totals;
  const p = previous?.totals;
  const delta = (now: number, before: number | undefined, onChrome = false) =>
    before === undefined ? null : <Delta now={now} before={before} label={compareLabel} onChrome={onChrome} />;
  return (
    <>
      {range.revenue && (
        <div className="col-span-2 lg:col-span-1">
          <StatCard label="Thực thu" icon="wallet" accent foot={delta(range.revenue.net.total, previous?.revenue?.net.total, true)}>
            <CountUp value={range.revenue.net.total} format={money} />
          </StatCard>
        </div>
      )}
      <StatCard label="Khách" icon="users" foot={delta(c.customers, p?.customers)}>
        <CountUp value={c.customers} />
      </StatCard>
      <StatCard label="Lượt khách" icon="desk" foot={delta(c.visits, p?.visits)}>
        <CountUp value={c.visits} />
        {c.voided_visits > 0 && <span className="ml-2 text-sm font-normal text-text-muted">({c.voided_visits} đã hủy)</span>}
      </StatCard>
      <StatCard label="Buổi liệu trình đã trừ" icon="package" foot={delta(c.sessions_used, p?.sessions_used)}>
        <CountUp value={c.sessions_used} />
      </StatCard>
      <StatCard label="Gói liệu trình bán ra" icon="layers" foot={delta(c.packages_sold, p?.packages_sold)}>
        <CountUp value={c.packages_sold} />
      </StatCard>
    </>
  );
}

const METRIC_LABEL: Record<Metric, string> = { revenue: "Thực thu", customers: "Khách", visits: "Lượt khách" };

function valueOf(b: RangeBucket | undefined, metric: Metric): number | undefined {
  if (!b) return undefined;
  if (metric === "revenue") return b.revenue?.net.total;
  return metric === "customers" ? b.counts.customers : b.counts.visits;
}

function TrendCard({
  range,
  previous,
  period,
  metric,
  onMetric,
}: {
  range: UseQueryResult<RangeReport>;
  previous: RangeReport | undefined;
  period: Period;
  metric: Metric;
  onMetric: (m: Metric) => void;
}) {
  const hasMoney = range.data?.revenue != null;
  const shown: Metric = metric === "revenue" && !hasMoney ? "visits" : metric;
  const metrics: Metric[] = hasMoney ? ["revenue", "customers", "visits"] : ["customers", "visits"];
  const isMoney = shown === "revenue";
  return (
    <section aria-labelledby="trend-title" className="card grid content-start gap-3 p-4 lg:col-span-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 id="trend-title" className="flex items-center gap-2 font-semibold">
          <Icon name="chart" className="size-5 text-accent" /> Xu hướng {METRIC_LABEL[shown].toLowerCase()}
        </h2>
        <div className="segmented" role="group" aria-label="Chỉ số trên biểu đồ">
          {metrics.map((m) => (
            <button
              key={m}
              type="button"
              aria-pressed={shown === m}
              onClick={() => {
                onMetric(m);
              }}
            >
              {METRIC_LABEL[m]}
            </button>
          ))}
        </div>
      </div>
      {range.isPending && (
        <Loading label="Đang tải biểu đồ">
          <div className="skeleton h-[260px]" />
        </Loading>
      )}
      {range.data && (
        <LineChart
          label={`${METRIC_LABEL[shown]} theo ${period.bucket === "day" ? "ngày" : period.bucket === "week" ? "tuần" : period.bucket === "month" ? "tháng" : "năm"}, ${period.label}`}
          seriesLabel={period.label}
          previousLabel={previous ? "Kỳ trước" : undefined}
          format={isMoney ? money : (n) => n.toLocaleString("vi-VN")}
          axisFormat={isMoney ? compactVnd : (n) => n.toLocaleString("vi-VN")}
          points={range.data.series.map((b, i) => ({
            key: b.start,
            label: bucketLabel(b.start, period.bucket),
            title: bucketTitle(b, period.bucket),
            value: valueOf(b, shown) ?? 0,
            previous: valueOf(previous?.series[i], shown),
          }))}
        />
      )}
    </section>
  );
}

function MoneyMix({ revenue, pending, title }: { revenue: Revenue | null | undefined; pending: boolean; title: string }) {
  if (!pending && !revenue) return null;
  return (
    <section aria-labelledby="mix-title" className="card p-4">
      <h2 id="mix-title" className="mb-4 flex items-center gap-2 font-semibold">
        <Icon name="wallet" className="size-5 text-accent" /> {title}
      </h2>
      {pending && (
        <Loading label="Đang tải cơ cấu">
          <div className="grid content-start gap-3">
            <div className="skeleton h-3" />
            <div className="skeleton h-16" />
            <div className="skeleton h-3" />
            <div className="skeleton h-16" />
          </div>
        </Loading>
      )}
      {revenue && (
        <div className="grid content-start gap-6">
          <SplitBar
            label="Theo hình thức"
            format={money}
            parts={[
              { label: "Tiền mặt", value: revenue.net.cash, tone: "primary" },
              { label: "Chuyển khoản", value: revenue.net.bank_transfer, tone: "accent" },
            ]}
          />
          <SplitBar
            label="Theo loại"
            format={money}
            parts={[
              { label: "Bán gói liệu trình", value: revenue.net_by_kind.package, tone: "primary" },
              { label: "Dịch vụ lẻ", value: revenue.net_by_kind.one_off, tone: "muted" },
            ]}
          />
          {(revenue.voids.total > 0 || revenue.refunds.total > 0) && (
            <p className="rounded-[10px] bg-surface-2 p-3 text-sm text-text-muted">
              Đã trừ: hủy {money(revenue.voids.total)}, hoàn tiền {money(revenue.refunds.total)}.
            </p>
          )}
        </div>
      )}
    </section>
  );
}

const PERIOD_COLUMN: Record<Bucket, string> = { day: "Ngày", week: "Tuần", month: "Tháng", year: "Năm" };

/** Per bucket, like the paper book's daily totals; also the table view of the chart (dataviz). */
function Breakdown({ range, bucket }: { range: RangeReport; bucket: Bucket }) {
  const withMoney = range.revenue != null;
  const rows = [...range.series].reverse();
  const cell = (n: number | undefined) => (n === undefined ? "" : money(n));
  return (
    <section aria-labelledby="breakdown-title" className="card overflow-hidden">
      <h2 id="breakdown-title" className="flex items-center gap-2 px-4 pt-4 pb-3 font-semibold">
        <Icon name="menu-book" className="size-5 text-accent" /> Chi tiết theo {PERIOD_COLUMN[bucket].toLowerCase()}
      </h2>
      <ResponsiveTable
        caption={`Chi tiết theo ${PERIOD_COLUMN[bucket].toLowerCase()}, mới nhất trước`}
        rows={rows}
        rowKey={(b) => b.start}
        cardsBelow={1024}
        className="rt-pad"
        columns={[
          { id: "period", header: PERIOD_COLUMN[bucket], card: "title", rowHeader: true, className: "font-medium whitespace-nowrap", cell: (b) => bucketTitle(b, bucket) },
          { id: "customers", header: "Khách", numeric: true, cell: (b) => b.counts.customers },
          { id: "visits", header: "Lượt", numeric: true, cell: (b) => b.counts.visits },
          { id: "sessions", header: "Buổi LT", cardLabel: "Buổi liệu trình", numeric: true, cell: (b) => b.counts.sessions_used },
          { id: "oneoff", header: "DV lẻ", cardLabel: "Dịch vụ lẻ", numeric: true, cell: (b) => b.counts.one_off_services },
          { id: "sold", header: "Gói LT bán", cardLabel: "Gói liệu trình bán", numeric: true, cell: (b) => b.counts.packages_sold },
          ...(withMoney
            ? [
                { id: "cash", header: "Tiền mặt", numeric: true, cell: (b: RangeBucket) => cell(b.revenue?.net.cash) },
                { id: "bank", header: "Chuyển khoản", numeric: true, cell: (b: RangeBucket) => cell(b.revenue?.net.bank_transfer) },
                { id: "pkg", header: "Liệu trình", cardLabel: "Thu bán gói", numeric: true, cell: (b: RangeBucket) => cell(b.revenue?.net_by_kind.package) },
                { id: "one", header: "Dịch vụ lẻ", cardLabel: "Thu dịch vụ lẻ", numeric: true, cell: (b: RangeBucket) => cell(b.revenue?.net_by_kind.one_off) },
                { id: "net", header: "Thực thu", numeric: true, card: "aside" as const, className: "font-semibold", cell: (b: RangeBucket) => cell(b.revenue?.net.total) },
              ]
            : []),
        ]}
        footer={{
          table: (
            <tr className="ledger-day">
              <th scope="row">Cả kỳ</th>
              <td className="num">{range.totals.customers}</td>
              <td className="num">{range.totals.visits}</td>
              <td className="num">{range.totals.sessions_used}</td>
              <td className="num">{range.totals.one_off_services}</td>
              <td className="num">{range.totals.packages_sold}</td>
              {range.revenue && (
                <>
                  <td className="num">{money(range.revenue.net.cash)}</td>
                  <td className="num">{money(range.revenue.net.bank_transfer)}</td>
                  <td className="num">{money(range.revenue.net_by_kind.package)}</td>
                  <td className="num">{money(range.revenue.net_by_kind.one_off)}</td>
                  <td className="num">{money(range.revenue.net.total)}</td>
                </>
              )}
            </tr>
          ),
          cards: (
            <p className="rt-card ledger-day flex justify-between font-semibold">
              <span>Cả kỳ</span>
              <span className="tabular">
                {range.revenue ? money(range.revenue.net.total) : `${String(range.totals.visits)} lượt`}
              </span>
            </p>
          ),
        }}
      />
    </section>
  );
}

/** Each technician's visits, sessions and one-off services over the period, most visits first. */
function TechniciansCard({ range, period }: { range: RangeReport | undefined; period: Period }) {
  const technicians = range?.technicians;
  return (
    <CardList title={`Kỹ thuật viên · ${period.label}`} icon="users">
      {!technicians ? (
        <RowsSkeleton rows={4} />
      ) : technicians.length === 0 ? (
        <EmptyState title="Chưa có lượt khách nào">Kỹ thuật viên sẽ hiện ở đây khi có lượt được ghi.</EmptyState>
      ) : (
        <ol className="grid content-start gap-3">
          {technicians.map((t) => (
            <li key={t.staff_id} className="grid gap-1">
              <span className="flex items-baseline justify-between gap-2">
                <span className="truncate font-medium">{t.name}</span>
                <span className="text-sm font-semibold whitespace-nowrap tabular">{t.visits} lượt</span>
              </span>
              <span className="text-xs text-text-muted">
                {t.sessions_used} buổi liệu trình · {t.one_off_services} dịch vụ lẻ
              </span>
              <span aria-hidden="true" className="split-bar block h-1.5">
                <span
                  className="split-part tone-accent block"
                  style={{ width: `${String((t.visits / Math.max(1, ...technicians.map((x) => x.visits))) * 100)}%` }}
                />
              </span>
            </li>
          ))}
        </ol>
      )}
    </CardList>
  );
}

function CardList({ title, icon, children }: { title: string; icon: "users" | "sparkle" | "package"; children: ReactNode }) {
  return (
    <section aria-label={title} className="card grid content-start gap-3 p-4">
      <h2 className="flex items-center gap-2 font-semibold">
        <Icon name={icon} className="size-5 text-accent" /> {title}
      </h2>
      {children}
    </section>
  );
}

function Rank({ i }: { i: number }) {
  return (
    <span aria-hidden="true" className="grid size-7 flex-none place-items-center rounded-full bg-accent-soft text-xs font-semibold text-accent-text">
      {i + 1}
    </span>
  );
}

function TopServices({ range, label }: { range: RangeReport | undefined; label: string }) {
  return (
    <CardList title={`Dịch vụ dùng nhiều · ${label}`} icon="sparkle">
      {!range ? (
        <RowsSkeleton rows={4} />
      ) : range.top_services.length === 0 ? (
        <p className="text-sm text-text-muted">Chưa có dịch vụ nào trong kỳ này.</p>
      ) : (
        <ol className="grid content-start gap-2">
          {range.top_services.slice(0, 5).map((s, i) => (
            <li key={s.service_id} className="flex items-center gap-3">
              <Rank i={i} />
              <span className="min-w-0 flex-1">
                <span className="block">{serviceLabel(s.name, s.category_name)}</span>
                {s.one_off_revenue != null && s.one_off_revenue > 0 && (
                  <span className="block text-xs text-text-muted tabular">lẻ {money(s.one_off_revenue)}</span>
                )}
              </span>
              <span className="text-sm font-semibold whitespace-nowrap tabular">{s.sessions_used + s.one_off_services} lượt</span>
            </li>
          ))}
        </ol>
      )}
    </CardList>
  );
}

function TopPackages({ range, label }: { range: RangeReport | undefined; label: string }) {
  return (
    <CardList title={`Mẫu gói bán chạy · ${label}`} icon="package">
      {!range ? (
        <RowsSkeleton rows={4} />
      ) : range.top_packages.length === 0 ? (
        <p className="text-sm text-text-muted">Chưa bán gói nào trong kỳ này.</p>
      ) : (
        <ol className="grid content-start gap-2">
          {range.top_packages.slice(0, 5).map((p, i) => (
            <li key={p.plan_id ?? p.name} className="flex items-center gap-3">
              <Rank i={i} />
              <span className="min-w-0 flex-1">
                <span className="block">{p.name}</span>
                {p.revenue != null && <span className="block text-xs text-text-muted tabular">{money(p.revenue)}</span>}
              </span>
              <span className="text-sm font-semibold whitespace-nowrap tabular">{p.sold} gói</span>
            </li>
          ))}
        </ol>
      )}
    </CardList>
  );
}
