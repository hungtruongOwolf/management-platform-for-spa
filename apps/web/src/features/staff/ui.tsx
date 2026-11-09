import { formatVnd } from "@vpyd/ui";
import { Icon } from "../../app/Icon";
import { STATUS_ICON, STATUS_LABEL, daysOf, monthLabel, shiftMonth, weekdayShort, type AttendanceStatus } from "./labels";

export const vnd = (n: number) => formatVnd(n);

/** A net that may be negative, with the minus sign "−" (U+2212) every deduction here uses (Intl writes "-"). */
export const signedVnd = (n: number) => (n < 0 ? `−${formatVnd(-n)}` : formatVnd(n));

/** "2026-11-05" → "T5, 05/11/2026" (dd/mm/yyyy, design-system § Định dạng). */
export function dayLabel(iso: string): string {
  const [y, m, d] = iso.split("-");
  return `${weekdayShort(iso)}, ${d ?? ""}/${m ?? ""}/${y ?? ""}`;
}

/** A status's mark (decorative: the words sit beside it or in the accessible name). */
export function StatusMark({ status, className = "size-4" }: { status: AttendanceStatus; className?: string }) {
  return <Icon name={STATUS_ICON[status]} className={className} />;
}

/**
 * The attendance legend as the cells' own marks (no sentence of symbols, F20): each status's
 * icon on its colour, "–" for a day not recorded yet, then how lateness shows — the month
 * calendar's minutes chip, or the month grid's red underline.
 */
export function AttendanceLegend({ late }: { late: "chip" | "underline" }) {
  const statuses: AttendanceStatus[] = ["present", "leave_p", "leave_dx", "absent_k"];
  return (
    <ul className="my-legend" aria-label="Chú thích">
      {statuses.map((st) => (
        <li key={st}>
          <span aria-hidden="true" className={`my-legend-mark att-${st}`}>
            <StatusMark status={st} className="size-3.5" />
          </span>
          {STATUS_LABEL[st]}
        </li>
      ))}
      <li>
        <span aria-hidden="true" className="my-legend-mark att-none">
          –
        </span>
        Chưa chấm
      </li>
      <li>
        {late === "chip" ? (
          <span aria-hidden="true" className="my-day-late is-inline">
            12′
          </span>
        ) : (
          <span aria-hidden="true" className="my-legend-mark is-late" />
        )}
        {late === "chip" ? "Đi muộn (phút)" : "Gạch đỏ dưới ô: đi muộn"}
      </li>
    </ul>
  );
}

export function ClosedBanner() {
  return (
    <p role="status" className="note flex items-center gap-2">
      <Icon name="alert" className="size-4" />
      Tháng này đã chốt lương: chỉ xem. Cần sửa thì ghi một khoản thưởng/phạt điều chỉnh vào tháng đang mở.
    </p>
  );
}

/** Previous / next month around the current one (never past the current month). */
export function MonthPicker({ month, current, onChange }: { month: string; current: string; onChange: (m: string) => void }) {
  return (
    <div className="month-picker" role="group" aria-label="Chọn tháng">
      <button
        type="button"
        className="icon-btn"
        aria-label="Tháng trước"
        onClick={() => {
          onChange(shiftMonth(month, -1));
        }}
      >
        <Icon name="chevron-left" />
      </button>
      <span className="month-picker-label" aria-live="polite">
        {monthLabel(month)}
      </span>
      <button
        type="button"
        className="icon-btn"
        aria-label="Tháng sau"
        disabled={month >= current}
        onClick={() => {
          onChange(shiftMonth(month, 1));
        }}
      >
        <Icon name="chevron-right" />
      </button>
    </div>
  );
}

/** Latest day a day picker of `month` offers: its last day, or today in the current month. */
export function lastDayOrToday(month: string, today: string): string {
  const last = daysOf(month).at(-1) ?? today;
  return last < today ? last : today;
}
