import type {
  Appointment,
  AppointmentCreate,
  AppointmentGroup,
  AppointmentGroupCreate,
  AppointmentService,
  AppointmentSettings,
  AppointmentStatus,
  AppointmentUpdate,
  Calendar,
  CalendarVisit,
  TimelinePhase,
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

/** A family booked at once: all or nobody (spec appointments.md §10.3). */
export async function bookGroup(
  body: AppointmentGroupCreate,
  idempotencyKey: string,
): Promise<AppointmentGroup> {
  return (await api("/appointments/group", {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as AppointmentGroup;
}

/** Owner only: the advisory "max customers per slot" (null = no limit). */
export async function saveSlotLimit(
  maxPerSlot: number | null,
): Promise<AppointmentSettings> {
  return (await api("/appointment-settings", {
    method: "PUT",
    body: { max_per_slot: maxPerSlot },
  })) as AppointmentSettings;
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

/** Labels of the timeline phases (spec appointments.md §10.1). Always shown with the colour. */
export const PHASE_LABEL: Record<TimelinePhase, string> = {
  waiting: "Chưa đến",
  in_service: "Đang làm",
  done: "Xong",
  no_show: "Không đến",
  cancelled: "Đã hủy",
  voided: "Lượt đã hủy",
};

/**
 * One card on the desk's timeline: a booking, or a visit recorded without one (walk-in). A
 * booking that turned into a visit stays the booking (the server never sends it twice).
 */
export type Entry =
  | { kind: "appointment"; id: string; a: Appointment }
  | { kind: "visit"; id: string; v: CalendarVisit };

export interface EntryView {
  id: string;
  kind: Entry["kind"];
  day: string;
  start: number;
  minutes: number;
  phase: TimelinePhase;
  staffId: string | null;
  customerName: string;
  groupId: string | null;
  services: AppointmentService[];
}

export function view(e: Entry): EntryView {
  const x = e.kind === "appointment" ? e.a : e.v;
  return {
    id: e.id,
    kind: e.kind,
    day: x.business_date,
    start: spaMinutes(x.starts_at),
    minutes: x.duration_min,
    phase: x.phase,
    staffId: x.staff_id ?? null,
    customerName: x.customer_name,
    groupId: e.kind === "appointment" ? (e.a.group_id ?? null) : null,
    services: x.services,
  };
}

/** Bookings and walk-ins of `day` (every day when omitted), earliest first. */
export function entriesOf(cal: Calendar, day?: string): Entry[] {
  const list: Entry[] = [
    ...cal.appointments.map((a) => ({
      kind: "appointment" as const,
      id: a.id,
      a,
    })),
    ...cal.visits.map((v) => ({ kind: "visit" as const, id: v.id, v })),
  ];
  return list
    .filter((e) => day === undefined || view(e).day === day)
    .sort((x, y) => {
      const a = x.kind === "appointment" ? x.a.starts_at : x.v.starts_at;
      const b = y.kind === "appointment" ? y.a.starts_at : y.v.starts_at;
      return a < b ? -1 : a > b ? 1 : 0;
    });
}

/** Took or takes a place on the timeline (not cancelled, missed or voided). */
export function counts(phase: TimelinePhase): boolean {
  return phase === "waiting" || phase === "in_service" || phase === "done";
}

/**
 * People booked or served in the slot `[start, start + slot)` of `day`: what the desk asked to see
 * instead of a capacity (spec §10.4). Compared with the owner's `max_per_slot` when set.
 */
export function peopleAt(cal: Calendar, day: string, start: number): number {
  const end = start + cal.slot_minutes;
  return entriesOf(cal, day)
    .map(view)
    .filter(
      (e) => counts(e.phase) && e.start < end && e.start + e.minutes > start,
    ).length;
}

/** Most people at once anywhere in `[start, start + minutes)` of `day`. */
export function peakPeople(
  cal: Calendar,
  day: string,
  start: number,
  minutes: number,
): number {
  let peak = 0;
  for (let m = start; m < start + minutes; m += cal.slot_minutes)
    peak = Math.max(peak, peopleAt(cal, day, m));
  return peak;
}

/**
 * A booking's length from its services, rounded up to the slot (same rule as the server's
 * `services_minutes`): the timed services' sum. A service without a duration ("theo tư vấn") is an
 * hour only when nothing else is timed; next to a timed one it is an add-on done in that time.
 * No services = 60.
 */
export function autoMinutes(
  durations: (number | null | undefined)[],
  slot: number,
): number {
  const timed = durations.filter((d): d is number => d != null && d > 0);
  const sum =
    timed.length > 0
      ? timed.reduce((t, d) => t + d, 0)
      : Math.max(1, durations.length) * 60;
  return Math.max(slot, Math.ceil(sum / slot) * slot);
}

/** Minutes a service adds to `autoMinutes` of its booking (0 = an add-on done meanwhile). */
export function addedMinutes(
  duration: number | null | undefined,
  all: (number | null | undefined)[],
): number {
  if (duration != null && duration > 0) return duration;
  return all.some((d) => d != null && d > 0) ? 0 : 60;
}

/**
 * The calendar of one technician (`who` = staff id), of bookings nobody is assigned to
 * (`"none"`), or everyone (`""`, or an id that is no technician any more). A view filter only:
 * the server already sent what the viewer may see.
 */
export function forStaff(cal: Calendar, who: string): Calendar {
  const techIds = new Set(cal.technicians.map((t) => t.id));
  if (who !== "none" && !techIds.has(who)) return cal;
  const keep = (staffId: string | null | undefined) =>
    who === "none" ? !staffId || !techIds.has(staffId) : staffId === who;
  return {
    ...cal,
    technicians: cal.technicians.filter((t) => t.id === who),
    appointments: cal.appointments.filter((a) => keep(a.staff_id)),
    visits: cal.visits.filter((v) => keep(v.staff_id)),
  };
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
