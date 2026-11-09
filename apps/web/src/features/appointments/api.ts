import type {
  Appointment,
  AppointmentCreate,
  AppointmentGroup,
  AppointmentGroupCreate,
  AppointmentGroupMoved,
  AppointmentService,
  AppointmentSettings,
  AppointmentUpdate,
  Calendar,
  CalendarVisit,
  TimelinePhase,
} from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api, ifMatch } from "../../lib/api";
import { addMessages } from "../../lib/errors";

// Error messages only the calendar and booking sheets can meet: here, not in the startup chunk (lib/errors.ts).
addMessages(
  {
    "appointment.staff_busy": "KTV này đã có lịch trùng giờ.",
    "appointment.customer_busy":
      "Khách này đã có lịch hẹn trùng giờ: đổi giờ, hoặc mở lịch đã có để sửa.",
    "appointment.in_the_past":
      "Không đặt lịch cho ngày đã qua. Khách đã làm thì ghi lượt nhập bù ở Quầy.",
    "appointment.outside_opening_hours":
      "Lịch hẹn phải nằm trọn trong giờ mở cửa: chọn giờ sớm hơn hoặc rút ngắn thời lượng.",
    "appointment.already_arrived": "Khách đã được đánh dấu là đã đến.",
    "appointment.not_started":
      "Chưa tới giờ hẹn, chưa đánh dấu không đến được.",
    "appointment.not_arrived":
      "Lịch hẹn này chưa báo đã đến (có thể vừa được hoàn tác ở máy khác).",
    "appointment.not_in_group": "Lịch hẹn này không thuộc nhóm nào.",
  },
  {
    not_on_slot: "Giờ hẹn phải đúng ô 15 phút (ví dụ 10:00, 10:15).",
    too_far: "Chỉ đặt lịch trước tối đa 90 ngày.",
    customer_or_guest: "Chọn khách có hồ sơ hoặc nhập khách mới.",
    not_same_day: "Mọi người trong nhóm phải hẹn cùng một ngày.",
    range_too_long: "Chỉ xem tối đa 14 ngày.",
  },
);

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

/** "Hoàn tác đã đến": a mistaken arrival goes back to booked (today, no visit yet). */
export async function undoArrival(id: string): Promise<Appointment> {
  return (await api(`/appointments/${id}/undo-arrival`, { method: "POST" })) as Appointment;
}

/**
 * "Đổi cả nhóm": `a` moves to `startsAt` and every other member still booked shifts by the same
 * amount, all or none. A refusal names the member: `422` with `errors[].field =
 * "appointments.<id>"` and the member's code (see [groupMoveRefusal]).
 */
export async function moveGroup(a: Appointment, startsAt: string): Promise<AppointmentGroupMoved> {
  return (await api(`/appointments/${a.id}/move-group`, {
    method: "POST",
    body: { starts_at: startsAt },
    headers: ifMatch(a.version),
  })) as AppointmentGroupMoved;
}

const MEMBER_REFUSAL: Record<string, string> = {
  "appointment.staff_busy": "KTV của người này đã có lịch khác vào giờ mới",
  "appointment.customer_busy": "người này đã có lịch hẹn khác vào giờ mới",
  "appointment.in_the_past": "giờ mới rơi vào ngày đã qua",
  "appointment.outside_opening_hours": "giờ mới ra ngoài giờ mở cửa",
  "staff.inactive": "KTV của người này đã nghỉ việc",
};

/** "Chị Hà: KTV của người này đã có lịch khác vào giờ mới." for a refused whole-group move. */
export function groupMoveRefusal(
  errors: { field: string; code: string }[],
  nameOf: (id: string) => string,
): string | undefined {
  const parts = errors.flatMap((e) => {
    const m = /^appointments\.([^.]+)/.exec(e.field);
    if (!m?.[1]) return [];
    return [`${nameOf(m[1])}: ${MEMBER_REFUSAL[e.code] ?? "giờ mới không hợp lệ"}.`];
  });
  return parts.length ? `Không đổi được cả nhóm. ${parts.join(" ")}` : undefined;
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

/**
 * The tag of a visit recorded without a booking, on the calendar and in "Khách của tôi" (F23,
 * 2026-11-08). Not the old "Vãng lai": that is the anonymous walk-in ("Khách vãng lai"), and most
 * visits without a booking are named customers.
 */
export const NO_BOOKING = "Không đặt lịch";

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
 * The app-wide status chip of each phase (design-system rulebook 7, one semantic set):
 * waiting = neutral info, in service = needs attention, done = green (spec §10.7), missed = danger,
 * cancelled / voided = muted. Every chip carries its label too.
 */
export const PHASE_BADGE: Record<TimelinePhase, string> = {
  waiting: "badge-info",
  in_service: "badge-warn",
  done: "badge-ok",
  no_show: "badge-danger",
  cancelled: "badge-muted",
  voided: "badge-muted",
};

/** Up to 5 companion notes, 1–100 characters each (server `app.valid_companions`). */
export const MAX_COMPANIONS = 5;

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
  /** What the card and the list say (names only: a read-only calendar carries no ids). */
  services: Pick<AppointmentService, "service_name" | "category_name">[];
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

/** Shortest and longest booking (server `MIN_DURATION_MINUTES` / `MAX_DURATION_MINUTES`). */
export const MIN_MINUTES = 5;
export const MAX_MINUTES = 480;

/**
 * A booking's length from its services, exactly (same rule as the server's `services_minutes`;
 * EM 2026-11-07 decision A: a 70-minute service is 70 minutes, no longer rounded up to the 15-minute
 * grid). The timed services' sum; a service without a duration ("theo tư vấn") is an hour only
 * when nothing else is timed, next to a timed one it is an add-on done in that time.
 * No services = 60.
 */
export function autoMinutes(durations: (number | null | undefined)[]): number {
  const timed = durations.filter((d): d is number => d != null && d > 0);
  const sum =
    timed.length > 0
      ? timed.reduce((t, d) => t + d, 0)
      : Math.max(1, durations.length) * 60;
  return Math.min(MAX_MINUTES, Math.max(MIN_MINUTES, sum));
}

/**
 * Lengths the desk can pick: every slot multiple up to 8 hours, plus the services' own length and
 * the current one when they fall between (70 next to 60 and 75), in order.
 */
export function lengthChoices(slot: number, ...exact: number[]): number[] {
  const grid = Array.from(
    { length: Math.floor(MAX_MINUTES / slot) },
    (_, i) => (i + 1) * slot,
  );
  return [...new Set([...grid, ...exact])]
    .filter((m) => m >= MIN_MINUTES && m <= MAX_MINUTES)
    .sort((a, b) => a - b);
}

/** Same person: equal digits once a leading +84 / 84 is read as 0. */
function samePhone(a: string, b: string): boolean {
  const digits = (s: string) => s.replace(/\D/g, "").replace(/^84/, "0");
  return digits(a) !== "" && digits(a) === digits(b);
}

/**
 * The open booking that already holds this person over `[start, start + minutes)` of `day`: the
 * same customer record, or a newcomer with the same phone outside `groupId` (a family may share a
 * phone). Mirrors the server's `409 appointment.customer_busy`, so the form says it before sending.
 */
export function customerClash(
  cal: Calendar,
  who: { customerId: string | null; guestPhone: string | null },
  day: string,
  start: number,
  minutes: number,
  except: { id: string | null; groupId: string | null },
): Appointment | null {
  return (
    cal.appointments.find(
      (a) =>
        isOpen(a) &&
        a.id !== except.id &&
        a.business_date === day &&
        spaMinutes(a.starts_at) < start + minutes &&
        start < spaMinutes(a.starts_at) + a.duration_min &&
        ((who.customerId !== null && a.customer_id === who.customerId) ||
          (who.guestPhone !== null &&
            a.is_guest &&
            !!a.customer_phone &&
            samePhone(a.customer_phone, who.guestPhone) &&
            (except.groupId === null || a.group_id !== except.groupId))),
    ) ?? null
  );
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
