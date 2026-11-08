import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { timeOf } from "../front-desk/api";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { myDayQuery, type MyDayData } from "./api";
import { STATUS_LABEL, percentOfBp } from "./labels";
import { StatusMark, dayLabel, vnd } from "./ui";

/** `YYYY-MM-DD` ± n days. */
export function shiftDay(day: string, n: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

const SOURCE_LABEL: Record<string, string> = { one_off: "Dịch vụ lẻ", package: "Buổi liệu trình" };
const UPSELL_LABEL: Record<string, string> = {
  package: "Chốt gói",
  service: "Giới thiệu dịch vụ lẻ",
  package_refund: "Hoàn tiền gói (trừ lại)",
};

/** Totals of the day's own work and what it earns. */
export function dayTotals(d: MyDayData): { value: number; commission: number | null; upsell: number; upsellCommission: number | null } {
  const lines = d.visits.flatMap((v) => v.lines);
  const sum = (xs: (number | null | undefined)[]) => (xs.some((x) => x == null) ? null : xs.reduce<number>((n, x) => n + (x ?? 0), 0));
  return {
    value: lines.reduce((n, l) => n + l.value, 0),
    commission: d.commission_rate_bp == null ? null : sum(lines.map((l) => l.commission)),
    upsell: d.upsell.reduce((n, u) => n + u.value, 0),
    upsellCommission: d.commission_rate_bp == null ? null : sum(d.upsell.map((u) => u.commission)),
  };
}

/**
 * One day of "Công và lương của tôi" in detail (EM 2026-11-07): attendance, compliments and
 * complaints, bonuses and penalties, the customers served with each service's value and
 * commission, and what was sold or recommended. A bottom sheet on phones, a dialog from 640 px;
 * the previous / next day buttons move without closing it. Only the session's own data
 * (`GET /me/staff/day`); no phone numbers anywhere.
 */
export function MyDaySheet({ date, today, onDay, onClose }: { date: string; today: string; onDay: (date: string) => void; onClose: () => void }) {
  const query = useQuery({ ...myDayQuery(date), placeholderData: keepPreviousData });
  const data = query.data?.data;
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={`Ngày ${dayLabel(date)}`}
      description={data?.closed ? "Tháng đã chốt lương: số liệu là số cuối." : "Tạm tính, còn thay đổi đến khi chốt lương."}
      footer={
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          Đóng
        </button>
      }
    >
      <div className="grid gap-4">
        <div role="group" aria-label="Chọn ngày" className="my-day-nav">
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              onDay(shiftDay(date, -1));
            }}
          >
            <Icon name="chevron-left" /> Ngày trước
          </button>
          <button
            type="button"
            className="btn btn-secondary"
            disabled={date >= today}
            onClick={() => {
              onDay(shiftDay(date, 1));
            }}
          >
            Ngày sau <Icon name="chevron-right" />
          </button>
        </div>
        {query.isPending ? (
          <RowsSkeleton rows={4} />
        ) : query.isError && !query.data ? (
          <ErrorState error={query.error} onRetry={() => void query.refetch()} />
        ) : !data ? (
          <p className="text-text-muted">Tài khoản chưa gắn với hồ sơ nhân viên.</p>
        ) : (
          <DayDetail d={data} refreshing={query.isPlaceholderData} />
        )}
      </div>
    </BottomSheet>
  );
}

function DayDetail({ d, refreshing }: { d: MyDayData; refreshing: boolean }) {
  const t = dayTotals(d);
  const a = d.attendance;
  const r = d.review;
  const rate = d.commission_rate_bp;
  const money = (n: number | null | undefined) => (n == null ? "—" : vnd(n));
  return (
    <div className={`my-day-detail ${refreshing ? "is-refreshing" : ""}`} aria-busy={refreshing || undefined}>
      <section aria-labelledby="day-att" className="my-day-block">
        <h3 id="day-att">Chấm công</h3>
        {a ? (
          <dl className="detail-grid">
            <dt>Trạng thái</dt>
            <dd>
              <span className={`my-legend-mark att-${a.status}`} aria-hidden="true">
                <StatusMark status={a.status} className="size-3.5" />
              </span>{" "}
              {STATUS_LABEL[a.status]}
              {a.late_minutes > 0 && <span className="badge badge-warn ml-2">Muộn {a.late_minutes} phút</span>}
            </dd>
            {a.status === "present" && (
              <>
                <dt>Giờ vào / ra</dt>
                <dd className="tabular">
                  {a.check_in ?? "—"} / {a.check_out ?? "—"}
                </dd>
              </>
            )}
            {a.note && (
              <>
                <dt>Ghi chú</dt>
                <dd>{a.note}</dd>
              </>
            )}
          </dl>
        ) : (
          <p className="text-sm text-text-muted">{d.date > d.today ? "Ngày chưa tới." : "Chưa chấm công ngày này."}</p>
        )}
      </section>

      <section aria-labelledby="day-review" className="my-day-block">
        <h3 id="day-review">Khen / phàn nàn</h3>
        {r ? (
          <p>
            <strong>{r.compliments}</strong> lời khen · <strong>{r.complaints}</strong> phàn nàn
            {r.note && <span className="block text-sm text-text-muted">{r.note}</span>}
          </p>
        ) : (
          <p className="text-sm text-text-muted">Không có.</p>
        )}
        {d.adjustments.length > 0 && (
          <ul className="work-list">
            {d.adjustments.map((x) => (
              <li key={x.id} className="work-row">
                <span className={`badge ${x.kind === "bonus" ? "badge-ok" : "badge-danger"}`}>{x.kind === "bonus" ? "Thưởng" : "Phạt"}</span>
                <span className="min-w-0 flex-1">{x.reason}</span>
                <span className="font-semibold tabular">
                  {[x.amount != null ? vnd(x.amount) : "", x.points != null ? `${String(x.points)} điểm` : ""].filter(Boolean).join(" · ")}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="day-visits" className="my-day-block">
        <h3 id="day-visits">Khách đã làm</h3>
        {d.visits.length === 0 ? (
          <p className="text-sm text-text-muted">Không có lượt khách nào.</p>
        ) : (
          <table className="work-table">
            <caption className="sr-only">Khách đã làm ngày {dayLabel(d.date)}</caption>
            <colgroup>
              <col />
              <col className="work-col-value" />
              <col className="work-col-value" />
            </colgroup>
            <thead>
              <tr>
                <th scope="col">Khách / dịch vụ</th>
                <th scope="col" className="num">
                  Giá trị
                </th>
                <th scope="col" className="num">
                  Hoa hồng
                </th>
              </tr>
            </thead>
            {d.visits.map((v) => (
              <tbody key={v.visit_id}>
                <tr className="work-group">
                  <th scope="rowgroup" colSpan={3}>
                    <span className="tabular">{timeOf(v.check_in_at)}</span> · {v.customer_name}
                    {/* The session note of a visit I worked on (EM 2026-11-07 item 3). */}
                    {v.note && <span className="block text-sm font-normal text-text-muted">Ghi chú buổi: {v.note}</span>}
                  </th>
                </tr>
                {v.lines.map((l, i) => (
                  <tr key={`${l.service_name}-${String(i)}`}>
                    <th scope="row">
                      {l.service_name}
                      {l.qty > 1 ? ` ×${String(l.qty)}` : ""}{" "}
                      <span className="block text-xs text-text-muted">{SOURCE_LABEL[l.source] ?? l.source}</span>
                    </th>
                    <td className="num">{vnd(l.value)}</td>
                    <td className="num">{money(l.commission)}</td>
                  </tr>
                ))}
              </tbody>
            ))}
            <tfoot>
              <tr className="work-total">
                <th scope="row">Cộng {d.visits.length} lượt khách</th>
                <td className="num">{vnd(t.value)}</td>
                <td className="num">{money(t.commission)}</td>
              </tr>
            </tfoot>
          </table>
        )}
      </section>

      <section aria-labelledby="day-upsell" className="my-day-block">
        <h3 id="day-upsell">Giới thiệu / chốt được</h3>
        {d.upsell.length === 0 ? (
          <p className="text-sm text-text-muted">Không có.</p>
        ) : (
          <table className="work-table">
            <caption className="sr-only">Giới thiệu / chốt được ngày {dayLabel(d.date)}</caption>
            <colgroup>
              <col />
              <col className="work-col-value" />
              <col className="work-col-value" />
            </colgroup>
            <thead>
              <tr>
                <th scope="col">Gói / dịch vụ</th>
                <th scope="col" className="num">
                  Giá trị
                </th>
                <th scope="col" className="num">
                  Hoa hồng
                </th>
              </tr>
            </thead>
            <tbody>
              {d.upsell.map((u, i) => (
                <tr key={`${u.kind}-${u.description}-${String(i)}`}>
                  <th scope="row">
                    {u.description}
                    {u.qty > 1 ? ` ×${String(u.qty)}` : ""}{" "}
                    <span className="block text-xs text-text-muted">
                      {UPSELL_LABEL[u.kind] ?? u.kind} · {u.customer_name}
                    </span>
                  </th>
                  <td className="num">{vnd(u.value)}</td>
                  <td className="num">{money(u.commission)}</td>
                </tr>
              ))}
            </tbody>
            {d.upsell.length > 1 && (
              <tfoot>
                <tr className="work-total">
                  <th scope="row">Cộng</th>
                  <td className="num">{vnd(t.upsell)}</td>
                  <td className="num">{money(t.upsellCommission)}</td>
                </tr>
              </tfoot>
            )}
          </table>
        )}
      </section>
      <p className="field-hint">
        {rate == null
          ? "Chủ spa chưa nhập tỷ lệ hoa hồng của bạn, nên chưa tính được hoa hồng."
          : `Hoa hồng ${percentOfBp(rate)} giá trị. Phiếu lương cộng cả tháng rồi làm tròn một lần, nên có thể lệch vài đồng.`}
      </p>
    </div>
  );
}
