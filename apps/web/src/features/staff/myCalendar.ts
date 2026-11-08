import { type EntryView, spaMinutes } from "../appointments/api";
import type { MyCalendarEntry } from "./api";

/** A technician's own calendar entry as a card of the shared time grid. */
export function entryView(e: MyCalendarEntry): EntryView {
  return {
    id: e.id,
    kind: e.kind,
    day: e.business_date,
    start: spaMinutes(e.starts_at),
    minutes: e.duration_min,
    phase: e.phase,
    staffId: null,
    customerName: e.customer_name,
    groupId: null,
    services: e.services,
  };
}

/**
 * The minutes the technician's grid draws: the spa's opening hours, widened to whole slots around
 * any shown entry outside them. A walk-in served after closing (21:30) or past midnight used to sit
 * outside the frame and vanish from the technician's own calendar (CI 2026-11-08 00:03 ICT).
 */
export function gridSpan(opens: number, closes: number, slot: number, shown: EntryView[]): { opens: number; closes: number } {
  const step = Math.max(1, slot);
  let from = opens;
  let to = closes;
  for (const v of shown) {
    from = Math.min(from, Math.floor(v.start / step) * step);
    to = Math.max(to, Math.ceil((v.start + Math.max(v.minutes, step)) / step) * step);
  }
  return { opens: Math.max(0, from), closes: Math.min(24 * 60, to) };
}

/** "Xong 3 · Đang làm 1 · Chưa đến 2" for `day` (the whole loaded week when `null`). */
export function weekSummary(entries: MyCalendarEntry[], day: string | null): string {
  const list = entries.filter((e) => day === null || e.business_date === day);
  const n = (...phases: MyCalendarEntry["phase"][]) => list.filter((e) => phases.includes(e.phase)).length;
  const lost = n("no_show", "cancelled");
  return [`Xong ${String(n("done"))}`, `Đang làm ${String(n("in_service"))}`, `Chưa đến ${String(n("waiting"))}`, lost > 0 ? `Hủy/không đến ${String(lost)}` : ""]
    .filter(Boolean)
    .join(" · ");
}

/**
 * Where "Khách của tôi" brings its grid into view (F22, EM live test 2026-11-08). The grid keeps
 * #89's widened span (a visit at 02:00 starts it at 02:00), so it used to open on hours of empty
 * night. Today: an hour before now, so what is on now and next is in view. Another day: the
 * opening hour, or the first block when every block is before opening. A whole hour (the gutter's
 * labelled bands); `null` when that is the grid's first row already (nothing to scroll past).
 */
export function focusMinute({
  gridOpens,
  gridCloses,
  opens,
  now,
  showsToday,
  shown,
}: {
  /** The grid's first and last minute (`gridSpan`). */
  gridOpens: number;
  gridCloses: number;
  /** The spa's opening minute. */
  opens: number;
  /** Minutes since midnight now, spa time. */
  now: number;
  showsToday: boolean;
  shown: EntryView[];
}): number | null {
  const hour = (m: number) => Math.floor(m / 60) * 60;
  let target: number;
  if (showsToday && now >= gridOpens && now <= gridCloses) target = hour(now - 60);
  else if (shown.length > 0 && shown.every((v) => v.start < opens)) target = hour(Math.min(...shown.map((v) => v.start)));
  else target = hour(opens);
  return target > Math.ceil(gridOpens / 60) * 60 ? target : null;
}
