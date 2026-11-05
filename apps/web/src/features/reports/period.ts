/**
 * Report periods (EM round 3: "30 ngày, 1 tháng, 1 năm… linh hoạt"): a preset or a custom range,
 * resolved against the spa's today into business days, a chart bucket size, and the previous
 * period of the same length the KPI row compares against. Pure, so it is unit tested.
 */

import { PRESETS, type Preset } from "./search";

export { PRESETS, type Preset };

export type Bucket = "day" | "week" | "month" | "year";

export interface Period {
  preset: Preset;
  from: string;
  to: string;
  bucket: Bucket;
  /** The period of equal length right before (calendar-aligned for month and year presets). */
  previous: { from: string; to: string };
  /** "7 ngày qua", "Tháng 11/2026", "01/10 – 15/10/2026". */
  label: string;
  /** "so với 7 ngày trước đó". */
  compareLabel: string;
}

export const PRESET_LABEL: Record<Preset, string> = {
  today: "Hôm nay",
  "7d": "7 ngày",
  "30d": "30 ngày",
  this_month: "Tháng này",
  last_month: "Tháng trước",
  this_year: "Năm nay",
  "12m": "12 tháng",
  custom: "Tùy chọn",
};

/**
 * Longest custom range: `/reports/range` serves 366 days by day or week and 732 days (2 years) by
 * month or year (longer: 422 `range_too_long`); `bucketFor` never picks day/week past a quarter.
 */
export const MAX_RANGE_DAYS = 732;

const DAY_MS = 86_400_000;

function toDate(iso: string): Date {
  return new Date(`${iso}T00:00:00Z`);
}

function iso(d: Date): string {
  return d.toISOString().slice(0, 10);
}

export function shiftDays(day: string, days: number): string {
  return iso(new Date(toDate(day).getTime() + days * DAY_MS));
}

/** Inclusive number of days from `from` to `to`. */
export function daysBetween(from: string, to: string): number {
  return Math.round((toDate(to).getTime() - toDate(from).getTime()) / DAY_MS) + 1;
}

function ymd(day: string): [number, number, number] {
  const d = toDate(day);
  return [d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate()];
}

function make(y: number, m: number, d: number): string {
  return iso(new Date(Date.UTC(y, m, d)));
}

function lastDayOfMonth(y: number, m: number): number {
  return new Date(Date.UTC(y, m + 1, 0)).getUTCDate();
}

/** Same day-of-month `months` earlier, clamped to that month's length (31/03 → 29/02). */
function shiftMonths(day: string, months: number): string {
  const [y, m, d] = ymd(day);
  const target = new Date(Date.UTC(y, m - months, 1));
  const ty = target.getUTCFullYear();
  const tm = target.getUTCMonth();
  return make(ty, tm, Math.min(d, lastDayOfMonth(ty, tm)));
}

/** Day buckets up to a month, weeks up to a quarter, months up to two years, then years. */
export function bucketFor(days: number): Bucket {
  if (days <= 31) return "day";
  if (days <= 92) return "week";
  if (days <= 731) return "month";
  return "year";
}

/** "30/10" and "30/10/2026": written by hand, because vi-VN Intl prints day+month as "30-10". */
const dm = { format: (d: Date) => `${String(d.getUTCDate()).padStart(2, "0")}/${String(d.getUTCMonth() + 1).padStart(2, "0")}` };
const dmy = { format: (d: Date) => `${dm.format(d)}/${String(d.getUTCFullYear())}` };

/** "01/10 – 15/10/2026", or "05/11/2026" for one day. */
export function rangeLabel(from: string, to: string): string {
  if (from === to) return dmy.format(toDate(from));
  const sameYear = from.slice(0, 4) === to.slice(0, 4);
  return `${(sameYear ? dm : dmy).format(toDate(from))} – ${dmy.format(toDate(to))}`;
}

const monthLabel = (day: string) => {
  const [y, m] = ymd(day);
  return `Tháng ${String(m + 1)}/${String(y)}`;
};

function sameLengthBefore(from: string, to: string) {
  const len = daysBetween(from, to);
  const prevTo = shiftDays(from, -1);
  return { from: shiftDays(prevTo, -(len - 1)), to: prevTo };
}

/**
 * Resolves a preset (or a custom `from`/`to`) on the spa's `today`. Invalid custom input
 * (missing, reversed, future) falls back to the last 7 days so a hand-edited URL never breaks.
 */
export function resolvePeriod(preset: Preset, today: string, custom?: { from?: string; to?: string }): Period {
  const [y, m, d] = ymd(today);
  switch (preset) {
    case "today":
      return {
        preset,
        from: today,
        to: today,
        bucket: "day",
        previous: { from: shiftDays(today, -1), to: shiftDays(today, -1) },
        label: "Hôm nay",
        compareLabel: "so với hôm qua",
      };
    case "7d":
    case "30d": {
      const len = preset === "7d" ? 7 : 30;
      const from = shiftDays(today, -(len - 1));
      return {
        preset,
        from,
        to: today,
        bucket: "day",
        previous: sameLengthBefore(from, today),
        label: `${String(len)} ngày qua`,
        compareLabel: `so với ${String(len)} ngày trước đó`,
      };
    }
    case "this_month": {
      const from = make(y, m, 1);
      const prevFrom = shiftMonths(from, 1);
      return {
        preset,
        from,
        to: today,
        bucket: "day",
        previous: { from: prevFrom, to: shiftMonths(today, 1) },
        label: monthLabel(today),
        compareLabel: "so với cùng kỳ tháng trước",
      };
    }
    case "last_month": {
      const from = make(y, m - 1, 1);
      const to = make(y, m, 0);
      const prevFrom = make(y, m - 2, 1);
      return {
        preset,
        from,
        to,
        bucket: "day",
        previous: { from: prevFrom, to: make(y, m - 1, 0) },
        label: monthLabel(from),
        compareLabel: "so với tháng trước đó",
      };
    }
    case "this_year": {
      const from = make(y, 0, 1);
      return {
        preset,
        from,
        to: today,
        bucket: "month",
        previous: { from: make(y - 1, 0, 1), to: make(y - 1, m, Math.min(d, lastDayOfMonth(y - 1, m))) },
        label: `Năm ${String(y)}`,
        compareLabel: "so với cùng kỳ năm trước",
      };
    }
    case "12m": {
      const from = make(y, m - 11, 1);
      return {
        preset,
        from,
        to: today,
        bucket: "month",
        previous: { from: make(y - 1, m - 11, 1), to: shiftMonths(today, 12) },
        label: "12 tháng qua",
        compareLabel: "so với 12 tháng trước đó",
      };
    }
    case "custom": {
      const valid = (s: string | undefined): s is string => !!s && /^\d{4}-\d{2}-\d{2}$/.test(s) && s <= today;
      const from = custom?.from;
      const to = custom?.to;
      if (!valid(from) || !valid(to) || from > to || daysBetween(from, to) > MAX_RANGE_DAYS) {
        return { ...resolvePeriod("7d", today), preset: "custom" };
      }
      return {
        preset,
        from,
        to,
        bucket: bucketFor(daysBetween(from, to)),
        previous: sameLengthBefore(from, to),
        label: rangeLabel(from, to),
        compareLabel: "so với kỳ trước cùng độ dài",
      };
    }
  }
}

export function isPreset(v: unknown): v is Preset {
  return typeof v === "string" && (PRESETS as readonly string[]).includes(v);
}

/** Bucket label on the chart axis: "05/11", "T11/26", "2026"; weeks show their Monday. */
export function bucketLabel(start: string, bucket: Bucket): string {
  if (bucket === "year") return start.slice(0, 4);
  if (bucket === "month") {
    const [y, m] = ymd(start);
    return `T${String(m + 1)}/${String(y).slice(2)}`;
  }
  return dm.format(toDate(start));
}
