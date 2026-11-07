import { Icon } from "../../app/Icon";
import type {
  Adjustment,
  DailyReview,
  DailyReviewEntryWrite,
  StaffSheet,
} from "./api";
import { AdjustmentActions } from "./AdjustmentsTab";
import { AttendanceChoice } from "./AttendanceChoice";
import { Counter } from "./Counter";
import type { AttendanceStatus } from "./labels";
import { cellOf } from "./progress";
import { vnd } from "./ui";

/**
 * One person's card in the daily book (EM 2026-11-07, image 65: a card per person in a grid, not
 * rows of one shared table). Three bands, the same in every card so a grid reads evenly:
 * 1. header: name, role · times, and the chips that add information ("Chưa chấm công", "Đi muộn");
 *    no chip repeats the chosen status, the filled tile already says it;
 * 2. attendance: four separate tiles, each with its colour and icon (AttendanceChoice);
 * 3. the day's record: Khen / Phàn nàn steppers, then "Giờ & ghi chú" and "+ Ghi thưởng / phạt",
 *    then the day's bonuses/penalties and notes as compact rows.
 * The card is a size container: steppers sit side by side when they fit, one per line otherwise
 * (staff.css `.book-card`).
 */
export function BookCard({
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
  // Decision C: a manager's own card and other managers' cards are the owner's to write.
  const locked = !s.can_record;
  const shut = readOnly || locked;
  // While a review save runs, show the counts being saved; the server's answer replaces them.
  const c = savingReview?.compliments ?? review?.compliments ?? 0;
  const k = savingReview?.complaints ?? review?.complaints ?? 0;
  const missing = canAttendance && !cell && !savingStatus;
  const late = cell && cell.late_minutes > 0 ? cell.late_minutes : 0;
  const times = [
    cell?.check_in && `đến ${cell.check_in}`,
    cell?.check_out && `về ${cell.check_out}`,
  ]
    .filter(Boolean)
    .join(" · ");
  const hasLog = Boolean(cell?.note || review?.note || adjustments.length > 0);
  return (
    <li
      id={`book-${s.staff_id}`}
      className={`book-card${missing ? " is-missing" : ""}`}
    >
      <header className="book-card-head">
        <div className="min-w-0">
          <h2 className="book-card-name">{s.full_name}</h2>
          <p className="text-sm text-text-muted">
            {s.position}
            {times && ` · ${times}`}
          </p>
        </div>
        {(missing || late > 0) && (
          <p className="book-card-flags">
            {missing && (
              <span className="badge badge-warn">Chưa chấm công</span>
            )}
            {late > 0 && (
              <span className="badge badge-danger">Đi muộn {late} phút</span>
            )}
          </p>
        )}
      </header>

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
        <p className="book-card-hint">
          <Icon name="lock" className="size-4 shrink-0" />
          Chỉ chủ spa chấm công, ghi khen/phàn nàn và thưởng/phạt cho quản lý.
        </p>
      )}
      {error && (
        <p role="alert" className="book-card-error">
          <Icon name="alert" className="size-4 shrink-0" />
          Chưa lưu được: {error}
        </p>
      )}

      {(canReview || hasLog || !shut) && (
        <section
          className="book-record"
          aria-label={`Ghi nhận trong ngày, ${s.full_name}`}
        >
          {canReview && (
            <div className="book-counters">
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
            </div>
          )}
          {!shut && (
            <div className="book-actions">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={onDetails}
              >
                <Icon name="clock" className="size-4" />
                Giờ & ghi chú
              </button>
              {canReview && (
                <button
                  type="button"
                  className="btn btn-secondary"
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
                  <span
                    className={`badge ${a.kind === "bonus" ? "badge-ok" : "badge-danger"}`}
                  >
                    {a.kind === "bonus" ? "Thưởng" : "Phạt"}
                  </span>
                  <span className="book-log-amount tabular">
                    {[
                      a.amount != null ? vnd(a.amount) : "",
                      a.points != null ? `${String(a.points)} điểm` : "",
                    ]
                      .filter(Boolean)
                      .join(" · ")}
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
