import type {
  Appointment,
  Calendar,
  CustomerSummary,
  Service,
  ServiceCategory,
} from "@vpyd/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type ReactNode } from "react";
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
  hhmm,
  instantAt,
  minutesOf,
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
    const auto = autoMinutes(
      editing.services.map((s) => s.duration_min),
      slot,
    );
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
  // Editing runs several requests: what already went through is not sent again on a retry.
  const patched = useRef<Appointment | null>(null);
  const missingRef = useRef<HTMLParagraphElement | null>(null);

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
      slot,
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
  const inRange = day >= cal.from && day <= cal.to;
  const max = cal.max_per_slot ?? null;
  const already = inRange
    ? peakPeople(cal, day, firstStart, lastEnd - firstStart)
    : 0;
  const adding = editing ? served.length - 1 : served.length;
  const overMax = max !== null && inRange && already + adding > max;

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
        patched.current ??= await updateAppointment(editing, {
          starts_at,
          duration_min: minutesOfPerson(first),
          staff_id: first.staffId || null,
          service_ids: first.serviceIds,
          note: note.trim() || null,
          companions: notes,
        });
        for (const p of others)
          await bookAppointment(
            {
              ...person(p),
              starts_at: instantAt(day, startOf(p)),
              group_with: editing.id,
            },
            p.bookingKey,
          );
        return patched.current;
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
  /** The newcomer whose phone already has a record becomes that customer. */
  const adoptExisting = async (id: string) => {
    const c = await queryClient.query(customerQuery(id));
    const digits = (s: string) => s.replace(/\D/g, "").replace(/^84/, "0");
    const guest =
      people.find(
        (p) => p.who === "guest" && digits(p.guestPhone) === digits(c.phone),
      ) ?? people.find((p) => p.who === "guest");
    if (guest)
      change(guest.key, {
        who: "customer",
        customer: {
          id: c.id,
          full_name: c.full_name,
          phone: c.phone,
          gender: c.gender ?? null,
        },
      });
    save.reset();
  };

  const submit = () => {
    setAttempted(true);
    if (missing.length > 0) {
      missingRef.current?.focus();
      return;
    }
    if (!save.isPending) save.mutate();
  };

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

  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={editing ? `Đổi lịch · ${editing.customer_name}` : "Đặt lịch hẹn"}
      footer={
        <>
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
          return (
            <fieldset key={p.key} className="appt-person">
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
                      aria-invalid={clashing.includes(p) || undefined}
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
          <DateField
            label="Ngày hẹn"
            value={day}
            min={spaToday()}
            onChange={(iso) => {
              if (iso) setDay(iso);
            }}
          />
          <div className="field">
            <label htmlFor="booking-start">Giờ hẹn</label>
            <select
              id="booking-start"
              className="input"
              value={start}
              onChange={(e) => {
                setStart(Number(e.target.value));
              }}
            >
              {startOptions.map((m) => {
                const n = inRange ? peopleAt(cal, day, m) : 0;
                const tail =
                  max !== null && inRange
                    ? ` · ${String(n)}/${String(max)} khách`
                    : n > 0
                      ? ` · ${String(n)} khách`
                      : "";
                return (
                  <option key={m} value={m}>
                    {hhmm(m)}
                    {tail}
                  </option>
                );
              })}
            </select>
          </div>
        </div>
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
        {clashing.length > 0 && (
          <p role="status" className="callout">
            {clashing.map((p) => personLabel(p, people.indexOf(p))).join(", ")}{" "}
            đang cùng KTV với người khác trong lịch này, trùng giờ: chọn KTV
            khác, đổi giờ bắt đầu hoặc để “Chưa gán KTV”.
          </p>
        )}
        {overMax && (
          <p role="status" className="callout">
            Khung {hhmm(firstStart)} đã có {already}/{max} khách
            {adding > 0
              ? `; thêm ${String(adding)} người sẽ vượt mức tối đa`
              : ""}
            . Vẫn đặt được nếu khách chịu chờ.
          </p>
        )}
        {attempted && missing.length > 0 && (
          <p
            ref={missingRef}
            tabIndex={-1}
            role="alert"
            className="field-error"
          >
            Còn thiếu: {missing.join(", ")}.
          </p>
        )}
        {taken && (
          <p role="alert" className="button-row field-error">
            Số điện thoại của khách mới đã có hồ sơ khách.
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                void adoptExisting(taken);
              }}
            >
              Dùng hồ sơ này
            </button>
          </p>
        )}
        {save.isError && !taken && (
          <p role="alert" className="field-error">
            {bookingErrorText(
              save.error,
              cal,
              editing ? served.slice(1) : served,
              editing ? sentLabels.slice(1) : sentLabels,
            )}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}

type Change = (k: number, patch: Partial<Person>) => void;

/** " · Người đi cùng 2" after a companion's label: read out, not shown (the legend shows it). */
function Who({ legend }: { legend: string | null }) {
  return legend ? <span className="sr-only"> · {legend}</span> : null;
}

/** Who the person is: a customer record, a newcomer (the customer) or a name / note (a companion). */
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
          {main ? "Khách mới" : "Chỉ ghi tên"}
        </button>
      </div>
      {p.who === "customer" &&
        (p.customer ? (
          <p className="appt-chosen-person">
            <strong>
              <CustomerName
                name={p.customer.full_name}
                gender={p.customer.gender}
              />
            </strong>
            <span className="field-hint tabular-nums">{p.customer.phone}</span>
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
        {Array.from({ length: 480 / slot }, (_, x) => (x + 1) * slot).map(
          (m) => (
            <option key={m} value={m}>
              {m} phút{m === auto ? " (theo dịch vụ)" : ""}
            </option>
          ),
        )}
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
