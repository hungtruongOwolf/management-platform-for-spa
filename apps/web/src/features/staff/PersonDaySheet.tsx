import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useId, useRef, useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { TimeField } from "../../components/TimeField";
import { messageFor } from "../../lib/errors";
import { meQuery, type Me } from "../auth/session";
import { LazyRecordTrace } from "../record-history/lazy";
import { canTrace } from "../record-history/query";
import { invalidateStaff, saveAttendance, saveReviews, type AttendanceEntryWrite, type DailyReview, type StaffSheet } from "./api";
import { STATUSES, STATUS_HINT, STATUS_LABEL, type AttendanceStatus } from "./labels";
import { cellOf } from "./progress";
import { Counter } from "./Counter";
import { StatusMark, dayLabel } from "./ui";

/**
 * "Ai ghi, ai sửa" of one book's cell, the last element of its part of the sheet (design
 * record-history §2.8); a `type="button"` disclosure, never a submit. Only for a cell the server
 * serves: a person of the caller's book (never the caller's own profile, never a month the person
 * did not work: the books leave both out, `hr::scope`), on a day that has come. On 403 focus goes
 * to the sheet's close button.
 */
function CellTrace({
  staff,
  date,
  book,
  noun,
  inside,
}: {
  staff: StaffSheet;
  date: string;
  book: "attendance" | "daily_review";
  noun: string;
  /** An element of the sheet, to find its close button from. */
  inside: () => Element | null;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  if (!canTrace(me, book)) return null;
  return (
    <LazyRecordTrace
      record={{ type: book, staffId: staff.staff_id, date }}
      noun={`${noun} ${dayLabel(date)} của ${staff.full_name}`}
      day={date}
      focusOnHide={() => inside()?.closest("dialog")?.querySelector<HTMLElement>(".sheet-close")}
    />
  );
}

/**
 * Whether the caller may see the history of a cell of a book they keep (`canAttendance`: the
 * attendance book, `canReview`: the daily reviews). A day they cannot write opens read-only when
 * its month is closed, or when this is true (spec record-history K49): an open month's day of a
 * person only the owner records has nothing beyond its card but its history.
 */
export function tracesBooks(me: Me | null | undefined, canAttendance: boolean, canReview: boolean): boolean {
  return (canAttendance && canTrace(me, "attendance")) || (canReview && canTrace(me, "daily_review"));
}

/** "T4, 12/11/2026 · vào ca 09:00" (a role with the reviews only has no shift to say). */
const sheetLine = (staff: StaffSheet, date: string) => (staff.shift_start ? `${dayLabel(date)} · vào ca ${staff.shift_start}` : dayLabel(date));

/**
 * One person, one day — everything the sheet has for that cell: attendance (code, times, note)
 * and the daily review (praise, complaints, note). Each part saves only when it changed and only
 * when the user may write it (the API enforces the same).
 */
export function PersonDaySheet({
  staff,
  date,
  review,
  canAttendance,
  canReview,
  onClose,
}: {
  staff: StaffSheet;
  date: string;
  review: DailyReview | undefined;
  canAttendance: boolean;
  canReview: boolean;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const form = useRef<HTMLFormElement>(null);
  const current = cellOf(staff, date);
  const [status, setStatus] = useState<AttendanceStatus | null>(current?.status ?? "present");
  const [checkIn, setCheckIn] = useState(current?.check_in ?? "");
  const [checkOut, setCheckOut] = useState(current?.check_out ?? "");
  const [note, setNote] = useState(current?.note ?? "");
  const [compliments, setCompliments] = useState(review?.compliments ?? 0);
  const [complaints, setComplaints] = useState(review?.complaints ?? 0);
  const [reviewNote, setReviewNote] = useState(review?.note ?? "");
  const present = status === "present";

  const attendanceEntry = (clear: boolean): AttendanceEntryWrite =>
    clear
      ? { staff_id: staff.staff_id, date, status: null }
      : {
          staff_id: staff.staff_id,
          date,
          status,
          check_in: present && checkIn ? checkIn : null,
          check_out: present && checkOut ? checkOut : null,
          note: note.trim() || null,
        };
  const attendanceChanged =
    !current ||
    current.status !== status ||
    (current.check_in ?? "") !== (present ? checkIn : "") ||
    (current.check_out ?? "") !== (present ? checkOut : "") ||
    (current.note ?? "") !== note.trim();
  const reviewChanged =
    (review?.compliments ?? 0) !== compliments || (review?.complaints ?? 0) !== complaints || (review?.note ?? "") !== reviewNote.trim();

  const save = useMutation({
    mutationFn: async (clearAttendance: boolean) => {
      if (canAttendance && (clearAttendance || attendanceChanged)) await saveAttendance([attendanceEntry(clearAttendance)]);
      if (canReview && reviewChanged) {
        await saveReviews([{ staff_id: staff.staff_id, date, compliments, complaints, note: reviewNote.trim() || null }]);
      }
    },
    onSuccess: () => {
      onClose();
    },
    // Refetch even after a partial failure, so the page shows what the server holds.
    onSettled: () => invalidateStaff(queryClient),
  });
  const formId = "person-day-form";
  const trace = (book: "attendance" | "daily_review", noun: string) => (
    <CellTrace staff={staff} date={date} book={book} noun={noun} inside={() => form.current} />
  );

  return (
    <BottomSheet
      open
      onClose={onClose}
      title={staff.full_name}
      description={sheetLine(staff, date)}
      footer={
        <>
          {canAttendance && current && (
            <button
              type="button"
              className="btn btn-secondary"
              disabled={save.isPending}
              onClick={() => {
                save.mutate(true);
              }}
            >
              Xóa chấm công
            </button>
          )}
          <button type="submit" form={formId} className="btn btn-primary" disabled={save.isPending || (canAttendance && status === null)}>
            Lưu
          </button>
        </>
      }
    >
      <form
        id={formId}
        ref={form}
        className="grid gap-5"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate(false);
        }}
      >
        {canAttendance && (
          <fieldset className="grid gap-3">
            <legend className="panel-title mb-1">Chấm công</legend>
            <div className="grid gap-2">
              {STATUSES.map((st) => (
                <label key={st} className="choice">
                  <input
                    type="radio"
                    name="status"
                    checked={status === st}
                    onChange={() => {
                      setStatus(st);
                    }}
                  />
                  <span>
                    <span className="font-semibold">{STATUS_LABEL[st]}</span>
                    <span className="block text-sm text-text-muted">{STATUS_HINT[st]}</span>
                  </span>
                </label>
              ))}
            </div>
            {status !== null && status !== "present" && staff.visit_days.includes(date) && (
              <p role="alert" className="callout text-sm">
                {staff.full_name} có lượt khách được ghi trong ngày này. Kiểm tra lại trước khi lưu ngày nghỉ.
              </p>
            )}
            {present && (
              <div className="grid grid-cols-2 gap-4">
                <label className="filter-field">
                  Giờ đến
                  <TimeField value={checkIn} onChange={setCheckIn} />
                </label>
                <label className="filter-field">
                  Giờ về
                  <TimeField value={checkOut} onChange={setCheckOut} />
                </label>
              </div>
            )}
            <label className="filter-field">
              Ghi chú chấm công (lý do nghỉ, đi muộn)
              <textarea
                rows={2}
                maxLength={500}
                value={note}
                onChange={(e) => {
                  setNote(e.target.value);
                }}
              />
            </label>
            {trace("attendance", "chấm công")}
          </fieldset>
        )}
        {canReview && (
          <fieldset className="grid gap-3">
            <legend className="panel-title mb-1">Khen, phàn nàn (không bắt buộc)</legend>
            <div className="flex flex-wrap gap-4">
              <Counter label="Khen" value={compliments} disabled={false} onChange={setCompliments} />
              <Counter label="Phàn nàn" value={complaints} disabled={false} onChange={setComplaints} />
            </div>
            <label className="filter-field">
              Ghi chú đánh giá (khách khen gì, phàn nàn gì)
              <textarea
                rows={2}
                maxLength={500}
                value={reviewNote}
                onChange={(e) => {
                  setReviewNote(e.target.value);
                }}
              />
            </label>
            {trace("daily_review", "khen, phàn nàn")}
          </fieldset>
        )}
        {save.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}

/**
 * The same cell, read-only (spec record-history K30, K49; like "Xem" on a scorecard, EM
 * 2026-11-08): a day of a closed month, or of a person whose records only the owner writes. The
 * values and no input; each book's part ends with its history where the caller may see it. Only
 * the books the caller keeps (`canAttendance`, `canReview`).
 */
export function PersonDayView({
  staff,
  date,
  review,
  canAttendance,
  canReview,
  onClose,
}: {
  staff: StaffSheet;
  date: string;
  review: DailyReview | undefined;
  canAttendance: boolean;
  canReview: boolean;
  onClose: () => void;
}) {
  const id = useId();
  const body = useRef<HTMLDivElement>(null);
  const cell = cellOf(staff, date);
  const times =
    cell?.status === "present"
      ? [cell.check_in && `đến ${cell.check_in}`, cell.check_out && `về ${cell.check_out}`].filter(Boolean).join(" · ")
      : "";
  const trace = (book: "attendance" | "daily_review", noun: string) => (
    <CellTrace staff={staff} date={date} book={book} noun={noun} inside={() => body.current} />
  );
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={staff.full_name}
      description={`${sheetLine(staff, date)} · chỉ xem`}
      footer={
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          Đóng
        </button>
      }
    >
      <div ref={body} className="my-day-detail">
        {canAttendance && (
          <section aria-labelledby={`${id}-att`} className="my-day-block">
            <h3 id={`${id}-att`}>Chấm công</h3>
            {cell ? (
              <dl className="detail-grid">
                <dt>Trạng thái</dt>
                <dd>
                  <span className={`my-legend-mark att-${cell.status}`} aria-hidden="true">
                    <StatusMark status={cell.status} className="size-3.5" />
                  </span>{" "}
                  {STATUS_LABEL[cell.status]}
                  {cell.late_minutes > 0 && <span className="badge badge-warn ml-2">Muộn {cell.late_minutes} phút</span>}
                </dd>
                {times && (
                  <>
                    <dt>Giờ</dt>
                    <dd>{times}</dd>
                  </>
                )}
                {cell.note && (
                  <>
                    <dt>Ghi chú</dt>
                    <dd>{cell.note}</dd>
                  </>
                )}
              </dl>
            ) : (
              <p className="text-sm text-text-muted">Chưa chấm công ngày này.</p>
            )}
            {trace("attendance", "chấm công")}
          </section>
        )}
        {canReview && (
          <section aria-labelledby={`${id}-review`} className="my-day-block">
            <h3 id={`${id}-review`}>Khen, phàn nàn</h3>
            <p>
              <strong>{review?.compliments ?? 0}</strong> lời khen · <strong>{review?.complaints ?? 0}</strong> phàn nàn
              {review?.note && <span className="block text-sm text-text-muted">{review.note}</span>}
            </p>
            {trace("daily_review", "khen, phàn nàn")}
          </section>
        )}
      </div>
    </BottomSheet>
  );
}
