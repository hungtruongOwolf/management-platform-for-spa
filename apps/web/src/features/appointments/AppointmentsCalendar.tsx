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
import { useState } from "react";
import "./calendar.css";
import { CustomerName } from "../../app/CustomerName";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { StickyActions } from "../../components/StickyActions";
import { usePhone } from "../../components/useMediaQuery";
import { messageFor, retryable } from "../../lib/errors";
import { useSearchState } from "../../lib/searchState";
import { useReached } from "../../lib/useReached";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../../lib/format";
import type { CartLine } from "../front-desk/api";
import { cartFromAppointment } from "../front-desk/prefill";
import { frontDeskSearch } from "../front-desk/search";
import { formatDate } from "../sales/api";
import { BookingSheet, type Draft } from "./BookingSheet";
import { AppointmentSheet, PhaseBadge, dayTitle } from "./AppointmentSheet";
import { type TimeColumn, TimeGrid, range, servicesText } from "./TimeGrid";
import {
  type Entry,
  PHASE_LABEL,
  calendarQuery,
  counts,
  daysFrom,
  entriesOf,
  forStaff,
  hhmm,
  minutesOf,
  mondayOf,
  paymentLabel,
  peakPeople,
  peopleAt,
  saveSlotLimit,
  NO_BOOKING,
  spaMinutes,
  view,
} from "./api";

/**
 * "Không đặt lịch" marks a visit recorded without a booking (F23: the old tag read as the
 * anonymous "Khách vãng lai", although most such visits are named customers); "Nhóm · 3" a
 * family booking.
 */
function entryTag(e: Entry, cal: Calendar): string | null {
  if (e.kind === "visit") return NO_BOOKING;
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
  const [limitOpen, setLimitOpen] = useState(false);
  const isOwner = can(me, "record.void_closed_day");
  const locked = day < today && !isOwner;
  // The server refuses a booking on a past day for everyone, the owner too (`check_day`), and
  // today once the spa has closed: no start is left (ADR-0019 D20). The spa runs on UTC+7.
  const closedToday = useReached(
    day === today && cal.data
      ? Date.parse(`${today}T${cal.data.closes_at}:00+07:00`)
      : null,
  );
  const bookable = day > today || (day === today && !closedToday);

  const opened = cal.data
    ? (entriesOf(cal.data).find((e) => e.id === openId) ?? null)
    : null;
  // "Xem theo KTV": a view filter on the loaded calendar (booking checks still see everyone).
  const who = search.ktv;
  const shown = cal.data ? forStaff(cal.data, who) : undefined;
  const book = () => {
    const now = spaMinutes(new Date().toISOString());
    const slot = cal.data?.slot_minutes ?? 15;
    const opens = minutesOf(cal.data?.opens_at ?? "09:00");
    const closes = minutesOf(cal.data?.closes_at ?? "21:00");
    // Late in the day "now" is past the last start the sheet offers: keep to that one.
    const start =
      day === today
        ? Math.max(
            opens,
            Math.min(closes - slot, Math.ceil(now / slot) * slot),
          )
        : opens;
    // Looking at one technician: the booking is for them.
    const staffId = shown && shown !== cal.data && who !== "none" ? who : "";
    setDraft({ mode: "new", day, start, staffId });
  };
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
  return (
    <section aria-labelledby="cal-title" className="cal">
      <div className="cal-bar">
        <div className="cal-heading">
          <h2 id="cal-title" className="panel-title">
            Lịch hẹn ·{" "}
            {layout === "tuan"
              ? `tuần ${formatDate(from).slice(0, 5)}–${formatDate(to).slice(0, 5)}`
              : dayTitle(day)}
          </h2>
          {shown && (
            <DaySummary cal={shown} day={layout === "tuan" ? null : day} />
          )}
        </div>
        {/* The primary action, top right (EM round 6); phones keep it in the bottom bar. */}
        {bookable && !phone && (
          <button type="button" className="btn btn-primary" onClick={book}>
            <Icon name="plus" /> Đặt lịch
          </button>
        )}
      </div>
      <div className="cal-bar">
        <div className="cal-toolbar">
          <div
            role="group"
            aria-label="Chọn ngày"
            className="cal-toolbar-group"
          >
            <button
              type="button"
              className="btn btn-secondary btn-icon"
              onClick={() => {
                shift(-step);
              }}
              aria-label={layout === "tuan" ? "Tuần trước" : "Ngày trước"}
            >
              <Icon name="chevron-left" />
            </button>
            <DateField
              label="Ngày"
              hideLabel
              className="filter-field cal-date"
              value={day}
              onChange={(iso) => {
                setSearch({ ngay: iso && iso !== today ? iso : "" });
              }}
            />
            <button
              type="button"
              className="btn btn-secondary btn-icon"
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
                className="btn btn-secondary"
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
          {cal.data && (
            <label className="cal-who">
              <span className="sr-only">Xem theo KTV</span>
              <select
                className="input"
                value={
                  who === "none" ||
                  cal.data.technicians.some((t) => t.id === who)
                    ? who
                    : ""
                }
                onChange={(e) => {
                  setSearch({ ktv: e.target.value });
                }}
              >
                <option value="">Tất cả KTV</option>
                {cal.data.technicians.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.full_name}
                  </option>
                ))}
                <option value="none">Chưa gán KTV</option>
              </select>
            </label>
          )}
          {can(me, "appointment.settings") && (
            <button
              type="button"
              className="btn btn-secondary"
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
        <p className="note">
          Ngày đã chốt: chỉ xem. Chủ spa sửa và nhập bù được.
        </p>
      )}
      {cal.isPending && <CalendarSkeleton layout={layout} />}
      {cal.isError && !cal.data && (
        <div role="alert" className="alert">
          <span>Không tải được lịch hẹn. {messageFor(cal.error)}</span>
          {retryable(cal.error) && (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void cal.refetch()}
            >
              Thử lại
            </button>
          )}
        </div>
      )}
      {shown &&
        // Not while the previous range stands in for the one being loaded.
        !cal.isPlaceholderData &&
        isEmpty(shown, layout === "tuan" ? null : day) && (
          <EmptyHint
            when={
              layout === "tuan"
                ? "tuần này"
                : day === today
                  ? "hôm nay"
                  : "ngày này"
            }
            filtered={shown !== cal.data}
            onGrid={layout === "ngay" && bookable}
            canBook={bookable}
          />
        )}
      {cal.data && shown && (
        <div className={cal.isPlaceholderData ? "is-refreshing" : ""}>
          {layout === "ngay" && (
            <DayGrid
              cal={shown}
              everyone={cal.data}
              day={day}
              canBook={bookable}
              unassigned={shown === cal.data || who === "none"}
              onOpen={setOpenId}
              onBook={(start, staffId) => {
                setDraft({ mode: "new", day, start, staffId });
              }}
            />
          )}
          {layout === "tuan" && (
            <WeekGrid
              cal={shown}
              from={from}
              onOpen={setOpenId}
              onDay={(d) => {
                setSearch({ ngay: d === today ? "" : d, kieu: "ngay" });
              }}
            />
          )}
          {layout === "ds" && (
            <DayList
              cal={shown}
              everyone={cal.data}
              day={day}
              onOpen={setOpenId}
            />
          )}
        </div>
      )}
      {bookable && phone && (
        // Phones keep the primary action in thumb reach (design-system StickyActions).
        <StickyActions label="Đặt lịch">
          <button type="button" className="btn btn-primary" onClick={book}>
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
            // Booked for another day: go there, so the booking just made is on screen (F10).
            if (a.business_date !== day)
              setSearch({ ngay: a.business_date === today ? "" : a.business_date });
            setOpenId(a.id);
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
    <p className="cal-summary" aria-live="polite">
      Chưa đến {n("waiting")} · Đang làm {n("in_service")} · Xong {n("done")}
      {lost > 0 ? ` · Hủy/không đến ${String(lost)}` : ""}
    </p>
  );
}

/** Nothing on the day (or the loaded week) of this view. */
function isEmpty(cal: Calendar, day: string | null): boolean {
  return entriesOf(cal, day ?? undefined).length === 0;
}

/**
 * An empty day teaches the next step (rulebook 9): "Chưa có lịch hẹn hôm nay" + where to book.
 * A note only (EM review 2026-11-06): the toolbar's "Đặt lịch" is the one way in, no second button.
 */
function EmptyHint({
  when,
  filtered,
  onGrid,
  canBook,
}: {
  when: string;
  /** Looking at one technician: say so, the others may have bookings. */
  filtered: boolean;
  /** The day grid is below: its free slots book too. */
  onGrid: boolean;
  canBook: boolean;
}) {
  return (
    <p className="card cal-empty">
      <span>
        <strong>
          Chưa có lịch hẹn {when}
          {filtered ? " của KTV đang xem" : ""}.
        </strong>{" "}
        {canBook &&
          (onGrid
            ? "Bấm một ô giờ trống bên dưới hoặc “Đặt lịch” để thêm."
            : "Bấm “Đặt lịch” để thêm.")}
      </span>
    </p>
  );
}

/** Loading: the shape of what is coming (a grid, seven days, or rows), never a spinner. */
function CalendarSkeleton({ layout }: { layout: "ngay" | "tuan" | "ds" }) {
  if (layout === "ngay")
    return (
      <div aria-hidden="true" className="cal-skel-grid">
        {Array.from({ length: 5 * 8 }, (_, i) => (
          <div
            key={i}
            className={i % 5 !== 0 && i % 3 === 0 ? "skeleton" : ""}
          />
        ))}
      </div>
    );
  if (layout === "tuan")
    return (
      <div aria-hidden="true" className="cal-week">
        {Array.from({ length: 7 }, (_, i) => (
          <div key={i} className="cal-day">
            <div className="cal-skel-rows">
              {Array.from({ length: 4 }, (_, j) => (
                <div key={j} className="skeleton cal-skel-line" />
              ))}
            </div>
          </div>
        ))}
      </div>
    );
  return (
    <div aria-hidden="true" className="card cal-panel">
      <div className="cal-skel-rows">
        {Array.from({ length: 5 }, (_, j) => (
          <div key={j} className="skeleton cal-skel-line" />
        ))}
      </div>
    </div>
  );
}

/** "2 khách", or "2/4" against the owner's max; empty when nobody is there and no max is set. */
function slotCount(n: number, max: number | null): string {
  if (max !== null) return `${String(n)}/${String(max)}`;
  return n > 0 ? `${String(n)} khách` : "";
}

/**
 * The desk's day grid (round 6, Google-Calendar style): one column per technician (+ "Chưa gán
 * KTV") on the shared `TimeGrid`, every free slot a "book here" button, and per hour the busiest
 * slot's people count against the owner's max.
 */
function DayGrid({
  cal,
  day,
  everyone,
  canBook,
  unassigned,
  onOpen,
  onBook,
}: {
  cal: Calendar;
  /** The unfiltered calendar: a group's size counts everyone in it. */
  everyone: Calendar;
  day: string;
  canBook: boolean;
  /** Draw the "Chưa gán KTV" column (everyone, or only the unassigned ones). */
  unassigned: boolean;
  onOpen: (id: string) => void;
  onBook: (start: number, staffId: string) => void;
}) {
  const opens = minutesOf(cal.opens_at);
  const closes = minutesOf(cal.closes_at);
  const entries = entriesOf(cal, day);
  const shown = entries
    .map(view)
    // Cancelled, missed and voided ones free their place; the list view still shows them.
    .filter((e) => counts(e.phase));
  const byId = new Map(entries.map((e) => [e.id, e]));
  const techIds = new Set(cal.technicians.map((t) => t.id));
  const head = (title: string) => <span>{title}</span>;
  const columns: TimeColumn[] = [
    ...cal.technicians.map((t) => ({
      key: t.id,
      head: head(t.full_name),
      title: t.full_name,
      day,
      bookAs: t.id,
      items: shown.filter((e) => e.staffId === t.id),
    })),
    ...(unassigned
      ? [
          {
            key: "none",
            head: head("Chưa gán KTV"),
            title: "Chưa gán KTV",
            day,
            bookAs: "",
            items: shown.filter((e) => !e.staffId || !techIds.has(e.staffId)),
          },
        ]
      : []),
  ];
  const max = cal.max_per_slot ?? null;
  return (
    <TimeGrid
      opens={opens}
      closes={closes}
      slot={cal.slot_minutes}
      columns={columns}
      label={`Lịch theo KTV ngày ${formatDate(day)}`}
      cornerTitle={
        max === null
          ? "Giờ và số khách trong khung"
          : "Giờ và số khách / tối đa"
      }
      canBook={canBook}
      onBook={(m, col) => {
        onBook(m, col.bookAs ?? "");
      }}
      onOpen={onOpen}
      tagOf={(e) => {
        const entry = byId.get(e.id);
        return entry ? entryTag(entry, everyone) : null;
      }}
      gutter={(m) => {
        // One number per hour (its busiest slot), not one per 15-minute row (EM round 5).
        const n = peakPeople(cal, day, m, Math.min(60, closes - m));
        const text = n > 0 || max !== null ? slotCount(n, max) : "";
        const full = max !== null && n >= max;
        return (
          text && (
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
          )
        );
      }}
    />
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
              <span className="cal-day-title">{dayTitle(d)}</span>
              <span className="cal-day-meta">
                {live.length} khách
                {full > 0 ? ` · ${String(full)} khung đủ khách` : ""}
              </span>
            </button>
            {/* The old week look (EM review 2026-11-06): one tinted block per booking, in the
                phase's colours of the day grid; the phase is said in words on the first line. */}
            {items.map((e) => {
              const v = view(e);
              return (
                <button
                  key={e.id}
                  type="button"
                  className="appt cal-chip"
                  data-phase={v.phase}
                  data-kind={e.kind}
                  onClick={() => {
                    onOpen(e.id);
                  }}
                >
                  {/* Time and phase wrap as whole groups: a narrow day card no longer cuts the
                      phase to "Đan…" (EM polish round 2 sweep). */}
                  <span className="cal-chip-when">
                    <span>{hhmm(v.start)}</span>{" "}
                    <span className="cal-chip-phase">
                      <span aria-hidden="true" className="appt-dot" />
                      {PHASE_LABEL[v.phase]}
                    </span>
                  </span>
                  <span className="cal-chip-who">
                    {v.customerName}
                    {e.kind === "visit" ? ` · ${NO_BOOKING}` : ""}
                  </span>
                </button>
              );
            })}
            {items.length === 0 && (
              <p className="cal-day-empty">Chưa có lịch hẹn.</p>
            )}
          </div>
        );
      })}
    </div>
  );
}

function DayList({
  cal,
  everyone,
  day,
  onOpen,
}: {
  cal: Calendar;
  /** The unfiltered calendar: a group's size counts everyone in it. */
  everyone: Calendar;
  day: string;
  onOpen: (id: string) => void;
}) {
  const items = entriesOf(cal, day);
  // The empty day is said above the views (EmptyHint), with the way to book.
  if (items.length === 0) return null;
  // Missed / cancelled / voided wait behind a disclosure with their count (rulebook 8).
  const live = items.filter((e) => counts(view(e).phase));
  const closed = items.filter((e) => !counts(view(e).phase));
  const row = (e: Entry) => {
    const v = view(e);
    const tag = entryTag(e, everyone);
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
          className="cal-row"
          data-closed={counts(v.phase) ? undefined : ""}
          onClick={() => {
            onOpen(e.id);
          }}
        >
          <span className="cal-row-time">{range(v.start, v.minutes)}</span>
          <span className="cal-row-main">
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
            <span>
              {servicesText(v.services)}
              {tag ? ` · ${tag}` : ""}
            </span>
            <span>
              {staffName ? `KTV ${staffName}` : "Chưa gán KTV"} · {pay}
            </span>
          </span>
          <PhaseBadge phase={v.phase} />
        </button>
      </li>
    );
  };
  return (
    <div className="card cal-panel">
      {live.length > 0 && (
        <ul
          className="cal-list"
          aria-label={`Lịch hẹn ngày ${formatDate(day)}`}
        >
          {live.map(row)}
        </ul>
      )}
      {closed.length > 0 && (
        <details className="disclosure cal-closed" open={live.length === 0}>
          <summary>Đã hủy / không đến ({closed.length})</summary>
          <ul
            className="cal-list"
            aria-label={`Lịch đã hủy / không đến ngày ${formatDate(day)}`}
          >
            {closed.map(row)}
          </ul>
        </details>
      )}
    </div>
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
          {dayTitle(v.business_date)} · {NO_BOOKING} ·{" "}
          <PhaseBadge phase={v.phase} />
        </>
      }
      footer={
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          Đóng
        </button>
      }
    >
      <div className="appt-sheet">
        <dl className="detail-grid">
          <dt>Khách</dt>
          <dd>
            <CustomerName name={v.customer_name} gender={v.customer_gender} />
            {" · "}
            <Link
              to="/khach-hang/$customerId"
              params={{ customerId: v.customer_id }}
              className="link underline"
            >
              Mở hồ sơ
            </Link>
          </dd>
          <dt>Dịch vụ</dt>
          <dd>{servicesText(v.services)}</dd>
          <dt>KTV</dt>
          <dd>{v.staff_name}</dd>
          <dt>Thanh toán</dt>
          <dd>{visitPayment(v)}</dd>
        </dl>
        <p className="field-hint">
          Lượt ghi thẳng ở “Ghi lượt khách”. Sửa hoặc hủy lượt ở danh sách lượt
          hôm nay.
        </p>
      </div>
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
            aria-busy={save.isPending}
          >
            Lưu số khách tối đa
          </button>
        </>
      }
    >
      <form
        id="slot-limit-form"
        className="appt-form"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          if (valid && !save.isPending) save.mutate();
        }}
      >
        <div className="field">
          <label htmlFor="slot-limit">Số khách tối đa</label>
          <input
            id="slot-limit"
            inputMode="numeric"
            className="input"
            value={text}
            onChange={(e) => {
              setText(e.target.value);
            }}
            aria-invalid={!valid}
            aria-describedby="slot-limit-msg"
          />
          {valid ? (
            <span id="slot-limit-msg" className="field-hint">
              Để trống = không giới hạn. Lịch hiện “số khách / tối đa” ở mỗi
              khung giờ và cảnh báo khi đủ, nhưng vẫn cho đặt khi khách chịu
              chờ.
            </span>
          ) : (
            <span id="slot-limit-msg" role="alert" className="field-error">
              Nhập số từ 1 đến 200, hoặc để trống.
            </span>
          )}
        </div>
        {save.isError && (
          <p role="alert" className="field-error">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
