import { useInfiniteQuery } from "@tanstack/react-query";
import { Fragment } from "react";
import { longDay, ledgerQuery, weekdayOf } from "./api";
import type { LedgerDay, LedgerRow } from "./contract";
import { CustomerCell } from "./PackagesOverviewPage";
import { daysBetween, type Period } from "./period";
import { EmptyState, ErrorState, RowsSkeleton, money } from "./ui";

/** `GET /reports/ledger` accepts at most 92 days after `from`. */
export const LEDGER_MAX_DAYS = 93;

const time = new Intl.DateTimeFormat("vi-VN", { hour: "2-digit", minute: "2-digit", timeZone: "Asia/Ho_Chi_Minh" });

const ENTRY_LABEL: Record<LedgerRow["entry"], string> = { sale: "", void: "Hủy hóa đơn", refund: "Hoàn tiền" };

/**
 * "Sổ doanh thu" (spec R1-2 US3): the paper revenue book's columns, one row per sale, void or
 * refund, grouped by day with the day's totals (`days[]` covers every day of the range, not only
 * the loaded page). Void and refund rows are negative and red; an original that was voided later
 * is struck through. Needs `report.revenue.read` (the tab is hidden otherwise; the API says 403).
 */
export function LedgerTab({ period }: { period: Period }) {
  const tooLong = daysBetween(period.from, period.to) > LEDGER_MAX_DAYS;
  const page = useInfiniteQuery({ ...ledgerQuery(period.from, period.to), enabled: !tooLong });
  if (tooLong) {
    return (
      <p role="status" className="card p-4 text-text-muted">
        Sổ doanh thu xem tối đa 3 tháng một lần. Chọn “30 ngày”, “Tháng này”, “Tháng trước” hoặc một khoảng tùy chọn ngắn hơn.
      </p>
    );
  }
  if (page.isError) return <ErrorState error={page.error} onRetry={() => void page.refetch()} />;
  if (page.isPending) return <RowsSkeleton rows={8} />;
  const days = page.data.pages[0]?.days ?? [];
  const rows = page.data.pages.flatMap((p) => p.rows);
  const dayOf = new Map(days.map((d) => [d.date, d]));
  const sum = days.reduce((s, d) => ({ cash: s.cash + d.cash, bank: s.bank + d.bank_transfer, total: s.total + d.total, entries: s.entries + d.entries }), {
    cash: 0,
    bank: 0,
    total: 0,
    entries: 0,
  });
  return (
    <>
      <dl className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Figure label="Số giao dịch" value={sum.entries.toLocaleString("vi-VN")} />
        <Figure label="Tiền mặt" value={money(sum.cash)} />
        <Figure label="Chuyển khoản" value={money(sum.bank)} />
        <Figure label="Tổng thực thu" value={money(sum.total)} strong />
      </dl>
      <div className="card overflow-hidden">
        {rows.length === 0 ? (
          <EmptyState title="Chưa có giao dịch nào">Hóa đơn, khoản hủy và hoàn tiền trong kỳ sẽ hiện ở đây.</EmptyState>
        ) : (
          <div className="table-wrap" tabIndex={0} role="region" aria-label="Sổ doanh thu (cuộn ngang để xem đủ cột)">
            <table className="data-table">
              <caption className="sr-only">Sổ doanh thu, theo ngày rồi theo giờ ghi</caption>
              <thead>
                <tr>
                  <th scope="col">Giờ</th>
                  <th scope="col">Khách</th>
                  <th scope="col">Dịch vụ 1 lần</th>
                  <th scope="col">Liệu trình</th>
                  <th scope="col" className="num">
                    Chuyển khoản
                  </th>
                  <th scope="col" className="num">
                    Tiền mặt
                  </th>
                  <th scope="col" className="num">
                    Chưa thanh toán
                  </th>
                  <th scope="col">Ghi chú</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r, i) => {
                  const startsDay = rows[i - 1]?.business_date !== r.business_date;
                  const day = dayOf.get(r.business_date);
                  return (
                    <Fragment key={`${r.entry}-${r.id}`}>
                      {startsDay && <DayRow date={r.business_date} day={day} />}
                      <tr className={r.entry === "sale" ? (r.voided ? "ledger-voided" : "") : "ledger-negative"}>
                        <td className="tabular">{time.format(new Date(r.at))}</td>
                        <td>
                          <CustomerCell id={r.customer_id} name={r.customer_name} gender={r.customer_gender ?? null} />
                          {r.entry !== "sale" && <span className="badge badge-danger mt-1">{ENTRY_LABEL[r.entry]}</span>}
                          {r.voided && <span className="sr-only"> (đã hủy sau đó)</span>}
                        </td>
                        <td>{r.one_off ?? ""}</td>
                        <td>{r.package ?? ""}</td>
                        <td className="num">{r.bank_transfer ? money(r.bank_transfer) : ""}</td>
                        <td className="num">{r.cash ? money(r.cash) : ""}</td>
                        <td className="num">{r.unpaid ? money(r.unpaid) : ""}</td>
                        <td className="text-text-muted">{r.note ?? ""}</td>
                      </tr>
                    </Fragment>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
      {page.hasNextPage && (
        <div className="flex justify-center">
          <button type="button" className="btn btn-secondary min-w-40" disabled={page.isFetchingNextPage} onClick={() => void page.fetchNextPage()}>
            {page.isFetchingNextPage ? "Đang tải…" : "Xem thêm giao dịch"}
          </button>
        </div>
      )}
    </>
  );
}

function DayRow({ date, day }: { date: string; day: LedgerDay | undefined }) {
  return (
    <tr className="ledger-day">
      <th scope="rowgroup" colSpan={4} className="text-left">
        {weekdayOf(date)} {longDay(date)}
        {day && <span className="ml-2 text-xs font-normal text-text-muted">{day.entries} giao dịch</span>}
      </th>
      <td className="num">{day ? money(day.bank_transfer) : ""}</td>
      <td className="num">{day ? money(day.cash) : ""}</td>
      <td className="num">{day?.unpaid ? money(day.unpaid) : ""}</td>
      <td className="num">{day ? <span>Cộng {money(day.total)}</span> : ""}</td>
    </tr>
  );
}

function Figure({ label, value, strong = false }: { label: string; value: string; strong?: boolean }) {
  return (
    <div className={`rounded-[10px] p-3 ${strong ? "bg-primary-soft" : "bg-surface-2"}`}>
      <dt className="text-xs text-text-muted">{label}</dt>
      <dd className={`mt-0.5 text-lg font-semibold tabular ${strong ? "text-primary" : ""}`}>{value}</dd>
    </div>
  );
}
