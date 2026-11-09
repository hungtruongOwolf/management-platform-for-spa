/**
 * Date labels written one way everywhere (design-system § Định dạng): days dd/mm/yyyy with
 * leading zeros, weekday in words, months "Tháng 11/2026". Business days are ISO `YYYY-MM-DD`
 * strings in the spa's day (Asia/Ho_Chi_Minh), formatted as UTC so the day never shifts.
 */

/** "2026-11-05" → "Tháng 11/2026" (a heading over a month of history, sentence case). */
export function monthLabel(iso: string): string {
  const [y, m] = iso.split("-");
  return y && m ? `Tháng ${String(Number(m))}/${y}` : iso;
}

const spaParts = new Intl.DateTimeFormat("en-CA", { year: "numeric", month: "2-digit", day: "2-digit", timeZone: "Asia/Ho_Chi_Minh" });

/** Today in the spa's time zone, "YYYY-MM-DD" (the business day every screen starts from). */
export function spaToday(now: Date = new Date()): string {
  return spaParts.format(now);
}

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

const spaClock = new Intl.DateTimeFormat("en-GB", {
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
  timeZone: "Asia/Ho_Chi_Minh",
});

/**
 * "19:27" for an instant (an API timestamp), always on the spa's clock. Never
 * `toLocaleString()`: that uses the browser's time zone (F27: the EM in the US saw 06:27).
 */
export function spaTime(iso: string): string {
  return spaClock.format(new Date(iso));
}

/** "19:27 08/11/2026" for an instant, on the spa's clock and calendar. */
export function spaDateTime(iso: string): string {
  const [y, m, d] = spaParts.format(new Date(iso)).split("-");
  return `${spaTime(iso)} ${d ?? ""}/${m ?? ""}/${y ?? ""}`;
}
