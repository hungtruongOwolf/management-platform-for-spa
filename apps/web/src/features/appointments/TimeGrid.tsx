import { useEffect, useRef, useState, type ReactNode } from "react";
import "./calendar.css";
import { serviceLabel } from "../../lib/names";
import { spaToday } from "../corrections/api";
import {
  type Block,
  type Lane,
  blockOf,
  cardLines,
  columnLanes,
  gutterBands,
} from "./layout";
import { type EntryView, PHASE_LABEL, hhmm, spaMinutes } from "./api";

export function range(start: number, minutes: number): string {
  return `${hhmm(start)}–${hhmm(start + minutes)}`;
}

export function servicesText(
  list: { service_name: string; category_name: string }[],
): string {
  return list
    .map((s) => serviceLabel(s.service_name, s.category_name))
    .join(", ");
}

/** One column of the time grid: a technician of the desk's day, or a day of a technician's week. */
export interface TimeColumn {
  key: string;
  /** The column header (text, or a richer node with its own words). */
  head: ReactNode;
  /** The header's tooltip / full text. */
  title: string;
  /** The business day of the column: past slots get the quiet wash, today's shows the now line. */
  day: string;
  /** Cards of the column (already filtered to those that hold a place on the timeline). */
  items: EntryView[];
  /** Free slots book with this technician (`""` = unassigned); omitted = read-only column. */
  bookAs?: string;
}

/** Gutter and default minimum column width (calendar.css `--gutter-w`, `--col-min`). */
const GUTTER_PX = 104;
const COLUMN_MIN_PX = 168;

/**
 * Whether `cols` columns of at least `colMin` px overflow the grid's frame. Only then the frame
 * scrolls sideways; otherwise it is no scroll box at all, so the page scrolls as one and the
 * header row sticks under the top bar.
 */
function useFits(cols: number, colMin: number) {
  const frame = useRef<HTMLDivElement | null>(null);
  const [wide, setWide] = useState(false);
  useEffect(() => {
    const el = frame.current;
    if (!el) return;
    const check = () => {
      // clientWidth excludes the 1 px borders, like the grid's own box.
      setWide(GUTTER_PX + cols * colMin > el.clientWidth);
    };
    check();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(check);
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, [cols, colMin]);
  return { frame, wide };
}

/**
 * The calendar's time grid, in two layers (round 6, Google-Calendar style): the slot grid (hour
 * lines, one cell per slot and column) and, above it, one event layer per column with absolutely
 * placed cards at their real length. A card never replaces the cells under it, so the lines run
 * on around it. With `onBook`, a free slot of a column with `bookAs` is a "book here" button (the
 * desk); without it the grid is read-only (a technician's own calendar).
 */
export function TimeGrid({
  opens,
  closes,
  slot,
  columns,
  label,
  corner = "Giờ",
  cornerTitle,
  colMin = COLUMN_MIN_PX,
  gutter,
  tagOf,
  canBook = false,
  onBook,
  onOpen,
}: {
  opens: number;
  closes: number;
  slot: number;
  columns: TimeColumn[];
  /** The grid's accessible name. */
  label: string;
  corner?: string;
  cornerTitle?: string;
  /** Narrower columns (a week of one person) before the frame scrolls sideways. */
  colMin?: number;
  /** Extra content of an hour's gutter band, under the hour (the desk's people count). */
  gutter?: (minute: number) => ReactNode;
  /** "Vãng lai", "Nhóm · 3 người": the second word group of a card. */
  tagOf: (e: EntryView) => string | null;
  canBook?: boolean;
  onBook?: (start: number, column: TimeColumn) => void;
  onOpen: (id: string) => void;
}) {
  const rows = Math.max(1, Math.round((closes - opens) / slot));
  const laid = columns.map((c, i) => ({
    ...c,
    col: i + 2,
    lane: columnLanes(c.items),
  }));
  const cols = laid.length;
  const now = spaMinutes(new Date().toISOString());
  const today = spaToday();
  const past = (day: string, m: number) =>
    day < today || (day === today && m + slot <= now);
  const { frame, wide } = useFits(cols, colMin);
  const showsNow = now >= opens && now <= closes;
  const todayCols = laid.filter((c) => c.day === today);
  // One day on every column (the desk's day): the line runs across the gutter too.
  const nowLines =
    !showsNow || todayCols.length === 0
      ? []
      : todayCols.length === cols
        ? [{ key: "all", column: `1 / span ${String(cols + 1)}` }]
        : todayCols.map((c) => ({
            key: c.key,
            column: `${String(c.col)} / span 1`,
          }));

  return (
    <div ref={frame} className={`card cal-scroll ${wide ? "is-wide" : ""}`}>
      <div
        className="cal-grid"
        style={
          {
            "--cols": String(cols),
            "--rows": String(rows),
            "--col-min": `${String(colMin)}px`,
          } as React.CSSProperties
        }
        aria-label={label}
        role="group"
      >
        <div
          className="cal-head cal-corner"
          style={{ gridColumn: 1, gridRow: 1 }}
        >
          <span title={cornerTitle}>{corner}</span>
        </div>
        {laid.map((g) => (
          <div
            key={g.key}
            className={`cal-head ${g.day === today && todayCols.length < cols ? "is-today" : ""}`}
            style={{ gridColumn: g.col, gridRow: 1 }}
            title={g.title}
          >
            {g.head}
          </div>
        ))}
        {gutterBands(opens, closes, slot).map(({ row, span, m }) => {
          const hour = m % 60 === 0;
          // One gutter band per hour (EM polish round 2): the hour on top, its extras under it.
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
              {hour && gutter?.(m)}
            </div>
          );
        })}
        {laid.flatMap((g) =>
          Array.from({ length: rows }, (_, r) => {
            const m = opens + r * slot;
            const gone = past(g.day, m);
            const cls = `cal-cell ${m % 60 === 0 ? "is-hour" : ""} ${gone ? "is-past" : ""}`;
            const style = { gridColumn: g.col, gridRow: r + 2 };
            if (!onBook || g.bookAs === undefined)
              return (
                <div
                  key={`${g.key}-${String(r)}`}
                  aria-hidden="true"
                  className={`${cls} is-static`}
                  style={style}
                />
              );
            const disabled = !canBook || gone;
            return (
              <button
                key={`${g.key}-${String(r)}`}
                type="button"
                tabIndex={-1}
                aria-label={`Đặt lịch ${hhmm(m)}${g.bookAs ? ` với ${g.title}` : ""}`}
                disabled={disabled}
                className={`${cls} ${disabled ? "is-past" : ""}`}
                style={style}
                onClick={() => {
                  onBook(m, g);
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
              if (!block) return null;
              return (
                <DayEvent
                  key={e.id}
                  e={e}
                  block={block}
                  lane={g.lane.get(e.id) ?? { lane: 0, lanes: 1 }}
                  tag={tagOf(e)}
                  late={
                    g.day === today &&
                    e.phase === "in_service" &&
                    now >= e.start + e.minutes
                  }
                  onOpen={onOpen}
                />
              );
            })}
          </div>
        ))}
        {nowLines.map((l) => (
          <div
            key={`now-${l.key}`}
            className="cal-now"
            aria-hidden="true"
            style={
              {
                gridColumn: l.column,
                gridRow: `2 / span ${String(rows)}`,
                "--top": String((now - opens) / slot),
              } as React.CSSProperties
            }
          />
        ))}
      </div>
    </div>
  );
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
