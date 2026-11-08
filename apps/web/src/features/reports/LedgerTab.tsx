import { useInfiniteQuery } from "@tanstack/react-query";
import { Fragment } from "react";
import { usePhone } from "../../components/useMediaQuery";
import { dayLabel, spaTime } from "../../lib/format";
import { ledgerQuery } from "./api";
import type { LedgerDay, LedgerRow } from "./contract";
import { CustomerCell } from "./PackagesOverviewPage";
import { daysBetween, type Period } from "./period";
import { EmptyState, ErrorState, RowsSkeleton, money } from "./ui";

/** `GET /reports/ledger` accepts at most 92 days after `from`. */
export const LEDGER_MAX_DAYS = 93;


const ENTRY_LABEL: Record<LedgerRow["entry"], string> = { sale: "", void: "Hủy hóa đơn", refund: "Hoàn tiền" };

/**
 * "Sổ doanh thu" (spec R1-2 US3): the paper revenue book's columns, one row per sale, void or
 * refund, grouped by day with the day's totals (`days[]` covers every day of the range, not only
 * the loaded page). Void and refund rows are negative and red; an original that was voided later
 * is struck through. Needs `report.revenue.read` (the tab is hidden otherwise; the API says 403).
 */
export function LedgerTab({ period }: { period: Period }) {
  const phone = usePhone();
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
      <div className="card rt-host overflow-hidden">
        {rows.length === 0 ? (
          <EmptyState title="Chưa có giao dịch nào">
            Hóa đơn, khoản hủy và hoàn tiền trong kỳ sẽ hiện ở đây. Chọn kỳ dài hơn, hoặc ghi lượt ở “Ghi lượt khách”.
          </EmptyState>
        ) : phone ? (
          <LedgerCards rows={rows} dayOf={dayOf} />
        ) : (
          <div className="table-wrap" tabIndex={0} role="region" aria-label="Sổ doanh thu (cuộn ngang để xem đủ cột)">
            <table className="data-table data-table-fixed ledger-table">
              <caption className="sr-only">Sổ doanh thu, theo ngày rồi theo giờ ghi</caption>
              {/* Fixed columns (rulebook §8): a long package name wraps inside its own column. */}
              <colgroup>
                <col style={{ width: "64px" }} />
                <col style={{ width: "14%" }} />
                <col style={{ width: "15%" }} />
                <col style={{ width: "18%" }} />
                <col style={{ width: "11%" }} />
                <col style={{ width: "10%" }} />
                <col style={{ width: "9%" }} />
                <col />
              </colgroup>
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
                  <th scope="col" className="num th-wrap">
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
                        <td className="tabular">{spaTime(r.at)}</td>
                        <td>
                          <CustomerCell id={r.customer_id} name={r.customer_name} gender={r.customer_gender ?? null} />
                          {r.entry !== "sale" && <span className="badge badge-danger mt-1">{ENTRY_LABEL[r.entry]}</span>}
                          {r.voided && <span className="sr-only"> (đã hủy sau đó)</span>}
                        </td>
                        <td>{r.one_off ?? ""}</td>
                        <td>
                          {r.package ?? ""}
                          {r.beneficiary_name && <span className="block text-text-muted">Mua cho {r.beneficiary_name}</span>}
                        </td>
                        <td className="num">{r.bank_transfer ? money(r.bank_transfer) : ""}</td>
                        <td className="num">{r.cash ? money(r.cash) : ""}</td>
                        <td className="num">{r.unpaid ? money(r.unpaid) : ""}</td>
                        <td className="text-text-muted">
                          {r.upsell_by && <span className="block">Giới thiệu: {r.upsell_by}</span>}
                          {r.note ?? ""}
                        </td>
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
        {dayLabel(date)}
        {day && <span className="ml-2 text-xs font-normal text-text-muted">{day.entries} giao dịch</span>}
      </th>
      <td className="num">{day ? money(day.bank_transfer) : ""}</td>
      <td className="num">{day ? money(day.cash) : ""}</td>
      <td className="num">{day?.unpaid ? money(day.unpaid) : ""}</td>
      <td className="num">{day ? <span>Cộng {money(day.total)}</span> : ""}</td>
    </tr>
  );
}

/**
 * Phones: the same book as day groups of cards (customer, time, amount; services and the split
 * below), never a sideways-scrolling 8-column table (design-system § Mobile patterns).
 */
function LedgerCards({ rows, dayOf }: { rows: LedgerRow[]; dayOf: Map<string, LedgerDay> }) {
  return (
    <div className="rt-cards-wrap">
      {rows.map((r, i) => {
        const startsDay = rows[i - 1]?.business_date !== r.business_date;
        const day = dayOf.get(r.business_date);
        const amount = r.cash + r.bank_transfer;
        const tone = r.entry === "sale" ? (r.voided ? "ledger-voided" : "") : "ledger-negative";
        return (
          <Fragment key={`${r.entry}-${r.id}`}>
            {startsDay && (
              <h3 className="rt-card ledger-day flex flex-wrap justify-between gap-2 font-semibold">
                <span>{dayLabel(r.business_date)}</span>
                {day && (
                  <span className="tabular">
                    Cộng {money(day.total)} <span className="text-xs font-normal text-text-muted">· {day.entries} giao dịch</span>
                  </span>
                )}
              </h3>
            )}
            <article className={`rt-card ${tone}`}>
              <div className="rt-card-head">
                <div className="rt-card-title">
                  <CustomerCell id={r.customer_id} name={r.customer_name} gender={r.customer_gender ?? null} />
                  <span className="rt-card-sub tabular">
                    {spaTime(r.at)}
                    {r.entry !== "sale" && ` · ${ENTRY_LABEL[r.entry]}`}
                    {r.voided && " · đã hủy sau đó"}
                  </span>
                </div>
                <span className="rt-card-aside tabular">{amount ? money(amount) : ""}</span>
              </div>
              <dl className="rt-card-fields">
                {r.one_off && (
                  <div>
                    <dt>Dịch vụ 1 lần</dt>
                    <dd>{r.one_off}</dd>
                  </div>
                )}
                {r.package && (
                  <div>
                    <dt>Liệu trình</dt>
                    <dd>{r.package}</dd>
                  </div>
                )}
                {amount !== 0 && (
                  <div>
                    <dt>Hình thức</dt>
                    <dd className="tabular">
                      {r.cash !== 0 && r.bank_transfer !== 0
                        ? `Tiền mặt ${money(r.cash)} · CK ${money(r.bank_transfer)}`
                        : r.cash !== 0
                          ? "Tiền mặt"
                          : "Chuyển khoản"}
                    </dd>
                  </div>
                )}
                {r.unpaid ? (
                  <div>
                    <dt>Chưa thanh toán</dt>
                    <dd className="tabular">{money(r.unpaid)}</dd>
                  </div>
                ) : null}
                {r.beneficiary_name && (
                  <div>
                    <dt>Mua cho</dt>
                    <dd>{r.beneficiary_name}</dd>
                  </div>
                )}
                {r.upsell_by && (
                  <div>
                    <dt>Giới thiệu</dt>

                    <dd>{r.upsell_by}</dd>
                  </div>
                )}
                {r.note && (
                  <div>
                    <dt>Ghi chú</dt>
                    <dd>{r.note}</dd>
                  </div>
                )}
              </dl>
            </article>
          </Fragment>
        );
      })}
    </div>
  );
}

function Figure({ label, value, strong = false }: { label: string; value: string; strong?: boolean }) {
  return (
    <div className={`rounded-md p-3 ${strong ? "bg-primary-soft" : "bg-surface-2"}`}>
      <dt className="text-xs text-text-muted">{label}</dt>
      <dd className={`mt-0.5 text-lg font-semibold tabular ${strong ? "text-primary" : ""}`}>{value}</dd>
    </div>
  );
}
