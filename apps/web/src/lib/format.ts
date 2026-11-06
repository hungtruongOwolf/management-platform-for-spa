/**
 * Date labels written one way everywhere (design-system § Định dạng): days dd/mm/yyyy with
 * leading zeros, weekday in words, months "Tháng 11/2026". Business days are ISO `YYYY-MM-DD`
 * strings in the spa's day (Asia/Ho_Chi_Minh), formatted as UTC so the day never shifts.
 */

const pad = (n: number) => String(n).padStart(2, "0");

/** "2026-11-05" → "Tháng 11/2026" (a heading over a month of history, sentence case). */
export function monthLabel(iso: string): string {
  const [y, m] = iso.split("-");
  return y && m ? `Tháng ${String(Number(m))}/${y}` : iso;
}

const spaParts = new Intl.DateTimeFormat("en-CA", { year: "numeric", month: "2-digit", day: "2-digit", timeZone: "Asia/Ho_Chi_Minh" });

/** "Thứ 5" … "Thứ 7", "Chủ nhật" for a business day (numbers, as the paper books write them). */
export function weekdayLabel(iso: string): string {
  const day = new Date(`${iso}T00:00:00Z`).getUTCDay();
  return day === 0 ? "Chủ nhật" : `Thứ ${String(day + 1)}`;
}

/** "Thứ 5, 05/11/2026" for a business day. */
export function dayLabel(iso: string): string {
  const [y, m, d] = iso.split("-");
  return `${weekdayLabel(iso)}, ${d ?? ""}/${m ?? ""}/${y ?? ""}`;
}

/** Today's `dayLabel` for an instant, in the spa's time zone (top bar, home greeting). */
export function longDate(at: Date): string {
  return dayLabel(spaParts.format(at));
}

/** "05/11/2026" for a UTC `Date` (axis/tooltip helpers that already hold a Date). */
export function dmy(d: Date): string {
  return `${pad(d.getUTCDate())}/${pad(d.getUTCMonth() + 1)}/${String(d.getUTCFullYear())}`;
}
