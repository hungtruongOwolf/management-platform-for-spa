import type { QueryClient } from "@tanstack/react-query";
import type { AttendanceDay, AttendanceEntryWrite, AttendanceSheet, DailyReviewEntryWrite, ReviewSheet } from "./api";

/*
 * After a save in the daily book, the server's answer goes straight into the month sheets, so the
 * book is free again after one round trip instead of two (perf page §4d: the save used to wait
 * for the whole sheet to be fetched again). The sheets are still refetched behind it, for the
 * totals the server computes; a failed save never touches the cache, so the cards fall back to
 * the server's data by themselves.
 */

/**
 * The month sheet after a save: every written person-day is replaced by what the server answered
 * for it, or removed when it answered nothing (a cleared day).
 */
export function withAttendance(sheet: AttendanceSheet, written: AttendanceEntryWrite[], saved: AttendanceDay[]): AttendanceSheet {
  if (written.length === 0) return sheet;
  return {
    ...sheet,
    staff: sheet.staff.map((s) => {
      const dates = new Set(written.filter((e) => e.staff_id === s.staff_id).map((e) => e.date));
      if (dates.size === 0) return s;
      const mine = saved.filter((d) => d.staff_id === s.staff_id && d.date.startsWith(sheet.month));
      return { ...s, days: [...s.days.filter((d) => !dates.has(d.date)), ...mine].sort((a, b) => a.date.localeCompare(b.date)) };
    }),
  };
}

/** The month sheet with these praise / complaint counts in place; all zero and no note clears the day. */
export function withReviews(sheet: ReviewSheet, saved: DailyReviewEntryWrite[]): ReviewSheet {
  if (saved.length === 0) return sheet;
  return {
    ...sheet,
    staff: sheet.staff.map((s) => {
      const mine = saved.filter((e) => e.staff_id === s.staff_id && e.date.startsWith(sheet.month));
      if (mine.length === 0) return s;
      const dates = new Set(mine.map((e) => e.date));
      const kept = s.days.filter((d) => !dates.has(d.date));
      const added = mine
        .map((e) => ({ staff_id: e.staff_id, date: e.date, compliments: e.compliments ?? 0, complaints: e.complaints ?? 0, note: e.note ?? null }))
        .filter((d) => d.compliments > 0 || d.complaints > 0 || d.note !== null);
      const days = [...kept, ...added].sort((a, b) => a.date.localeCompare(b.date));
      return {
        ...s,
        days,
        compliments: days.reduce((n, d) => n + d.compliments, 0),
        complaints: days.reduce((n, d) => n + d.complaints, 0),
      };
    }),
  };
}

export function putAttendance(queryClient: QueryClient, written: AttendanceEntryWrite[], saved: AttendanceDay[]): void {
  queryClient.setQueriesData<AttendanceSheet>({ queryKey: ["staff", "attendance"] }, (sheet) => sheet && withAttendance(sheet, written, saved));
}

export function putReviews(queryClient: QueryClient, saved: DailyReviewEntryWrite[]): void {
  queryClient.setQueriesData<ReviewSheet>({ queryKey: ["staff", "reviews"] }, (sheet) => sheet && withReviews(sheet, saved));
}
