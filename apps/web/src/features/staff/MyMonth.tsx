import { useQuery } from "@tanstack/react-query";
import { spaToday } from "../corrections/api";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { myMonthQuery, type AttendanceDay } from "./api";
import { LEVEL_LABEL, STATUS_CODE, STATUS_LABEL, daysOf } from "./labels";
import { PayslipView } from "./PayrollTab";
import { ClosedBanner, dayLabel, vnd } from "./ui";

function Total({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

const WEEK_HEAD = ["T2", "T3", "T4", "T5", "T6", "T7", "CN"];

/**
 * A technician's month as a compact calendar (spec §10 AC8.2): 7 columns Monday–Sunday, each day
 * with its code; fits 390 px without horizontal scroll.
 */
export function MonthCalendar({ month, today, days }: { month: string; today: string; days: AttendanceDay[] }) {
  const all = daysOf(month);
  // Monday-first offset of the 1st: getUTCDay() is 0 for Sunday.
  const lead = ((new Date(`${month}-01T00:00:00Z`).getUTCDay() + 6) % 7);
  const byDate = new Map(days.map((d) => [d.date, d]));
  return (
    <div className="my-calendar">
      {WEEK_HEAD.map((w) => (
        <span key={w} aria-hidden="true" className="my-calendar-head">
          {w}
        </span>
      ))}
      <ol className="contents" aria-label={`Lịch công ${month}`}>
        {Array.from({ length: lead }, (_, i) => (
          <li key={`lead-${String(i)}`} aria-hidden="true" />
        ))}
        {all.map((date) => {
          const d = byDate.get(date);
          const label = `${dayLabel(date)}: ${d ? STATUS_LABEL[d.status] : date > today ? "chưa tới" : "chưa chấm"}${d && d.late_minutes > 0 ? `, muộn ${String(d.late_minutes)} phút` : ""}`;
          return (
            <li
              key={date}
              aria-label={label}
              title={label}
              className={`my-calendar-day att-${d?.status ?? "none"}${d && d.late_minutes > 0 ? " is-late" : ""}${date === today ? " is-today" : ""}`}
            >
              <span aria-hidden="true">{Number(date.slice(8))}</span>
              <span aria-hidden="true" className="code">
                {d ? STATUS_CODE[d.status] : date > today ? "" : "–"}
              </span>
            </li>
          );
        })}
      </ol>
    </div>
  );
}

/**
 * "Tháng của tôi": a technician's own attendance, reviews, evaluation, bonuses/penalties and
 * payslip (`GET /me/staff`). The server picks the person from the session; nothing here can ask
 * for someone else.
 */
export function MyMonth({ month }: { month: string }) {
  const query = useQuery(myMonthQuery(month));
  if (query.isPending) return <RowsSkeleton rows={5} />;
  if (query.isError) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
  const data = query.data.data;
  if (!query.data.linked || !data) {
    return (
      <EmptyState title="Tài khoản chưa gắn với hồ sơ nhân viên">
        Nhờ chủ spa gắn tài khoản này vào hồ sơ của bạn để xem ngày công, đánh giá và phiếu lương.
      </EmptyState>
    );
  }
  const t = data.attendance.totals;
  const compliments = data.reviews.reduce((n, r) => n + r.compliments, 0);
  const complaints = data.reviews.reduce((n, r) => n + r.complaints, 0);
  const today = spaToday();
  return (
    <div className="grid content-start gap-6">
      {data.closed && <ClosedBanner />}
      <section className="card grid gap-3 p-4 sm:p-5" aria-labelledby="my-attendance">
        <h2 id="my-attendance" className="panel-title">
          Ngày công
        </h2>
        <dl className="my-totals">
          <Total label="Đi làm" value={`${String(t.present)} ngày`} />
          <Total label="Đi muộn" value={t.late_count ? `${String(t.late_count)} lần · ${String(t.late_minutes)}′` : "0"} />
          <Total label="Nghỉ P / ĐX" value={`${String(t.leave_p)} / ${String(t.leave_dx)}`} />
          <Total label="Nghỉ K" value={String(t.absent_k)} />
          <Total label="Khen / Phàn nàn" value={`${String(compliments)} / ${String(complaints)}`} />
        </dl>
        <MonthCalendar month={data.month} today={today} days={data.attendance.days} />
        <p className="text-sm text-text-muted">✓ đi làm · P nghỉ có phép · ĐX nghỉ đột xuất · K nghỉ không phép · – chưa chấm · gạch đỏ: đi muộn</p>
      </section>
      <section className="card grid gap-2 p-4 sm:p-5" aria-labelledby="my-evaluation">
        <h2 id="my-evaluation" className="panel-title">
          Đánh giá
        </h2>
        {data.evaluation ? (
          <p>
            Tháng này: <strong>{data.evaluation.total_points} điểm · {LEVEL_LABEL[data.evaluation.result]}</strong>
            {data.evaluation.comment && <span className="block text-sm text-text-muted">{data.evaluation.comment}</span>}
          </p>
        ) : (
          <p className="empty-block">Chưa có đánh giá tháng (quản lý chấm ngày 1–5 tháng sau).</p>
        )}
        {data.adjustments.length > 0 && (
          <ul className="grid gap-1 text-sm">
            {data.adjustments.map((a) => (
              <li key={a.id}>
                {a.kind === "bonus" ? "Thưởng" : "Phạt"} {a.amount != null ? vnd(a.amount) : ""} {a.points != null ? `${String(a.points)} điểm` : ""} · {a.reason}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section className="grid gap-2" aria-labelledby="my-payslip">
        <h2 id="my-payslip" className="panel-title">
          Phiếu lương
        </h2>
        {data.payslip ? <PayslipView slip={data.payslip} closed={data.closed} /> : <p className="text-text-muted">Chủ spa chưa nhập lương cho bạn.</p>}
      </section>
    </div>
  );
}
