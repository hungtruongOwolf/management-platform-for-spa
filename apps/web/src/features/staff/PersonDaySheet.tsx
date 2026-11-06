import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { messageFor } from "../../lib/errors";
import { invalidateStaff, saveAttendance, saveReviews, type AttendanceEntryWrite, type DailyReview, type StaffSheet } from "./api";
import { STATUSES, STATUS_HINT, STATUS_LABEL, type AttendanceStatus } from "./labels";
import { cellOf } from "./progress";
import { Counter } from "./Counter";
import { dayLabel } from "./ui";

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

  return (
    <BottomSheet
      open
      onClose={onClose}
      title={staff.full_name}
      description={`${dayLabel(date)} · vào ca ${staff.shift_start}`}
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
        className="grid gap-5"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate(false);
        }}
      >
        {canAttendance && (
          <fieldset className="grid gap-3">
            <legend className="section-title mb-1">Chấm công</legend>
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
            {present && (
              <div className="grid grid-cols-2 gap-4">
                <label className="filter-field">
                  Giờ đến
                  <input
                    type="time"
                    value={checkIn}
                    onChange={(e) => {
                      setCheckIn(e.target.value);
                    }}
                  />
                </label>
                <label className="filter-field">
                  Giờ về
                  <input
                    type="time"
                    value={checkOut}
                    onChange={(e) => {
                      setCheckOut(e.target.value);
                    }}
                  />
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
          </fieldset>
        )}
        {canReview && (
          <fieldset className="grid gap-3">
            <legend className="section-title mb-1">Khen, phàn nàn (không bắt buộc)</legend>
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
