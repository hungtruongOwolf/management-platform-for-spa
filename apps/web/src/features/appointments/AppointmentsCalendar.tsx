import type {
  Appointment,
  Calendar,
  CalendarVisit,
  TimelinePhase,
} from "@vpyd/api-client";
import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import "./calendar.css";
import { CustomerName } from "../../app/CustomerName";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { StickyActions } from "../../components/StickyActions";
import { usePhone } from "../../components/useMediaQuery";
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { useSearchState } from "../../lib/searchState";
import { serviceLabel } from "../../lib/names";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import type { CartLine } from "../front-desk/api";
import { cartFromAppointment } from "../front-desk/prefill";
import { frontDeskSearch } from "../front-desk/search";
import { activeStaffQuery, formatDate } from "../sales/api";
import { BookingSheet, type Draft } from "./BookingSheet";
import { GroupCheckIn } from "./GroupCheckIn";
import {
  type Entry,
  PHASE_LABEL,
  arriveAppointment,
  calendarQuery,
  closeAppointment,
  counts,
  daysFrom,
  entriesOf,
  hhmm,
  isOpen,
  minutesOf,
  mondayOf,
  paymentLabel,
  peakPeople,
  peopleAt,
  saveSlotLimit,
  spaMinutes,
  updateAppointment,
  view,
} from "./api";

const CustomerForm = lazy(() =>
  import("../customers/CustomerForm").then((m) => ({
    default: m.CustomerForm,
  })),
);

const WEEKDAY = ["CN", "Thứ 2", "Thứ 3", "Thứ 4", "Thứ 5", "Thứ 6", "Thứ 7"];

/** "2026-11-06" → "Thứ 6, 06/11". */
function dayTitle(day: string): string {
  const d = new Date(`${day}T00:00:00Z`);
  return `${WEEKDAY[d.getUTCDay()] ?? ""}, ${formatDate(day).slice(0, 5)}`;
}

function range(start: number, minutes: number): string {
  return `${hhmm(start)}–${hhmm(start + minutes)}`;
}

function servicesText(
  list: { service_name: string; category_name: string }[],
): string {
  return list
    .map((s) => serviceLabel(s.service_name, s.category_name))
    .join(", ");
}

function PhaseBadge({ phase }: { phase: TimelinePhase }) {
  return (
    <span className="appt appt-badge" data-phase={phase}>
      {PHASE_LABEL[phase]}
    </span>
  );
}

/** "Vãng lai" marks a visit recorded without a booking; "Nhóm · 3" a family booking. */
function entryTag(e: Entry, cal: Calendar): string | null {
  if (e.kind === "visit") return "Vãng lai";
  const g = e.a.group_id;
  if (!g) return null;
  const n = cal.appointments.filter((x) => x.group_id === g).length;
  return `Nhóm · ${String(n)} người`;
}

/**
 * The appointments tab of /quay (spec appointments.md §7, §10): the desk's timeline. Bookings and
 * visits recorded without one (walk-ins) on one grid — technicians as columns, who is waiting, in
 * service or done — a week overview, and a list (the phone layout). Click an empty slot to book,
 * a card to open it. Every rule (overlap, hours, closed days) is the server's; this only previews.
 */
export function AppointmentsCalendar({
  onComplete,
}: {
  /** Opens the visit cart for this appointment, filled with its services and technician. */
  onComplete: (a: Appointment, lines: CartLine[]) => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const [search, setSearch] = useSearchState(frontDeskSearch);
  const phone = usePhone();
  const today = spaToday();
  const day = search.ngay || today;
  const layout = phone && search.kieu === "ngay" ? "ds" : search.kieu;
  const week = mondayOf(day);
  const [from, to] =
    layout === "tuan" ? [week, daysFrom(week, 7)[6] ?? week] : [day, day];
  const showsToday = from <= today && today <= to;
  const cal = useQuery({
    ...calendarQuery(from, to),
    placeholderData: keepPreviousData,
    // "Đang làm" turns into "Xong" with the clock (phases are derived on every read).
    refetchInterval: showsToday ? 60_000 : false,
  });
  const [openId, setOpenId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  // Frozen when opened: members leave the "open" list as their visits are recorded.
  const [groupMembers, setGroupMembers] = useState<Appointment[]>([]);
  const [limitOpen, setLimitOpen] = useState(false);
  const isOwner = can(me, "record.void_closed_day");
  const locked = day < today && !isOwner;

  const opened = cal.data
    ? (entriesOf(cal.data).find((e) => e.id === openId) ?? null)
    : null;
  const shift = (days: number) => {
    const d = new Date(`${day}T00:00:00Z`);
    d.setUTCDate(d.getUTCDate() + days);
    const iso = d.toISOString().slice(0, 10);
    setSearch({ ngay: iso === today ? "" : iso });
  };
  const step = layout === "tuan" ? 7 : 1;
  const startFrom = (a: Appointment) => {
    setOpenId(null);
    void cartFromAppointment(queryClient, a)
      .catch(() => [])
      .then((lines) => {
        onComplete(a, lines);
      });
  };
  const openGroup = (groupId: string) => {
    setOpenId(null);
    setGroupMembers(
      (cal.data?.appointments ?? []).filter(
        (a) => a.group_id === groupId && isOpen(a) && a.business_date <= today,
      ),
    );
  };

  return (
    <section
      aria-labelledby="cal-title"
      className="grid min-w-0 content-start gap-3"
    >
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="grid gap-1">
          <h2 id="cal-title" className="text-lg font-semibold">
            Lịch hẹn ·{" "}
            {layout === "tuan"
              ? `tuần ${formatDate(from).slice(0, 5)}–${formatDate(to).slice(0, 5)}`
              : dayTitle(day)}
          </h2>
          {cal.data && (
            <DaySummary cal={cal.data} day={layout === "tuan" ? null : day} />
          )}
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <div
            role="group"
            aria-label="Chọn ngày"
            className="flex items-end gap-1"
          >
            <button
              type="button"
              className="btn btn-secondary min-h-11 px-3"
              onClick={() => {
                shift(-step);
              }}
              aria-label={layout === "tuan" ? "Tuần trước" : "Ngày trước"}
            >
              <Icon name="chevron-left" />
            </button>
            <DateField
              label="Ngày"
              value={day}
              onChange={(iso) => {
                setSearch({ ngay: iso && iso !== today ? iso : "" });
              }}
            />
            <button
              type="button"
              className="btn btn-secondary min-h-11 px-3"
              onClick={() => {
                shift(step);
              }}
              aria-label={layout === "tuan" ? "Tuần sau" : "Ngày sau"}
            >
              <Icon name="chevron-right" />
            </button>
            {day !== today && (
              <button
                type="button"
                className="btn btn-secondary min-h-11"
                onClick={() => {
                  setSearch({ ngay: "" });
                }}
              >
                Hôm nay
              </button>
            )}
          </div>
          <div role="group" aria-label="Kiểu xem" className="segmented">
            {(phone
              ? ([
                  ["ds", "Danh sách"],
                  ["tuan", "Tuần"],
                ] as const)
              : ([
                  ["ngay", "Ngày"],
                  ["tuan", "Tuần"],
                  ["ds", "Danh sách"],
                ] as const)
            ).map(([k, label]) => (
              <button
                key={k}
                type="button"
                aria-pressed={layout === k}
                onClick={() => {
                  setSearch({ kieu: k === "ds" && phone ? "ngay" : k });
                }}
              >
                {label}
              </button>
            ))}
          </div>
          {can(me, "appointment.settings") && (
            <button
              type="button"
              className="btn btn-secondary min-h-11"
              onClick={() => {
                setLimitOpen(true);
              }}
            >
              Số khách tối đa
              {cal.data?.max_per_slot
                ? `: ${String(cal.data.max_per_slot)}`
                : ""}
            </button>
          )}
        </div>
      </div>
      {locked && (
        <p className="rounded-[10px] bg-surface-2 p-2 text-sm text-text-muted">
          Ngày đã chốt: chỉ xem. Chủ spa sửa và nhập bù được.
        </p>
      )}
      {cal.isPending && <div aria-hidden="true" className="h-64 skeleton" />}
      {cal.isError && (
        <p role="alert" className="text-danger">
          {messageFor(cal.error)}{" "}
          <button
            type="button"
            className="underline"
            onClick={() => void cal.refetch()}
          >
            Thử lại
          </button>
        </p>
      )}
      {cal.data && (
        <div className={cal.isPlaceholderData ? "is-refreshing" : ""}>
          {layout === "ngay" && (
            <DayGrid
              cal={cal.data}
              day={day}
              canBook={!locked}
              onOpen={setOpenId}
              onBook={(start, staffId) => {
                setDraft({ mode: "new", day, start, staffId });
              }}
            />
          )}
          {layout === "tuan" && (
            <WeekGrid
              cal={cal.data}
              from={from}
              onOpen={setOpenId}
              onDay={(d) => {
                setSearch({ ngay: d === today ? "" : d, kieu: "ngay" });
              }}
            />
          )}
          {layout === "ds" && (
            <DayList cal={cal.data} day={day} onOpen={setOpenId} />
          )}
        </div>
      )}
      {!locked && (
        <StickyActions label="Đặt lịch">
          <button
            type="button"
            className="btn btn-primary min-h-11"
            onClick={() => {
              const now = spaMinutes(new Date().toISOString());
              const slot = cal.data?.slot_minutes ?? 15;
              const opens = minutesOf(cal.data?.opens_at ?? "09:00");
              const start =
                day === today
                  ? Math.max(opens, Math.ceil(now / slot) * slot)
                  : opens;
              setDraft({ mode: "new", day, start, staffId: "" });
            }}
          >
            <Icon name="plus" /> Đặt lịch
          </button>
        </StickyActions>
      )}

      {cal.data && opened?.kind === "appointment" && (
        <AppointmentSheet
          key={opened.id}
          a={opened.a}
          cal={cal.data}
          locked={opened.a.business_date < today && !isOwner}
          canCreateCustomer={can(me, "customer.write")}
          canWriteHealth={can(me, "customer.health_notes.write")}
          onClose={() => {
            setOpenId(null);
          }}
          onEdit={() => {
            setOpenId(null);
            setDraft({ mode: "edit", appointment: opened.a });
          }}
          onComplete={() => {
            startFrom(opened.a);
          }}
          onOpenOther={setOpenId}
          onGroupCheckIn={openGroup}
        />
      )}
      {opened?.kind === "visit" && (
        <VisitSheet
          v={opened.v}
          onClose={() => {
            setOpenId(null);
          }}
        />
      )}
      {cal.data && draft && (
        <BookingSheet
          cal={cal.data}
          draft={draft}
          onClose={() => {
            setDraft(null);
          }}
          onSaved={(a) => {
            setDraft(null);
            setOpenId(a.id);
          }}
        />
      )}
      {cal.data && groupMembers.length > 0 && (
        <GroupCheckIn
          cal={cal.data}
          members={groupMembers}
          onClose={() => {
            setGroupMembers([]);
          }}
          onDone={() => {
            setGroupMembers([]);
          }}
        />
      )}
      {limitOpen && cal.data && (
        <SlotLimitSheet
          current={cal.data.max_per_slot ?? null}
          onClose={() => {
            setLimitOpen(false);
          }}
        />
      )}
    </section>
  );
}

/** "Chưa đến 3 · Đang làm 2 · Xong 5" for the day (or the week). */
function DaySummary({ cal, day }: { cal: Calendar; day: string | null }) {
  const list = entriesOf(cal, day ?? undefined).map(view);
  const n = (...phases: TimelinePhase[]) =>
    list.filter((e) => phases.includes(e.phase)).length;
  const lost = n("no_show", "cancelled");
  return (
    <p className="text-sm text-text-muted" aria-live="polite">
      Chưa đến {n("waiting")} · Đang làm {n("in_service")} · Xong {n("done")}
      {lost > 0 ? ` · Hủy/không đến ${String(lost)}` : ""}
    </p>
  );
}

interface Laid {
  key: string;
  title: string;
  staffId: string;
  items: ReturnType<typeof view>[];
}

/** Greedy lanes: overlapping cards of one column sit side by side. */
function lanes(list: ReturnType<typeof view>[]): Map<string, number> {
  const ends: number[] = [];
  const lane = new Map<string, number>();
  for (const e of [...list].sort((x, y) => x.start - y.start)) {
    let i = ends.findIndex((end) => end <= e.start);
    if (i === -1) {
      i = ends.length;
      ends.push(0);
    }
    ends[i] = e.start + e.minutes;
    lane.set(e.id, i);
  }
  return lane;
}

/** "2 khách", or "2/4" against the owner's max; empty when nobody is there and no max is set. */
function slotCount(n: number, max: number | null): string {
  if (max !== null) return `${String(n)}/${String(max)}`;
  return n > 0 ? `${String(n)} khách` : "";
}

function DayGrid({
  cal,
  day,
  canBook,
  onOpen,
  onBook,
}: {
  cal: Calendar;
  day: string;
  canBook: boolean;
  onOpen: (id: string) => void;
  onBook: (start: number, staffId: string) => void;
}) {
  const opens = minutesOf(cal.opens_at);
  const closes = minutesOf(cal.closes_at);
  const slot = cal.slot_minutes;
  const rows = Math.max(1, Math.round((closes - opens) / slot));
  const entries = entriesOf(cal, day);
  const shown = entries
    .map(view)
    // Cancelled, missed and voided ones free their place; the list view still shows them.
    .filter((e) => counts(e.phase));
  const byId = new Map(entries.map((e) => [e.id, e]));
  const techIds = new Set(cal.technicians.map((t) => t.id));
  const groups: Laid[] = [
    ...cal.technicians.map((t) => ({
      key: t.id,
      title: t.full_name,
      staffId: t.id,
      items: shown.filter((e) => e.staffId === t.id),
    })),
    {
      key: "none",
      title: "Chưa gán KTV",
      staffId: "",
      items: shown.filter((e) => !e.staffId || !techIds.has(e.staffId)),
    },
  ];
  let col = 2;
  const laid = groups.map((g) => {
    const l = lanes(g.items);
    const width = Math.max(1, ...[...l.values()].map((v) => v + 1));
    const first = col;
    col += width;
    // Rows a card already covers get no slot button underneath (no stacked controls).
    const busy = new Set<number>();
    for (const e of g.items) {
      const s = Math.round((e.start - opens) / slot);
      for (let k = 0; k < Math.max(1, Math.round(e.minutes / slot)); k++)
        busy.add(s + k);
    }
    return { ...g, first, width, lane: l, busy };
  });
  const now = spaMinutes(new Date().toISOString());
  const isToday = day === spaToday();
  const past = (m: number) => day < spaToday() || (isToday && m + slot <= now);
  const max = cal.max_per_slot ?? null;

  return (
    <div className="cal-scroll">
      <div
        className="cal-grid"
        style={
          {
            "--cols": String(col - 2),
            "--rows": String(rows),
          } as React.CSSProperties
        }
        aria-label={`Lịch theo KTV ngày ${formatDate(day)}`}
        role="group"
      >
        <div
          className="cal-head cal-corner"
          style={{ gridColumn: 1, gridRow: 1 }}
        >
          <span
            className="text-xs text-text-muted"
            title={
              max === null
                ? "Giờ và số khách trong khung"
                : "Giờ và số khách / tối đa"
            }
          >
            Giờ
          </span>
        </div>
        {laid.map((g) => (
          <div
            key={g.key}
            className="cal-head"
            style={{
              gridColumn: `${String(g.first)} / span ${String(g.width)}`,
              gridRow: 1,
            }}
          >
            <span>{g.title}</span>
          </div>
        ))}
        {Array.from({ length: rows }, (_, r) => {
          const m = opens + r * slot;
          const hour = m % 60 === 0;
          // One number per hour (its busiest slot), not one per 15-minute row (EM round 5).
          const n = hour
            ? peakPeople(cal, day, m, Math.min(60, closes - m))
            : 0;
          const text = hour && (n > 0 || max !== null) ? slotCount(n, max) : "";
          const full = max !== null && n >= max;
          return (
            <div
              key={`g${String(r)}`}
              className={`cal-gutter ${hour ? "is-hour" : ""}`}
              style={{ gridColumn: 1, gridRow: r + 2 }}
            >
              <span>{hour ? hhmm(m) : ""}</span>
              {text && (
                <span
                  className={`cal-count ${full ? "is-full" : ""}`}
                  title={
                    full
                      ? "Đã đủ số khách tối đa"
                      : "Số khách đông nhất trong giờ này"
                  }
                >
                  <span className="sr-only">{hhmm(m)}: </span>
                  {text}
                </span>
              )}
            </div>
          );
        })}
        {laid.flatMap((g) =>
          Array.from({ length: rows }, (_, r) => {
            if (g.busy.has(r)) return null;
            const m = opens + r * slot;
            const disabled = !canBook || past(m);
            return (
              <button
                key={`${g.key}-${String(r)}`}
                type="button"
                tabIndex={-1}
                aria-label={`Đặt lịch ${hhmm(m)}${g.staffId ? ` với ${g.title}` : ""}`}
                disabled={disabled}
                className={`cal-cell ${m % 60 === 0 ? "is-hour" : ""} ${disabled ? "is-past" : ""}`}
                style={{
                  gridColumn: `${String(g.first)} / span ${String(g.width)}`,
                  gridRow: r + 2,
                }}
                onClick={() => {
                  onBook(m, g.staffId);
                }}
              />
            );
          }),
        )}
        {laid.flatMap((g) =>
          g.items.map((e) => {
            const s = Math.max(0, Math.round((e.start - opens) / slot));
            const span = Math.max(1, Math.round(e.minutes / slot));
            const entry = byId.get(e.id);
            const tag = entry ? entryTag(entry, cal) : null;
            return (
              <button
                key={e.id}
                type="button"
                className="appt cal-event"
                data-phase={e.phase}
                data-kind={e.kind}
                style={{
                  gridColumn: g.first + (g.lane.get(e.id) ?? 0),
                  gridRow: `${String(s + 2)} / span ${String(span)}`,
                }}
                onClick={() => {
                  onOpen(e.id);
                }}
                aria-label={`${range(e.start, e.minutes)} ${e.customerName}, ${PHASE_LABEL[e.phase]}${tag ? `, ${tag}` : ""}`}
              >
                <strong>{e.customerName}</strong>
                <span className="cal-event-phase">
                  {PHASE_LABEL[e.phase]}
                  {tag ? ` · ${tag}` : ""}
                </span>
                {span > 2 && <span>{servicesText(e.services)}</span>}
              </button>
            );
          }),
        )}
        {isToday && now >= opens && now <= closes && (
          <div
            className="cal-now"
            aria-hidden="true"
            style={{
              gridColumn: `1 / span ${String(col - 1)}`,
              gridRow: String(Math.floor((now - opens) / slot) + 2),
              top: `${String((((now - opens) % slot) / slot) * 26)}px`,
            }}
          />
        )}
      </div>
    </div>
  );
}

function WeekGrid({
  cal,
  from,
  onOpen,
  onDay,
}: {
  cal: Calendar;
  from: string;
  onOpen: (id: string) => void;
  onDay: (day: string) => void;
}) {
  const today = spaToday();
  const opens = minutesOf(cal.opens_at);
  const closes = minutesOf(cal.closes_at);
  const max = cal.max_per_slot ?? null;
  return (
    <div className="cal-week">
      {daysFrom(from, 7).map((d) => {
        const items = entriesOf(cal, d);
        const live = items.map(view).filter((e) => counts(e.phase));
        let full = 0;
        if (max !== null)
          for (let m = opens; m < closes; m += cal.slot_minutes)
            if (peopleAt(cal, d, m) >= max) full += 1;
        return (
          <div key={d} className={`cal-day ${d === today ? "is-today" : ""}`}>
            <button
              type="button"
              className="cal-day-head"
              onClick={() => {
                onDay(d);
              }}
              aria-label={`Xem ngày ${formatDate(d)}`}
            >
              <strong>{dayTitle(d)}</strong>
              <span className="text-xs text-text-muted">
                {live.length} khách
                {full > 0 ? ` · ${String(full)} khung đủ khách` : ""}
              </span>
            </button>
            {items.map((e) => {
              const v = view(e);
              return (
                <button
                  key={e.id}
                  type="button"
                  className="appt cal-chip"
                  data-phase={v.phase}
                  onClick={() => {
                    onOpen(e.id);
                  }}
                >
                  <span className="font-semibold tabular-nums">
                    {hhmm(v.start)} · {PHASE_LABEL[v.phase]}
                  </span>
                  <span>
                    {v.customerName}
                    {e.kind === "visit" ? " · Vãng lai" : ""}
                  </span>
                </button>
              );
            })}
            {items.length === 0 && (
              <p className="px-1 text-xs text-text-muted">Chưa có khách.</p>
            )}
          </div>
        );
      })}
    </div>
  );
}

function DayList({
  cal,
  day,
  onOpen,
}: {
  cal: Calendar;
  day: string;
  onOpen: (id: string) => void;
}) {
  const items = entriesOf(cal, day);
  if (items.length === 0)
    return (
      <p className="rounded-[14px] border border-border bg-surface p-4 text-text-muted">
        Chưa có khách ngày này. Bấm “Đặt lịch” để thêm.
      </p>
    );
  return (
    <ul className="cal-list" aria-label={`Lịch hẹn ngày ${formatDate(day)}`}>
      {items.map((e) => {
        const v = view(e);
        const tag = entryTag(e, cal);
        const staffName =
          e.kind === "appointment" ? e.a.staff_name : e.v.staff_name;
        const pay =
          e.kind === "appointment"
            ? paymentLabel(e.a, formatVnd)
            : visitPayment(e.v);
        return (
          <li key={e.id}>
            <button
              type="button"
              className="appt cal-card"
              data-phase={v.phase}
              onClick={() => {
                onOpen(e.id);
              }}
            >
              <span className="cal-card-time">{range(v.start, v.minutes)}</span>
              <span className="cal-card-main">
                <strong>
                  <CustomerName
                    name={v.customerName}
                    gender={
                      e.kind === "appointment"
                        ? e.a.customer_gender
                        : e.v.customer_gender
                    }
                  />
                </strong>
                <span className="text-sm text-text-muted">
                  {servicesText(v.services)}
                  {tag ? ` · ${tag}` : ""}
                </span>
                <span className="text-sm">
                  {staffName ? `KTV ${staffName}` : "Chưa gán KTV"} · {pay}
                </span>
              </span>
              <PhaseBadge phase={v.phase} />
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function visitPayment(v: CalendarVisit): string {
  switch (v.payment_state) {
    case "paid":
      return `Đã thanh toán ${formatVnd(v.paid_amount)}`;
    case "voided":
      return "Lượt đã hủy";
    default:
      return "Trừ liệu trình, không phát sinh tiền";
  }
}

// ------------------------------------------------------------------------- appointment sheet

function AppointmentSheet({
  a,
  cal,
  locked,
  canCreateCustomer,
  canWriteHealth,
  onClose,
  onEdit,
  onComplete,
  onOpenOther,
  onGroupCheckIn,
}: {
  a: Appointment;
  cal: Calendar;
  locked: boolean;
  canCreateCustomer: boolean;
  canWriteHealth: boolean;
  onClose: () => void;
  onEdit: () => void;
  onComplete: () => void;
  onOpenOther: (id: string) => void;
  onGroupCheckIn: (groupId: string) => void;
}) {
  const queryClient = useQueryClient();
  const staff = useQuery(activeStaffQuery);
  const [staffId, setStaffId] = useState(
    a.staff_id ?? cal.technicians[0]?.id ?? "",
  );
  const [closing, setClosing] = useState<"cancel" | "no-show" | null>(null);
  const [reason, setReason] = useState("");
  const [creating, setCreating] = useState(false);
  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: ["appointments"] });
  const action = useMutation({
    mutationFn: (run: () => Promise<Appointment>) => run(),
    onSuccess: async () => {
      setClosing(null);
      await refresh();
    },
  });
  const today = spaToday();
  const open = isOpen(a);
  const [openedAt] = useState(() => Date.now());
  const started = new Date(a.starts_at).getTime() <= openedAt;
  const start = spaMinutes(a.starts_at);
  const link = (customerId: string) => {
    setCreating(false);
    action.mutate(() => updateAppointment(a, { customer_id: customerId }));
  };
  const group = a.group_id
    ? cal.appointments.filter((x) => x.group_id === a.group_id)
    : [];
  const groupOpen = group.filter(isOpen);
  const canServe = open && !locked && a.business_date <= today;

  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`${range(start, a.duration_min)} · ${a.customer_name}`}
      description={
        <>
          {dayTitle(a.business_date)} · <PhaseBadge phase={a.phase} />
        </>
      }
      footer={
        open && !locked ? (
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={onEdit}
            >
              Đổi lịch
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={a.is_guest || !canServe}
              onClick={onComplete}
            >
              {a.status === "arrived" ? "Ghi lượt" : "Khách đến · ghi lượt"}
            </button>
          </>
        ) : (
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Đóng
          </button>
        )
      }
    >
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
        <dt className="text-text-muted">Khách</dt>
        <dd>
          <CustomerName name={a.customer_name} gender={a.customer_gender} />
          {a.is_guest ? " · khách mới, chưa có hồ sơ" : ""}
          {a.customer_id && (
            <>
              {" · "}
              <Link
                to="/khach-hang/$customerId"
                params={{ customerId: a.customer_id }}
                className="underline"
              >
                Mở hồ sơ
              </Link>
            </>
          )}
        </dd>
        <dt className="text-text-muted">SĐT</dt>
        <dd className="tabular-nums">
          {a.customer_phone.replace(/^\+84/, "0")}
        </dd>
        <dt className="text-text-muted">Dịch vụ</dt>
        <dd>
          {servicesText(a.services)} ({a.duration_min} phút)
        </dd>
        <dt className="text-text-muted">KTV</dt>
        <dd>{a.staff_name ?? "Chưa gán"}</dd>
        <dt className="text-text-muted">Thanh toán</dt>
        <dd>{paymentLabel(a, formatVnd)}</dd>
        {a.note && (
          <>
            <dt className="text-text-muted">Ghi chú</dt>
            <dd>{a.note}</dd>
          </>
        )}
        {a.status_reason && (
          <>
            <dt className="text-text-muted">Lý do</dt>
            <dd>{a.status_reason}</dd>
          </>
        )}
      </dl>

      {group.length > 1 && (
        <div className="mt-3 grid gap-2 rounded-[10px] border border-border p-3 text-sm">
          <p className="font-semibold">Cùng nhóm ({group.length} người)</p>
          <ul className="grid gap-1">
            {group.map((x) => (
              <li key={x.id} className="flex flex-wrap items-center gap-2">
                {x.id === a.id ? (
                  <strong>{x.customer_name}</strong>
                ) : (
                  <button
                    type="button"
                    className="underline"
                    onClick={() => {
                      onOpenOther(x.id);
                    }}
                  >
                    {x.customer_name}
                  </button>
                )}
                <PhaseBadge phase={x.phase} />
              </li>
            ))}
          </ul>
          {groupOpen.length > 1 && canServe && a.group_id && (
            <button
              type="button"
              className="btn btn-primary min-h-11 justify-self-start"
              onClick={() => {
                if (a.group_id) onGroupCheckIn(a.group_id);
              }}
            >
              Khách đến cả nhóm · ghi {groupOpen.length} lượt
            </button>
          )}
        </div>
      )}

      {locked && open && (
        <p className="mt-3 text-sm text-text-muted">
          Ngày đã chốt, liên hệ chủ spa.
        </p>
      )}

      {open && !locked && a.is_guest && (
        <div className="mt-3 grid gap-2 rounded-[10px] bg-warning-bg p-3 text-sm text-warning-text">
          <p>Khách mới: tạo hồ sơ (điền sẵn tên và SĐT) để ghi lượt được.</p>
          {canCreateCustomer && !creating && (
            <button
              type="button"
              className="btn btn-secondary justify-self-start"
              onClick={() => {
                setCreating(true);
              }}
            >
              Tạo hồ sơ khách
            </button>
          )}
          {creating && (
            <Suspense fallback={<p role="status">Đang mở biểu mẫu…</p>}>
              <div className="text-text">
                <CustomerForm
                  prefill={{
                    full_name: a.customer_name,
                    phone: a.customer_phone.replace(/^\+84/, "0"),
                  }}
                  canWriteHealth={canWriteHealth}
                  onSaved={(c) => {
                    link(c.id);
                  }}
                  onOpenExisting={link}
                  onCancel={() => {
                    setCreating(false);
                  }}
                />
              </div>
            </Suspense>
          )}
        </div>
      )}

      {open &&
        !locked &&
        a.status === "booked" &&
        a.business_date === today && (
          <div className="mt-3 flex flex-wrap items-end gap-2">
            <label className="grid gap-1 text-sm">
              <span className="font-semibold">KTV làm</span>
              <select
                className="min-h-11 rounded-[6px] border border-border bg-surface px-3"
                value={staffId}
                onChange={(e) => {
                  setStaffId(e.target.value);
                }}
              >
                <option value="">Chọn KTV…</option>
                {(staff.data ?? []).map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.full_name}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className="btn btn-secondary min-h-11"
              disabled={staffId === "" || action.isPending}
              onClick={() => {
                action.mutate(() => arriveAppointment(a.id, staffId));
              }}
            >
              Chỉ báo đã đến (ghi lượt sau)
            </button>
          </div>
        )}

      {open && !locked && (
        <div className="mt-3 grid gap-2">
          {closing === null ? (
            <div className="flex flex-wrap gap-2">
              {a.status === "booked" && started && (
                <button
                  type="button"
                  className="btn btn-secondary min-h-11"
                  onClick={() => {
                    setClosing("no-show");
                    setReason("");
                  }}
                >
                  Không đến
                </button>
              )}
              <button
                type="button"
                className="btn btn-secondary min-h-11 text-danger"
                onClick={() => {
                  setClosing("cancel");
                  setReason("");
                }}
              >
                Hủy lịch
              </button>
            </div>
          ) : (
            <form
              className="grid gap-2"
              onSubmit={(e) => {
                e.preventDefault();
                if (reason.trim() === "") return;
                const outcome = closing;
                action.mutate(() =>
                  closeAppointment(a.id, outcome, reason.trim()),
                );
              }}
            >
              <label className="grid gap-1 text-sm">
                <span className="font-semibold">
                  {closing === "cancel"
                    ? "Lý do hủy lịch"
                    : "Ghi chú không đến"}{" "}
                  (bắt buộc)
                </span>
                <input
                  autoFocus
                  value={reason}
                  maxLength={500}
                  onChange={(e) => {
                    setReason(e.target.value);
                  }}
                  className="min-h-11 rounded-[6px] border border-border bg-surface px-3"
                />
              </label>
              <div className="flex gap-2">
                <button
                  type="submit"
                  className="btn btn-primary min-h-11"
                  aria-disabled={reason.trim() === ""}
                >
                  {closing === "cancel" ? "Hủy lịch" : "Đánh dấu không đến"}
                </button>
                <button
                  type="button"
                  className="btn btn-secondary min-h-11"
                  onClick={() => {
                    setClosing(null);
                  }}
                >
                  Thôi
                </button>
              </div>
            </form>
          )}
        </div>
      )}
      {action.isError && (
        <p role="alert" className="mt-2 text-sm text-danger">
          {conflictMessage(action.error, cal)}
        </p>
      )}
    </BottomSheet>
  );
}

/** "KTV Lan đã có lịch 10:00–11:30 (Nguyễn Thị Hà)" for a 409 staff_busy; else the usual text. */
function conflictMessage(error: unknown, cal: Calendar): string {
  if (
    error instanceof ApiError &&
    error.code === "appointment.staff_busy" &&
    error.existingId
  ) {
    const other = cal.appointments.find((x) => x.id === error.existingId);
    if (other)
      return `${other.staff_name ?? "KTV"} đã có lịch ${range(spaMinutes(other.starts_at), other.duration_min)} (${other.customer_name}). Chọn KTV khác.`;
  }
  return messageFor(error);
}

/** A walk-in visit on the timeline: who, what, by whom, paid how (no session note here). */
function VisitSheet({ v, onClose }: { v: CalendarVisit; onClose: () => void }) {
  const start = spaMinutes(v.starts_at);
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`${range(start, v.duration_min)} · ${v.customer_name}`}
      description={
        <>
          {dayTitle(v.business_date)} · Vãng lai ·{" "}
          <PhaseBadge phase={v.phase} />
        </>
      }
      footer={
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          Đóng
        </button>
      }
    >
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
        <dt className="text-text-muted">Khách</dt>
        <dd>
          <CustomerName name={v.customer_name} gender={v.customer_gender} />
          {" · "}
          <Link
            to="/khach-hang/$customerId"
            params={{ customerId: v.customer_id }}
            className="underline"
          >
            Mở hồ sơ
          </Link>
        </dd>
        <dt className="text-text-muted">Dịch vụ</dt>
        <dd>{servicesText(v.services)}</dd>
        <dt className="text-text-muted">KTV</dt>
        <dd>{v.staff_name}</dd>
        <dt className="text-text-muted">Thanh toán</dt>
        <dd>{visitPayment(v)}</dd>
      </dl>
      <p className="mt-3 text-sm text-text-muted">
        Lượt ghi thẳng ở “Ghi lượt khách”. Sửa hoặc hủy lượt ở danh sách lượt
        hôm nay.
      </p>
    </BottomSheet>
  );
}

/** Owner only: "số khách tối đa mỗi khung giờ" (empty = no limit). Advisory, never blocks. */
function SlotLimitSheet({
  current,
  onClose,
}: {
  current: number | null;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [text, setText] = useState(current === null ? "" : String(current));
  const value = text.trim() === "" ? null : Number(text);
  const valid =
    value === null || (Number.isInteger(value) && value >= 1 && value <= 200);
  const save = useMutation({
    mutationFn: () => saveSlotLimit(value),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["appointments"] });
      onClose();
    },
  });
  return (
    <BottomSheet
      open
      onClose={onClose}
      title="Số khách tối đa mỗi khung giờ"
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Thôi
          </button>
          <button
            type="submit"
            form="slot-limit-form"
            className="btn btn-primary"
            disabled={save.isPending}
          >
            Lưu
          </button>
        </>
      }
    >
      <form
        id="slot-limit-form"
        className="grid gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) save.mutate();
        }}
      >
        <label className="grid gap-1 text-sm">
          <span>Số khách tối đa (để trống = không giới hạn)</span>
          <input
            inputMode="numeric"
            className="min-h-11 rounded-[6px] border border-border bg-surface px-3"
            value={text}
            onChange={(e) => {
              setText(e.target.value);
            }}
            aria-invalid={!valid}
          />
        </label>
        <p className="text-sm text-text-muted">
          Lịch hiện “số khách / tối đa” ở mỗi khung giờ và cảnh báo khi đủ,
          nhưng vẫn cho đặt khi khách chịu chờ.
        </p>
        {!valid && (
          <p role="alert" className="text-sm text-danger">
            Nhập số từ 1 đến 200, hoặc để trống.
          </p>
        )}
        {save.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
