import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { DateField } from "../../components/DateField";
import { messageFor } from "../../lib/errors";
import { Icon } from "../../app/Icon";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { activeStaffQuery } from "../sales/api";
import {
  adjustmentsQuery,
  attendanceQuery,
  evaluationsQuery,
  invalidateStaff,
  reviewsQuery,
  saveAttendance,
  saveReviews,
  type Adjustment,
  type AttendanceEntryWrite,
  type DailyReview,
  type StaffSheet,
} from "./api";
import { AdjustmentForm } from "./AdjustmentsTab";
import { Counter } from "./Counter";
import { monthLabel, STATUSES, STATUS_CODE, STATUS_LABEL, type AttendanceStatus } from "./labels";
import { PersonDaySheet } from "./PersonDaySheet";
import { cellOf, employedOn, evaluationDueMonth, incompleteDays, missingOn } from "./progress";
import { ClosedBanner, dayLabel, lastDayOrToday, vnd } from "./ui";

/** "2026-11-04" → "T3 04/11". */
const shortDay = (iso: string) => dayLabel(iso).slice(0, -5).replace(",", "");

/**
 * "Sổ hằng ngày" (spec §10 US7): one page per day. The manager goes down the list once — per
 * person: attendance (one tap), praise/complaints, bonuses/penalties, details — with a progress
 * bar of who is still unrecorded, the past days still incomplete, and a prompt for the monthly
 * evaluation only while it is due. Every control is UX only; each API enforces its permission
 * and the payroll close lock on its own.
 */
export function DailyBook({
  month,
  day,
  today,
  canAttendance,
  canReview,
  onDay,
  onOpenEvaluation,
}: {
  month: string;
  day: string;
  today: string;
  canAttendance: boolean;
  canReview: boolean;
  onDay: (d: string) => void;
  onOpenEvaluation: (month: string) => void;
}) {
  const queryClient = useQueryClient();
  const attendance = useQuery({ ...attendanceQuery(month), enabled: canAttendance });
  const reviews = useQuery({ ...reviewsQuery(month), enabled: canReview });
  const adjustments = useQuery({ ...adjustmentsQuery(month), enabled: canReview });
  const staff = useQuery({ ...activeStaffQuery, enabled: canReview });
  const dueMonth = canReview ? evaluationDueMonth(today) : null;
  const due = useQuery({ ...evaluationsQuery(dueMonth ?? month), enabled: dueMonth !== null });
  const [details, setDetails] = useState<StaffSheet | null>(null);
  const [adjusting, setAdjusting] = useState<StaffSheet | null>(null);
  const [notice, setNotice] = useState("");
  const onError = (e: unknown) => {
    setNotice(messageFor(e));
  };
  const onSaved = () => invalidateStaff(queryClient);
  const mark = useMutation({ mutationFn: saveAttendance, onSuccess: onSaved, onError });
  const review = useMutation({ mutationFn: saveReviews, onSuccess: onSaved, onError });

  const pending = (canAttendance && attendance.isPending) || (canReview && reviews.isPending);
  if (pending) return <RowsSkeleton rows={6} />;
  if (attendance.isError) return <ErrorState error={attendance.error} onRetry={() => void attendance.refetch()} />;
  if (reviews.isError) return <ErrorState error={reviews.error} onRetry={() => void reviews.refetch()} />;

  const people: StaffSheet[] =
    attendance.data?.staff ??
    // A role with evaluation.record only: the review sheet lists the same people, without attendance.
    (reviews.data?.staff ?? []).map((s) => ({
      staff_id: s.staff_id,
      full_name: s.full_name,
      position: s.position,
      shift_start: "",
      days: [],
      totals: { present: 0, late_count: 0, late_minutes: 0, leave_p: 0, leave_dx: 0, absent_k: 0, unrecorded: 0 },
    }));
  const closed = attendance.data?.closed ?? reviews.data?.closed ?? false;
  const date = day.startsWith(month) ? day : month === today.slice(0, 7) ? today : lastDayOrToday(month, today);
  const working = people.filter((s) => employedOn(s, date));
  const missing = canAttendance ? missingOn(working, date) : [];
  const recorded = working.length - missing.length;
  const pastGaps = canAttendance ? incompleteDays(month, people, today).filter((d) => d !== date) : [];
  const future = date > today;
  const readOnly = closed || future;
  const reviewOf = (id: string): DailyReview | undefined =>
    reviews.data?.staff.find((s) => s.staff_id === id)?.days.find((d) => d.date === date);
  const adjustmentsOf = (id: string): Adjustment[] =>
    (adjustments.data?.items ?? []).filter((a) => a.staff_id === id && a.occurred_on === date);
  const dueMissing = due.data && !due.data.closed ? due.data.staff.filter((s) => !s.evaluation) : [];
  const record = (entries: AttendanceEntryWrite[]) => {
    setNotice("");
    mark.mutate(entries);
  };

  return (
    <div className="grid content-start gap-4">
      {closed && <ClosedBanner />}
      {dueMonth && dueMissing.length > 0 && (
        <div role="status" className="book-banner book-banner-accent">
          <Icon name="alert" className="size-5 shrink-0" />
          <p className="min-w-0 flex-1">
            <strong>Đến hạn đánh giá {monthLabel(dueMonth).toLowerCase()}:</strong> còn {dueMissing.length} người chưa chấm điểm (
            {dueMissing.map((s) => s.full_name).join(", ")}).
          </p>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              onOpenEvaluation(dueMonth);
            }}
          >
            Đánh giá ngay
          </button>
        </div>
      )}
      {pastGaps.length > 0 && (
        <div role="status" className="book-banner">
          <p className="min-w-0 flex-1">
            <strong>Còn {pastGaps.length} ngày chưa chấm đủ</strong> trong {monthLabel(month).toLowerCase()}:
          </p>
          <div className="flex flex-wrap gap-2">
            {pastGaps.map((d) => (
              <button
                key={d}
                type="button"
                className="book-chip"
                onClick={() => {
                  onDay(d);
                }}
              >
                {shortDay(d)}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="book-head">
        <DateField
          label="Ngày"
          value={date}
          min={`${month}-01`}
          max={lastDayOrToday(month, today)}
          onChange={(d) => {
            if (d) onDay(d);
          }}
        />
        {canAttendance && working.length > 0 && (
          <div className="book-progress" role="status" aria-label="Tiến độ chấm công">
            <p>
              {missing.length === 0 ? (
                <strong className="text-success">✓ Xong ngày này: đã chấm công đủ {working.length} người</strong>
              ) : (
                <>
                  Đã chấm công <strong className="tabular">{recorded}/{working.length}</strong> người
                </>
              )}
            </p>
            <div className="book-progress-bar" aria-hidden="true">
              <span style={{ width: `${String(working.length ? (recorded * 100) / working.length : 0)}%` }} />
            </div>
          </div>
        )}
      </div>

      {canAttendance && missing.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-text-muted">Còn thiếu:</span>
          {missing.map((s) => (
            <button
              key={s.staff_id}
              type="button"
              className="book-chip"
              aria-label={`Tới ${s.full_name}`}
              onClick={() => {
                // Scroll to the card without touching the URL (a #hash would add a history entry).
                const el = document.getElementById(`book-${s.staff_id}`);
                el?.scrollIntoView({ block: "center" });
                el?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus({ preventScroll: true });
              }}
            >
              {s.full_name}
            </button>
          ))}
          {!readOnly && (
            <button
              type="button"
              className="btn btn-secondary"
              disabled={mark.isPending}
              onClick={() => {
                record(missing.map((s) => ({ staff_id: s.staff_id, date, status: "present" })));
              }}
            >
              Cả spa đi làm
            </button>
          )}
        </div>
      )}
      <p className="text-sm text-text-muted">
        {canAttendance ? "Chấm công là bắt buộc mỗi ngày. " : ""}
        {canReview ? "Khen, phàn nàn, thưởng/phạt: có thì ghi, không bắt buộc." : ""}
      </p>
      {notice && (
        <p role="alert" className="text-sm text-danger">
          {notice}
        </p>
      )}

      {working.length === 0 ? (
        <p className="text-text-muted">Không có nhân viên nào làm việc ngày này.</p>
      ) : (
        <ul className="grid gap-3" aria-label={`Sổ ngày ${dayLabel(date)}`}>
          {working.map((s) => (
            <PersonCard
              key={s.staff_id}
              staff={s}
              date={date}
              review={reviewOf(s.staff_id)}
              adjustments={adjustmentsOf(s.staff_id)}
              canAttendance={canAttendance}
              canReview={canReview}
              readOnly={readOnly}
              busy={mark.isPending || review.isPending}
              onStatus={(status) => {
                record([{ staff_id: s.staff_id, date, status }]);
              }}
              onReview={(compliments, complaints) => {
                setNotice("");
                review.mutate([{ staff_id: s.staff_id, date, compliments, complaints, note: reviewOf(s.staff_id)?.note ?? null }]);
              }}
              onAdjust={() => {
                setAdjusting(s);
              }}
              onDetails={() => {
                setDetails(s);
              }}
            />
          ))}
        </ul>
      )}

      {details && (
        <PersonDaySheet
          staff={details}
          date={date}
          review={reviewOf(details.staff_id)}
          canAttendance={canAttendance}
          canReview={canReview}
          onClose={() => {
            setDetails(null);
          }}
        />
      )}
      {adjusting && (
        <AdjustmentForm
          today={today}
          closedMonth={null}
          staff={staff.data ?? []}
          fixed={{ staffId: adjusting.staff_id, name: adjusting.full_name, date }}
          onClose={() => {
            setAdjusting(null);
          }}
        />
      )}
    </div>
  );
}

function PersonCard({
  staff: s,
  date,
  review,
  adjustments,
  canAttendance,
  canReview,
  readOnly,
  busy,
  onStatus,
  onReview,
  onAdjust,
  onDetails,
}: {
  staff: StaffSheet;
  date: string;
  review: DailyReview | undefined;
  adjustments: Adjustment[];
  canAttendance: boolean;
  canReview: boolean;
  readOnly: boolean;
  busy: boolean;
  onStatus: (status: AttendanceStatus) => void;
  onReview: (compliments: number, complaints: number) => void;
  onAdjust: () => void;
  onDetails: () => void;
}) {
  const cell = cellOf(s, date);
  const c = review?.compliments ?? 0;
  const k = review?.complaints ?? 0;
  return (
    <li id={`book-${s.staff_id}`} className={`card book-card${canAttendance && !cell ? " is-missing" : ""}`}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="font-semibold">{s.full_name}</p>
          <p className="text-sm text-text-muted">
            {s.position}
            {cell?.check_in && ` · đến ${cell.check_in}`}
            {cell?.check_out && ` · về ${cell.check_out}`}
            {cell && cell.late_minutes > 0 && <span className="text-danger"> · muộn {cell.late_minutes} phút</span>}
          </p>
        </div>
        {canAttendance &&
          (cell ? (
            <span className="badge badge-ok">{STATUS_LABEL[cell.status]}</span>
          ) : (
            <span className="badge badge-warn">Chưa chấm công</span>
          ))}
      </div>
      {canAttendance && (
        <div className="segmented book-status" role="group" aria-label={`Chấm công ${s.full_name}`}>
          {STATUSES.map((st) => (
            <button
              key={st}
              type="button"
              aria-pressed={cell?.status === st}
              title={STATUS_LABEL[st]}
              aria-label={`${STATUS_CODE[st]} ${STATUS_LABEL[st]}`}
              disabled={readOnly || busy}
              onClick={() => {
                onStatus(st);
              }}
            >
              {STATUS_CODE[st]}
            </button>
          ))}
        </div>
      )}
      {canReview && (
        <div className="flex flex-wrap gap-x-5 gap-y-2">
          <Counter
            label="Khen"
            name={s.full_name}
            value={c}
            disabled={readOnly || busy}
            onChange={(v) => {
              onReview(v, k);
            }}
          />
          <Counter
            label="Phàn nàn"
            name={s.full_name}
            value={k}
            disabled={readOnly || busy}
            onChange={(v) => {
              onReview(c, v);
            }}
          />
        </div>
      )}
      {(cell?.note || review?.note || adjustments.length > 0) && (
        <ul className="grid gap-1 text-sm">
          {cell?.note && <li className="text-text-muted">Chấm công: {cell.note}</li>}
          {review?.note && <li className="text-text-muted">Đánh giá: {review.note}</li>}
          {adjustments.map((a) => (
            <li key={a.id}>
              <span className={`badge ${a.kind === "bonus" ? "badge-ok" : "badge-danger"}`}>{a.kind === "bonus" ? "Thưởng" : "Phạt"}</span>{" "}
              {[a.amount != null ? vnd(a.amount) : "", a.points != null ? `${String(a.points)} điểm` : ""].filter(Boolean).join(" · ")} · {a.reason}
            </li>
          ))}
        </ul>
      )}
      {!readOnly && (
        <div className="flex flex-wrap gap-2">
          {canReview && (
            <button type="button" className="btn btn-secondary" onClick={onAdjust}>
              + Thưởng / phạt
            </button>
          )}
          <button type="button" className="btn btn-secondary" onClick={onDetails}>
            Giờ & ghi chú
          </button>
        </div>
      )}
    </li>
  );
}
