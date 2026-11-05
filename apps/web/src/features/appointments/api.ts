import type {
  Appointment,
  AppointmentCreate,
  AppointmentStatus,
  AppointmentUpdate,
  Calendar,
} from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api, ifMatch } from "../../lib/api";

/** Calendar of `from..to` (spa business days, inclusive; ≤ 14 days). */
export function calendarQuery(from: string, to: string = from) {
  return queryOptions({
    queryKey: ["appointments", from, to],
    queryFn: async () =>
      (await api(
        `/appointments?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
      )) as Calendar,
    staleTime: 15_000,
  });
}

export async function bookAppointment(
  body: AppointmentCreate,
  idempotencyKey: string,
): Promise<Appointment> {
  return (await api("/appointments", {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as Appointment;
}

export async function updateAppointment(
  a: Appointment,
  body: AppointmentUpdate,
): Promise<Appointment> {
  return (await api(`/appointments/${a.id}`, {
    method: "PATCH",
    body,
    headers: ifMatch(a.version),
  })) as Appointment;
}

export async function arriveAppointment(
  id: string,
  staffId: string,
): Promise<Appointment> {
  return (await api(`/appointments/${id}/arrive`, {
    method: "POST",
    body: { staff_id: staffId },
  })) as Appointment;
}

export async function closeAppointment(
  id: string,
  outcome: "cancel" | "no-show",
  reason: string,
): Promise<Appointment> {
  return (await api(`/appointments/${id}/${outcome}`, {
    method: "POST",
    body: { reason },
  })) as Appointment;
}

export const STATUS_LABEL: Record<AppointmentStatus, string> = {
  booked: "Đã đặt",
  arrived: "Đã đến",
  done: "Hoàn tất",
  no_show: "Không đến",
  cancelled: "Đã hủy",
};

/** Text for "đã thanh toán chưa" (R1 collects in full when the visit is recorded). */
export function paymentLabel(
  a: Appointment,
  money: (n: number) => string,
): string {
  switch (a.payment_state) {
    case "unpaid":
      return "Chưa thanh toán";
    case "paid":
      return `Đã thanh toán ${money(a.paid_amount)}`;
    case "no_charge":
      return "Trừ liệu trình, không phát sinh tiền";
    case "voided":
      return "Lượt đã hủy";
    case "none":
      return "Không thu";
  }
}

/** "09:00" → minutes after midnight. */
export function minutesOf(hm: string): number {
  const [h, m] = hm.split(":").map(Number);
  return (h ?? 0) * 60 + (m ?? 0);
}

/** Minutes after the spa's midnight of an instant (UTC+7, no DST). */
export function spaMinutes(iso: string): number {
  const d = new Date(iso);
  return ((d.getUTCHours() + 7) % 24) * 60 + d.getUTCMinutes();
}

/** "HH:MM" of minutes after midnight. */
export function hhmm(minutes: number): string {
  return `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
}

/** The instant of `day` (YYYY-MM-DD) at `minutes` after the spa's midnight. */
export function instantAt(day: string, minutes: number): string {
  return new Date(`${day}T${hhmm(minutes)}:00+07:00`).toISOString();
}

/** Open = still holds its slot. */
export function isOpen(a: Appointment): boolean {
  return a.status === "booked" || a.status === "arrived";
}

/**
 * Free places in the slot `[start, start + slot)` of `day`: capacity minus open appointments
 * running then (mirrors the server's capacity rule; the server still decides).
 */
export function freeAt(cal: Calendar, day: string, start: number): number {
  const end = start + cal.slot_minutes;
  const busy = cal.appointments.filter(
    (a) =>
      isOpen(a) &&
      a.business_date === day &&
      spaMinutes(a.starts_at) < end &&
      spaMinutes(a.starts_at) + a.duration_min > start,
  ).length;
  return Math.max(0, cal.capacity - busy);
}

/** Days `from..from+n-1` (YYYY-MM-DD). */
export function daysFrom(from: string, n: number): string[] {
  const base = new Date(`${from}T00:00:00Z`);
  return Array.from({ length: n }, (_, i) => {
    const d = new Date(base);
    d.setUTCDate(d.getUTCDate() + i);
    return d.toISOString().slice(0, 10);
  });
}

/** Monday of the ISO week of `day`. */
export function mondayOf(day: string): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - ((d.getUTCDay() + 6) % 7));
  return d.toISOString().slice(0, 10);
}
