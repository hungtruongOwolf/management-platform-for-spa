/**
 * Geometry of the day grid's event layer (Google-Calendar style): events float over the slot
 * grid instead of replacing its cells, so the hour lines and the free slots around an event stay.
 * Pure functions: unit-tested in layout.test.ts.
 */

export interface Span {
  id: string;
  /** Minutes after the spa's midnight. */
  start: number;
  minutes: number;
}

export interface Lane {
  /** 0-based lane of the event inside its overlap cluster. */
  lane: number;
  /** Lanes of that cluster: the event is `1 / lanes` of the column wide. */
  lanes: number;
}

/**
 * Side-by-side lanes for one technician column. Only events that overlap (directly or through a
 * chain) share the width; an event alone at its time takes the whole column.
 */
export function columnLanes(items: readonly Span[]): Map<string, Lane> {
  const out = new Map<string, Lane>();
  const sorted = [...items].sort(
    (a, b) => a.start - b.start || b.minutes - a.minutes,
  );
  let cluster: { id: string; lane: number }[] = [];
  let ends: number[] = [];
  let clusterEnd = -Infinity;
  const close = () => {
    for (const c of cluster)
      out.set(c.id, { lane: c.lane, lanes: Math.max(1, ends.length) });
    cluster = [];
    ends = [];
  };
  for (const e of sorted) {
    if (e.start >= clusterEnd) close();
    let lane = ends.findIndex((end) => end <= e.start);
    if (lane === -1) {
      lane = ends.length;
      ends.push(0);
    }
    ends[lane] = e.start + e.minutes;
    cluster.push({ id: e.id, lane });
    clusterEnd = Math.max(clusterEnd, e.start + e.minutes);
  }
  close();
  return out;
}

export interface Block {
  /** Offset from the first row, in slots (fractional: a walk-in checks in at 17:37). */
  top: number;
  /** Height in slots: exactly the event's real time span, clipped to opening hours. */
  height: number;
}

/**
 * Where an event sits on the grid: `top` and `height` in slots, so `top * slotHeight` is pixels.
 * Clipped to `[opens, closes)`; `null` when nothing of it is inside (a visit recorded after
 * closing never grows the grid).
 */
export function blockOf(
  start: number,
  minutes: number,
  opens: number,
  closes: number,
  slot: number,
): Block | null {
  const from = Math.max(start, opens);
  const to = Math.min(start + Math.max(0, minutes), closes);
  if (to <= from) return null;
  return { top: (from - opens) / slot, height: (to - from) / slot };
}

/** Text lines of a day-grid card (calendar.css: 26 px slots, 18 px name lines, 16 px others). */
export interface CardLines {
  /** Customer name (bold): up to two lines, never cut mid-word when it wraps. */
  name: number;
  /** Phase dot + phase word + tag ("Vãng lai", "Nhóm · 3 người"); 0 = the dot sits by the name. */
  meta: number;
  services: number;
}

const SLOT_PX = 26;
/** 1 px gap above and below, 1 px border, 3 px padding, top and bottom. */
const CARD_CHROME_PX = 10;
const NAME_LINE_PX = 18;
const LINE_PX = 16;

/**
 * Whole lines a card holds, so no line is cut through its middle (EM polish round 2: "Bùi Văn …",
 * "Xong · Vã…" in three cut lines). Name first, then the phase, then the services with what is
 * left. A card sharing its column with one other is narrow: its phase and tag may take two lines.
 */
export function cardLines(heightInSlots: number, lanes: number): CardLines {
  if (heightInSlots < 2) return { name: 1, meta: 0, services: 0 };
  if (heightInSlots < 3) return { name: 1, meta: 1, services: 0 };
  // Three or more lanes leave a sliver: name and phase one line each, cut with "…", no services,
  // instead of words broken letter by letter.
  const name = lanes > 2 ? 1 : 2;
  const meta = lanes === 2 ? 2 : 1;
  const free =
    heightInSlots * SLOT_PX -
    CARD_CHROME_PX -
    name * NAME_LINE_PX -
    meta * LINE_PX;
  const services = lanes > 2 ? 0 : Math.max(0, Math.floor(free / LINE_PX));
  return { name, meta, services };
}

/**
 * The time gutter's bands: one per hour (from its first slot to the next hour), plus a band for
 * the slots before the first full hour when the spa opens off the hour. Rows are 0-based slots.
 */
export function gutterBands(
  opens: number,
  closes: number,
  slot: number,
): { row: number; span: number; m: number }[] {
  const rows = Math.max(1, Math.round((closes - opens) / slot));
  const bands: { row: number; span: number; m: number }[] = [];
  for (let r = 0; r < rows; r += 1) {
    const m = opens + r * slot;
    const last = bands.at(-1);
    if (last && m % 60 !== 0) last.span += 1;
    else bands.push({ row: r, span: 1, m });
  }
  return bands;
}
