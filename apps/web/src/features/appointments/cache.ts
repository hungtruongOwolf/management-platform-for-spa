import type { QueryClient } from "@tanstack/react-query";
import type { Appointment, Calendar } from "@vpyd/api-client";

/** Calendar with `saved` in place of the booking of the same id (calendars without it unchanged). */
export function withAppointment(calendar: Calendar, saved: Appointment): Calendar {
  if (!calendar.appointments.some((a) => a.id === saved.id)) return calendar;
  return { ...calendar, appointments: calendar.appointments.map((a) => (a.id === saved.id ? saved : a)) };
}

/**
 * A status change (arrived, cancelled, no-show, linked profile) shows from the server's answer at
 * once, in every cached calendar, instead of after a refetch of the whole range (perf page §4d).
 * The calendars are still refetched behind it, for what the change moves elsewhere (counts, group).
 */
export function putAppointment(queryClient: QueryClient, saved: Appointment): void {
  queryClient.setQueriesData<Calendar>({ queryKey: ["appointments"] }, (calendar) => calendar && withAppointment(calendar, saved));
}
