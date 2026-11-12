import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { meQuery } from "../auth/session";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { AdjustmentLogSection } from "./AdjustmentsTab";
import { adjustmentsQuery, attendanceQuery, reviewsQuery, type StaffReviews, type StaffSheet } from "./api";
import { STATUS_LABEL, daysOf, weekdayShort } from "./labels";
import { PersonDaySheet, PersonDayView, tracesBooks } from "./PersonDaySheet";
import { cellOf, employedOn } from "./progress";
import { AttendanceLegend, ClosedBanner, StatusMark, dayLabel, vnd } from "./ui";

/**
 * "Bảng công tháng" (spec §10 US8): the sheet's month grid — rows = staff, columns = days, totals
 * per person — usable at 390 px (horizontal scroll, sticky names), then the month's bonus/penalty
 * log. A cell opens the same person-day sheet as the daily book; a cell that cannot be written
 * opens it read-only (a closed month; a person only the owner records, when its history may be
 * seen: spec record-history K30, K49).
 */
export function MonthSheet({
  month,
  today,
  canReview,
  staffId = "",
  onAll,
}: {
  month: string;
  today: string;
  canReview: boolean;
  /** One person's month (EM 2026-11-08, from their evaluation history): their row and their days. */
  staffId?: string;
  onAll?: () => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const attendance = useQuery(attendanceQuery(month));
  const reviews = useQuery({ ...reviewsQuery(month), enabled: canReview });
  const [editing, setEditing] = useState<{ staff: StaffSheet; date: string; view: boolean } | null>(null);

  if (attendance.isPending) return <RowsSkeleton rows={6} />;
  if (attendance.isError) return <ErrorState error={attendance.error} onRetry={() => void attendance.refetch()} />;
  const sheet = attendance.data;
  const reviewsOf = (id: string) => reviews.data?.staff.find((s) => s.staff_id === id);
  const person = staffId ? sheet.staff.find((s) => s.staff_id === staffId) : undefined;
  const people = staffId ? (person ? [person] : []) : sheet.staff;
  const writable = (s: StaffSheet) => !sheet.closed && s.can_record;
  const traced = tracesBooks(me, true, canReview);
  /** Whether a person's cells open at all: to write, else read-only (the grid's own rule for the days: none to come, none outside their work). */
  const opens = (s: StaffSheet) => writable(s) || sheet.closed || traced;

  return (
    <div className="grid content-start gap-6">
      {sheet.closed && <ClosedBanner />}
      {staffId && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-text-muted">Chỉ xem:</span>
          <button type="button" className="book-chip" aria-label={`Xem cả spa thay vì ${person?.full_name ?? "một người"}`} onClick={onAll}>
            {person?.full_name ?? "Nhân viên đã chọn"}
            <Icon name="x" className="size-4" />
          </button>
        </div>
      )}
      {people.length === 0 ? (
        <p className="empty-block">{staffId ? "Người này không làm việc trong tháng này." : "Chưa có nhân viên nào làm việc trong tháng này."}</p>
      ) : (
        <MonthGrid
          month={sheet.month}
          today={today}
          people={people}
          reviewsOf={canReview ? reviewsOf : undefined}
          opens={opens}
          hint={sheet.closed ? "Bấm một ô để xem." : "Bấm một ô để sửa."}
          onCell={(staff, date) => {
            if (opens(staff)) setEditing({ staff, date, view: !writable(staff) });
          }}
        />
      )}
      {person && <PersonDays person={person} reviews={reviewsOf(person.staff_id)} month={month} canReview={canReview} />}
      {canReview && !staffId && <AdjustmentLogSection month={month} today={today} canWrite={!sheet.closed} />}
      {editing &&
        (editing.view ? (
          <PersonDayView
            staff={editing.staff}
            date={editing.date}
            review={reviewsOf(editing.staff.staff_id)?.days.find((d) => d.date === editing.date)}
            canAttendance
            canReview={canReview}
            onClose={() => {
              setEditing(null);
            }}
          />
        ) : (
          <PersonDaySheet
            staff={editing.staff}
            date={editing.date}
            review={reviewsOf(editing.staff.staff_id)?.days.find((d) => d.date === editing.date)}
            canAttendance
            canReview={canReview}
            onClose={() => {
              setEditing(null);
            }}
          />
        ))}
    </div>
  );
}

export function MonthGrid({
  month,
  today,
  people,
  reviewsOf,
  opens,
  hint,
  onCell,
}: {
  month: string;
  today: string;
  people: StaffSheet[];
  reviewsOf: ((id: string) => StaffReviews | undefined) | undefined;
  /** Whether this person's days are buttons (to write or to view); otherwise shown, not opened. */
  opens: (s: StaffSheet) => boolean;
  hint: string;
  onCell: (s: StaffSheet, date: string) => void;
}) {
  const days = daysOf(month);
  return (
    <div className="grid gap-2">
      <div className="card table-wrap attendance-grid" role="region" tabIndex={0} aria-label={`Bảng công ${month}, cuộn ngang để xem các ngày`}>
        <table className="data-table">
          <caption className="sr-only">Bảng chấm công {month}</caption>
          <thead>
            <tr>
              <th scope="col" className="sticky-col">
                Nhân viên
              </th>
              {days.map((d) => (
                <th key={d} scope="col" className={weekdayShort(d) === "CN" ? "is-sunday" : undefined}>
                  <span className="block">{Number(d.slice(8))}</span>
                  <span className="block text-xs font-normal text-text-muted">{weekdayShort(d)}</span>
                </th>
              ))}
              <th scope="col" className="total-col">
                Công
              </th>
              <th scope="col" className="total-col">
                Muộn
              </th>
              <th scope="col" className="total-col">
                Nghỉ
              </th>
              <th scope="col" className="total-col">
                Chưa chấm
              </th>
              {reviewsOf && (
                <th scope="col" className="total-col">
                  Khen/Phàn nàn
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {people.map((s) => {
              const r = reviewsOf?.(s.staff_id);
              return (
                <tr key={s.staff_id}>
                  <th scope="row" className="sticky-col">
                    {s.full_name}
                    {!s.can_record && <span className="block text-xs font-normal text-text-muted">Chủ spa chấm</span>}
                  </th>
                  {days.map((d) => {
                    const cell = cellOf(s, d);
                    const off = !employedOn(s, d) || d > today;
                    return (
                      <td key={d} className="p-0 text-center">
                        {off ? (
                          <span className="text-text-muted" aria-hidden="true">
                            ·
                          </span>
                        ) : !opens(s) ? (
                          // Decision C: the owner's to write; shown, not opened (without its history to see).
                          <span className={`att-cell att-${cell?.status ?? "none"}${cell && cell.late_minutes > 0 ? " is-late" : ""}`}>
                            {cell ? <StatusMark status={cell.status} /> : <span aria-hidden="true">–</span>}
                            <span className="sr-only">{cellText(s, d, cell)}</span>
                          </span>
                        ) : (
                          <button
                            type="button"
                            className={`att-cell att-${cell?.status ?? "none"}${cell && cell.late_minutes > 0 ? " is-late" : ""}`}
                            aria-label={cellText(s, d, cell)}
                            onClick={() => {
                              onCell(s, d);
                            }}
                          >
                            {cell ? <StatusMark status={cell.status} /> : "–"}
                          </button>
                        )}
                      </td>
                    );
                  })}
                  <td className="total-col">{s.totals.present}</td>
                  <td className="total-col">
                    {s.totals.late_count > 0 ? `${String(s.totals.late_count)} lần / ${String(s.totals.late_minutes)}′` : "–"}
                  </td>
                  <td className="total-col">
                    <LeaveCounts totals={s.totals} />
                  </td>
                  <td className={`total-col${s.totals.unrecorded > 0 ? " text-danger" : ""}`}>{s.totals.unrecorded || "–"}</td>
                  {reviewsOf && (
                    <td className="total-col">
                      {r?.compliments ?? 0}/{r?.complaints ?? 0}
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <AttendanceLegend late="underline" />
      <p className="text-sm text-text-muted">{hint}</p>
    </div>
  );
}

/** A cell's name in words: who, which day, the status and minutes late. */
function cellText(s: StaffSheet, d: string, cell: ReturnType<typeof cellOf>): string {
  const late = cell && cell.late_minutes > 0 ? `, muộn ${String(cell.late_minutes)} phút` : "";
  return `${s.full_name}, ${dayLabel(d)}: ${cell ? STATUS_LABEL[cell.status] : "chưa chấm"}${late}`;
}

/** The month's days off by kind, each as its mark and count ("–" when none), named in words. */
function LeaveCounts({ totals }: { totals: StaffSheet["totals"] }) {
  const kinds = (
    [
      ["leave_p", totals.leave_p, "có phép"],
      ["leave_dx", totals.leave_dx, "đột xuất"],
      ["absent_k", totals.absent_k, "không phép"],
    ] as const
  ).filter(([, n]) => n > 0);
  if (kinds.length === 0) return <>–</>;
  return (
    <span className="leave-counts">
      {kinds.map(([st, n, word]) => (
        <span key={st} className={`leave-count att-${st}`}>
          <StatusMark status={st} className="size-3.5" />
          {n}
          <span className="sr-only"> {word}</span>
        </span>
      ))}
    </span>
  );
}

/**
 * One person's month, day by day, read-only (EM 2026-11-08): only the days with something
 * recorded — attendance (times, minutes late, note), praise / complaints with their note, and the
 * day's bonuses / penalties. Built from the month queries the page already holds; the grid above
 * still opens a day to edit where allowed.
 */
function PersonDays({
  person,
  reviews,
  month,
  canReview,
}: {
  person: StaffSheet;
  reviews: StaffReviews | undefined;
  month: string;
  canReview: boolean;
}) {
  const adjustments = useQuery({ ...adjustmentsQuery(month), enabled: canReview });
  const mine = (adjustments.data?.items ?? []).filter((a) => a.staff_id === person.staff_id);
  const days = daysOf(month).filter(
    (d) =>
      person.days.some((c) => c.date === d) ||
      reviews?.days.some((r) => r.date === d && (r.compliments > 0 || r.complaints > 0 || r.note)) ||
      mine.some((a) => a.occurred_on === d),
  );
  return (
    <section className="grid gap-2" aria-labelledby="person-days">
      <h2 id="person-days" className="panel-title">
        Từng ngày · {person.full_name}
      </h2>
      {days.length === 0 ? (
        <p className="empty-block">Chưa có ngày nào được ghi trong tháng này.</p>
      ) : (
        <ol className="card person-days">
          {days.map((d) => {
            const cell = cellOf(person, d);
            const review = reviews?.days.find((r) => r.date === d);
            const adj = mine.filter((a) => a.occurred_on === d);
            const times = [cell?.check_in && `đến ${cell.check_in}`, cell?.check_out && `về ${cell.check_out}`].filter(Boolean).join(" · ");
            return (
              <li key={d} className="person-day">
                <p className="person-day-date">{dayLabel(d)}</p>
                <div className="grid gap-1 text-sm">
                  {cell && (
                    <p>
                      <span className={`att-cell att-${cell.status}`} aria-hidden="true">
                        <StatusMark status={cell.status} />
                      </span>{" "}
                      {STATUS_LABEL[cell.status]}
                      {times && ` · ${times}`}
                      {cell.late_minutes > 0 && <span className="text-danger"> · muộn {cell.late_minutes} phút</span>}
                      {cell.note && <span className="block text-text-muted">{cell.note}</span>}
                    </p>
                  )}
                  {review && (review.compliments > 0 || review.complaints > 0 || review.note) && (
                    <p>
                      {review.compliments} khen · {review.complaints} phàn nàn
                      {review.note && <span className="block text-text-muted">{review.note}</span>}
                    </p>
                  )}
                  {adj.map((a) => (
                    <p key={a.id}>
                      <span className={`badge ${a.kind === "bonus" ? "badge-ok" : "badge-danger"}`}>{a.kind === "bonus" ? "Thưởng" : "Phạt"}</span>{" "}
                      {[a.amount != null ? vnd(a.amount) : "", a.points != null ? `${String(a.points)} điểm` : ""].filter(Boolean).join(" · ")} ·{" "}
                      {a.reason}
                    </p>
                  ))}
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
