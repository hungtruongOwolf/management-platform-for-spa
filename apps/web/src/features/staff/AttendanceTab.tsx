import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { usePhone } from "../../components/useMediaQuery";
import { messageFor } from "../../lib/errors";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import {
  attendanceQuery,
  invalidateStaff,
  saveAttendance,
  type AttendanceDay,
  type AttendanceEntryWrite,
  type StaffSheet,
} from "./api";
import { STATUSES, STATUS_CODE, STATUS_HINT, STATUS_LABEL, daysOf, weekdayShort, type AttendanceStatus } from "./labels";
import { ClosedBanner, dayLabel } from "./ui";

interface Editing {
  staff: StaffSheet;
  date: string;
}

function employedOn(s: StaffSheet, date: string): boolean {
  return (!s.joined_on || date >= s.joined_on) && (!s.left_on || date <= s.left_on);
}

/**
 * Chấm công: phones get one day as a list per person (tap a code, done); wider screens also get
 * the month grid (person × day, like the sheet). Every tap saves at once; "Cả spa đi làm" marks
 * everyone not yet recorded that day as present in one request.
 */
export function AttendanceTab({ month, day, today, onDay }: { month: string; day: string; today: string; onDay: (d: string) => void }) {
  const phone = usePhone();
  const query = useQuery(attendanceQuery(month));
  const [view, setView] = useState<"day" | "month">("day");
  const [editing, setEditing] = useState<Editing | null>(null);
  const [notice, setNotice] = useState("");
  const queryClient = useQueryClient();
  const save = useMutation({
    mutationFn: saveAttendance,
    onSuccess: async () => {
      await invalidateStaff(queryClient);
    },
    onError: (e) => {
      setNotice(messageFor(e));
    },
  });

  if (query.isPending) return <RowsSkeleton rows={6} />;
  if (query.isError) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
  const sheet = query.data;
  const inMonth = day.startsWith(sheet.month) ? day : sheet.month === today.slice(0, 7) ? today : `${sheet.month}-01`;
  const closed = sheet.closed;
  const record = (entries: AttendanceEntryWrite[]) => {
    setNotice("");
    save.mutate(entries);
  };
  const people = sheet.staff;
  const showGrid = !phone && view === "month";

  return (
    <div className="grid content-start gap-4">
      {closed && <ClosedBanner />}
      <div className="flex flex-wrap items-end gap-3">
        {!phone && (
          <div className="segmented" role="group" aria-label="Cách xem">
            {(
              [
                ["day", "Theo ngày"],
                ["month", "Cả tháng"],
              ] as const
            ).map(([v, label]) => (
              <button
                key={v}
                type="button"
                aria-pressed={view === v}
                onClick={() => {
                  setView(v);
                }}
              >
                {label}
              </button>
            ))}
          </div>
        )}
        {!showGrid && (
          <DateField
            label="Ngày chấm công"
            value={inMonth}
            min={`${sheet.month}-01`}
            max={today < `${sheet.month}-31` ? today : undefined}
            onChange={(d) => {
              if (d) onDay(d);
            }}
          />
        )}
        {!showGrid && !closed && inMonth <= today && (
          <button
            type="button"
            className="btn btn-secondary"
            disabled={save.isPending}
            onClick={() => {
              const missing = people.filter((s) => employedOn(s, inMonth) && !s.days.some((d) => d.date === inMonth));
              if (missing.length === 0) {
                setNotice("Mọi người đã được chấm công ngày này.");
                return;
              }
              record(missing.map((s) => ({ staff_id: s.staff_id, date: inMonth, status: "present" })));
            }}
          >
            Cả spa đi làm
          </button>
        )}
      </div>
      {notice && (
        <p role="status" className="text-sm text-text-muted">
          {notice}
        </p>
      )}
      {people.length === 0 ? (
        <p className="text-text-muted">Chưa có nhân viên nào làm việc trong tháng này.</p>
      ) : showGrid ? (
        <MonthGrid
          month={sheet.month}
          today={today}
          people={people}
          onCell={(staff, date) => {
            if (!closed) setEditing({ staff, date });
          }}
        />
      ) : (
        <DayList
          date={inMonth}
          today={today}
          closed={closed}
          people={people}
          busy={save.isPending}
          onStatus={(s, status) => {
            record([{ staff_id: s.staff_id, date: inMonth, status }]);
          }}
          onDetails={(staff) => {
            setEditing({ staff, date: inMonth });
          }}
        />
      )}
      {editing && (
        <DayEditor
          editing={editing}
          busy={save.isPending}
          onClose={() => {
            setEditing(null);
          }}
          onSave={(entry) => {
            save.mutate([entry], {
              onSuccess: () => {
                setEditing(null);
              },
            });
          }}
          error={save.error}
        />
      )}
    </div>
  );
}

function cellOf(s: StaffSheet, date: string): AttendanceDay | undefined {
  return s.days.find((d) => d.date === date);
}

function DayList({
  date,
  today,
  closed,
  people,
  busy,
  onStatus,
  onDetails,
}: {
  date: string;
  today: string;
  closed: boolean;
  people: StaffSheet[];
  busy: boolean;
  onStatus: (s: StaffSheet, status: AttendanceStatus) => void;
  onDetails: (s: StaffSheet) => void;
}) {
  const future = date > today;
  return (
    <ul className="grid gap-2" aria-label={`Chấm công ${dayLabel(date)}`}>
      {people
        .filter((s) => employedOn(s, date))
        .map((s) => {
          const cell = cellOf(s, date);
          return (
            <li key={s.staff_id} className="card grid gap-3 p-3 sm:grid-cols-[minmax(10rem,1fr)_auto] sm:items-center">
              <div className="min-w-0">
                <p className="font-semibold">{s.full_name}</p>
                <p className="text-sm text-text-muted">
                  {cell ? (
                    <>
                      {STATUS_LABEL[cell.status]}
                      {cell.check_in && ` · đến ${cell.check_in}`}
                      {cell.late_minutes > 0 && <span className="text-danger"> · muộn {cell.late_minutes} phút</span>}
                      {cell.note && ` · ${cell.note}`}
                    </>
                  ) : (
                    "Chưa chấm"
                  )}
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <div className="segmented" role="group" aria-label={`Trạng thái của ${s.full_name}`}>
                  {STATUSES.map((st) => (
                    <button
                      key={st}
                      type="button"
                      aria-pressed={cell?.status === st}
                      title={STATUS_LABEL[st]}
                      aria-label={STATUS_LABEL[st]}
                      disabled={closed || future || busy}
                      onClick={() => {
                        onStatus(s, st);
                      }}
                    >
                      {STATUS_CODE[st]}
                    </button>
                  ))}
                </div>
                {!closed && !future && (
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      onDetails(s);
                    }}
                  >
                    Giờ & ghi chú
                  </button>
                )}
              </div>
            </li>
          );
        })}
    </ul>
  );
}

function MonthGrid({
  month,
  today,
  people,
  onCell,
}: {
  month: string;
  today: string;
  people: StaffSheet[];
  onCell: (s: StaffSheet, date: string) => void;
}) {
  const days = daysOf(month);
  return (
    <div className="table-wrap attendance-grid">
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
            <th scope="col" className="num">
              Công
            </th>
            <th scope="col" className="num">
              Muộn
            </th>
            <th scope="col" className="num">
              P/ĐX/K
            </th>
          </tr>
        </thead>
        <tbody>
          {people.map((s) => (
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
                        aria-label={`${s.full_name}, ${dayLabel(d)}: ${cell ? STATUS_LABEL[cell.status] : "chưa chấm"}${cell && cell.late_minutes > 0 ? `, muộn ${String(cell.late_minutes)} phút` : ""}`}
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
              <td className="num">{s.totals.present}</td>
              <td className="num">
                {s.totals.late_count > 0 ? `${String(s.totals.late_count)} lần / ${String(s.totals.late_minutes)}′` : "–"}
              </td>
              <td className="num">
                {s.totals.leave_p}/{s.totals.leave_dx}/{s.totals.absent_k}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="mt-2 text-sm text-text-muted">✓ đi làm · P nghỉ có phép · ĐX nghỉ đột xuất · K nghỉ không lý do · gạch đỏ dưới ô: đi muộn. Bấm một ô để sửa.</p>
    </div>
  );
}

function DayEditor({
  editing,
  busy,
  error,
  onClose,
  onSave,
}: {
  editing: Editing;
  busy: boolean;
  error: unknown;
  onClose: () => void;
  onSave: (e: AttendanceEntryWrite) => void;
}) {
  const current = cellOf(editing.staff, editing.date);
  const [status, setStatus] = useState<AttendanceStatus | null>(current?.status ?? "present");
  const [checkIn, setCheckIn] = useState(current?.check_in ?? "");
  const [checkOut, setCheckOut] = useState(current?.check_out ?? "");
  const [note, setNote] = useState(current?.note ?? "");
  const formId = "attendance-day-form";
  const present = status === "present";
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`Chấm công ${editing.staff.full_name}`}
      description={`${dayLabel(editing.date)} · vào ca ${editing.staff.shift_start}`}
      footer={
        <>
          {current && (
            <button
              type="button"
              className="btn btn-secondary"
              disabled={busy}
              onClick={() => {
                onSave({ staff_id: editing.staff.staff_id, date: editing.date, status: null });
              }}
            >
              Xóa chấm công
            </button>
          )}
          <button type="submit" form={formId} className="btn btn-primary" disabled={busy || status === null}>
            Lưu
          </button>
        </>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          onSave({
            staff_id: editing.staff.staff_id,
            date: editing.date,
            status,
            check_in: present && checkIn ? checkIn : null,
            check_out: present && checkOut ? checkOut : null,
            note: note.trim() || null,
          });
        }}
      >
        <fieldset className="grid gap-2">
          <legend className="mb-1 text-xs text-text-muted">Trạng thái</legend>
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
                <span className="font-semibold">
                  {STATUS_CODE[st]} {STATUS_LABEL[st]}
                </span>
                <span className="block text-sm text-text-muted">{STATUS_HINT[st]}</span>
              </span>
            </label>
          ))}
        </fieldset>
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
          Ghi chú (lý do nghỉ, sự việc)
          <textarea
            rows={2}
            maxLength={500}
            value={note}
            onChange={(e) => {
              setNote(e.target.value);
            }}
          />
        </label>
        {error !== null && error !== undefined && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
