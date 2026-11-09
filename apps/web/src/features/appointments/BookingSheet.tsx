import type {
  Appointment,
  Calendar,
  CustomerSummary,
  Service,
  ServiceCategory,
} from "@vpyd/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { CustomerName } from "../../app/CustomerName";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import { spaToday } from "../corrections/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { customerQuery } from "../customers/api";
import { formatDate } from "../sales/api";
import {
  MAX_COMPANIONS,
  addedMinutes,
  autoMinutes,
  bookAppointment,
  bookGroup,
  calendarQuery,
  customerClash,
  groupMoveRefusal,
  hhmm,
  instantAt,
  lengthChoices,
  minutesOf,
  moveGroup,
  peakPeople,
  peopleAt,
  spaMinutes,
  updateAppointment,
} from "./api";

export type Draft =
  | { mode: "new"; day: string; start: number; staffId: string }
  | { mode: "edit"; appointment: Appointment };

const MAX_SERVICES = 5;

/**
 * One person of the booking form: `people[0]` is the customer who books, the others are their
 * companions (spec front-desk-and-packages §10.10). A companion with services is booked for real
 * (own technician, own block on the calendar); one without is only a note on the booking.
 * `duration` null = follows the services (spec appointments §10.2); `start` null = with the
 * booking (companions only).
 */
export interface Person {
  key: number;
  who: "customer" | "guest";
  customer: CustomerSummary | null;
  guestName: string;
  guestPhone: string;
  serviceIds: string[];
  staffId: string;
  duration: number | null;
  start: number | null;
  /** Idempotency key of a companion added to an existing booking (`group_with`). */
  bookingKey: string;
}

export function blank(key: number, staffId = ""): Person {
  return {
    key,
    who: "customer",
    customer: null,
    guestName: "",
    guestPhone: "",
    serviceIds: [],
    staffId,
    duration: null,
    start: null,
    bookingKey: crypto.randomUUID(),
  };
}

/** A new companion row: known by a name or a note until someone picks a customer record. */
export function companion(key: number, note = ""): Person {
  return { ...blank(key), who: "guest", guestName: note };
}

function nameOf(p: Person): string | null {
  return p.who === "customer"
    ? (p.customer?.full_name ?? null)
    : p.guestName.trim() || null;
}

/** "Khách (Chị Hà)" / "Người đi cùng 2 (Chồng của chị Hà)": how errors and hints name a person. */
export function personLabel(p: Person, i: number): string {
  const name = nameOf(p);
  const who = i === 0 ? "Khách" : `Người đi cùng ${String(i)}`;
  return name ? `${who} (${name})` : who;
}

/** A companion who gets a service is booked; one without stays a note (round 6 Q-U5). */
export function isServed(p: Person, i: number): boolean {
  return i === 0 || p.serviceIds.length > 0;
}

/** The note-only companions, as the server's `companions[]`. */
export function noteCompanions(people: Person[]): string[] {
  return people
    .filter((p, i) => !isServed(p, i))
    .map((p) => nameOf(p) ?? "")
    .filter((n) => n !== "")
    .slice(0, MAX_COMPANIONS);
}

/** What still stops the form from being sent, in words (never a silent no-op, spec §10.5). */
export function missingParts(people: Person[], editing: boolean): string[] {
  const out: string[] = [];
  people.forEach((p, i) => {
    const who = i > 0 ? ` của ${personLabel(p, i)}` : "";
    if (i === 0) {
      if (!editing) {
        if (p.who === "customer" && p.customer === null) out.push("khách");
        if (p.who === "guest" && p.guestName.trim() === "")
          out.push("tên khách mới");
        if (p.who === "guest" && p.guestPhone.trim() === "")
          out.push("SĐT khách mới");
      }
      if (p.serviceIds.length === 0) out.push("dịch vụ");
      return;
    }
    if (p.who === "customer" && p.customer === null) out.push(`khách${who}`);
    if (p.who === "guest" && p.guestName.trim() === "")
      out.push(`tên hoặc ghi chú${who}`);
  });
  return out;
}

/**
 * Where the first missing part is, so a click on "Đặt lịch" brings it into view and focus instead
 * of naming it at the bottom of a long form (EM 2026-11-07). Same order as `missingParts`.
 */
export function firstMissing(
  people: Person[],
  editing: boolean,
): {
  key: number;
  what: "customer" | "guest-name" | "guest-phone" | "service";
} | null {
  for (const [i, p] of people.entries()) {
    const own = i > 0 || !editing;
    if (own && p.who === "customer" && p.customer === null)
      return { key: p.key, what: "customer" };
    if (own && p.who === "guest" && p.guestName.trim() === "")
      return { key: p.key, what: "guest-name" };
    if (i === 0 && !editing && p.who === "guest" && p.guestPhone.trim() === "")
      return { key: p.key, what: "guest-phone" };
    if (i === 0 && p.serviceIds.length === 0)
      return { key: p.key, what: "service" };
  }
  return null;
}

/** Brings a form control into the middle of the sheet and focuses it (no-op when it is gone). */
function reveal(el: HTMLElement | null | undefined) {
  if (!el) return;
  // jsdom has no scrollIntoView.
  if ("scrollIntoView" in el) el.scrollIntoView({ block: "center" });
  el.focus({ preventScroll: true });
}

const FIELD_NAMES: Record<string, string> = {
  service_ids: "dịch vụ",
  "guest.phone": "SĐT",
  "guest.full_name": "tên khách",
  customer_id: "khách",
  starts_at: "giờ hẹn",
  duration_min: "thời lượng",
  people: "số người",
  companions: "người đi cùng",
};

/** "Nguyễn Thị Hà đã có lịch 10:00–11:30 ngày đó (KTV Lan)…": one person, two bookings at once. */
export function clashText(other: Appointment): string {
  const s = spaMinutes(other.starts_at);
  return `${other.customer_name} đã có lịch ${hhmm(s)}–${hhmm(s + other.duration_min)} ngày ${formatDate(other.business_date)} (${other.staff_name ?? "chưa gán KTV"}). Một người không có hai lịch trùng giờ: đổi giờ, hoặc mở lịch đã có để sửa.`;
}

/**
 * Every server refusal as one Vietnamese sentence. `sent` are the people of the request in its
 * order (`people[i]` of a group) and `labels` how the form names each of them.
 */
export function bookingErrorText(
  error: unknown,
  cal: Calendar,
  sent: Person[],
  labels: string[] = sent.map(personLabel),
): string {
  if (!(error instanceof ApiError)) return messageFor(error);
  if (error.code === "appointment.staff_busy") {
    const other = cal.appointments.find((x) => x.id === error.existingId);
    if (other) {
      const s = spaMinutes(other.starts_at);
      return `${other.staff_name ?? "KTV"} đã có lịch ${hhmm(s)}–${hhmm(s + other.duration_min)} (${other.customer_name}). Chọn giờ hoặc KTV khác.`;
    }
    return sent.length > 1
      ? "Hai người đang chọn cùng một KTV trùng giờ, hoặc KTV đã có lịch. Chọn KTV khác."
      : messageFor(error);
  }
  if (error.code === "appointment.customer_busy") {
    const other = cal.appointments.find((x) => x.id === error.existingId);
    return other
      ? clashText(other)
      : `${messageFor(error)} (${sent.length > 1 ? "một người trong nhóm" : "khách"} đã có lịch khác).`;
  }
  if (error.code === "appointment.outside_opening_hours")
    return `${messageFor(error)} Giờ mở cửa ${cal.opens_at}–${cal.closes_at}.`;
  if (error.code === "validation_failed" && error.errors.length > 0) {
    return error.errors
      .map((f) => {
        const m = /^people\[(\d+)\]\.(.+)$/.exec(f.field);
        const field = m?.[2] ?? f.field;
        const label = m ? labels[Number(m[1])] : undefined;
        const who = label ? `${label}: ` : "";
        const along = /^companions\[(\d+)\]$/.exec(field);
        const name = along
          ? `người đi cùng ${String(Number(along[1]) + 1)}`
          : (FIELD_NAMES[field] ?? field);
        const text =
          f.code === "duplicate" && field === "customer_id"
            ? "khách này đã có trong nhóm."
            : f.code === "count_out_of_range" && field === "people"
              ? "một lịch có tối đa 6 người được làm."
              : f.code === "count_out_of_range"
                ? `chọn 1–${String(MAX_SERVICES)} dịch vụ.`
                : `${name} — ${(fieldMessage(error, f.field) ?? "không hợp lệ.").toLowerCase()}`;
        return `${who}${text}`;
      })
      .join(" ");
  }
  return messageFor(error);
}

/**
 * "Đặt lịch hẹn" (spec appointments.md §2, §10.2–10.5; front-desk-and-packages §10.10): the
 * customer, then "Người đi cùng". Each companion is a name (or a customer record) and, if they
 * get a service, their own services, technician, length and start: a booking of their own in the
 * group. Without a service a companion is a note. Nobody served alongside → POST /appointments;
 * otherwise POST /appointments/group (all or nobody). Editing: PATCH the booking (note-only
 * companions included), then each new served companion joins it (`group_with`).
 */
export function BookingSheet({
  cal,
  draft,
  onClose,
  onSaved,
}: {
  cal: Calendar;
  draft: Draft;
  onClose: () => void;
  onSaved: (a: Appointment) => void;
}) {
  const queryClient = useQueryClient();
  const editing = draft.mode === "edit" ? draft.appointment : null;
  const allServices = useQuery(servicesQuery(false));
  const categories = useQuery(categoriesQuery);
  const slot = cal.slot_minutes;
  const opens = minutesOf(cal.opens_at);
  const closes = minutesOf(cal.closes_at);
  const nextKey = useRef(1 + (editing?.companions.length ?? 0));
  const [people, setPeople] = useState<Person[]>(() => {
    if (!editing) return [blank(0, draft.mode === "new" ? draft.staffId : "")];
    const auto = autoMinutes(editing.services.map((s) => s.duration_min));
    return [
      {
        ...blank(0, editing.staff_id ?? ""),
        serviceIds: editing.services.map((s) => s.service_id),
        duration: editing.duration_min === auto ? null : editing.duration_min,
      },
      ...editing.companions.map((n, j) => companion(j + 1, n)),
    ];
  });
  const [day, setDay] = useState(
    editing
      ? editing.business_date
      : draft.mode === "new"
        ? draft.day
        : spaToday(),
  );
  const [start, setStart] = useState(
    editing
      ? spaMinutes(editing.starts_at)
      : draft.mode === "new"
        ? draft.start
        : opens,
  );
  const [note, setNote] = useState(editing?.note ?? "");
  const [attempted, setAttempted] = useState(false);
  // One key per booking, kept across retries so a resend never books twice.
  const [key] = useState(() => crypto.randomUUID());
  // Editing runs several requests: what already went through is not sent again on a retry, until
  // the form changes (then it goes again with the new values). `latest` is the newest copy the
  // server returned: its version is the one the next request must match.
  const latest = useRef<Appointment | null>(null);
  const groupMoved = useRef(false);
  const patched = useRef(false);
  const formRef = useRef<HTMLFormElement | null>(null);

  const serviceOf = (id: string): Service | undefined =>
    allServices.data?.find((s) => s.id === id);
  /** Minutes of `p` from its services, using the catalog (or the booking's own copy). */
  const autoOf = (p: Person) =>
    autoMinutes(
      p.serviceIds.map(
        (id) =>
          serviceOf(id)?.duration_min ??
          editing?.services.find((s) => s.service_id === id)?.duration_min,
      ),
    );
  const minutesOfPerson = (p: Person) => p.duration ?? autoOf(p);
  const startOf = (p: Person) => p.start ?? start;
  const served = people.filter(isServed);
  const notes = noteCompanions(people);
  const firstStart = Math.min(...served.map(startOf));
  const lastEnd = Math.max(
    ...served.map((p) => startOf(p) + minutesOfPerson(p)),
  );

  const change = (k: number, patch: Partial<Person>) => {
    setPeople((ps) => ps.map((p) => (p.key === k ? { ...p, ...patch } : p)));
  };

  const missing = missingParts(people, editing !== null);
  // One technician on two people whose times overlap.
  const clashing = served.filter((p, i) =>
    served.some(
      (q, j) =>
        j < i &&
        p.staffId !== "" &&
        q.staffId === p.staffId &&
        startOf(p) < startOf(q) + minutesOfPerson(q) &&
        startOf(q) < startOf(p) + minutesOfPerson(p),
    ),
  );
  // The day being booked: the calendar on screen, or that day's own calendar when another date is
  // picked in the form (F13: the "x/3 khách" counts and the clash checks then went blank).
  const onScreen = day >= cal.from && day <= cal.to;
  const dayCal = useQuery({ ...calendarQuery(day), enabled: !onScreen });
  const known: Calendar | undefined = onScreen ? cal : dayCal.data;
  const max = cal.max_per_slot ?? null;
  const already = known
    ? peakPeople(known, day, firstStart, lastEnd - firstStart)
    : 0;
  const adding = editing ? served.length - 1 : served.length;
  const overMax = max !== null && known !== undefined && already + adding > max;

  // When (EM 2026-11-07, 2026-11-08 F12): a past day and a slot of today already over are refused,
  // here and by the server (`appointment.in_the_past`); a service already done is a visit recorded
  // at Quầy ("Đổi giờ / nhập bù"). The slot under way is still bookable. An edited booking that
  // keeps its time is not re-judged.
  const today = spaToday();
  const moved =
    editing === null ||
    day !== editing.business_date ||
    start !== spaMinutes(editing.starts_at);
  const pastDay = moved && day < today;
  const slotNow =
    Math.floor(spaMinutes(new Date().toISOString()) / slot) * slot;
  const earliest = Math.min(
    ...served.filter((_, i) => i > 0 || moved).map(startOf),
  );
  const pastStart = day === today && earliest < slotNow;

  // "Đổi lịch" of one member of a group (EM 2026-11-07 item 5c): move the whole group by the same
  // amount (all or none, on the server), or only this person. Members already arrived or done stay.
  const groupMates = editing?.group_id
    ? cal.appointments.filter((x) => x.group_id === editing.group_id && x.id !== editing.id)
    : [];
  const mateMoves = groupMates.filter((x) => x.status === "booked");
  const mateStays = groupMates.filter((x) => x.status === "arrived" || x.status === "done");
  const askGroup = editing !== null && moved && mateMoves.length > 0;
  const [moveAll, setMoveAll] = useState(true);
  const wholeGroup = askGroup && moveAll;

  // Who already holds a booking at that time (the server refuses: 409 appointment.customer_busy).
  const holderOf = (p: Person, i: number) => {
    if (i === 0 && editing)
      return {
        customerId: editing.customer_id ?? null,
        guestPhone: editing.is_guest ? (editing.customer_phone ?? null) : null,
      };
    return p.who === "customer"
      ? { customerId: p.customer?.id ?? null, guestPhone: null }
      : { customerId: null, guestPhone: p.guestPhone.trim() || null };
  };
  const clashes = new Map<number, Appointment>();
  if (known)
    served.forEach((p, i) => {
      // Like the server: an edited booking is judged only once its time or length changes.
      // (`moved` is always true for a new booking, so `editing` is set here.)
      if (
        i === 0 &&
        editing &&
        !moved &&
        minutesOfPerson(p) === editing.duration_min
      )
        return;
      const other = customerClash(
        known,
        holderOf(p, i),
        day,
        startOf(p),
        minutesOfPerson(p),
        {
          id: i === 0 ? (editing?.id ?? null) : null,
          groupId: editing?.group_id ?? null,
        },
      );
      if (other) clashes.set(p.key, other);
    });
  const firstClash = [...clashes.values()][0];

  const person = (p: Person) => ({
    ...(p.who === "customer"
      ? { customer_id: p.customer?.id ?? null }
      : {
          guest: {
            full_name: p.guestName.trim(),
            phone: p.guestPhone.trim() || null,
          },
        }),
    service_ids: p.serviceIds,
    staff_id: p.staffId || null,
    duration_min: minutesOfPerson(p),
  });

  const save = useMutation({
    mutationFn: async (): Promise<Appointment> => {
      const starts_at = instantAt(day, start);
      const [first, ...others] = served;
      if (!first) throw new Error("no person");
      const withNotes = notes.length > 0 ? { companions: notes } : {};
      if (editing) {
        // The whole group first (one server transaction); then this booking's other changes.
        if (wholeGroup && !groupMoved.current) {
          const moved = await moveGroup(latest.current ?? editing, starts_at);
          latest.current = moved.moved.find((x) => x.id === editing.id) ?? latest.current;
          groupMoved.current = true;
        }
        if (!patched.current) {
          latest.current = await updateAppointment(latest.current ?? editing, {
            starts_at,
            duration_min: minutesOfPerson(first),
            staff_id: first.staffId || null,
            service_ids: first.serviceIds,
            note: note.trim() || null,
            companions: notes,
          });
          patched.current = true;
        }
        for (const p of others)
          await bookAppointment(
            {
              ...person(p),
              starts_at: instantAt(day, startOf(p)),
              group_with: editing.id,
            },
            p.bookingKey,
          );
        return latest.current ?? editing;
      }
      if (others.length === 0)
        return bookAppointment(
          {
            ...person(first),
            starts_at,
            note: note.trim() || null,
            ...withNotes,
          },
          key,
        );
      const group = await bookGroup(
        {
          starts_at,
          note: note.trim() || null,
          people: served.map((p) => ({
            ...person(p),
            ...(p.start === null ? {} : { starts_at: instantAt(day, p.start) }),
          })),
          ...withNotes,
        },
        key,
      );
      const opened = group.appointments[0];
      if (!opened) throw new Error("empty group");
      return opened;
    },
    onSuccess: async (a) => {
      await queryClient.invalidateQueries({ queryKey: ["appointments"] });
      onSaved(a);
    },
  });

  const taken =
    save.error instanceof ApiError && save.error.code === "customer.phone_taken"
      ? save.error.existingId
      : undefined;
  /** "Dùng hồ sơ này" could not open that record (offline, no longer there): said, not swallowed. */
  const [adoptProblem, setAdoptProblem] = useState<string | null>(null);
  /** The newcomer whose phone already has a record becomes that customer. */
  const adoptExisting = async (id: string) => {
    const c = await queryClient
      .query(customerQuery(id))
      .catch((e: unknown) => {
        setAdoptProblem(messageFor(e));
        return null;
      });
    if (!c) return;
    setAdoptProblem(null);
    const digits = (s: string) => s.replace(/\D/g, "").replace(/^84/, "0");
    const guest =
      people.find(
        (p) => p.who === "guest" && c.phone != null && digits(p.guestPhone) === digits(c.phone),
      ) ?? people.find((p) => p.who === "guest");
    if (guest)
      change(guest.key, {
        who: "customer",
        customer: {
          id: c.id,
          full_name: c.full_name,
          named: c.named,
          phone: c.phone ?? null,
          gender: c.gender ?? null,
        },
      });
    save.reset();
  };

  const byId = (id: string) =>
    formRef.current?.querySelector<HTMLElement>(`#${id}`);
  const personField = (key: number) =>
    formRef.current?.querySelector<HTMLElement>(
      `#booking-person-${String(key)} :is(input, select)`,
    );

  const submit = () => {
    setAttempted(true);
    const gap = firstMissing(people, editing !== null);
    if (gap) {
      reveal(
        gap.what === "customer"
          ? personField(gap.key)
          : byId(`booking-${gap.what}-${String(gap.key)}`),
      );
      return;
    }
    if (pastDay) {
      reveal(formRef.current?.querySelector<HTMLElement>("#booking-day input"));
      return;
    }
    if (pastStart) {
      reveal(byId("booking-start"));
      return;
    }
    const clashing = [...clashes.keys()][0];
    if (clashing !== undefined) {
      reveal(personField(clashing));
      return;
    }
    if (!save.isPending) save.mutate();
  };

  // A refusal is about what was sent (F11): once the day, time or anyone in the form changes, the
  // old sentence ("Lan đã có lịch…") no longer applies, so it goes until the next try.
  useEffect(() => {
    if (save.isError) save.reset();
    // Only on an edit of the form, not when the refusal itself arrives.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [day, start, people]);
  // An edit after a partial failure: the steps that went through are sent again with the new values.
  useEffect(() => {
    groupMoved.current = false;
    patched.current = false;
  }, [day, start, people, note, wholeGroup]);

  // A server refusal: bring the field it is about into view (the sentence is in the footer).
  useEffect(() => {
    const e = save.error;
    if (!(e instanceof ApiError)) return;
    if (
      e.code === "appointment.outside_opening_hours" ||
      e.code === "appointment.in_the_past"
    )
      reveal(byId("booking-start"));
    else if (
      e.code === "appointment.staff_busy" &&
      served.length === 1 &&
      served[0]
    )
      reveal(byId(`booking-staff-${String(served[0].key)}`));
    // Only when a new refusal arrives, not on every keystroke after it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [save.error]);

  const groups = (categories.data ?? []).map((c) => ({
    c,
    items: (allServices.data ?? []).filter((s) => s.category_id === c.id),
  }));
  const startOptions = Array.from(
    { length: Math.max(0, (closes - opens) / slot) },
    (_, i) => opens + i * slot,
  );
  const sentLabels = served.map((p) => personLabel(p, people.indexOf(p)));
  const companionsCount = people.length - 1;
  const servedCompanions = served.length - 1;
  const staffClashText =
    clashing.length > 0
      ? `${clashing.map((p) => personLabel(p, people.indexOf(p))).join(", ")} đang cùng KTV với người khác trong lịch này, trùng giờ: chọn KTV khác, đổi giờ bắt đầu hoặc để “Chưa gán KTV”.`
      : null;
  const capacityText = overMax
    ? `Khung ${hhmm(firstStart)} đã có ${String(already)}/${String(max)} khách${adding > 0 ? `; thêm ${String(adding)} người sẽ vượt mức tối đa` : ""}. Vẫn đặt được nếu khách chịu chờ.`
    : null;
  const nowText = hhmm(spaMinutes(new Date().toISOString()));

  /**
   * The one message the desk must see now, pinned in the footer next to the button (EM
   * 2026-11-07: errors at the bottom of a long form were out of sight). Blocking first, then the
   * one-time question about an earlier start, then the advisory warnings.
   */
  const notice: { tone: "error" | "warn"; text: string } | null =
    save.isError && !taken
      ? {
          tone: "error",
          text:
            (save.error instanceof ApiError &&
              groupMoveRefusal(
                save.error.errors,
                (id) => cal.appointments.find((x) => x.id === id)?.customer_name ?? "Một người trong nhóm",
              )) ||
            bookingErrorText(
            save.error,
            known ?? cal,
            editing ? served.slice(1) : served,
            editing ? sentLabels.slice(1) : sentLabels,
          ),
        }
      : attempted && missing.length > 0
        ? { tone: "error", text: `Còn thiếu: ${missing.join(", ")}.` }
        : pastDay
          ? {
              tone: "error",
              text: `${formatDate(day)} đã qua: không đặt lịch cho ngày cũ. Khách đã làm thì ghi lượt nhập bù ở Quầy.`,
            }
          : pastStart
            ? {
                tone: "error",
                text: `Giờ hẹn ${hhmm(earliest)} đã qua (bây giờ ${nowText}): chọn từ ${hhmm(slotNow)} trở đi. Khách đã làm xong thì ghi lượt ở Quầy (Đổi giờ / nhập bù).`,
              }
            : firstClash
              ? { tone: "error", text: clashText(firstClash) }
              : staffClashText
                ? { tone: "warn", text: staffClashText }
                : capacityText
                  ? { tone: "warn", text: capacityText }
                  : null;

  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={editing ? `Đổi lịch · ${editing.customer_name}` : "Đặt lịch hẹn"}
      footer={
        <>
          {taken ? (
            <div role="alert" className="alert appt-footer-note">
              <span>
                Số điện thoại của khách mới đã có hồ sơ khách.
                {adoptProblem && ` Chưa mở được hồ sơ: ${adoptProblem}`}
              </span>
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  void adoptExisting(taken);
                }}
              >
                Dùng hồ sơ này
              </button>
            </div>
          ) : (
            notice && (
              <p
                role={notice.tone === "error" ? "alert" : "status"}
                className={`appt-footer-note ${notice.tone === "error" ? "alert" : "callout"}`}
              >
                {notice.text}
              </p>
            )
          )}
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Thôi
          </button>
          {/* Always pressable (round 5): a click with something missing names it. */}
          <button
            type="submit"
            form="booking-form"
            className="btn btn-primary"
            aria-busy={save.isPending}
          >
            {editing
              ? "Lưu thay đổi"
              : served.length > 1
                ? `Đặt lịch ${String(served.length)} người`
                : "Đặt lịch"}
          </button>
        </>
      }
    >
      <form
        ref={formRef}
        id="booking-form"
        className="appt-form"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        {people.map((p, i) => {
          const legend = i === 0 ? "Khách" : `Người đi cùng ${String(i)}`;
          const id = (what: string) => `booking-${what}-${String(p.key)}`;
          const main = i === 0;
          const booked = isServed(p, i);
          const held = clashes.get(p.key);
          const sameStaff = clashing.includes(p);
          return (
            <fieldset key={p.key} id={id("person")} className="appt-person">
              <legend className="appt-person-head">
                <span>{legend}</span>
                {!main && (
                  <button
                    type="button"
                    className="btn btn-ghost"
                    aria-label={`Bỏ người này (${legend})`}
                    onClick={() => {
                      setPeople((ps) => ps.filter((x) => x.key !== p.key));
                    }}
                  >
                    Bỏ người này
                  </button>
                )}
              </legend>
              {(!editing || !main) && (
                <WhoFields
                  p={p}
                  main={main}
                  legend={legend}
                  id={id}
                  booked={booked}
                  change={change}
                />
              )}
              {held && (
                <p className="field-error">
                  Đã có lịch {hhmm(spaMinutes(held.starts_at))}–
                  {hhmm(spaMinutes(held.starts_at) + held.duration_min)} cùng
                  lúc: đổi giờ hoặc sửa lịch đã có.
                </p>
              )}
              <ServiceFields
                p={p}
                main={main}
                suffix={main ? "" : ` · ${legend}`}
                id={id}
                groups={groups}
                loading={allServices.isPending}
                serviceOf={serviceOf}
                editing={editing}
                change={change}
              />
              {booked && (
                <div className="appt-fields">
                  <LengthField
                    p={p}
                    auto={autoOf(p)}
                    minutes={minutesOfPerson(p)}
                    slot={slot}
                    label={
                      <>
                        Thời lượng
                        <Who legend={main ? null : legend} />
                      </>
                    }
                    id={id}
                    change={change}
                  />
                  <div className="field">
                    <label htmlFor={id("staff")}>
                      {main
                        ? "KTV (không bắt buộc)"
                        : "KTV làm cho người đi cùng"}
                      <Who legend={main ? null : legend} />
                    </label>
                    <select
                      id={id("staff")}
                      className="input"
                      value={p.staffId}
                      aria-invalid={sameStaff || undefined}
                      aria-describedby={
                        sameStaff ? id("staff-clash") : undefined
                      }
                      onChange={(e) => {
                        change(p.key, { staffId: e.target.value });
                      }}
                    >
                      <option value="">Chưa gán KTV</option>
                      {cal.technicians.map((t) => (
                        <option key={t.id} value={t.id}>
                          {t.full_name}
                        </option>
                      ))}
                    </select>
                    {sameStaff && (
                      <span id={id("staff-clash")} className="field-error">
                        Trùng giờ với người khác cùng KTV này.
                      </span>
                    )}
                  </div>
                  {!main && (
                    <div className="field">
                      <label htmlFor={id("start")}>
                        Giờ bắt đầu
                        <Who legend={legend} />
                      </label>
                      <select
                        id={id("start")}
                        className="input"
                        value={p.start ?? ""}
                        onChange={(e) => {
                          const v = e.target.value;
                          change(p.key, { start: v === "" ? null : Number(v) });
                        }}
                      >
                        <option value="">
                          Cùng giờ với khách ({hhmm(start)})
                        </option>
                        {startOptions.map((m) => (
                          <option key={m} value={m}>
                            {hhmm(m)}
                          </option>
                        ))}
                      </select>
                    </div>
                  )}
                </div>
              )}
            </fieldset>
          );
        })}

        {companionsCount < MAX_COMPANIONS && (
          <div className="appt-companions">
            <button
              type="button"
              className="btn btn-secondary justify-self-start"
              onClick={() => {
                setPeople((ps) => [...ps, companion(nextKey.current++)]);
                // A new, still empty row is not "missing" yet (F9): the earlier failed try is
                // judged again on the next "Đặt lịch".
                setAttempted(false);
              }}
            >
              <Icon name="plus" /> Người đi cùng
            </button>
            <span className="field-hint">
              Chọn dịch vụ và KTV nếu người đi cùng cũng làm; không chọn dịch vụ
              thì chỉ ghi chú trên lịch hẹn.
            </span>
          </div>
        )}

        <div className="appt-fields">
          <div id="booking-day" className="grid">
            <DateField
              label="Ngày hẹn"
              value={day}
              min={today}
              invalid={pastDay}
              onChange={(iso) => {
                if (iso) setDay(iso);
              }}
            />
          </div>
          <div className="field">
            <label htmlFor="booking-start">Giờ hẹn</label>
            <select
              id="booking-start"
              className="input"
              value={start}
              aria-describedby={
                pastStart || overMax ? "booking-start-note" : undefined
              }
              onChange={(e) => {
                setStart(Number(e.target.value));
              }}
            >
              {startOptions.map((m) => {
                const n = known ? peopleAt(known, day, m) : 0;
                const tail =
                  max !== null && known
                    ? ` · ${String(n)}/${String(max)} khách`
                    : n > 0
                      ? ` · ${String(n)} khách`
                      : "";
                const gone = day === today && m < slotNow;
                return (
                  // A slot already over cannot be picked (F12); an edited booking still shows its
                  // own time.
                  <option key={m} value={m} disabled={gone && m !== start}>
                    {hhmm(m)}
                    {tail}
                    {gone ? " · đã qua" : ""}
                  </option>
                );
              })}
            </select>
            {(pastStart || overMax) && (
              <span id="booking-start-note" className="field-hint">
                {pastStart
                  ? `Giờ này đã qua (bây giờ ${nowText}). Khách đã làm xong thì ghi lượt ở Quầy.`
                  : `Khung giờ đã đủ ${String(max)} khách.`}
              </span>
            )}
          </div>
        </div>
        {askGroup && (
          <fieldset className="grid gap-2" aria-describedby="booking-group-move-hint">
            <legend>Người cùng nhóm</legend>
            <div className="choice-row">
            <label className="choice">
              <input
                type="radio"
                name="booking-group-move"
                checked={moveAll}
                onChange={() => {
                  setMoveAll(true);
                }}
              />
              Đổi cả nhóm ({String(mateMoves.length + 1)} người, cùng dời một khoảng)
            </label>
            <label className="choice">
              <input
                type="radio"
                name="booking-group-move"
                checked={!moveAll}
                onChange={() => {
                  setMoveAll(false);
                }}
              />
              Chỉ người này
            </label>
            </div>
            <span id="booking-group-move-hint" className="field-hint">
              {moveAll
                ? `Cùng dời: ${mateMoves.map((x) => x.customer_name).join(", ")}. Một người vướng thì không ai bị đổi.`
                : `Giữ nguyên giờ của ${mateMoves.map((x) => x.customer_name).join(", ")}.`}
              {mateStays.length > 0 &&
                ` Không đổi: ${mateStays.map((x) => `${x.customer_name} (${x.status === "done" ? "đã xong" : "đã đến"})`).join(", ")}.`}
            </span>
          </fieldset>
        )}
        <div className="field">
          <label htmlFor="booking-note">Ghi chú lịch hẹn</label>
          <textarea
            id="booking-note"
            rows={2}
            maxLength={500}
            value={note}
            aria-describedby="booking-note-hint"
            onChange={(e) => {
              setNote(e.target.value);
            }}
            className="input"
          />
          <span id="booking-note-hint" className="field-hint">
            Không ghi thông tin sức khỏe ở đây (ghi ở hồ sơ khách).
          </span>
        </div>
        <p className="appt-recap">
          <strong>
            {formatDate(day)} · {hhmm(firstStart)}–{hhmm(lastEnd)}
          </strong>{" "}
          ({lastEnd - firstStart} phút)
          {servedCompanions > 0 ? ` · ${String(served.length)} người làm` : ""}
          {notes.length > 0
            ? ` · ${String(notes.length)} người đi cùng (ghi chú)`
            : ""}{" "}
          · chưa thanh toán (thu khi ghi lượt)
        </p>
        {/* Errors and warnings: pinned in the footer, by the button (`notice`). */}
      </form>
    </BottomSheet>
  );
}

type Change = (k: number, patch: Partial<Person>) => void;

/** " · Người đi cùng 2" after a companion's label: read out, not shown (the legend shows it). */
function Who({ legend }: { legend: string | null }) {
  return legend ? <span className="sr-only"> · {legend}</span> : null;
}

/**
 * Who the person is: a customer record, a newcomer (the customer) or a name / note (a companion).
 * The customer has one way to a newcomer, the search's "Khách mới" (it keeps what was typed), and
 * "Tìm khách có hồ sơ" back (F7: a "Khách mới" tab next to that button said the same twice). A
 * companion starts as a name or note, so the two-way switch stays there.
 */
function WhoFields({
  p,
  main,
  legend,
  id,
  booked,
  change,
}: {
  p: Person;
  main: boolean;
  legend: string;
  id: (what: string) => string;
  booked: boolean;
  change: Change;
}) {
  return (
    <>
      {main ? (
        p.who === "guest" && (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => {
              change(p.key, { who: "customer" });
            }}
          >
            Tìm khách có hồ sơ
          </button>
        )
      ) : (
        <div
          role="group"
          aria-label={`Loại khách ${legend}`}
          className="segmented justify-self-start"
        >
          <button
            type="button"
            aria-pressed={p.who === "customer"}
            onClick={() => {
              change(p.key, { who: "customer" });
            }}
          >
            Khách có hồ sơ
          </button>
          <button
            type="button"
            aria-pressed={p.who === "guest"}
            onClick={() => {
              change(p.key, { who: "guest" });
            }}
          >
            Chỉ ghi tên
          </button>
        </div>
      )}
      {p.who === "customer" &&
        (p.customer ? (
          <p className="appt-chosen-person">
            <strong>
              <CustomerName
                name={p.customer.full_name}
                gender={p.customer.gender}
              />
            </strong>
            {/* A profile without a phone (decision E) shows no empty hint. */}
            {p.customer.phone && <span className="field-hint tabular-nums">{p.customer.phone}</span>}
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                change(p.key, { customer: null });
              }}
            >
              Đổi khách
            </button>
          </p>
        ) : (
          <CustomerSearch
            onSelect={(c) => {
              change(p.key, { customer: c });
            }}
            onCreate={(typed) => {
              change(
                p.key,
                /\d/.test(typed)
                  ? { who: "guest", guestPhone: typed }
                  : { who: "guest", guestName: typed },
              );
            }}
          />
        ))}
      {p.who === "guest" && (
        <div className="appt-fields">
          <div className="field">
            <label htmlFor={id("guest-name")}>
              {main ? "Tên khách" : "Tên hoặc ghi chú"}
              <Who legend={main ? null : legend} />
            </label>
            <input
              id={id("guest-name")}
              className="input"
              value={p.guestName}
              maxLength={main ? 200 : 100}
              placeholder={main ? undefined : "Chồng của chị Hà"}
              autoComplete="off"
              autoFocus={!main && p.guestName === ""}
              onChange={(e) => {
                change(p.key, { guestName: e.target.value });
              }}
            />
          </div>
          {(main || booked) && (
            <div className="field">
              <label htmlFor={id("guest-phone")}>
                {main ? "Số điện thoại" : "SĐT (không bắt buộc)"}
                <Who legend={main ? null : legend} />
              </label>
              <input
                id={id("guest-phone")}
                className="input"
                inputMode="tel"
                autoComplete="off"
                value={p.guestPhone}
                onChange={(e) => {
                  change(p.key, { guestPhone: e.target.value });
                }}
              />
            </div>
          )}
        </div>
      )}
    </>
  );
}

/** The person's services: picking is adding; an untimed add-on next to a timed one is "kèm". */
function ServiceFields({
  p,
  main,
  suffix,
  id,
  groups,
  loading,
  serviceOf,
  editing,
  change,
}: {
  p: Person;
  main: boolean;
  suffix: string;
  id: (what: string) => string;
  groups: { c: ServiceCategory; items: Service[] }[];
  loading: boolean;
  serviceOf: (id: string) => Service | undefined;
  editing: Appointment | null;
  change: Change;
}) {
  const booked = (sid: string) =>
    main ? editing?.services.find((s) => s.service_id === sid) : undefined;
  const chosen = p.serviceIds.map((sid) => ({
    id: sid,
    name: serviceOf(sid)?.name ?? booked(sid)?.service_name ?? "",
    duration: serviceOf(sid)?.duration_min ?? booked(sid)?.duration_min ?? null,
  }));
  const chipMinutes = (d: number | null) => {
    const m = addedMinutes(
      d,
      chosen.map((c) => c.duration),
    );
    return m ? `${String(m)} phút` : "kèm";
  };
  return (
    <div className="field">
      {/* The group's visible label; the picker itself is named "Thêm dịch vụ". */}
      <span>{main ? "Dịch vụ" : "Dịch vụ (không bắt buộc)"}</span>
      {chosen.length > 0 && (
        <ul className="appt-services" aria-label={`Dịch vụ đã chọn${suffix}`}>
          {chosen.map((s, j) => (
            <li key={`${s.id}-${String(j)}`}>
              <span>
                {s.name}
                <span className="appt-minutes">{chipMinutes(s.duration)}</span>
              </span>
              <button
                type="button"
                className="btn btn-ghost btn-icon"
                aria-label={`Bỏ ${s.name}${suffix}`}
                onClick={() => {
                  change(p.key, {
                    serviceIds: p.serviceIds.filter((_, x) => x !== j),
                  });
                }}
              >
                <Icon name="x" />
              </button>
            </li>
          ))}
        </ul>
      )}
      {p.serviceIds.length < MAX_SERVICES && (
        <select
          id={id("service")}
          aria-label={`Thêm dịch vụ${suffix}`}
          className="input"
          value=""
          aria-describedby={id("service-hint")}
          onChange={(e) => {
            // Picking is adding (round 5: a pick that waited for "Thêm" was lost).
            const picked = e.target.value;
            if (picked)
              change(p.key, { serviceIds: [...p.serviceIds, picked] });
          }}
        >
          <option value="">
            {loading
              ? "Đang tải bảng giá…"
              : chosen.length > 0
                ? "+ Thêm dịch vụ khác…"
                : "Chọn dịch vụ…"}
          </option>
          {groups
            .filter((g) => g.items.length > 0)
            .map((g) => (
              <optgroup key={g.c.id} label={g.c.name}>
                {g.items.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                    {s.duration_min ? ` · ${String(s.duration_min)} phút` : ""}
                  </option>
                ))}
              </optgroup>
            ))}
        </select>
      )}
      <span id={id("service-hint")} className="field-hint">
        {main || chosen.length > 0
          ? `Chọn là thêm, tối đa ${String(MAX_SERVICES)} dịch vụ. Dịch vụ không có thời lượng làm kèm, không cộng giờ.`
          : "Chưa chọn dịch vụ: chỉ ghi chú trên lịch hẹn, không làm, không tính tiền."}
      </span>
    </div>
  );
}

/** Length of one person's booking: follows the services until set by hand. */
function LengthField({
  p,
  auto,
  minutes,
  slot,
  label,
  id,
  change,
}: {
  p: Person;
  auto: number;
  minutes: number;
  slot: number;
  label: ReactNode;
  id: (what: string) => string;
  change: Change;
}) {
  return (
    <div className="field">
      <label htmlFor={id("length")}>{label}</label>
      <select
        id={id("length")}
        className="input"
        value={minutes}
        aria-describedby={id("length-hint")}
        onChange={(e) => {
          const m = Number(e.target.value);
          change(p.key, { duration: m === auto ? null : m });
        }}
      >
        {/* Exact lengths (decision A): 70 sits between 60 and 75 instead of becoming 75. */}
        {lengthChoices(slot, auto, minutes).map((m) => (
          <option key={m} value={m}>
            {m} phút{m === auto ? " (theo dịch vụ)" : ""}
          </option>
        ))}
      </select>
      <span id={id("length-hint")} className="field-hint" aria-live="polite">
        {p.duration === null
          ? `Theo dịch vụ: ${String(auto)} phút`
          : `Đã chỉnh tay · theo dịch vụ: ${String(auto)} phút`}
      </span>
      {p.duration !== null && (
        <button
          type="button"
          className="btn btn-ghost justify-self-start"
          onClick={() => {
            change(p.key, { duration: null });
          }}
        >
          Dùng lại theo dịch vụ
        </button>
      )}
    </div>
  );
}
