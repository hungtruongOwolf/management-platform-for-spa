import { useQuery } from "@tanstack/react-query";
import { spaToday } from "../corrections/api";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { myMonthQuery, type AttendanceDay, type MyStaff } from "./api";
import { FLAG_LABEL, LEVEL_SHORT, STATUS_CODE, STATUS_LABEL, daysOf } from "./labels";
import { MyDaySheet } from "./MyDaySheet";
import { PayslipView } from "./PayrollTab";
import { ClosedBanner, dayLabel, vnd } from "./ui";

type MonthData = NonNullable<MyStaff["data"]>;

function Tile({ label, value, sub }: { label: string; value: string; sub?: string | undefined }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>
        {value}
        {sub && <span className="my-tile-sub">{sub}</span>}
      </dd>
    </div>
  );
}

const WEEK_HEAD = ["T2", "T3", "T4", "T5", "T6", "T7", "CN"];

/** What a day's cell says, in words (its accessible name and tooltip). */
export function dayStatusText(date: string, today: string, d: AttendanceDay | undefined): string {
  const status = d ? STATUS_LABEL[d.status] : date > today ? "chưa tới" : "chưa chấm";
  return `${dayLabel(date)}: ${status}${d && d.late_minutes > 0 ? `, muộn ${String(d.late_minutes)} phút` : ""}`;
}

/**
 * A technician's month as a calendar (spec §10 AC8.2, redesigned EM 2026-11-07): 7 equal columns
 * Monday–Sunday of calmer cells — the day number top left, the status mark centred, minutes late
 * as a chip top right, future days quiet — fitting 390 px without sideways scroll. A day up to
 * today is a button that opens its detail (`onDay`).
 */
export function MonthCalendar({
  month,
  today,
  days,
  selected,
  onDay,
}: {
  month: string;
  today: string;
  days: AttendanceDay[];
  selected?: string;
  onDay?: (date: string) => void;
}) {
  const all = daysOf(month);
  // Monday-first offset of the 1st: getUTCDay() is 0 for Sunday.
  const lead = (new Date(`${month}-01T00:00:00Z`).getUTCDay() + 6) % 7;
  const byDate = new Map(days.map((d) => [d.date, d]));
  return (
    <div className="my-calendar">
      {WEEK_HEAD.map((w) => (
        <span key={w} aria-hidden="true" className={`my-calendar-head ${w === "CN" ? "is-sunday" : ""}`}>
          {w}
        </span>
      ))}
      <ol className="contents" aria-label={`Lịch công ${month}`}>
        {Array.from({ length: lead }, (_, i) => (
          <li key={`lead-${String(i)}`} aria-hidden="true" />
        ))}
        {all.map((date, i) => {
          const d = byDate.get(date);
          const future = date > today;
          const label = dayStatusText(date, today, d);
          const late = d && d.late_minutes > 0 ? d.late_minutes : 0;
          const cls = [
            "my-day",
            future ? "is-future" : `att-${d?.status ?? "none"}`,
            date === today ? "is-today" : "",
            (lead + i) % 7 === 6 ? "is-sunday" : "",
          ]
            .filter(Boolean)
            .join(" ");
          const inner = (
            <>
              <span aria-hidden="true" className="my-day-num">
                {Number(date.slice(8))}
              </span>
              <span aria-hidden="true" className="my-day-mark">
                {d ? STATUS_CODE[d.status] : future ? "" : "–"}
              </span>
              {late > 0 && (
                <span aria-hidden="true" className="my-day-late">
                  {late}′
                </span>
              )}
            </>
          );
          return (
            <li key={date} className="my-calendar-cell">
              {future || !onDay ? (
                <span className={cls} aria-label={label} title={label} role="img">
                  {inner}
                </span>
              ) : (
                <button
                  type="button"
                  className={cls}
                  aria-label={label}
                  title={label}
                  aria-haspopup="dialog"
                  aria-current={selected === date ? "date" : undefined}
                  onClick={() => {
                    onDay(date);
                  }}
                >
                  {inner}
                </button>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}

/** The legend as the cells' own marks, small (no sentence of symbols). */
function Legend() {
  const items: [string, string, string][] = [
    ["att-present", "✓", "Đi làm"],
    ["att-leave_p", "P", "Nghỉ có phép"],
    ["att-leave_dx", "ĐX", "Nghỉ đột xuất"],
    ["att-absent_k", "K", "Nghỉ không phép"],
    ["att-none", "–", "Chưa chấm"],
  ];
  return (
    <ul className="my-legend" aria-label="Chú thích">
      {items.map(([cls, mark, word]) => (
        <li key={cls}>
          <span aria-hidden="true" className={`my-legend-mark ${cls}`}>
            {mark}
          </span>
          {word}
        </li>
      ))}
      <li>
        <span aria-hidden="true" className="my-day-late is-inline">
          12′
        </span>
        Đi muộn (phút)
      </li>
    </ul>
  );
}

/** The month at a glance: what I will be paid, what it is made of, what still needs someone. */
function Summary({ data }: { data: MonthData }) {
  const slip = data.payslip;
  const bonus = data.adjustments.filter((a) => a.kind === "bonus").reduce((n, a) => n + (a.amount ?? 0), 0);
  const penalty = data.adjustments.filter((a) => a.kind === "penalty").reduce((n, a) => n + (a.amount ?? 0), 0);
  const flags = (slip?.flags ?? []).filter((f) => f !== "evaluation_missing");
  return (
    <section className="card grid gap-3 p-4 sm:p-5" aria-labelledby="my-summary">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 id="my-summary" className="panel-title">
          Tổng quan tháng
        </h2>
        <span className={`badge ${data.closed ? "badge-ok" : "badge-info"}`}>{data.closed ? "Đã chốt lương" : "Tạm tính"}</span>
      </div>
      <dl className="my-totals">
        <Tile label="Thực lĩnh" value={slip ? vnd(slip.net_pay) : "—"} sub={slip ? undefined : "Chưa có lương cứng"} />
        <Tile label="Hoa hồng" value={slip ? vnd(slip.service_commission + slip.closing_commission) : "—"} sub="làm + giới thiệu / chốt" />
        <Tile label="Thưởng / phạt" value={`+${vnd(bonus)}`} sub={penalty > 0 ? `−${vnd(penalty)}` : "không phạt"} />
        <div>
          <dt>Đánh giá tháng</dt>
          <dd>
            {data.evaluation ? (
              <span className="badge badge-ok">
                {data.evaluation.total_points} điểm · {LEVEL_SHORT[data.evaluation.result]}
              </span>
            ) : (
              <span className="badge badge-warn">Chưa đánh giá</span>
            )}
            <span className="my-tile-sub">{data.evaluation ? "tính vào mức Tết" : "quản lý chấm ngày 1–5 tháng sau"}</span>
          </dd>
        </div>
      </dl>
      {flags.length > 0 && (
        <ul className="flex flex-wrap gap-2" aria-label="Cần xem">
          {flags.map((f) => (
            <li key={f} className="badge badge-warn">
              {FLAG_LABEL[f]}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/**
 * "Công và lương của tôi" (EM 2026-11-07 rearrangement): the month's summary first (net pay,
 * commission, bonuses, the evaluation's status), then the days as a calendar whose days open
 * their detail, the evaluation and bonuses, and the payslip with its work grouped by type. Only
 * the signed-in person's own data (`GET /me/staff`, `GET /me/staff/day`): the server picks the
 * person from the session; nothing here can ask for someone else.
 */
export function MyMonth({ month, day, onDay }: { month: string; day: string; onDay: (date: string) => void }) {
  const query = useQuery(myMonthQuery(month));
  const today = spaToday();
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
  return (
    <div className="my-month grid content-start gap-6">
      {data.closed && <ClosedBanner />}
      <Summary data={data} />
      <section className="card grid gap-4 p-4 sm:p-5" aria-labelledby="my-attendance">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <h2 id="my-attendance" className="panel-title">
            Ngày công
          </h2>
          <p className="text-sm text-text-muted">Bấm một ngày để xem chi tiết.</p>
        </div>
        <dl className="my-totals">
          <Tile label="Đi làm" value={`${String(t.present)} ngày`} />
          <Tile label="Đi muộn" value={`${String(t.late_count)} lần`} sub={t.late_minutes ? `${String(t.late_minutes)} phút` : undefined} />
          <Tile label="Nghỉ P / ĐX" value={`${String(t.leave_p)} / ${String(t.leave_dx)}`} />
          <Tile label="Nghỉ K" value={String(t.absent_k)} />
          <Tile label="Khen / Phàn nàn" value={`${String(compliments)} / ${String(complaints)}`} />
        </dl>
        <MonthCalendar month={data.month} today={today} days={data.attendance.days} selected={day} onDay={onDay} />
        <Legend />
      </section>
      <section className="card grid gap-2 p-4 sm:p-5" aria-labelledby="my-evaluation">
        <h2 id="my-evaluation" className="panel-title">
          Đánh giá và thưởng / phạt
        </h2>
        {data.evaluation ? (
          <p>
            Tháng này: <strong>{data.evaluation.total_points} điểm · {LEVEL_SHORT[data.evaluation.result]}</strong>
            {data.evaluation.comment && <span className="block text-sm text-text-muted">{data.evaluation.comment}</span>}
          </p>
        ) : (
          <p className="text-sm text-text-muted">Chưa có đánh giá tháng (quản lý chấm ngày 1–5 tháng sau).</p>
        )}
        {data.adjustments.length > 0 ? (
          <ul className="work-list">
            {data.adjustments.map((a) => (
              <li key={a.id} className="work-row">
                <span className={`badge ${a.kind === "bonus" ? "badge-ok" : "badge-danger"}`}>{a.kind === "bonus" ? "Thưởng" : "Phạt"}</span>
                <span className="min-w-0 flex-1">
                  {a.reason}
                  <span className="block text-sm text-text-muted">{dayLabel(a.occurred_on)}</span>
                </span>
                <span className="font-semibold tabular">
                  {[a.amount != null ? vnd(a.amount) : "", a.points != null ? `${String(a.points)} điểm` : ""].filter(Boolean).join(" · ")}
                </span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-text-muted">Chưa có khoản thưởng / phạt nào trong tháng.</p>
        )}
      </section>
      <section className="grid gap-2" aria-labelledby="my-payslip">
        <h2 id="my-payslip" className="panel-title">
          Phiếu lương
        </h2>
        {data.payslip ? (
          <PayslipView slip={data.payslip} closed={data.closed} showFlags={false} />
        ) : (
          <p className="text-text-muted">Chủ spa chưa nhập lương cho bạn.</p>
        )}
      </section>
      {day && (
        <MyDaySheet
          date={day}
          today={today}
          onDay={onDay}
          onClose={() => {
            onDay("");
          }}
        />
      )}
    </div>
  );
}
