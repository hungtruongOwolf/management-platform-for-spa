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

/** "Xong 3 · Đang làm 1 · Chưa đến 2" for `day` (the whole loaded week when `null`). */
export function weekSummary(entries: MyCalendarEntry[], day: string | null): string {
  const list = entries.filter((e) => day === null || e.business_date === day);
  const n = (...phases: MyCalendarEntry["phase"][]) => list.filter((e) => phases.includes(e.phase)).length;
  const lost = n("no_show", "cancelled");
  return [`Xong ${String(n("done"))}`, `Đang làm ${String(n("in_service"))}`, `Chưa đến ${String(n("waiting"))}`, lost > 0 ? `Hủy/không đến ${String(lost)}` : ""]
    .filter(Boolean)
    .join(" · ");
}
