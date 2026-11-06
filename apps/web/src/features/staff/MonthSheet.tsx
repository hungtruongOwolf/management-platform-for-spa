import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { AdjustmentLogSection } from "./AdjustmentsTab";
import { attendanceQuery, reviewsQuery, type StaffReviews, type StaffSheet } from "./api";
import { STATUS_CODE, STATUS_LABEL, daysOf, weekdayShort } from "./labels";
import { PersonDaySheet } from "./PersonDaySheet";
import { cellOf, employedOn } from "./progress";
import { ClosedBanner, dayLabel } from "./ui";

/**
 * "Bảng công tháng" (spec §10 US8): the sheet's month grid — rows = staff, columns = days, totals
 * per person — usable at 390 px (horizontal scroll, sticky names), then the month's bonus/penalty
 * log. A cell opens the same person-day sheet as the daily book.
 */
export function MonthSheet({ month, today, canReview }: { month: string; today: string; canReview: boolean }) {
  const attendance = useQuery(attendanceQuery(month));
  const reviews = useQuery({ ...reviewsQuery(month), enabled: canReview });
  const [editing, setEditing] = useState<{ staff: StaffSheet; date: string } | null>(null);

  if (attendance.isPending) return <RowsSkeleton rows={6} />;
  if (attendance.isError) return <ErrorState error={attendance.error} onRetry={() => void attendance.refetch()} />;
  const sheet = attendance.data;
  const reviewsOf = (id: string) => reviews.data?.staff.find((s) => s.staff_id === id);

  return (
    <div className="grid content-start gap-6">
      {sheet.closed && <ClosedBanner />}
      {sheet.staff.length === 0 ? (
        <p className="text-text-muted">Chưa có nhân viên nào làm việc trong tháng này.</p>
      ) : (
        <MonthGrid
          month={sheet.month}
          today={today}
          people={sheet.staff}
          reviewsOf={canReview ? reviewsOf : undefined}
          onCell={(staff, date) => {
            if (!sheet.closed) setEditing({ staff, date });
          }}
        />
      )}
      {canReview && <AdjustmentLogSection month={month} today={today} canWrite={!sheet.closed} />}
      {editing && (
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
      )}
    </div>
  );
}

export function MonthGrid({
  month,
  today,
  people,
  reviewsOf,
  onCell,
}: {
  month: string;
  today: string;
  people: StaffSheet[];
  reviewsOf: ((id: string) => StaffReviews | undefined) | undefined;
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
                P/ĐX/K
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
                        ) : (
                          <button
                            type="button"
                            className={`att-cell att-${cell?.status ?? "none"}${cell && cell.late_minutes > 0 ? " is-late" : ""}`}
                            aria-label={`${cell && cell.status !== "present" ? `${STATUS_CODE[cell.status]}, ` : ""}${s.full_name}, ${dayLabel(d)}: ${cell ? STATUS_LABEL[cell.status] : "chưa chấm"}${cell && cell.late_minutes > 0 ? `, muộn ${String(cell.late_minutes)} phút` : ""}`}
                            onClick={() => {
                              onCell(s, d);
                            }}
                          >
                            {cell ? STATUS_CODE[cell.status] : "–"}
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
                    {s.totals.leave_p}/{s.totals.leave_dx}/{s.totals.absent_k}
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
      <p className="text-sm text-text-muted">
        ✓ đi làm · P nghỉ có phép · ĐX nghỉ đột xuất · K nghỉ không phép · – chưa chấm · gạch đỏ dưới ô: đi muộn. Bấm một ô để sửa.
      </p>
    </div>
  );
}
