import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
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
  type DailyReviewEntryWrite,
  type StaffSheet,
} from "./api";
import { AdjustmentActions, AdjustmentForm } from "./AdjustmentsTab";
import { AttendanceChoice } from "./AttendanceChoice";
import { Counter } from "./Counter";
import { STATUS_WORD, monthLabel, type AttendanceStatus } from "./labels";
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
  const [adjusting, setAdjusting] = useState<{ staff: StaffSheet; entry?: Adjustment } | null>(null);
  // A leave code for someone who served customers that day: confirm first (likely a mistake).
  const [confirmLeave, setConfirmLeave] = useState<{ staff: StaffSheet; status: AttendanceStatus } | null>(null);
  // A failed save shows on the person's card; one covering several people ("Cả spa đi làm") on the
  // page. The card rolls back by itself: it renders the server's data, which did not change.
  const [failed, setFailed] = useState<{ staffId: string | null; message: string } | null>(null);
  const onError = (e: unknown, entries: { staff_id: string }[]) => {
    setFailed({ staffId: entries.length === 1 ? (entries[0]?.staff_id ?? null) : null, message: messageFor(e) });
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
      can_record: s.can_record,
      visit_days: [],
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
    setFailed(null);
    mark.mutate(entries);
  };
  // Only the rows the caller may write (decision C: a manager never for themselves or another manager).
  const everyonePresent = missing.filter((s) => s.can_record);
  /** What is being saved for a person right now: shown at once, replaced by the server's answer. */
  const savingStatus = (id: string): AttendanceStatus | null =>
    (mark.isPending ? mark.variables.find((e) => e.staff_id === id)?.status : null) ?? null;
  const savingReview = (id: string) => (review.isPending ? review.variables.find((e) => e.staff_id === id) : undefined);
  const notice = failed && failed.staffId === null ? failed.message : "";

  return (
    <div className="grid content-start gap-4">
      {closed && <ClosedBanner />}
      {dueMonth && dueMissing.length > 0 && (
        <div role="status" className="book-banner callout">
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
        <div role="status" className="note book-banner">
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
          // Same shape as the date field beside it (a label line, then a 44 px box), so the two
          // labels share one baseline and the two boxes another.
          <div className="book-progress" role="status" aria-label="Tiến độ chấm công">
            <span className="book-progress-label">Tiến độ</span>
            <div className="book-progress-box">
              {missing.length === 0 ? (
                <p className="flex items-center gap-1.5 font-semibold text-success">
                  <Icon name="check" className="size-4" />
                  Xong ngày này: đã chấm công đủ {working.length} người
                </p>
              ) : (
                <p>
                  Đã chấm công <strong className="tabular">{recorded}/{working.length}</strong> người
                </p>
              )}
              <div className="book-progress-bar" aria-hidden="true">
                <span style={{ width: `${String(working.length ? (recorded * 100) / working.length : 0)}%` }} />
              </div>
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
                el?.querySelector<HTMLButtonElement>('[role="radio"][tabindex="0"]')?.focus({ preventScroll: true });
              }}
            >
              {s.full_name}
            </button>
          ))}
          {!readOnly && everyonePresent.length > 0 && (
            <button
              type="button"
              className="btn btn-secondary"
              disabled={mark.isPending}
              onClick={() => {
                record(everyonePresent.map((s) => ({ staff_id: s.staff_id, date, status: "present" })));
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
        <p className="empty-block">Không có nhân viên nào làm việc ngày này.</p>
      ) : (
        <ul className="card book-list" aria-label={`Sổ ngày ${dayLabel(date)}`}>
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
              savingStatus={savingStatus(s.staff_id)}
              savingReview={savingReview(s.staff_id)}
              error={failed?.staffId === s.staff_id ? failed.message : ""}
              onStatus={(status) => {
                if (status !== "present" && s.visit_days.includes(date)) {
                  setConfirmLeave({ staff: s, status });
                  return;
                }
                record([{ staff_id: s.staff_id, date, status }]);
              }}
              onReview={(compliments, complaints) => {
                setFailed(null);
                review.mutate([{ staff_id: s.staff_id, date, compliments, complaints, note: reviewOf(s.staff_id)?.note ?? null }]);
              }}
              onAdjust={(entry) => {
                setAdjusting(entry ? { staff: s, entry } : { staff: s });
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
          fixed={{ staffId: adjusting.staff.staff_id, name: adjusting.staff.full_name, date: adjusting.entry?.occurred_on ?? date }}
          {...(adjusting.entry ? { editing: adjusting.entry } : {})}
          onClose={() => {
            setAdjusting(null);
          }}
        />
      )}
      {confirmLeave && (
        <BottomSheet
          open
          onClose={() => {
            setConfirmLeave(null);
          }}
          title={`${confirmLeave.staff.full_name} có lượt khách ngày này`}
          description={dayLabel(date)}
          footer={
            <>
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  setConfirmLeave(null);
                }}
              >
                Thôi
              </button>
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => {
                  record([{ staff_id: confirmLeave.staff.staff_id, date, status: confirmLeave.status }]);
                  setConfirmLeave(null);
                }}
              >
                Vẫn chấm {STATUS_WORD[confirmLeave.status]}
              </button>
            </>
          }
        >
          <p className="text-sm">
            Quầy đã ghi lượt khách do {confirmLeave.staff.full_name} làm trong ngày này. Chấm “{STATUS_WORD[confirmLeave.status]}” thì ngày
            này tính là ngày nghỉ. Kiểm tra lại lượt khách hoặc chấm “Đi làm” nếu người này có đến làm.
          </p>
        </BottomSheet>
      )}
    </div>
  );
}

/**
 * One person's row in the day's panel (EM 2026-11-07: the card was far bigger than its content).
 * Same grid areas in every row so they line up down the list (staff.css `.book-row`):
 * - who: name, role/times, the flags that add information (not recorded yet, late);
 * - the attendance choice, in words;
 * - the day's record: Khen / Phàn nàn steppers and "Ghi thưởng / phạt", the day's
 *   bonuses/penalties and notes as compact rows under it;
 * - "Giờ & ghi chú".
 * Wide rows: who on the left; attendance + "Giờ & ghi chú", then the record (two lines). Narrower rows
 * stack. No chip repeats the chosen status: the filled option already says it.
 */
function PersonCard({
  staff: s,
  date,
  review,
  adjustments,
  canAttendance,
  canReview,
  readOnly,
  busy,
  savingStatus,
  savingReview,
  error,
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
  savingStatus: AttendanceStatus | null;
  savingReview: DailyReviewEntryWrite | undefined;
  error: string;
  onStatus: (status: AttendanceStatus) => void;
  onReview: (compliments: number, complaints: number) => void;
  /** Without an entry: record a new one; with one: edit it. */
  onAdjust: (entry?: Adjustment) => void;
  onDetails: () => void;
}) {
  const cell = cellOf(s, date);
  // Decision C: a manager's own row and other managers' rows are the owner's to write.
  const locked = !s.can_record;
  const shut = readOnly || locked;
  // While a review save runs, show the counts being saved; the server's answer replaces them.
  const c = savingReview?.compliments ?? review?.compliments ?? 0;
  const k = savingReview?.complaints ?? review?.complaints ?? 0;
  const missing = canAttendance && !cell && !savingStatus;
  const times = [cell?.check_in && `đến ${cell.check_in}`, cell?.check_out && `về ${cell.check_out}`].filter(Boolean).join(" · ");
  const hasLog = Boolean(cell?.note || review?.note || adjustments.length > 0);
  return (
    <li id={`book-${s.staff_id}`} className="book-row">
      <div className="book-row-who">
        <p className="book-row-name">{s.full_name}</p>
        <p className="text-sm text-text-muted">
          {s.position}
          {times && ` · ${times}`}
        </p>
        {(missing || (cell && cell.late_minutes > 0)) && (
          <p className="book-row-flags">
            {missing && <span className="badge badge-warn">Chưa chấm công</span>}
            {cell && cell.late_minutes > 0 && <span className="badge badge-danger">Đi muộn {cell.late_minutes} phút</span>}
          </p>
        )}
      </div>
      {!shut && (
        <button type="button" className="btn btn-secondary book-row-details" onClick={onDetails}>
          <Icon name="clock" className="size-4" />
          Giờ & ghi chú
        </button>
      )}
      {canAttendance && (
        <AttendanceChoice
          name={s.full_name}
          value={cell?.status ?? null}
          pending={savingStatus}
          busy={busy}
          disabled={shut}
          onChange={onStatus}
        />
      )}
      {locked && !readOnly && (canAttendance || canReview) && (
        <p className="book-row-hint text-sm text-text-muted">
          <Icon name="lock" className="size-4" />
          Chỉ chủ spa chấm công, ghi khen/phàn nàn và thưởng/phạt cho quản lý.
        </p>
      )}
      {error && (
        <p role="alert" className="book-row-error">
          <Icon name="alert" className="size-4" />
          Chưa lưu được: {error}
        </p>
      )}
      {(canReview || hasLog) && (
        <section className="book-record" aria-label={`Ghi nhận trong ngày, ${s.full_name}`}>
          {canReview && (
            <div className="book-record-row">
              <Counter
                label="Khen"
                name={s.full_name}
                value={c}
                disabled={shut}
                busy={busy}
                onChange={(v) => {
                  onReview(v, k);
                }}
              />
              <Counter
                label="Phàn nàn"
                name={s.full_name}
                value={k}
                disabled={shut}
                busy={busy}
                onChange={(v) => {
                  onReview(c, v);
                }}
              />
              {!shut && (
                <button
                  type="button"
                  className="btn btn-secondary book-record-add"
                  onClick={() => {
                    onAdjust();
                  }}
                >
                  <Icon name="plus" className="size-4" />
                  Ghi thưởng / phạt
                </button>
              )}
            </div>
          )}
          {hasLog && (
            <ul className="book-log">
              {adjustments.map((a) => (
                <li key={a.id}>
                  <span className={`badge ${a.kind === "bonus" ? "badge-ok" : "badge-danger"}`}>{a.kind === "bonus" ? "Thưởng" : "Phạt"}</span>
                  <span className="book-log-amount tabular">
                    {[a.amount != null ? vnd(a.amount) : "", a.points != null ? `${String(a.points)} điểm` : ""].filter(Boolean).join(" · ")}
                  </span>
                  <span className="book-log-text">{a.reason}</span>
                  {!readOnly && a.can_edit && (
                    <AdjustmentActions
                      entry={a}
                      name={s.full_name}
                      onEdit={() => {
                        onAdjust(a);
                      }}
                    />
                  )}
                </li>
              ))}
              {cell?.note && (
                <li>
                  <span className="book-log-label">Chấm công</span>
                  <span className="book-log-text">{cell.note}</span>
                </li>
              )}
              {review?.note && (
                <li>
                  <span className="book-log-label">Đánh giá</span>
                  <span className="book-log-text">{review.note}</span>
                </li>
              )}
            </ul>
          )}
        </section>
      )}
    </li>
  );
}

