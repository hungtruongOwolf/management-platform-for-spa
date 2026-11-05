import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { spaToday } from "../corrections/api";
import {
  addDays,
  compactVnd,
  dailyReportQuery,
  rangeReportQuery,
  relativeDay,
  shortDay,
  weekdayOf,
} from "./api";
import { BarChart, SplitBar } from "./charts";
import type { DailyReport, RangeReport } from "./contract";
import {
  CountUp,
  Delta,
  EmptyState,
  ErrorState,
  Loading,
  ManagementOnly,
  PageHeader,
  RowsSkeleton,
  StatCard,
  StatSkeleton,
  money,
} from "./ui";

type Metric = "revenue" | "visits";

/**
 * /bao-cao — "Tổng quan hôm nay": the day's headline numbers against yesterday, the last 7 days,
 * how the money came in, and each technician's work. Money appears only when the API sends it
 * (`report.revenue.read`); the counts are visible with `report.read`.
 */
export function ReportsPage() {
  return (
    <ManagementOnly>
      <ReportsPageBody />
    </ManagementOnly>
  );
}

function ReportsPageBody() {
  const today = spaToday();
  const [date, setDate] = useState(today);
  const daily = useQuery(dailyReportQuery(date));
  const range = useQuery(rangeReportQuery(addDays(date, -6), date));

  return (
    <section className="grid content-start gap-6">
      <PageHeader eyebrow="Báo cáo" title={date === today ? "Tổng quan hôm nay" : `Tổng quan ${relativeDay(date, today).toLowerCase()}`}>
        <div className="flex items-center gap-2" role="group" aria-label="Chọn ngày">
          <button
            type="button"
            className="btn btn-secondary px-3"
            aria-label="Ngày trước"
            onClick={() => {
              setDate((d) => addDays(d, -1));
            }}
          >
            <Icon name="chevron-left" />
          </button>
          <label className="sr-only" htmlFor="report-date">
            Ngày báo cáo
          </label>
          <input
            id="report-date"
            type="date"
            value={date}
            max={today}
            onChange={(e) => {
              setDate(e.target.value || today);
            }}
            className="min-h-11 rounded-[10px] border bg-surface px-3"
          />
          <button
            type="button"
            className="btn btn-secondary px-3"
            aria-label="Ngày sau"
            disabled={date >= today}
            onClick={() => {
              setDate((d) => addDays(d, 1));
            }}
          >
            <Icon name="chevron-right" />
          </button>
          {date !== today && (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                setDate(today);
              }}
            >
              Hôm nay
            </button>
          )}
        </div>
      </PageHeader>

      {daily.isError ? (
        <ErrorState error={daily.error} onRetry={() => void daily.refetch()} />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          {daily.data ? <Headline report={daily.data} /> : <StatSkeleton />}
        </div>
      )}

      <div className="grid gap-4 lg:grid-cols-3">
        <TrendCard range={range} date={date} />
        <MoneyMix report={daily.data} pending={daily.isPending} />
      </div>

      <div className="grid gap-4 lg:grid-cols-3">
        <section aria-labelledby="tech-title" className="card overflow-hidden lg:col-span-2">
          <h2 id="tech-title" className="flex items-center gap-2 px-4 pt-4 pb-3 font-semibold">
            <Icon name="users" className="size-5 text-accent" /> Kỹ thuật viên trong ngày
          </h2>
          {daily.data ? <Technicians report={daily.data} /> : <RowsSkeleton rows={4} />}
        </section>
        <TopServices range={range.data} />
      </div>
    </section>
  );
}

function Headline({ report }: { report: DailyReport }) {
  const y = report.compare.yesterday;
  const c = report.counts;
  return (
    <>
      {report.revenue && (
        <StatCard label="Thực thu" icon="wallet" accent foot={<span className="text-xs text-chrome-text-muted">Tiền mặt + chuyển khoản, đã trừ hủy và hoàn</span>}>
          <CountUp value={report.revenue.net.total} format={money} />
        </StatCard>
      )}
      <StatCard label="Khách" icon="users" foot={<Delta now={c.customers} before={y.customers} label="so với hôm qua" />}>
        <CountUp value={c.customers} />
      </StatCard>
      <StatCard label="Lượt khách" icon="desk" foot={<Delta now={c.visits} before={y.visits} label="so với hôm qua" />}>
        <CountUp value={c.visits} />
        {c.voided_visits > 0 && <span className="ml-2 text-sm font-normal text-text-muted">({c.voided_visits} đã hủy)</span>}
      </StatCard>
      <StatCard label="Buổi liệu trình đã trừ" icon="package" foot={<Delta now={c.sessions_used} before={y.sessions_used} label="so với hôm qua" />}>
        <CountUp value={c.sessions_used} />
      </StatCard>
      {!report.revenue && (
        <StatCard label="Gói bán ra" icon="layers" foot={<Delta now={c.packages_sold} before={y.packages_sold} label="so với hôm qua" />}>
          <CountUp value={c.packages_sold} />
        </StatCard>
      )}
    </>
  );
}

function TrendCard({ range, date }: { range: UseQueryResult<RangeReport>; date: string }) {
  const hasMoney = range.data?.series.some((b) => b.revenue) ?? false;
  const [metric, setMetric] = useState<Metric>("revenue");
  const shown: Metric = hasMoney ? metric : "visits";
  return (
    <section aria-labelledby="trend-title" className="card p-4 lg:col-span-2">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h2 id="trend-title" className="flex items-center gap-2 font-semibold">
          <Icon name="chart" className="size-5 text-accent" /> 7 ngày gần nhất
        </h2>
        {hasMoney && (
          <div className="segmented" role="group" aria-label="Chỉ số">
            {(
              [
                ["revenue", "Thực thu"],
                ["visits", "Lượt khách"],
              ] as const
            ).map(([m, label]) => (
              <button
                key={m}
                type="button"
                aria-pressed={shown === m}
                onClick={() => {
                  setMetric(m);
                }}
              >
                {label}
              </button>
            ))}
          </div>
        )}
      </div>
      {range.isError && <ErrorState error={range.error} onRetry={() => void range.refetch()} />}
      {range.isPending && (
        <Loading label="Đang tải biểu đồ">
          <div className="skeleton h-[220px]" />
        </Loading>
      )}
      {range.data && (
        <BarChart
          label={shown === "revenue" ? "Thực thu theo ngày" : "Lượt khách theo ngày"}
          highlight={date}
          format={shown === "revenue" ? compactVnd : (n) => String(n)}
          bars={range.data.series.map((b) => ({
            key: b.start,
            label: shortDay(b.start),
            sub: weekdayOf(b.start),
            value: shown === "revenue" ? (b.revenue?.net.total ?? 0) : b.counts.visits,
          }))}
        />
      )}
    </section>
  );
}

function MoneyMix({ report, pending }: { report: DailyReport | undefined; pending: boolean }) {
  return (
    <section aria-labelledby="mix-title" className="card p-4">
      <h2 id="mix-title" className="mb-4 flex items-center gap-2 font-semibold">
        <Icon name="wallet" className="size-5 text-accent" /> Cơ cấu trong ngày
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
      {report && report.revenue && (
        <div className="grid content-start gap-6">
          <SplitBar
            label="Theo hình thức"
            format={money}
            parts={[
              { label: "Tiền mặt", value: report.revenue.net.cash, tone: "primary" },
              { label: "Chuyển khoản", value: report.revenue.net.bank_transfer, tone: "accent" },
            ]}
          />
          <SplitBar
            label="Theo loại"
            format={money}
            parts={[
              { label: "Bán gói liệu trình", value: report.revenue.net_by_kind.package, tone: "primary" },
              { label: "Dịch vụ lẻ", value: report.revenue.net_by_kind.one_off, tone: "muted" },
            ]}
          />
          {(report.revenue.voids.total > 0 || report.revenue.refunds.total > 0) && (
            <p className="rounded-[10px] bg-surface-2 p-3 text-sm text-text-muted">
              Đã trừ: hủy {money(report.revenue.voids.total)}, hoàn tiền {money(report.revenue.refunds.total)}.
            </p>
          )}
        </div>
      )}
      {report && !report.revenue && (
        <dl className="grid grid-cols-2 gap-3 text-sm">
          <div className="rounded-[10px] bg-surface-2 p-3">
            <dt className="text-text-muted">Dịch vụ lẻ</dt>
            <dd className="text-xl font-semibold tabular">{report.counts.one_off_services}</dd>
          </div>
          <div className="rounded-[10px] bg-surface-2 p-3">
            <dt className="text-text-muted">Gói bán ra</dt>
            <dd className="text-xl font-semibold tabular">{report.counts.packages_sold}</dd>
          </div>
        </dl>
      )}
    </section>
  );
}

function Technicians({ report }: { report: DailyReport }) {
  if (report.technicians.length === 0) {
    return <EmptyState title="Chưa có lượt khách nào">Kỹ thuật viên sẽ hiện ở đây khi có lượt được ghi trong ngày.</EmptyState>;
  }
  const max = Math.max(...report.technicians.map((t) => t.visits));
  return (
    <div className="table-wrap">
      <table className="data-table">
        <thead>
          <tr>
            <th scope="col">Kỹ thuật viên</th>
            <th scope="col" className="num">
              Lượt
            </th>
            <th scope="col" className="num">
              Buổi liệu trình
            </th>
            <th scope="col" className="num">
              Dịch vụ lẻ
            </th>
            <th scope="col" className="w-1/4">
              <span className="sr-only">Tỷ lệ</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {report.technicians.map((t) => (
            <tr key={t.staff_id}>
              <td className="font-medium">{t.name}</td>
              <td className="num font-semibold">{t.visits}</td>
              <td className="num">{t.sessions_used}</td>
              <td className="num">{t.one_off_services}</td>
              <td aria-hidden="true">
                <span className="split-bar block h-2">
                  <span className="split-part tone-accent block" style={{ width: `${String((t.visits / max) * 100)}%` }} />
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function TopServices({ range }: { range: RangeReport | undefined }) {
  return (
    <section aria-labelledby="top-title" className="card p-4">
      <h2 id="top-title" className="mb-3 flex items-center gap-2 font-semibold">
        <Icon name="sparkle" className="size-5 text-accent" /> Dịch vụ dùng nhiều (7 ngày)
      </h2>
      {!range ? (
        <RowsSkeleton rows={4} />
      ) : range.top_services.length === 0 ? (
        <p className="text-sm text-text-muted">Chưa có dịch vụ nào trong 7 ngày.</p>
      ) : (
        <ol className="grid content-start gap-2">
          {range.top_services.slice(0, 5).map((s, i) => (
            <li key={s.service_id} className="flex items-center gap-3">
              <span aria-hidden="true" className="grid size-7 place-items-center rounded-full bg-accent-soft text-xs font-semibold text-accent-text">
                {i + 1}
              </span>
              <span className="min-w-0 flex-1 truncate">{s.name}</span>
              <span className="text-sm font-semibold tabular">{s.sessions_used + s.one_off_services} lượt</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
