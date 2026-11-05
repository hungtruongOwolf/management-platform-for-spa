import type {
  AppointmentComplete,
  AppointmentCompleted,
} from "@vpyd/api-client";
import { api } from "../../lib/api";

/**
 * Records the appointment's visit (same rules as POST /visits) and marks it done. Its own module:
 * the visit cart on the startup path needs only this, not the calendar's helpers.
 */
export async function completeAppointment(
  id: string,
  body: AppointmentComplete,
  idempotencyKey: string,
): Promise<AppointmentCompleted> {
  return (await api(`/appointments/${id}/complete`, {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as AppointmentCompleted;
}
