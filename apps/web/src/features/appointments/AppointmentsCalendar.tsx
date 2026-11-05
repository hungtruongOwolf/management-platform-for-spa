import type { Appointment, Calendar, CustomerSummary } from "@vpyd/api-client";
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
import { fieldMessage, messageFor } from "../../lib/errors";
import { useSearchState } from "../../lib/searchState";
import { serviceLabel } from "../../lib/names";
import { can, meQuery } from "../auth/session";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import { spaToday } from "../corrections/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { customerQuery } from "../customers/api";
import { frontDeskSearch } from "../front-desk/search";
import { activeStaffQuery, formatDate } from "../sales/api";
import {
  STATUS_LABEL,
  arriveAppointment,
  bookAppointment,
  calendarQuery,
  closeAppointment,
  daysFrom,
  freeAt,
  hhmm,
  instantAt,
  isOpen,
  minutesOf,
  mondayOf,
  paymentLabel,
  spaMinutes,
  updateAppointment,
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

function range(a: Appointment): string {
  const s = spaMinutes(a.starts_at);
  return `${hhmm(s)}–${hhmm(s + a.duration_min)}`;
}

function services(a: Appointment): string {
  return a.services
    .map((s) => serviceLabel(s.service_name, s.category_name))
    .join(", ");
}

function StatusBadge({ a }: { a: Appointment }) {
  return (
    <span className="appt appt-badge" data-status={a.status}>
      {STATUS_LABEL[a.status]}
    </span>
  );
}

type Draft =
  | { mode: "new"; day: string; start: number; staffId: string }
  | { mode: "edit"; appointment: Appointment };

/**
 * The appointments tab of /quay (spec appointments.md §7): a day grid with technicians as columns,
 * a week overview, and a list (the phone layout). Click an empty slot to book, an appointment to
 * open it. Every rule (overlap, capacity, hours, closed days) is the server's; this only previews.
 */
export function AppointmentsCalendar({
  onComplete,
}: {
  /** Opens the visit cart for this appointment (customer and services filled in). */
  onComplete: (a: Appointment) => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const [search, setSearch] = useSearchState(frontDeskSearch);
  const phone = usePhone();
  const today = spaToday();
  const day = search.ngay || today;
  const layout = phone && search.kieu === "ngay" ? "ds" : search.kieu;
  const week = mondayOf(day);
  const [from, to] =
    layout === "tuan" ? [week, daysFrom(week, 7)[6] ?? week] : [day, day];
  const cal = useQuery({
    ...calendarQuery(from, to),
    placeholderData: keepPreviousData,
  });
  const [openId, setOpenId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const isOwner = can(me, "record.void_closed_day");
  const locked = day < today && !isOwner;

  const opened = cal.data?.appointments.find((a) => a.id === openId) ?? null;
  const shift = (days: number) => {
    const d = new Date(`${day}T00:00:00Z`);
    d.setUTCDate(d.getUTCDate() + days);
    setSearch({
      ngay:
        d.toISOString().slice(0, 10) === today
          ? ""
          : d.toISOString().slice(0, 10),
    });
  };
  const step = layout === "tuan" ? 7 : 1;

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
            <DaySummary cal={cal.data} day={day} week={layout === "tuan"} />
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

      {cal.data && opened && (
        <AppointmentSheet
          key={opened.id}
          a={opened}
          cal={cal.data}
          locked={opened.business_date < today && !isOwner}
          canCreateCustomer={can(me, "customer.write")}
          canWriteHealth={can(me, "customer.health_notes.write")}
          onClose={() => {
            setOpenId(null);
          }}
          onEdit={() => {
            setOpenId(null);
            setDraft({ mode: "edit", appointment: opened });
          }}
          onComplete={() => {
            setOpenId(null);
            onComplete(opened);
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
    </section>
  );
}

function DaySummary({
  cal,
  day,
  week,
}: {
  cal: Calendar;
  day: string;
  week: boolean;
}) {
  const list = week
    ? cal.appointments
    : cal.appointments.filter((a) => a.business_date === day);
  const count = (s: Appointment["status"]) =>
    list.filter((a) => a.status === s).length;
  return (
    <p className="text-sm text-text-muted" aria-live="polite">
      {list.length} lịch · {count("booked")} đã đặt · {count("arrived")} đã đến
      · {count("done")} hoàn tất
      {count("no_show") + count("cancelled") > 0
        ? ` · ${String(count("no_show") + count("cancelled"))} hủy/không đến`
        : ""}{" "}
      · {cal.capacity} KTV
    </p>
  );
}

/** Greedy lanes: overlapping appointments of one column sit side by side. */
function lanes(list: Appointment[]): Map<string, number> {
  const ends: number[] = [];
  const lane = new Map<string, number>();
  for (const a of [...list].sort((x, y) =>
    x.starts_at < y.starts_at ? -1 : 1,
  )) {
    const s = spaMinutes(a.starts_at);
    let i = ends.findIndex((e) => e <= s);
    if (i === -1) {
      i = ends.length;
      ends.push(0);
    }
    ends[i] = s + a.duration_min;
    lane.set(a.id, i);
  }
  return lane;
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
  const shown = cal.appointments.filter(
    (a) =>
      a.business_date === day &&
      a.status !== "cancelled" &&
      a.status !== "no_show",
  );
  const techIds = new Set(cal.technicians.map((t) => t.id));
  const groups = [
    ...cal.technicians.map((t) => ({
      key: t.id,
      title: t.full_name,
      staffId: t.id,
      items: shown.filter((a) => a.staff_id === t.id),
    })),
    {
      key: "none",
      title: "Chưa gán KTV",
      staffId: "",
      items: shown.filter((a) => !a.staff_id || !techIds.has(a.staff_id)),
    },
  ];
  let col = 2;
  const laid = groups.map((g) => {
    const l = lanes(g.items);
    const width = Math.max(1, ...[...l.values()].map((v) => v + 1));
    const first = col;
    col += width;
    return { ...g, first, width, lane: l };
  });
  const now = spaMinutes(new Date().toISOString());
  const isToday = day === spaToday();
  const past = (m: number) => day < spaToday() || (isToday && m + slot <= now);

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
          <span className="text-xs text-text-muted">Giờ · trống</span>
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
          const free = freeAt(cal, day, m);
          const hour = m % 60 === 0;
          return (
            <div
              key={`g${String(r)}`}
              className={`cal-gutter ${hour ? "is-hour" : ""}`}
              style={{ gridColumn: 1, gridRow: r + 2 }}
            >
              <span>{hour ? hhmm(m) : ""}</span>
              <span
                className={`cal-free ${free === 0 ? "is-full" : ""}`}
                title={free === 0 ? "Kín" : `Còn ${String(free)} chỗ`}
              >
                <span className="sr-only">{hhmm(m)}: </span>
                {free === 0 ? "Kín" : free}
              </span>
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
          g.items.map((a) => {
            const s = Math.max(
              0,
              Math.round((spaMinutes(a.starts_at) - opens) / slot),
            );
            const span = Math.max(1, Math.round(a.duration_min / slot));
            return (
              <button
                key={a.id}
                type="button"
                className="appt cal-event"
                data-status={a.status}
                style={{
                  gridColumn: g.first + (g.lane.get(a.id) ?? 0),
                  gridRow: `${String(s + 2)} / span ${String(span)}`,
                }}
                onClick={() => {
                  onOpen(a.id);
                }}
                aria-label={`${range(a)} ${a.customer_name}, ${STATUS_LABEL[a.status]}`}
              >
                <span className="cal-event-time">
                  {range(a)} · {STATUS_LABEL[a.status]}
                </span>
                <strong>{a.customer_name}</strong>
                {span > 1 && <span>{services(a)}</span>}
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
              alignSelf: "start",
              marginTop: `${String((((now - opens) % slot) / slot) * 26)}px`,
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
  return (
    <div className="cal-week">
      {daysFrom(from, 7).map((d) => {
        const items = cal.appointments.filter((a) => a.business_date === d);
        const open = items.filter(isOpen);
        let full = 0;
        for (let m = opens; m < closes; m += cal.slot_minutes)
          if (freeAt(cal, d, m) === 0) full += 1;
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
                {items.length} lịch · {open.length} chưa xong
                {full > 0 ? ` · ${String(full)} khung kín` : ""}
              </span>
            </button>
            {items.map((a) => (
              <button
                key={a.id}
                type="button"
                className="appt cal-chip"
                data-status={a.status}
                onClick={() => {
                  onOpen(a.id);
                }}
              >
                <span className="font-semibold tabular-nums">
                  {hhmm(spaMinutes(a.starts_at))} · {STATUS_LABEL[a.status]}
                </span>
                <span>
                  {a.customer_name}
                  {a.staff_name ? ` · ${a.staff_name}` : ""}
                </span>
              </button>
            ))}
            {items.length === 0 && (
              <p className="px-1 text-xs text-text-muted">Chưa có lịch.</p>
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
  const items = cal.appointments.filter((a) => a.business_date === day);
  if (items.length === 0)
    return (
      <p className="rounded-[14px] border border-border bg-surface p-4 text-text-muted">
        Chưa có lịch hẹn ngày này. Bấm “Đặt lịch” để thêm.
      </p>
    );
  return (
    <ul className="cal-list" aria-label={`Lịch hẹn ngày ${formatDate(day)}`}>
      {items.map((a) => (
        <li key={a.id}>
          <button
            type="button"
            className="appt cal-card"
            data-status={a.status}
            onClick={() => {
              onOpen(a.id);
            }}
          >
            <span className="cal-card-time">{range(a)}</span>
            <span className="cal-card-main">
              <strong>
                <CustomerName
                  name={a.customer_name}
                  gender={a.customer_gender}
                />
              </strong>
              <span className="text-sm text-text-muted">{services(a)}</span>
              <span className="text-sm">
                {a.staff_name ? `KTV ${a.staff_name}` : "Chưa gán KTV"} ·{" "}
                {paymentLabel(a, formatVnd)}
              </span>
            </span>
            <StatusBadge a={a} />
          </button>
        </li>
      ))}
    </ul>
  );
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
}: {
  a: Appointment;
  cal: Calendar;
  locked: boolean;
  canCreateCustomer: boolean;
  canWriteHealth: boolean;
  onClose: () => void;
  onEdit: () => void;
  onComplete: () => void;
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
  const link = (customerId: string) => {
    setCreating(false);
    action.mutate(() => updateAppointment(a, { customer_id: customerId }));
  };

  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`${range(a)} · ${a.customer_name}`}
      description={
        <>
          {dayTitle(a.business_date)} · <StatusBadge a={a} />
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
              disabled={a.is_guest || a.business_date > today}
              onClick={onComplete}
            >
              Hoàn tất &amp; ghi lượt
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
          {services(a)} ({a.duration_min} phút)
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

      {locked && open && (
        <p className="mt-3 text-sm text-text-muted">
          Ngày đã chốt, liên hệ chủ spa.
        </p>
      )}

      {open && !locked && a.is_guest && (
        <div className="mt-3 grid gap-2 rounded-[10px] bg-warning-bg p-3 text-sm text-warning-text">
          <p>Khách mới: tạo hồ sơ (điền sẵn tên và SĐT) để hoàn tất được.</p>
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
              Khách đã đến
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
      return `${other.staff_name ?? "KTV"} đã có lịch ${range(other)} (${other.customer_name}). Chọn giờ hoặc KTV khác.`;
  }
  if (error instanceof ApiError && error.code === "appointment.slot_full")
    return `Khung giờ này đã kín (${String(cal.capacity)} KTV đều có khách). Chọn giờ khác.`;
  return messageFor(error);
}

// --------------------------------------------------------------------------- booking sheet

function BookingSheet({
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
  const [who, setWho] = useState<"customer" | "guest">("customer");
  const [customer, setCustomer] = useState<CustomerSummary | null>(null);
  const [guestName, setGuestName] = useState("");
  const [guestPhone, setGuestPhone] = useState("");
  const [serviceIds, setServiceIds] = useState<string[]>(
    editing ? editing.services.map((s) => s.service_id) : [],
  );
  const [pick, setPick] = useState("");
  const [staffId, setStaffId] = useState(
    editing
      ? (editing.staff_id ?? "")
      : draft.mode === "new"
        ? draft.staffId
        : "",
  );
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
        : minutesOf(cal.opens_at),
  );
  const [duration, setDuration] = useState<number | null>(
    editing ? editing.duration_min : null,
  );
  const [note, setNote] = useState(editing?.note ?? "");
  const [key] = useState(() => crypto.randomUUID());
  const slot = cal.slot_minutes;
  const opens = minutesOf(cal.opens_at);
  const closes = minutesOf(cal.closes_at);

  const chosen = serviceIds
    .map((id) => allServices.data?.find((s) => s.id === id))
    .filter((s) => s !== undefined);
  const auto =
    Math.ceil(
      chosen.reduce((sum, s) => sum + (s.duration_min ?? 60), 0) / slot,
    ) * slot || 60;
  const minutes = duration ?? auto;
  const save = useMutation({
    mutationFn: async () => {
      const starts_at = instantAt(day, start);
      if (editing)
        return updateAppointment(editing, {
          starts_at,
          duration_min: minutes,
          staff_id: staffId || null,
          service_ids: serviceIds,
          note: note.trim() || null,
        });
      return bookAppointment(
        {
          ...(who === "customer"
            ? { customer_id: customer?.id ?? null }
            : { guest: { full_name: guestName, phone: guestPhone } }),
          service_ids: serviceIds,
          staff_id: staffId || null,
          starts_at,
          duration_min: minutes,
          note: note.trim() || null,
        },
        key,
      );
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
  const ready =
    serviceIds.length > 0 &&
    (editing ||
      (who === "customer"
        ? customer !== null
        : guestName.trim() !== "" && guestPhone.trim() !== ""));
  const groups = (categories.data ?? []).map((c) => ({
    c,
    items: (allServices.data ?? []).filter((s) => s.category_id === c.id),
  }));
  const field =
    "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";

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
          <button
            type="submit"
            form="booking-form"
            className="btn btn-primary"
            aria-disabled={!ready || save.isPending}
          >
            {save.isPending
              ? "Đang lưu…"
              : editing
                ? "Lưu thay đổi"
                : "Đặt lịch"}
          </button>
        </>
      }
    >
      <form
        id="booking-form"
        className="grid gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready && !save.isPending) save.mutate();
        }}
      >
        {!editing && (
          <fieldset className="grid gap-2">
            <legend className="text-sm font-semibold">Khách</legend>
            <div
              role="group"
              aria-label="Loại khách"
              className="segmented justify-self-start"
            >
              <button
                type="button"
                aria-pressed={who === "customer"}
                onClick={() => {
                  setWho("customer");
                }}
              >
                Khách có hồ sơ
              </button>
              <button
                type="button"
                aria-pressed={who === "guest"}
                onClick={() => {
                  setWho("guest");
                }}
              >
                Khách mới
              </button>
            </div>
            {who === "customer" &&
              (customer ? (
                <p className="flex flex-wrap items-center gap-2">
                  <strong>
                    <CustomerName
                      name={customer.full_name}
                      gender={customer.gender}
                    />
                  </strong>
                  <span className="text-sm text-text-muted tabular-nums">
                    {customer.phone}
                  </span>
                  <button
                    type="button"
                    className="underline"
                    onClick={() => {
                      setCustomer(null);
                    }}
                  >
                    Đổi khách
                  </button>
                </p>
              ) : (
                <CustomerSearch
                  onSelect={setCustomer}
                  onCreate={(typed) => {
                    setWho("guest");
                    if (/\d/.test(typed)) setGuestPhone(typed);
                    else setGuestName(typed);
                  }}
                />
              ))}
            {who === "guest" && (
              <div className="grid gap-2 sm:grid-cols-2">
                <label className="grid gap-1 text-sm">
                  <span>Tên khách</span>
                  <input
                    className={field}
                    value={guestName}
                    maxLength={200}
                    onChange={(e) => {
                      setGuestName(e.target.value);
                    }}
                  />
                </label>
                <label className="grid gap-1 text-sm">
                  <span>Số điện thoại</span>
                  <input
                    className={field}
                    inputMode="tel"
                    value={guestPhone}
                    onChange={(e) => {
                      setGuestPhone(e.target.value);
                    }}
                  />
                  {fieldMessage(save.error, "guest.phone") && (
                    <span className="text-xs text-danger">
                      {fieldMessage(save.error, "guest.phone")}
                    </span>
                  )}
                </label>
              </div>
            )}
            {taken && (
              <p
                role="alert"
                className="flex flex-wrap items-center gap-2 text-sm text-danger"
              >
                Số này đã có hồ sơ khách.
                <button
                  type="button"
                  className="underline"
                  onClick={() => {
                    void queryClient.query(customerQuery(taken)).then((c) => {
                      setWho("customer");
                      setCustomer({
                        id: c.id,
                        full_name: c.full_name,
                        phone: c.phone,
                        gender: c.gender ?? null,
                      });
                      save.reset();
                    });
                  }}
                >
                  Dùng hồ sơ này
                </button>
              </p>
            )}
          </fieldset>
        )}

        <fieldset className="grid gap-2">
          <legend className="text-sm font-semibold">Dịch vụ (tối đa 5)</legend>
          {chosen.length > 0 && (
            <ul className="flex flex-wrap gap-2" aria-label="Dịch vụ đã chọn">
              {chosen.map((s, i) => (
                <li key={`${s.id}-${String(i)}`} className="badge badge-muted">
                  {s.name} ({s.duration_min ?? 60}′)
                  <button
                    type="button"
                    aria-label={`Bỏ ${s.name}`}
                    onClick={() => {
                      setServiceIds((ids) => ids.filter((_, j) => j !== i));
                      if (!editing) setDuration(null);
                    }}
                  >
                    <Icon name="x" className="size-4" />
                  </button>
                </li>
              ))}
            </ul>
          )}
          {serviceIds.length < 5 && (
            <div className="flex flex-wrap gap-2">
              <label htmlFor="booking-service" className="sr-only">
                Thêm dịch vụ
              </label>
              <select
                id="booking-service"
                className={`${field} w-56 min-w-0 flex-1`}
                value={pick}
                onChange={(e) => {
                  setPick(e.target.value);
                }}
              >
                <option value="">
                  {allServices.isPending
                    ? "Đang tải bảng giá…"
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
              <button
                type="button"
                className="btn btn-secondary min-h-11"
                onClick={() => {
                  if (pick) {
                    setServiceIds((ids) => [...ids, pick]);
                    setPick("");
                    if (!editing) setDuration(null);
                  }
                }}
              >
                Thêm
              </button>
            </div>
          )}
        </fieldset>

        <div className="grid gap-3 sm:grid-cols-2">
          <DateField
            label="Ngày hẹn"
            value={day}
            min={spaToday()}
            onChange={(iso) => {
              if (iso) setDay(iso);
            }}
            className="grid gap-1 text-sm"
          />
          <label className="grid gap-1 text-sm">
            <span>Giờ bắt đầu</span>
            <select
              className={field}
              value={start}
              onChange={(e) => {
                setStart(Number(e.target.value));
              }}
            >
              {Array.from(
                { length: Math.max(0, (closes - opens) / slot) },
                (_, i) => opens + i * slot,
              ).map((m) => {
                const free = freeAt(cal, day, m);
                return (
                  <option key={m} value={m}>
                    {hhmm(m)}
                    {day >= cal.from && day <= cal.to
                      ? free === 0
                        ? " (kín)"
                        : ` (còn ${String(free)})`
                      : ""}
                  </option>
                );
              })}
            </select>
          </label>
          <label className="grid gap-1 text-sm">
            <span>Thời lượng</span>
            <select
              className={field}
              value={minutes}
              onChange={(e) => {
                setDuration(Number(e.target.value));
              }}
            >
              {Array.from({ length: 480 / slot }, (_, i) => (i + 1) * slot).map(
                (m) => (
                  <option key={m} value={m}>
                    {m} phút
                    {duration === null && m === auto ? " (theo dịch vụ)" : ""}
                  </option>
                ),
              )}
            </select>
          </label>
          <label className="grid gap-1 text-sm">
            <span>KTV (không bắt buộc)</span>
            <select
              className={field}
              value={staffId}
              onChange={(e) => {
                setStaffId(e.target.value);
              }}
            >
              <option value="">Chưa gán KTV</option>
              {cal.technicians.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.full_name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label className="grid gap-1 text-sm">
          <span>Ghi chú lịch hẹn (không ghi thông tin sức khỏe)</span>
          <textarea
            rows={2}
            maxLength={500}
            value={note}
            onChange={(e) => {
              setNote(e.target.value);
            }}
            className="rounded-[6px] border border-border bg-surface p-2"
          />
        </label>
        <p className="text-sm text-text-muted">
          {formatDate(day)} · {hhmm(start)}–{hhmm(start + minutes)} · chưa thanh
          toán (thu khi hoàn tất)
        </p>
        {save.isError && !taken && (
          <p role="alert" className="text-sm text-danger">
            {conflictMessage(save.error, cal)}
            {fieldMessage(save.error, "starts_at")
              ? ` ${fieldMessage(save.error, "starts_at") ?? ""}`
              : ""}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
