import type {
  Appointment,
  Calendar,
  CustomerSummary,
  Service,
} from "@vpyd/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
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
  companionNotes,
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

/** Most people in one booking (server: 2–6 for a group). */
export const MAX_PEOPLE = 6;
const MAX_SERVICES = 5;

/** One person of the booking form. `duration` null = follows the services (spec §10.2). */
export interface Person {
  key: number;
  who: "customer" | "guest";
  customer: CustomerSummary | null;
  guestName: string;
  guestPhone: string;
  serviceIds: string[];
  staffId: string;
  duration: number | null;
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
  };
}

/** "Người 2 (Chị Mai)" — how errors and hints name a person. */
function personLabel(p: Person, i: number): string {
  const name =
    p.who === "customer" ? p.customer?.full_name : p.guestName.trim() || null;
  return `Người ${String(i + 1)}${name ? ` (${name})` : ""}`;
}

/** What still stops the form from being sent, in words (never a silent no-op, spec §10.5). */
export function missingParts(people: Person[], editing: boolean): string[] {
  const out: string[] = [];
  people.forEach((p, i) => {
    const who = people.length > 1 ? ` của ${personLabel(p, i)}` : "";
    if (!editing) {
      if (p.who === "customer" && p.customer === null) out.push(`khách${who}`);
      if (p.who === "guest" && p.guestName.trim() === "")
        out.push(`tên khách mới${who}`);
      if (p.who === "guest" && p.guestPhone.trim() === "")
        out.push(`SĐT khách mới${who}`);
    }
    if (p.serviceIds.length === 0) out.push(`dịch vụ${who}`);
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

/** Every server refusal as one Vietnamese sentence, naming the person when it is a group's. */
export function bookingErrorText(
  error: unknown,
  cal: Calendar,
  people: Person[],
): string {
  if (!(error instanceof ApiError)) return messageFor(error);
  if (error.code === "appointment.staff_busy") {
    const other = cal.appointments.find((x) => x.id === error.existingId);
    if (other) {
      const s = spaMinutes(other.starts_at);
      return `${other.staff_name ?? "KTV"} đã có lịch ${hhmm(s)}–${hhmm(s + other.duration_min)} (${other.customer_name}). Chọn giờ hoặc KTV khác.`;
    }
    return people.length > 1
      ? "Hai người trong nhóm đang chọn cùng một KTV trùng giờ, hoặc KTV đã có lịch. Chọn KTV khác."
      : messageFor(error);
  }
  if (error.code === "appointment.outside_opening_hours")
    return `${messageFor(error)} Giờ mở cửa ${cal.opens_at}–${cal.closes_at}.`;
  if (error.code === "validation_failed" && error.errors.length > 0) {
    return error.errors
      .map((f) => {
        const m = /^people\[(\d+)\]\.(.+)$/.exec(f.field);
        const field = m?.[2] ?? f.field;
        const person = m ? people[Number(m[1])] : undefined;
        const who = person ? `${personLabel(person, Number(m?.[1]))}: ` : "";
        const along = /^companions\[(\d+)\]$/.exec(field);
        const name = along
          ? `người đi cùng ${String(Number(along[1]) + 1)}`
          : (FIELD_NAMES[field] ?? field);
        const text =
          f.code === "duplicate" && field === "customer_id"
            ? "khách này đã có trong nhóm."
            : f.code === "count_out_of_range" && field === "people"
              ? `nhóm phải có 2–${String(MAX_PEOPLE)} người.`
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
 * "Đặt lịch hẹn" (spec appointments.md §2, §10.2–10.5): one or more people ("+ Thêm người", a
 * family) at the same day and time, each with their own services, technician and length. Picking
 * a service adds it at once and the length follows the services until the desk sets it by hand.
 * One person → POST /appointments; a group → POST /appointments/group (all or nobody). Editing an
 * appointment is the same form for its one person. Every rule is the server's; this previews.
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
  const nextKey = useRef(1);
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
  // People who come along, known only by a note (round 6 Q-U5): never served or charged.
  const [companions, setCompanions] = useState<string[]>([]);
  const [attempted, setAttempted] = useState(false);
  // One key per booking, kept across retries so a resend never books twice.
  const [key] = useState(() => crypto.randomUUID());
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
  const longest = Math.max(...people.map(minutesOfPerson));

  const change = (k: number, patch: Partial<Person>) => {
    setPeople((ps) => ps.map((p) => (p.key === k ? { ...p, ...patch } : p)));
  };

  const missing = missingParts(people, editing !== null);
  // Everyone starts together, so one technician on two people always overlaps.
  const clashing = people.filter(
    (p, i) =>
      p.staffId !== "" &&
      people.some((q, j) => j < i && q.staffId === p.staffId),
  );
  const inRange = day >= cal.from && day <= cal.to;
  const max = cal.max_per_slot ?? null;
  const already = inRange ? peakPeople(cal, day, start, longest) : 0;
  const overMax =
    max !== null && inRange && already + (editing ? 0 : people.length) > max;

  const save = useMutation({
    mutationFn: async (): Promise<Appointment> => {
      const starts_at = instantAt(day, start);
      const [first] = people;
      if (!first) throw new Error("no person");
      if (editing)
        return updateAppointment(editing, {
          starts_at,
          duration_min: minutesOfPerson(first),
          staff_id: first.staffId || null,
          service_ids: first.serviceIds,
          note: note.trim() || null,
        });
      const person = (p: Person) => ({
        ...(p.who === "customer"
          ? { customer_id: p.customer?.id ?? null }
          : { guest: { full_name: p.guestName, phone: p.guestPhone } }),
        service_ids: p.serviceIds,
        staff_id: p.staffId || null,
        duration_min: minutesOfPerson(p),
      });
      const along = companionNotes(companions);
      const withCompanions = along.length > 0 ? { companions: along } : {};
      if (people.length === 1)
        return bookAppointment(
          {
            ...person(first),
            starts_at,
            note: note.trim() || null,
            ...withCompanions,
          },
          key,
        );
      const group = await bookGroup(
        {
          starts_at,
          note: note.trim() || null,
          people: people.map(person),
          ...withCompanions,
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
              : people.length > 1
                ? `Đặt lịch ${String(people.length)} người`
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
          const chosen = p.serviceIds.map((id) => ({
            id,
            name:
              serviceOf(id)?.name ??
              editing?.services.find((s) => s.service_id === id)
                ?.service_name ??
              "",
            duration:
              serviceOf(id)?.duration_min ??
              editing?.services.find((s) => s.service_id === id)
                ?.duration_min ??
              null,
          }));
          const auto = autoOf(p);
          // An untimed add-on next to a timed service runs meanwhile: "(kèm)", not "(60′)".
          const chipMinutes = (d: number | null) => {
            const m = addedMinutes(
              d,
              chosen.map((c) => c.duration),
            );
            return m ? `${String(m)} phút` : "kèm";
          };
          const many = people.length > 1;
          const legend = many ? `Người ${String(i + 1)}` : "Khách";
          const suffix = many ? ` · ${legend}` : "";
          const id = (what: string) => `booking-${what}-${String(p.key)}`;
          return (
            <fieldset key={p.key} className="appt-person">
              <legend className="appt-person-head">
                <span>{legend}</span>
                {many && (
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={() => {
                      setPeople((ps) => ps.filter((x) => x.key !== p.key));
                    }}
                  >
                    Bỏ người này
                  </button>
                )}
              </legend>
              {!editing && (
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
                      Khách mới
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
                        <span className="field-hint tabular-nums">
                          {p.customer.phone}
                        </span>
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
                        <label htmlFor={id("guest-name")}>Tên khách</label>
                        <input
                          id={id("guest-name")}
                          className="input"
                          value={p.guestName}
                          maxLength={200}
                          autoComplete="off"
                          onChange={(e) => {
                            change(p.key, { guestName: e.target.value });
                          }}
                        />
                      </div>
                      <div className="field">
                        <label htmlFor={id("guest-phone")}>Số điện thoại</label>
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
                    </div>
                  )}
                </>
              )}

              <div className="field">
                {/* The group's visible label; the picker itself is named "Thêm dịch vụ". */}
                <span>Dịch vụ</span>
                {chosen.length > 0 && (
                  <ul
                    className="appt-services"
                    aria-label={`Dịch vụ đã chọn${suffix}`}
                  >
                    {chosen.map((s, j) => (
                      <li key={`${s.id}-${String(j)}`}>
                        <span>
                          {s.name}
                          <span className="appt-minutes">
                            {chipMinutes(s.duration)}
                          </span>
                        </span>
                        <button
                          type="button"
                          className="btn btn-ghost btn-icon"
                          aria-label={`Bỏ ${s.name}`}
                          onClick={() => {
                            change(p.key, {
                              serviceIds: p.serviceIds.filter(
                                (_, x) => x !== j,
                              ),
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
                        change(p.key, {
                          serviceIds: [...p.serviceIds, picked],
                        });
                    }}
                  >
                    <option value="">
                      {allServices.isPending
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
                              {s.duration_min
                                ? ` · ${String(s.duration_min)} phút`
                                : ""}
                            </option>
                          ))}
                        </optgroup>
                      ))}
                  </select>
                )}
                <span id={id("service-hint")} className="field-hint">
                  Chọn là thêm, tối đa {MAX_SERVICES} dịch vụ. Dịch vụ không có
                  thời lượng làm kèm, không cộng giờ.
                </span>
              </div>

              <div className="appt-fields">
                <div className="field">
                  <label htmlFor={id("length")}>Thời lượng{suffix}</label>
                  <select
                    id={id("length")}
                    className="input"
                    value={minutesOfPerson(p)}
                    aria-describedby={id("length-hint")}
                    onChange={(e) => {
                      const m = Number(e.target.value);
                      change(p.key, { duration: m === auto ? null : m });
                    }}
                  >
                    {Array.from(
                      { length: 480 / slot },
                      (_, x) => (x + 1) * slot,
                    ).map((m) => (
                      <option key={m} value={m}>
                        {m} phút{m === auto ? " (theo dịch vụ)" : ""}
                      </option>
                    ))}
                  </select>
                  <span
                    id={id("length-hint")}
                    className="field-hint"
                    aria-live="polite"
                  >
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
                <div className="field">
                  <label htmlFor={id("staff")}>
                    KTV (không bắt buộc){suffix}
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
              </div>
            </fieldset>
          );
        })}

        {!editing && people.length < MAX_PEOPLE && (
          <button
            type="button"
            className="btn btn-secondary justify-self-start"
            onClick={() => {
              setPeople((ps) => [...ps, blank(nextKey.current++)]);
            }}
          >
            <Icon name="plus" /> Thêm người (gia đình, nhóm)
          </button>
        )}

        {!editing && (
          <div className="appt-companions">
            {companions.map((c, j) => (
              <div key={j} className="appt-companion">
                <div className="field">
                  <label htmlFor={`booking-companion-${String(j)}`}>
                    Người đi cùng {j + 1}
                  </label>
                  <input
                    id={`booking-companion-${String(j)}`}
                    className="input"
                    value={c}
                    maxLength={100}
                    placeholder="Chồng của chị Hà"
                    autoComplete="off"
                    autoFocus={j === companions.length - 1 && c === ""}
                    onChange={(e) => {
                      const text = e.target.value;
                      setCompanions((list) =>
                        list.map((x, k) => (k === j ? text : x)),
                      );
                    }}
                  />
                </div>
                <button
                  type="button"
                  className="btn btn-ghost btn-icon"
                  aria-label={`Bỏ người đi cùng ${String(j + 1)}`}
                  onClick={() => {
                    setCompanions((list) => list.filter((_, k) => k !== j));
                  }}
                >
                  <Icon name="x" />
                </button>
              </div>
            ))}
            {companions.length < MAX_COMPANIONS && (
              <button
                type="button"
                className="btn btn-ghost justify-self-start"
                onClick={() => {
                  setCompanions((list) => [...list, ""]);
                }}
              >
                <Icon name="plus" /> Người đi cùng (ghi chú)
              </button>
            )}
            {companions.length > 0 && (
              <span className="field-hint">
                Chỉ ghi chú trên lịch hẹn: không chọn dịch vụ, không tính tiền.
              </span>
            )}
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
            <label htmlFor="booking-start">
              Giờ hẹn{people.length > 1 ? " (cả nhóm)" : ""}
            </label>
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
            {formatDate(day)} · {hhmm(start)}–{hhmm(start + longest)}
          </strong>{" "}
          ({longest} phút)
          {people.length > 1 ? ` · ${String(people.length)} người` : ""}
          {companionNotes(companions).length > 0
            ? ` · ${String(companionNotes(companions).length)} người đi cùng`
            : ""}{" "}
          · chưa thanh toán (thu khi ghi lượt)
        </p>
        {clashing.length > 0 && (
          <p role="status" className="callout">
            {clashing.map((p) => personLabel(p, people.indexOf(p))).join(", ")}{" "}
            đang cùng KTV với người khác trong nhóm, cùng giờ: chọn KTV khác
            hoặc để “Chưa gán KTV”.
          </p>
        )}
        {overMax && (
          <p role="status" className="callout">
            Khung {hhmm(start)} đã có {already}/{max} khách
            {editing
              ? ""
              : `; thêm ${String(people.length)} người sẽ vượt mức tối đa`}
            . Vẫn đặt được nếu khách chịu chờ.
          </p>
        )}
        {attempted && missing.length > 0 && (
          <p ref={missingRef} tabIndex={-1} role="alert" className="field-error">
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
            {bookingErrorText(save.error, cal, people)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
