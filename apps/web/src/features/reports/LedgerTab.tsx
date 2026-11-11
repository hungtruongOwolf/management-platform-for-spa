import { useInfiniteQuery, useSuspenseQuery } from "@tanstack/react-query";
import { Fragment, useRef, useState, type MouseEvent } from "react";
import { usePhone } from "../../components/useMediaQuery";
import { dayLabel, spaTime } from "../../lib/format";
import { meQuery, type Me } from "../auth/session";
import { LazyLedgerSaleSheet, usePreloadRecordHistory } from "../record-history/lazy";
import { canTrace } from "../record-history/query";
import { ledgerQuery } from "./api";
import type { LedgerDay, LedgerRow } from "./contract";
import { CustomerCell } from "./PackagesOverviewPage";
import { daysBetween, type Period } from "./period";
import { EmptyState, ErrorState, RowsSkeleton, money } from "./ui";

/** Days a `GET /reports/ledger` range may cover, `from` and `to` included (server `MAX_LEDGER_DAYS`). */
export const LEDGER_MAX_DAYS = 92;


const ENTRY_LABEL: Record<LedgerRow["entry"], string> = { sale: "", void: "Hủy hóa đơn", refund: "Hoàn tiền" };

/**
 * Whether the row's time opens its history (design record-history §2.4): the history right, and
 * the read right of what it re-anchors to. A one-off sale is its visit's (`visit.read`); a package
 * sale, its void, a refund are the package's (`customer.read`); always through a sale (`sale.read`).
 */
function traceable(me: Me | null, r: LedgerRow): boolean {
  if (r.entry === "refund") return canTrace(me, "package", "refund");
  if (r.kind === "package") return canTrace(me, "package", "sale");
  // A one-off sale outside any visit (a paper-book entry) has no history to open.
  return Boolean(r.visit_id) && canTrace(me, "visit", "sale");
}

/**
 * "Đã sửa" on the book (spec D9, AC-H8): only a standing sale of one-off services whose visit was
 * edited. Never a void row or the voided sale of a replaced visit; a package's lifecycle is no edit.
 */
const editedSale = (r: LedgerRow) => r.entry === "sale" && !r.voided && r.kind === "one_off" && r.edited === true;

/** "10:07, xem hóa đơn của chị Lê Thị Mai": the name starts with the visible time (WCAG 2.5.3). */
function timeLabel(r: LedgerRow): string {
  const what = r.entry === "refund" ? "hoàn tiền" : r.entry === "void" ? "hủy hóa đơn" : "hóa đơn";
  const title = r.customer_gender === "female" ? "chị " : r.customer_gender === "male" ? "anh " : "";
  return `${spaTime(r.at)}, xem ${what} của ${title}${r.customer_name}`;
}

/** The time of a row: a button to its history sheet when it has one. */
function TimeCell({ r, onOpen }: { r: LedgerRow; onOpen?: ((r: LedgerRow, from: HTMLElement) => void) | undefined }) {
  if (!onOpen) return <>{spaTime(r.at)}</>;
  return (
    <button
      type="button"
      className="time-button"
      aria-label={timeLabel(r)}
      onClick={(e) => {
        onOpen(r, e.currentTarget);
      }}
    >
      {spaTime(r.at)}
    </button>
  );
}

/**
 * "Sổ doanh thu" (spec R1-2 US3): the paper revenue book's columns, one row per sale, void or
 * refund, grouped by day with the day's totals (`days[]` covers every day of the range, not only
 * the loaded page). Void and refund rows are negative and red; an original that was voided later
 * is struck through. Needs `report.revenue.read` (the tab is hidden otherwise; the API says 403).
 */
export function LedgerTab({ period }: { period: Period }) {
  const phone = usePhone();
  const { data: me } = useSuspenseQuery(meQuery);
  const tooLong = daysBetween(period.from, period.to) > LEDGER_MAX_DAYS;
  const page = useInfiniteQuery({ ...ledgerQuery(period.from, period.to), enabled: !tooLong });
  /** The row whose history sheet is open (kept while it closes, so the dialog closes cleanly). */
  const [sheet, setSheet] = useState<{ row: LedgerRow; open: boolean } | null>(null);
  /** The row the sheet was opened from: its customer link takes focus if the history is refused. */
  const openedFrom = useRef<HTMLElement | null>(null);
  usePreloadRecordHistory(canTrace(me, "visit") || canTrace(me, "package"));
  const open = (row: LedgerRow, from: HTMLElement) => {
    openedFrom.current = from.closest("tr, article");
    setSheet({ row, open: true });
  };
  const onOpen = (r: LedgerRow) => (traceable(me, r) ? open : undefined);
  const close = () => {
    setSheet((s) => s && { ...s, open: false });
  };
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
          <LedgerCards rows={rows} dayOf={dayOf} onOpen={onOpen} />
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
                        <td className="tabular">
                          <TimeCell r={r} onOpen={onOpen(r)} />
                        </td>
                        <td>
                          <CustomerCell id={r.customer_id} name={r.customer_name} gender={r.customer_gender ?? null} />
                          {r.entry !== "sale" && <span className="badge badge-danger mt-1">{ENTRY_LABEL[r.entry]}</span>}
                          {editedSale(r) && <span className="badge badge-muted mt-1">Đã sửa</span>}
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
      {sheet && (
        <LazyLedgerSaleSheet
          row={sheet.row}
          open={sheet.open}
          onClose={close}
          onForbidden={() => {
            close();
            // The time button goes once `/me` drops the right: the row's customer link stays.
            const row = openedFrom.current;
            requestAnimationFrame(() => {
              row?.querySelector<HTMLElement>("a")?.focus();
            });
          }}
        />
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
function LedgerCards({
  rows,
  dayOf,
  onOpen,
}: {
  rows: LedgerRow[];
  dayOf: Map<string, LedgerDay>;
  onOpen: (r: LedgerRow) => ((r: LedgerRow, from: HTMLElement) => void) | undefined;
}) {
  return (
    <div className="rt-cards-wrap">
      {rows.map((r, i) => {
        const startsDay = rows[i - 1]?.business_date !== r.business_date;
        const day = dayOf.get(r.business_date);
        const amount = r.cash + r.bank_transfer;
        const tone = r.entry === "sale" ? (r.voided ? "ledger-voided" : "") : "ledger-negative";
        const opens = onOpen(r);
        // A tap anywhere on the card opens the sheet too; the customer's link and the time button
        // keep their own (the time button is the keyboard path, the card is never a <button>).
        const onCard = opens
          ? (e: MouseEvent<HTMLElement>) => {
              if (!(e.target as Element).closest("a, button") && !window.getSelection()?.toString()) opens(r, e.currentTarget);
            }
          : undefined;
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
            <article className={`rt-card ${tone} ${opens ? "rt-clickable" : ""}`} onClick={onCard}>
              <div className="rt-card-head">
                <div className="rt-card-title">
                  <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <CustomerCell id={r.customer_id} name={r.customer_name} gender={r.customer_gender ?? null} />
                    {editedSale(r) && <span className="badge badge-muted">Đã sửa</span>}
                  </span>
                  <span className="rt-card-sub tabular">
                    <TimeCell r={r} onOpen={opens} />
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
