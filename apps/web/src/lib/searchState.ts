/**
 * List state in the URL (frontend.md "State management"): search text, filters, tab, page. The URL
 * is the only copy, so Back, a reload or a shared link brings the exact list back.
 *
 * A screen declares its state once as a `SearchSchema` (one parser per key, each falling back to its
 * default on a missing or bad value — the URL is user input). The route passes `schema.validate` to
 * `validateSearch`, which types `Link search={…}` everywhere, and the screen reads and writes it with
 * `useSearchState(schema)`. Defaults are left out of the URL so plain links stay plain.
 *
 * Hand-written parsers instead of zod: router.tsx is on the startup path (≤ 125 KB gzip budget).
 */
import { useNavigate, useRouterState } from "@tanstack/react-router";
import { useCallback, useMemo } from "react";

/** Turns one raw search value (string, number, anything the URL held) into a typed value. */
export type Parser<T> = (raw: unknown) => T;

type Shape = Record<string, unknown>;

export interface SearchSchema<T extends Shape> {
  /** Always returns a complete, valid state. */
  parse: (raw: Record<string, unknown>) => T;
  /**
   * For the route's `validateSearch`: the valid non-default keys only. Typed as partial, so a
   * `<Link to>` without `search` stays legal and bad or default values never reach the URL.
   */
  validate: (raw: Record<string, unknown>) => Partial<T>;
  /** The values a missing key stands for. */
  defaults: T;
  /** Drops keys equal to their default, so the URL only carries what the user changed. */
  compact: (state: T) => Partial<T>;
}

export function searchSchema<T extends Shape>(parsers: { [K in keyof T]: Parser<T[K]> }): SearchSchema<T> {
  const keys = Object.keys(parsers) as (keyof T)[];
  const parse = (raw: Record<string, unknown>): T => {
    const out = {} as T;
    for (const k of keys) out[k] = parsers[k](raw[k as string]);
    return out;
  };
  const defaults = parse({});
  const compact = (state: T): Partial<T> => {
    const out: Partial<T> = {};
    for (const k of keys) if (state[k] !== defaults[k]) out[k] = state[k];
    return out;
  };
  return { parse, defaults, compact, validate: (raw) => compact(parse(raw)) };
}

/** Parsers for the usual kinds of list state. */
export const param = {
  /** One of a fixed set of strings (filter, tab). */
  oneOf<const V extends string>(values: readonly V[], fallback: NoInfer<V>): Parser<V> {
    return (raw) => (typeof raw === "string" && (values as readonly string[]).includes(raw) ? (raw as V) : fallback);
  },
  /** One of a fixed set of numbers (threshold, page size); "14" from a hand-typed URL counts. */
  oneOfNumber<const V extends number>(values: readonly V[], fallback: NoInfer<V>): Parser<V> {
    return (raw) => {
      const n = typeof raw === "string" ? Number(raw) : raw;
      return typeof n === "number" && (values as readonly number[]).includes(n) ? (n as V) : fallback;
    };
  },
  /** Free text (search box), trimmed to `max` characters; "" when absent. */
  text(max = 100): Parser<string> {
    return (raw) => (typeof raw === "string" ? raw.slice(0, max) : typeof raw === "number" ? String(raw).slice(0, max) : "");
  },
  /** A whole number in [min, max] (page number); `fallback` otherwise. */
  int(fallback: number, min = 0, max = Number.MAX_SAFE_INTEGER): Parser<number> {
    return (raw) => {
      const n = typeof raw === "string" ? Number(raw) : raw;
      return typeof n === "number" && Number.isInteger(n) && n >= min && n <= max ? n : fallback;
    };
  },
  /** A calendar day `YYYY-MM-DD`; "" when absent or malformed (the screen picks its own default). */
  day(): Parser<string> {
    // A real calendar day: Date rolls "2026-02-30" over to March, so the round trip must match.
    return (raw) => (typeof raw === "string" && /^\d{4}-\d{2}-\d{2}$/.test(raw) && isCalendarDay(raw) ? raw : "");
  },
};

export interface SetSearchOptions {
  /**
   * Add a history entry instead of replacing the current one. Default false: typing in a search box
   * or flipping a filter must not fill Back with every keystroke.
   */
  push?: boolean;
}

/**
 * Reads and writes the current route's search params through its schema. `set` merges a patch, so a
 * screen changes one key without knowing the others; it keeps the scroll position.
 */
export function useSearchState<T extends Shape>(
  schema: SearchSchema<T>,
): [T, (patch: Partial<T>, options?: SetSearchOptions) => void] {
  const raw = useRouterState({ select: (s) => s.location.search as Record<string, unknown> });
  const navigate = useNavigate();
  const state = useMemo(() => schema.parse(raw), [schema, raw]);
  const set = useCallback(
    (patch: Partial<T>, options: SetSearchOptions = {}) => {
      void navigate({
        to: ".",
        // The router hands back the current search; the schema re-validates it, then defaults drop out.
        search: ((prev: Record<string, unknown>) => schema.compact({ ...schema.parse(prev), ...patch })) as never,
        replace: !options.push,
        resetScroll: false,
      });
    },
    [navigate, schema],
  );
  return [state, set];
}

function isCalendarDay(iso: string): boolean {
  const d = new Date(`${iso}T00:00:00Z`);
  return !Number.isNaN(d.getTime()) && d.toISOString().startsWith(iso);
}
