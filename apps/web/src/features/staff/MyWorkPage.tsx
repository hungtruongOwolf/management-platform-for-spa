import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Icon } from "../../app/Icon";
import { PageHeader } from "../../app/PageHeader";
import { BottomSheet } from "../../components/BottomSheet";
import { usePhone } from "../../components/useMediaQuery";
import { useSearchState } from "../../lib/searchState";
import { PHASE_BADGE, PHASE_LABEL, counts, daysFrom, hhmm, minutesOf, mondayOf, spaMinutes, type EntryView } from "../appointments/api";
import { TimeGrid, range, servicesText, type TimeColumn } from "../appointments/TimeGrid";
import { spaToday } from "../../lib/format";
import { EmptyState, ErrorState } from "../reports/ui";
import { myCalendarQuery, type MyCalendarData, type MyCalendarEntry } from "./api";
import { shiftMonth, weekdayShort } from "./labels";
import { entryView, focusMinute, gridSpan, weekSummary } from "./myCalendar";
import { myWorkSearch } from "./search";
import { MonthPicker, dayLabel } from "./ui";
import "./staff.css";

type Layout = "ngay" | "tuan" | "ds";

/** "Nhóm · 3 người" for a family booking, "Không đặt lịch" for a visit without one (F23). */
function tagOf(e: MyCalendarEntry): string | null {
  if (e.walk_in) return "Không đặt lịch";
  if (e.group_size && e.group_size > 1) return `Nhóm · ${String(e.group_size)} người`;
  return null;
}

/** "03/11" of a day. */
function ddmm(day: string): string {
  return `${day.slice(8, 10)}/${day.slice(5, 7)}`;
}

/** A week column's header: "T2" over "03/11", a button that opens the day. */
function DayHead({ day, onDay }: { day: string; onDay: (day: string) => void }) {
  return (
    <button
      type="button"
      className="cal-head-day"
      aria-label={`Xem ngày ${dayLabel(day)}`}
      onClick={() => {
        onDay(day);
      }}
    >
      <span>{weekdayShort(day)}</span>
      <small>{ddmm(day)}</small>
    </button>
  );
}

/**
 * "Khách của tôi" (/khach-cua-toi, EM 2026-11-07): the technician's own calendar, Google-Calendar
 * style like the desk's "Lịch hẹn" — bookings assigned to them and visits they served, as blocks
 * at their real length on the shared `TimeGrid` (day or week), or a list. Read-only: nothing here
 * books or edits. The server picks the person from the session and sends names, times and
 * services and the session note of a visit they worked on — no phone, customer note or money
 * (`GET /me/appointments`).
 */
export function MyWorkPage() {
  const [search, patch] = useSearchState(myWorkSearch);
  const phone = usePhone();
  const today = spaToday();
  const day = search.ngay || today;
  // Phones: the one-column day grid by default, the week as a list; no 7-column grid at 390 px.
  const wanted: Layout = search.kieu || (phone ? "ngay" : "tuan");
  const layout: Layout = phone && wanted === "tuan" ? "ds" : wanted;
  const week = mondayOf(day);
  const days = daysFrom(week, 7);
  const query = useQuery({ ...myCalendarQuery(week, days[6] ?? week), placeholderData: keepPreviousData });
  const [openId, setOpenId] = useState<string | null>(null);
  const current = today.slice(0, 7);
  // Bookings reach 30 days ahead: next month is worth a look, later ones are empty.
  const latest = shiftMonth(current, 1);
  const go = (d: string) => {
    setOpenId(null);
    patch({ ngay: d === today ? "" : d });
  };
  const shift = (n: number) => {
    const d = new Date(`${day}T00:00:00Z`);
    d.setUTCDate(d.getUTCDate() + n);
    go(d.toISOString().slice(0, 10));
  };
  const step = layout === "ngay" ? 1 : 7;
  const views: [Layout, string][] = phone
    ? [
        ["ngay", "Ngày"],
        ["ds", "Danh sách tuần"],
      ]
    : [
        ["ngay", "Ngày"],
        ["tuan", "Tuần"],
        ["ds", "Danh sách"],
      ];
  const data = query.data?.data;
  const opened = data?.entries.find((e) => e.id === openId) ?? null;

  return (
    <section className="grid content-start gap-5">
      <PageHeader title="Khách của tôi">
        <MonthPicker
          month={day.slice(0, 7)}
          current={latest}
          onChange={(m) => {
            // A month jumps to its first day; the current month to today.
            go(m === current ? today : `${m}-01`);
          }}
        />
      </PageHeader>
      <section aria-labelledby="my-cal-title" className="cal">
        <div className="cal-bar">
          <div className="cal-heading">
            <h2 id="my-cal-title" className="panel-title">
              {layout === "ngay" ? dayLabel(day) : `Tuần ${ddmm(week)}–${ddmm(days[6] ?? week)}`}
            </h2>
            {data && (
              <p className="cal-summary" aria-live="polite">
                {weekSummary(data.entries, layout === "ngay" ? day : null)}
              </p>
            )}
          </div>
        </div>
        <div className="cal-bar">
          <div className="cal-toolbar">
            <div role="group" aria-label="Chọn ngày" className="cal-toolbar-group">
              <button
                type="button"
                className="btn btn-secondary btn-icon"
                aria-label={step === 7 ? "Tuần trước" : "Ngày trước"}
                onClick={() => {
                  shift(-step);
                }}
              >
                <Icon name="chevron-left" />
              </button>
              <button
                type="button"
                className="btn btn-secondary"
                disabled={day === today}
                onClick={() => {
                  go(today);
                }}
              >
                Hôm nay
              </button>
              <button
                type="button"
                className="btn btn-secondary btn-icon"
                aria-label={step === 7 ? "Tuần sau" : "Ngày sau"}
                onClick={() => {
                  shift(step);
                }}
              >
                <Icon name="chevron-right" />
              </button>
            </div>
            <div role="group" aria-label="Kiểu xem" className="segmented">
              {views.map(([k, label]) => (
                <button
                  key={k}
                  type="button"
                  aria-pressed={layout === k}
                  onClick={() => {
                    patch({ kieu: k });
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
        {query.isPending ? (
          <div aria-hidden="true" className="card cal-panel">
            <div className="cal-skel-rows">
              {Array.from({ length: 5 }, (_, j) => (
                <div key={j} className="skeleton cal-skel-line" />
              ))}
            </div>
          </div>
        ) : query.isError && !query.data ? (
          <ErrorState error={query.error} onRetry={() => void query.refetch()} />
        ) : !query.data.linked || !data ? (
          <EmptyState title="Tài khoản chưa gắn với hồ sơ nhân viên">
            Nhờ chủ spa gắn tài khoản này vào hồ sơ của bạn để xem lịch hẹn và khách đã làm.
          </EmptyState>
        ) : (
          <div className={query.isPlaceholderData ? "is-refreshing" : ""}>
            <MyCalendarView data={data} layout={layout} day={day} days={days} onOpen={setOpenId} onDay={(d) => {
              patch({ ngay: d === today ? "" : d, kieu: "ngay" });
            }} />
          </div>
        )}
        <p className="field-hint">Lịch do quầy đặt. Cần đổi giờ hay đổi khách, hãy báo quầy.</p>
      </section>
      {opened && (
        <EntrySheet
          e={opened}
          onClose={() => {
            setOpenId(null);
          }}
        />
      )}
    </section>
  );
}

function MyCalendarView({
  data,
  layout,
  day,
  days,
  onOpen,
  onDay,
}: {
  data: MyCalendarData;
  layout: Layout;
  day: string;
  days: string[];
  onOpen: (id: string) => void;
  onDay: (day: string) => void;
}) {
  const shownDays = layout === "ngay" ? [day] : days;
  const views = data.entries.map(entryView);
  const inRange = views.filter((v) => shownDays.includes(v.day));
  if (inRange.length === 0) {
    return (
      <p className="card cal-empty">
        <strong>Chưa có lịch hẹn hay lượt khách nào {layout === "ngay" ? (day === data.today ? "hôm nay" : "ngày này") : "tuần này"}.</strong>{" "}
        Lịch quầy đặt cho bạn sẽ hiện ở đây.
      </p>
    );
  }
  const byId = new Map(data.entries.map((e) => [e.id, e]));
  if (layout === "ds") return <WeekList data={data} days={days} views={inRange} onOpen={onOpen} />;
  const columns: TimeColumn[] = shownDays.map((d) => ({
    key: d,
    head: layout === "ngay" ? <span>Lịch của tôi</span> : <DayHead day={d} onDay={onDay} />,
    title: dayLabel(d),
    day: d,
    // Cancelled and missed bookings free their place; the list still shows them.
    items: inRange.filter((v) => v.day === d && counts(v.phase)),
  }));
  // Never hide the technician's own work outside opening hours (a late walk-in, past midnight).
  const span = gridSpan(
    minutesOf(data.opens_at),
    minutesOf(data.closes_at),
    data.slot_minutes,
    columns.flatMap((c) => c.items),
  );
  const focus = focusMinute({
    gridOpens: span.opens,
    gridCloses: span.closes,
    opens: minutesOf(data.opens_at),
    now: spaMinutes(new Date().toISOString()),
    showsToday: shownDays.includes(data.today),
    shown: columns.flatMap((c) => c.items),
  });
  return (
    <FocusedGrid view={`${layout}|${shownDays[0] ?? day}`} focus={focus}>
      <TimeGrid
        opens={span.opens}
        closes={span.closes}
        slot={data.slot_minutes}
        columns={columns}
        colMin={layout === "ngay" ? 168 : 112}
        label={layout === "ngay" ? `Lịch của tôi ngày ${dayLabel(day)}` : "Lịch của tôi trong tuần"}
        tagOf={(v) => {
          const e = byId.get(v.id);
          return e ? tagOf(e) : null;
        }}
        onOpen={onOpen}
      />
    </FocusedGrid>
  );
}

/**
 * Brings the grid's `focus` hour (F22, `focusMinute`) to the top of the view, just under the
 * sticky top bar and day headers, once per view (day or week, layout) — not on every refetch, so
 * the page never jumps under the reader. The page scrolls as one (no nested scroll box), so this
 * scrolls the window; nothing happens when the hour is already the grid's first row.
 */
function FocusedGrid({ view, focus, children }: { view: string; focus: number | null; children: ReactNode }) {
  const wrap = useRef<HTMLDivElement | null>(null);
  const done = useRef<string | null>(null);
  useEffect(() => {
    const el = wrap.current;
    if (!el || done.current === view) return;
    done.current = view;
    if (focus === null) return;
    const label = hhmm(focus);
    const band = Array.from(el.querySelectorAll<HTMLElement>(".cal-gutter.is-hour")).find(
      (b) => b.firstElementChild?.textContent === label,
    );
    if (!band) return;
    const head = el.querySelector<HTMLElement>(".cal-head");
    const sticky = head ? (Number.parseFloat(getComputedStyle(head).top) || 0) + head.getBoundingClientRect().height : 0;
    const top = band.getBoundingClientRect().top + window.scrollY - sticky - 8;
    if (top > window.scrollY) window.scrollTo({ top, behavior: "auto" });
  }, [view, focus]);
  return <div ref={wrap}>{children}</div>;
}

/** The week as a list grouped by day (phones, and the "Danh sách" view). */
function WeekList({ data, days, views, onOpen }: { data: MyCalendarData; days: string[]; views: EntryView[]; onOpen: (id: string) => void }) {
  const byId = new Map(data.entries.map((e) => [e.id, e]));
  return (
    <div className="card cal-panel">
      {days.map((d) => {
        const list = views.filter((v) => v.day === d);
        if (list.length === 0) return null;
        return (
          <section key={d} aria-label={dayLabel(d)} className="my-cal-day">
            <h3 className="my-cal-day-title">{d === data.today ? `Hôm nay · ${dayLabel(d)}` : dayLabel(d)}</h3>
            <ul className="cal-list">
              {list.map((v) => {
                const e = byId.get(v.id);
                const tag = e ? tagOf(e) : null;
                return (
                  <li key={v.id}>
                    <button
                      type="button"
                      className="cal-row"
                      data-closed={counts(v.phase) ? undefined : ""}
                      onClick={() => {
                        onOpen(v.id);
                      }}
                    >
                      <span className="cal-row-time">{range(v.start, v.minutes)}</span>
                      <span className="cal-row-main">
                        <strong>{v.customerName}</strong>
                        <span>
                          {servicesText(v.services)}
                          {tag ? ` · ${tag}` : ""}
                        </span>
                      </span>
                      <span className={`badge ${PHASE_BADGE[v.phase]}`}>{PHASE_LABEL[v.phase]}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </div>
  );
}

/** Read-only detail of one block: when, who, what. No actions but "Đóng". */
function EntrySheet({ e, onClose }: { e: MyCalendarEntry; onClose: () => void }) {
  const v = entryView(e);
  const tag = tagOf(e);
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`${range(v.start, v.minutes)} · ${e.customer_name}`}
      description={
        <>
          {dayLabel(e.business_date)}
          {tag ? ` · ${tag}` : ""} · <span className={`badge ${PHASE_BADGE[v.phase]}`}>{PHASE_LABEL[v.phase]}</span>
        </>
      }
      footer={
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          Đóng
        </button>
      }
    >
      <dl className="detail-grid">
        <dt>Khách</dt>
        <dd>{e.customer_name}</dd>
        <dt>Dịch vụ</dt>
        <dd>
          <ul className="grid gap-1">
            {e.services.map((s, i) => (
              <li key={`${s.service_name}-${String(i)}`}>
                {s.service_name}
                <span className="text-text-muted"> · {s.from_package ? "Buổi liệu trình" : e.kind === "visit" ? "Dịch vụ lẻ" : s.category_name}</span>
              </li>
            ))}
          </ul>
        </dd>
        <dt>Thời lượng</dt>
        <dd>{e.duration_min} phút</dd>
        <dt>{e.kind === "visit" ? "Lượt" : "Lịch hẹn"}</dt>
        <dd>{e.kind === "visit" ? (e.walk_in ? "Khách vãng lai, không đặt lịch" : "Lượt khách bạn làm cùng") : "Quầy đặt cho bạn"}</dd>
        {/* The session note of a visit I worked on (EM 2026-11-07 item 3); the server sends no other. */}
        {e.note && (
          <>
            <dt>Ghi chú buổi</dt>
            <dd className="whitespace-pre-line">{e.note}</dd>
          </>
        )}
      </dl>
    </BottomSheet>
  );
}

