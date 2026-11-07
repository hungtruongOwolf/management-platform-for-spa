import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState, type ReactNode } from "react";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { can, meQuery } from "../auth/session";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { evaluationHistoryQuery, type Evaluation, type HistoryMonth } from "./api";
import { CRITERION_LABEL, LEVEL_BADGE, LEVEL_LABEL, LEVEL_SHORT, monthLabel } from "./labels";

const when = new Intl.DateTimeFormat("vi-VN", {
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
  timeZone: "Asia/Ho_Chi_Minh",
});

/** "81 điểm · Mức 2" as the month's chip. */
export function ResultChip({ evaluation }: { evaluation: Evaluation | null | undefined }) {
  if (!evaluation) return <span className="badge badge-muted">Chưa đánh giá</span>;
  return (
    <span className={`badge ${LEVEL_BADGE[evaluation.result]}`}>
      {evaluation.total_points} điểm · {LEVEL_SHORT[evaluation.result]}
    </span>
  );
}

/**
 * A month's scorecard, read-only (EM 2026-11-08): every criterion's points against its maximum and
 * its level, the total and the month's result, the serious-issue cap, the coaching plan and the
 * comment, and who scored it when. Opens for any scored month, closed or not, and for rows the
 * viewer may not rescore; "Chấm lại" (where allowed) is passed in as `footer`.
 */
export function ScoreCard({
  name,
  month,
  evaluation: e,
  onClose,
  footer,
}: {
  name: string;
  month: string;
  evaluation: Evaluation;
  onClose: () => void;
  footer?: ReactNode;
}) {
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={`Phiếu đánh giá ${name}`}
      description={`${monthLabel(month)} · ${String(e.total_points)} điểm · ${LEVEL_LABEL[e.result]}`}
      footer={footer}
    >
      <div className="grid gap-4">
        <table className="data-table score-table">
          <caption className="sr-only">Điểm từng tiêu chí, {monthLabel(month)}</caption>
          <thead>
            <tr>
              <th scope="col">Tiêu chí</th>
              <th scope="col" className="text-right">
                Điểm
              </th>
              <th scope="col">Mức</th>
            </tr>
          </thead>
          <tbody>
            {e.criteria.map((c) => (
              <tr key={c.key}>
                <th scope="row">{CRITERION_LABEL[c.key] ?? c.key}</th>
                <td className="text-right tabular">
                  {c.points}/{c.max}
                </td>
                <td>{LEVEL_SHORT[c.level]}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th scope="row">Tổng</th>
              <td className="text-right tabular font-semibold">{e.total_points}/100</td>
              <td>
                <ResultChip evaluation={e} />
              </td>
            </tr>
          </tfoot>
        </table>
        {e.serious_issue && (
          <p className="callout text-sm">
            <Icon name="alert" className="size-4 shrink-0" />
            Có khiếu nại nghiêm trọng hoặc vi phạm nội quy nhiều lần: kết quả tháng tối đa Mức 3.
          </p>
        )}
        <dl className="score-texts">
          <div>
            <dt>Kế hoạch kèm cặp tháng sau</dt>
            <dd>{e.coaching_plan ?? "Không có"}</dd>
          </div>
          <div>
            <dt>Nhận xét của quản lý</dt>
            <dd>{e.comment ?? "Không có"}</dd>
          </div>
        </dl>
        <p className="text-sm text-text-muted">
          Chấm bởi {e.by_name} · {when.format(new Date(e.updated_at))}
        </p>
      </div>
    </BottomSheet>
  );
}

/** The last months' totals as small bars, oldest first; each bar names its month and result. */
function Trend({ months }: { months: HistoryMonth[] }) {
  const oldestFirst = [...months].reverse();
  return (
    <figure className="eval-trend">
      <figcaption className="text-sm text-text-muted">Điểm đánh giá {String(months.length)} tháng gần nhất</figcaption>
      <ol className="eval-trend-bars">
        {oldestFirst.map((m) => {
          const e = m.evaluation;
          return (
            <li
              key={m.month}
              aria-label={`${monthLabel(m.month)}: ${e ? `${String(e.total_points)} điểm, ${LEVEL_SHORT[e.result]}` : "chưa đánh giá"}`}
            >
              <span className="eval-trend-track" aria-hidden="true">
                {e && <span className={`eval-trend-bar is-${e.result}`} style={{ height: `${String(e.total_points)}%` }} />}
              </span>
              <span className="eval-trend-label" aria-hidden="true">
                {m.month.slice(5)}
              </span>
            </li>
          );
        })}
      </ol>
    </figure>
  );
}

/**
 * One person's evaluations across months (EM 2026-11-08): a trend of the last 12 months, then a
 * row per month — result, praise / complaints, leave and late counts — with the scorecard, the
 * month's days (the month sheet for this person) and, where allowed, the month to rescore in.
 * `GET /evaluations/history` (one request; `evaluation.record`, as the month list).
 */
export function EvaluationHistoryView({ staffId, onBack }: { staffId: string; onBack: () => void }) {
  const { data: me } = useSuspenseQuery(meQuery);
  const history = useQuery(evaluationHistoryQuery(staffId));
  const [open, setOpen] = useState<HistoryMonth | null>(null);
  const back = (
    <button type="button" className="btn btn-ghost justify-self-start" onClick={onBack}>
      <Icon name="chevron-left" className="size-4" />
      Đánh giá tháng
    </button>
  );
  if (history.isPending) {
    return (
      <div className="grid gap-4">
        {back}
        <RowsSkeleton rows={6} />
      </div>
    );
  }
  if (history.isError) {
    return (
      <div className="grid gap-4">
        {back}
        <ErrorState error={history.error} onRetry={() => void history.refetch()} />
      </div>
    );
  }
  const h = history.data;
  return (
    <div className="grid content-start gap-4">
      {back}
      <div>
        <h2 className="panel-title">Lịch sử đánh giá · {h.full_name}</h2>
        <p className="text-sm text-text-muted">
          {h.position} · {monthLabel(h.from)} – {monthLabel(h.to).toLowerCase()}
        </p>
      </div>
      {h.months.length === 0 ? (
        <p className="empty-block">Người này chưa làm tháng nào trong khoảng này.</p>
      ) : (
        <>
          <Trend months={h.months} />
          <ul className="card eval-list" aria-label={`Các tháng của ${h.full_name}`}>
            {h.months.map((m) => {
              const a = m.attendance;
              return (
                <li key={m.month} className="eval-row">
                  <div className="min-w-0 flex-1">
                    <p className="font-semibold">
                      {monthLabel(m.month)}
                      {m.closed && <span className="ml-2 text-sm font-normal text-text-muted">· đã chốt lương</span>}
                    </p>
                    <p className="text-sm text-text-muted">
                      {m.compliments} khen · {m.complaints} phàn nàn · nghỉ {a.leave_p + a.leave_dx + a.absent_k} ngày · muộn {a.late_count} lần
                    </p>
                  </div>
                  <ResultChip evaluation={m.evaluation} />
                  <div className="eval-actions">
                    {m.evaluation && (
                      <button
                        type="button"
                        className="btn btn-secondary"
                        aria-label={`Xem phiếu đánh giá ${monthLabel(m.month).toLowerCase()}`}
                        onClick={() => {
                          setOpen(m);
                        }}
                      >
                        Xem phiếu
                      </button>
                    )}
                    <Link
                      to="/nhan-vien"
                      search={{ tab: "month", month: m.month, staff: staffId }}
                      className="btn btn-ghost"
                      aria-label={`Từng ngày ${monthLabel(m.month).toLowerCase()}`}
                    >
                      Từng ngày
                    </Link>
                    {!m.closed && h.can_record && (
                      <Link to="/nhan-vien" search={{ tab: "evaluation", month: m.month }} className="btn btn-ghost">
                        {m.evaluation ? "Chấm lại" : "Chấm điểm"}
                      </Link>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      )}
      {can(me, "report.read") && (
        <Link to="/nhat-ky-ghi-chu" search={{ staff: staffId }} className="btn btn-ghost justify-self-start">
          Xem mọi ghi chú về {h.full_name}
          <Icon name="arrow-right" className="size-4" />
        </Link>
      )}
      {open?.evaluation && (
        <ScoreCard
          name={h.full_name}
          month={open.month}
          evaluation={open.evaluation}
          onClose={() => {
            setOpen(null);
          }}
        />
      )}
    </div>
  );
}
