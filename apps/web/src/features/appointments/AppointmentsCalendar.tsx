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
import { Suspense, lazy, useEffect, useRef, useState } from "react";
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
  type Block,
  type Lane,
  blockOf,
  cardLines,
  columnLanes,
  gutterBands,
} from "./layout";
import {
  type Entry,
  type EntryView,
  PHASE_BADGE,
  PHASE_LABEL,
  arriveAppointment,
  calendarQuery,
  closeAppointment,
  counts,
  daysFrom,
  entriesOf,
  forStaff,
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

/** The phase as the app's one status chip set (always with its words). */
function PhaseBadge({ phase }: { phase: TimelinePhase }) {
  return (
    <span className={`badge ${PHASE_BADGE[phase]}`}>{PHASE_LABEL[phase]}</span>
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
  // "Xem theo KTV": a view filter on the loaded calendar (booking checks still see everyone).
  const who = search.ktv;
  const shown = cal.data ? forStaff(cal.data, who) : undefined;
  const book = () => {
    const now = spaMinutes(new Date().toISOString());
    const slot = cal.data?.slot_minutes ?? 15;
    const opens = minutesOf(cal.data?.opens_at ?? "09:00");
    const start =
      day === today ? Math.max(opens, Math.ceil(now / slot) * slot) : opens;
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
  const openGroup = (groupId: string) => {
    setOpenId(null);
    setGroupMembers(
      (cal.data?.appointments ?? []).filter(
        (a) => a.group_id === groupId && isOpen(a) && a.business_date <= today,
      ),
    );
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
        {!locked && !phone && (
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
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => void cal.refetch()}
          >
            Thử lại
          </button>
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
            onGrid={layout === "ngay" && !locked}
            canBook={!locked}
          />
        )}
      {cal.data && shown && (
        <div className={cal.isPlaceholderData ? "is-refreshing" : ""}>
          {layout === "ngay" && (
            <DayGrid
              cal={shown}
              everyone={cal.data}
              day={day}
              canBook={!locked}
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
      {!locked && phone && (
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
      {groupMembers.length > 0 && (
        <GroupCheckIn
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

interface Column {
  key: string;
  title: string;
  staffId: string;
  items: EntryView[];
}

/** "2 khách", or "2/4" against the owner's max; empty when nobody is there and no max is set. */
function slotCount(n: number, max: number | null): string {
  if (max !== null) return `${String(n)}/${String(max)}`;
  return n > 0 ? `${String(n)} khách` : "";
}

/**
 * The day grid, in two layers (round 6, Google-Calendar style): the slot grid (hour lines, one
 * "book here" button per slot and column, always all of them) and, above it, one event layer per
 * technician column with absolutely placed cards. A card never replaces the cells under it, so the
 * lines run on around it and the free time before and after it stays clickable.
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
  const slot = cal.slot_minutes;
  const rows = Math.max(1, Math.round((closes - opens) / slot));
  const entries = entriesOf(cal, day);
  const shown = entries
    .map(view)
    // Cancelled, missed and voided ones free their place; the list view still shows them.
    .filter((e) => counts(e.phase));
  const byId = new Map(entries.map((e) => [e.id, e]));
  const techIds = new Set(cal.technicians.map((t) => t.id));
  const columns: Column[] = [
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
  // Equal columns, one grid track each; overlapping cards split their column into lanes.
  const laid = columns
    .filter((c) => c.staffId !== "" || unassigned)
    .map((c, i) => ({ ...c, col: i + 2, lane: columnLanes(c.items) }));
  const cols = laid.length;
  const now = spaMinutes(new Date().toISOString());
  const isToday = day === spaToday();
  const past = (m: number) => day < spaToday() || (isToday && m + slot <= now);
  const max = cal.max_per_slot ?? null;
  const { frame, wide } = useFits(cols);

  return (
    <div ref={frame} className={`card cal-scroll ${wide ? "is-wide" : ""}`}>
      <div
        className="cal-grid"
        style={
          {
            "--cols": String(cols),
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
            style={{ gridColumn: g.col, gridRow: 1 }}
            title={g.title}
          >
            <span>{g.title}</span>
          </div>
        ))}
        {gutterBands(opens, closes, slot).map(({ row, span, m }) => {
          const hour = m % 60 === 0;
          // One number per hour (its busiest slot), not one per 15-minute row (EM round 5).
          const n = hour
            ? peakPeople(cal, day, m, Math.min(60, closes - m))
            : 0;
          const text = hour && (n > 0 || max !== null) ? slotCount(n, max) : "";
          const full = max !== null && n >= max;
          // One gutter band per hour (EM polish round 2: the count chip collided with the hour on
          // one 26 px row): the hour on top, its count chip under it.
          return (
            <div
              key={`g${String(row)}`}
              className={`cal-gutter ${hour ? "is-hour" : ""}`}
              style={{
                gridColumn: 1,
                gridRow: `${String(row + 2)} / span ${String(span)}`,
              }}
            >
              {hour && <span>{hhmm(m)}</span>}
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
                style={{ gridColumn: g.col, gridRow: r + 2 }}
                onClick={() => {
                  onBook(m, g.staffId);
                }}
              />
            );
          }),
        )}
        {laid.map((g) => (
          // One column's event layer: spans its slots, lets clicks through except on the cards.
          <div
            key={`layer-${g.key}`}
            className="cal-layer"
            // Explicit "/ span 1": an absolute grid item with an auto end line would reach the grid edge.
            style={{
              gridColumn: `${String(g.col)} / span 1`,
              gridRow: `2 / span ${String(rows)}`,
            }}
          >
            {g.items.map((e) => {
              const block = blockOf(e.start, e.minutes, opens, closes, slot);
              const entry = byId.get(e.id);
              if (!block || !entry) return null;
              return (
                <DayEvent
                  key={e.id}
                  e={e}
                  block={block}
                  lane={g.lane.get(e.id) ?? { lane: 0, lanes: 1 }}
                  tag={entryTag(entry, everyone)}
                  late={
                    isToday &&
                    e.phase === "in_service" &&
                    now >= e.start + e.minutes
                  }
                  onOpen={onOpen}
                />
              );
            })}
          </div>
        ))}
        {isToday && now >= opens && now <= closes && (
          <div
            className="cal-now"
            aria-hidden="true"
            style={
              {
                gridColumn: `1 / span ${String(cols + 1)}`,
                gridRow: `2 / span ${String(rows)}`,
                "--top": String((now - opens) / slot),
              } as React.CSSProperties
            }
          />
        )}
      </div>
    </div>
  );
}

/** Gutter and minimum column width of the day grid (calendar.css `--gutter-w`, `--col-min`). */
const GUTTER_PX = 104;
const COLUMN_MIN_PX = 168;

/**
 * Whether `cols` technician columns overflow the grid's frame. Only then the frame scrolls
 * sideways; otherwise it is no scroll box at all, so the page scrolls as one and the header row
 * sticks under the top bar.
 */
function useFits(cols: number) {
  const frame = useRef<HTMLDivElement | null>(null);
  const [wide, setWide] = useState(false);
  useEffect(() => {
    const el = frame.current;
    if (!el) return;
    const check = () => {
      // clientWidth excludes the 1 px borders, like the grid's own box.
      setWide(GUTTER_PX + cols * COLUMN_MIN_PX > el.clientWidth);
    };
    check();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(check);
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, [cols]);
  return { frame, wide };
}

/**
 * One card of the event layer: as tall as its real time span, `1 / lanes` of the column wide.
 * Whole lines only (layout.ts `cardLines`): the name (up to two lines), then a phase dot + phase
 * word + tag, then the services with the room left. The full text is the tooltip and the name.
 */
function DayEvent({
  e,
  block,
  lane,
  tag,
  late,
  onOpen,
}: {
  e: EntryView;
  block: Block;
  lane: Lane;
  tag: string | null;
  /** Arrived and still "Đang làm" after the booked end: keeps its span, says "quá giờ". */
  late: boolean;
  onOpen: (id: string) => void;
}) {
  const parts = [PHASE_LABEL[e.phase], late ? "quá giờ" : null, tag].filter(
    (x): x is string => Boolean(x),
  );
  const phase = parts.join(" · ");
  const services = servicesText(e.services);
  const lines = cardLines(block.height, lane.lanes);
  const when = range(e.start, e.minutes);
  const dot = <span aria-hidden="true" className="appt-dot" />;
  return (
    <button
      type="button"
      className="appt cal-event"
      data-phase={e.phase}
      data-kind={e.kind}
      data-fit={lines.meta === 0 ? "compact" : undefined}
      style={
        {
          "--top": String(block.top),
          "--span": String(block.height),
          "--lane": String(lane.lane),
          "--lanes": String(lane.lanes),
          "--name-lines": String(lines.name),
          "--meta-lines": String(lines.meta),
          "--svc-lines": String(lines.services),
        } as React.CSSProperties
      }
      onClick={() => {
        onOpen(e.id);
      }}
      aria-label={[`${when} ${e.customerName}`, ...parts, services]
        .filter(Boolean)
        .join(", ")}
      title={[when, e.customerName, phase, services]
        .filter(Boolean)
        .join(" · ")}
    >
      {lines.meta === 0 ? (
        <span className="cal-event-name">
          {dot}
          <strong>{e.customerName}</strong>
        </span>
      ) : (
        <>
          {/* The spaces keep the word groups apart in the text, so it reads inside the accessible
              name (WCAG 2.5.3); flex layout ignores them on screen. */}
          <strong className="cal-event-name">{e.customerName}</strong>{" "}
          <span className="cal-event-meta">
            <span className="cal-event-phase">
              {dot}
              {PHASE_LABEL[e.phase]}
            </span>{" "}
            {late && <span className="cal-event-phase">quá giờ</span>}{" "}
            {tag && <span className="cal-event-tag">{tag}</span>}{" "}
            {/* No services line: they follow as the last word group, where there is room. */}
            {lines.services === 0 && services && (
              <span className="cal-event-tag">{services}</span>
            )}
          </span>{" "}
          {lines.services > 0 && services && (
            <span className="cal-event-services">{services}</span>
          )}
        </>
      )}
    </button>
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
                    {e.kind === "visit" ? " · Vãng lai" : ""}
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
  // The booked technician, or nobody: the desk picks who serves (EM 2026-11-07, it defaulted to
  // the first technician and checked in the wrong person).
  const [staffId, setStaffId] = useState(a.staff_id ?? "");
  const [arriveTried, setArriveTried] = useState(false);
  const [closing, setClosing] = useState<"cancel" | "no-show" | null>(null);
  const [reason, setReason] = useState("");
  const [reasonTried, setReasonTried] = useState(false);
  const [creating, setCreating] = useState(false);
  // Which button sent the running request (its spinner; the others wait).
  const [running, setRunning] = useState<"arrive" | "close" | "link" | null>(
    null,
  );
  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: ["appointments"] });
  const action = useMutation({
    mutationFn: (run: () => Promise<Appointment>) => run(),
    onSuccess: async () => {
      setClosing(null);
      await refresh();
    },
    onSettled: () => {
      setRunning(null);
    },
  });
  const send = (
    kind: "arrive" | "close" | "link",
    run: () => Promise<Appointment>,
  ) => {
    if (action.isPending) return;
    setRunning(kind);
    action.mutate(run);
  };
  const today = spaToday();
  const open = isOpen(a);
  const [openedAt] = useState(() => Date.now());
  const started = new Date(a.starts_at).getTime() <= openedAt;
  const start = spaMinutes(a.starts_at);
  const link = (customerId: string) => {
    setCreating(false);
    send("link", () => updateAppointment(a, { customer_id: customerId }));
  };
  const group = a.group_id
    ? cal.appointments.filter((x) => x.group_id === a.group_id)
    : [];
  const groupOpen = group.filter(isOpen);
  const canServe = open && !locked && a.business_date <= today;
  const canRecord = canServe && !a.is_guest;
  const reasonMissing = reason.trim() === "";
  const reasonLabel =
    closing === "cancel" ? "Lý do hủy lịch" : "Ghi chú không đến";

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
              // aria-disabled keeps it focusable; the notice above says why it waits.
              aria-disabled={!canRecord}
              onClick={() => {
                if (canRecord) onComplete();
              }}
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
      <div className="appt-sheet">
        <dl className="detail-grid">
          <dt>Khách</dt>
          <dd>
            <CustomerName name={a.customer_name} gender={a.customer_gender} />
            {a.is_guest ? " · khách mới, chưa có hồ sơ" : ""}
            {a.customer_id && (
              <>
                {" · "}
                <Link
                  to="/khach-hang/$customerId"
                  params={{ customerId: a.customer_id }}
                  className="link whitespace-nowrap underline"
                >
                  Mở hồ sơ
                </Link>
              </>
            )}
          </dd>
          <dt>SĐT</dt>
          <dd className="tabular-nums">
            {a.customer_phone
              ? a.customer_phone.replace(/^\+84/, "0")
              : "Chưa có"}
          </dd>
          <dt>Dịch vụ</dt>
          <dd>
            {servicesText(a.services)} ({a.duration_min} phút)
          </dd>
          <dt>KTV</dt>
          <dd>{a.staff_name ?? "Chưa gán"}</dd>
          <dt>Thanh toán</dt>
          <dd>{paymentLabel(a, formatVnd)}</dd>
          {a.companions.length > 0 && (
            <>
              <dt>Đi cùng</dt>
              <dd>{a.companions.join(", ")}</dd>
            </>
          )}
          {a.note && (
            <>
              <dt>Ghi chú</dt>
              <dd>{a.note}</dd>
            </>
          )}
          {a.status_reason && (
            <>
              <dt>Lý do</dt>
              <dd>{a.status_reason}</dd>
            </>
          )}
        </dl>

        {group.length > 1 && (
          <section className="appt-section" aria-label="Cùng nhóm">
            <p className="section-label">Cùng nhóm ({group.length} người)</p>
            <ul className="appt-members">
              {group.map((x) => (
                <li key={x.id}>
                  {x.id === a.id ? (
                    <strong>{x.customer_name}</strong>
                  ) : (
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={() => {
                        onOpenOther(x.id);
                      }}
                    >
                      {x.customer_name}
                    </button>
                  )}
                  <PhaseBadge phase={x.phase} />
                  {/* Who serves each person, at what time: the companion's own booking. */}
                  <span className="appt-member-meta">
                    {range(spaMinutes(x.starts_at), x.duration_min)} ·{" "}
                    {servicesText(x.services)} ·{" "}
                    {/* "KTV Lan", without doubling a name that already says "KTV …". */}
                    {x.staff_name === null || x.staff_name === undefined
                      ? "Chưa gán KTV"
                      : /^ktv\b/i.test(x.staff_name)
                        ? x.staff_name
                        : `KTV ${x.staff_name}`}
                  </span>
                </li>
              ))}
            </ul>
            {groupOpen.length > 1 && canServe && a.group_id && (
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  if (a.group_id) onGroupCheckIn(a.group_id);
                }}
              >
                Khách đến cả nhóm · ghi {groupOpen.length} lượt
              </button>
            )}
          </section>
        )}

        {locked && open && (
          <p className="note">Ngày đã chốt, liên hệ chủ spa.</p>
        )}

        {open && !locked && a.is_guest && (
          <div className="callout">
            <p>
              {a.customer_phone
                ? "Khách mới: tạo hồ sơ (điền sẵn tên và SĐT) để ghi lượt được."
                : "Người đi cùng chưa có hồ sơ: tạo hồ sơ (điền sẵn tên, thêm SĐT) để ghi lượt cho họ; KTV đã gán vẫn giữ."}
            </p>
            {canCreateCustomer && !creating && (
              <button
                type="button"
                className="btn btn-secondary"
                aria-busy={running === "link"}
                onClick={() => {
                  setCreating(true);
                }}
              >
                Tạo hồ sơ khách
              </button>
            )}
            {creating && (
              <Suspense
                fallback={
                  <div aria-hidden="true" className="skeleton cal-skel-line" />
                }
              >
                <div className="appt-callout-body">
                  <CustomerForm
                    prefill={{
                      full_name: a.customer_name,
                      phone: (a.customer_phone ?? "").replace(/^\+84/, "0"),
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
            <section className="appt-section" aria-label="Khách đã đến">
              <div className="button-row">
                <div className="field">
                  <label htmlFor={`arrive-staff-${a.id}`}>KTV làm</label>
                  <select
                    id={`arrive-staff-${a.id}`}
                    className="input"
                    value={staffId}
                    aria-invalid={(arriveTried && staffId === "") || undefined}
                    aria-describedby={`arrive-staff-${a.id}-msg`}
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
                </div>
                <button
                  type="button"
                  className="btn btn-secondary"
                  aria-busy={running === "arrive"}
                  onClick={() => {
                    setArriveTried(true);
                    if (staffId !== "")
                      send("arrive", () => arriveAppointment(a.id, staffId));
                    else
                      document.getElementById(`arrive-staff-${a.id}`)?.focus();
                  }}
                >
                  Chỉ báo đã đến (ghi lượt sau)
                </button>
              </div>
              {arriveTried && staffId === "" ? (
                <span id={`arrive-staff-${a.id}-msg`} className="field-error">
                  Chọn KTV làm cho khách rồi bấm lại.
                </span>
              ) : (
                <span id={`arrive-staff-${a.id}-msg`} className="field-hint">
                  {a.staff_id
                    ? "KTV đã gán khi đặt lịch; đổi nếu người khác làm."
                    : "Lịch chưa gán KTV: chọn người làm."}
                </span>
              )}
            </section>
          )}

        {open && !locked && (
          <section
            className="appt-section"
            aria-labelledby={`close-title-${a.id}`}
          >
            <p id={`close-title-${a.id}`} className="section-label">
              {a.status === "booked" && started
                ? "Khách không đến hoặc hủy lịch"
                : "Hủy lịch"}
            </p>
            {closing === null ? (
              // Real buttons of the control scale (EM 2026-11-07: "Hủy lịch" was a bare text),
              // the destructive one in the danger outline; each asks for its reason first.
              <div className="appt-close-actions">
                {a.status === "booked" && started && (
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      setClosing("no-show");
                      setReason("");
                      setReasonTried(false);
                    }}
                  >
                    Không đến
                  </button>
                )}
                <button
                  type="button"
                  className="btn btn-danger-outline"
                  onClick={() => {
                    setClosing("cancel");
                    setReason("");
                    setReasonTried(false);
                  }}
                >
                  <Icon name="x" /> Hủy lịch
                </button>
              </div>
            ) : (
              <form
                className="appt-form"
                noValidate
                onSubmit={(e) => {
                  e.preventDefault();
                  setReasonTried(true);
                  if (reasonMissing) return;
                  const outcome = closing;
                  send("close", () =>
                    closeAppointment(a.id, outcome, reason.trim()),
                  );
                }}
              >
                <div className="field">
                  <label htmlFor={`close-reason-${a.id}`}>
                    {reasonLabel} (bắt buộc)
                  </label>
                  <input
                    id={`close-reason-${a.id}`}
                    autoFocus
                    value={reason}
                    maxLength={500}
                    onChange={(e) => {
                      setReason(e.target.value);
                    }}
                    className="input"
                    aria-invalid={reasonTried && reasonMissing}
                    aria-describedby={`close-reason-${a.id}-msg`}
                  />
                  {reasonTried && reasonMissing ? (
                    <span
                      id={`close-reason-${a.id}-msg`}
                      className="field-error"
                    >
                      Nhập {reasonLabel.toLowerCase()} rồi bấm lại.
                    </span>
                  ) : (
                    <span
                      id={`close-reason-${a.id}-msg`}
                      className="field-hint"
                    >
                      Ghi vào lịch hẹn, ai mở lịch cũng thấy.
                    </span>
                  )}
                </div>
                <div className="button-row">
                  <button
                    type="submit"
                    className={
                      closing === "cancel"
                        ? "btn btn-danger"
                        : "btn btn-primary"
                    }
                    aria-busy={running === "close"}
                  >
                    {closing === "cancel" ? "Hủy lịch" : "Đánh dấu không đến"}
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      setClosing(null);
                    }}
                  >
                    Thôi
                  </button>
                </div>
              </form>
            )}
          </section>
        )}
        {action.isError && (
          <p role="alert" className="field-error">
            {conflictMessage(action.error, cal)}
          </p>
        )}
      </div>
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
