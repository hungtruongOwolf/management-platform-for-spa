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

/** How much text fits: 1 line under two slots, 2 lines under three, then the services too. */
export function density(heightInSlots: number): "compact" | "short" | "full" {
  if (heightInSlots < 2) return "compact";
  if (heightInSlots < 3) return "short";
  return "full";
}
