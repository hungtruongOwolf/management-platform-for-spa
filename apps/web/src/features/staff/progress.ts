import type { AttendanceDay, StaffSheet } from "./api";
import { daysOf, shiftMonth } from "./labels";

/** Whether the person works for the spa on `date` (joined, not yet left). */
export function employedOn(s: StaffSheet, date: string): boolean {
  return (!s.joined_on || date >= s.joined_on) && (!s.left_on || date <= s.left_on);
}

export function cellOf(s: StaffSheet, date: string): AttendanceDay | undefined {
  return s.days.find((d) => d.date === date);
}

/** People employed on `date` whose attendance is not recorded yet. */
export function missingOn(people: StaffSheet[], date: string): StaffSheet[] {
  return people.filter((s) => employedOn(s, date) && !cellOf(s, date));
}

/** Days of `month` before `today` on which someone employed has no attendance (oldest first). */
export function incompleteDays(month: string, people: StaffSheet[], today: string): string[] {
  return daysOf(month).filter((d) => d < today && missingOn(people, d).length > 0);
}

/**
 * The month whose evaluation is due on `today`, or null. Due window (spec §10 AC7.4, lead's
 * decision): the month's last day through day 10 of the next month.
 */
export function evaluationDueMonth(today: string): string | null {
  const month = today.slice(0, 7);
  const day = Number(today.slice(8));
  if (daysOf(month).at(-1) === today) return month;
  return day <= 10 ? shiftMonth(month, -1) : null;
}
