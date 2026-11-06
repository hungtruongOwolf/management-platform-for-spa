import { formatVnd } from "@vpyd/ui";
import { Icon } from "../../app/Icon";
import { daysOf, monthLabel, shiftMonth, weekdayShort } from "./labels";

export const vnd = (n: number) => formatVnd(n);

/** "2026-11-05" → "T5, 05/11/2026" (dd/mm/yyyy, design-system § Định dạng). */
export function dayLabel(iso: string): string {
  const [y, m, d] = iso.split("-");
  return `${weekdayShort(iso)}, ${d ?? ""}/${m ?? ""}/${y ?? ""}`;
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
