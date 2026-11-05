/**
 * The in-app navigation trail (design-system.md "Điều hướng"): which screen the user came from, under
 * what title, so a detail page's back control can say "‹ Theo dõi gói" and return there with its
 * filters and scroll intact.
 *
 * Keyed by the browser history index TanStack Router keeps in `history.state.__TSR_index`: Back and
 * Forward move the index, a push writes a new one, a replace (list state typed into the URL)
 * rewrites the current one. Kept in sessionStorage (per tab, survives a reload, dies with the tab);
 * storage failures only lose the trail, never the page.
 */

export interface TrailEntry {
  href: string;
  pathname: string;
  /** The page's own title, set by its PageHeader once rendered. */
  title?: string;
}

const KEY = "vpyd.trail";
const MAX = 60;
/** Screens a signed-in page must never send "back" to. */
const OUTSIDE = ["/dang-nhap", "/xac-thuc"];

let entries = new Map<number, TrailEntry>();
let loaded = false;

function load(): void {
  if (loaded) return;
  loaded = true;
  try {
    const raw = sessionStorage.getItem(KEY);
    if (!raw) return;
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return;
    for (const item of parsed) {
      if (!Array.isArray(item)) continue;
      const [index, entry] = item as [unknown, unknown];
      if (typeof index !== "number" || typeof entry !== "object" || entry === null) continue;
      const e = entry as Record<string, unknown>;
      if (typeof e["href"] !== "string" || typeof e["pathname"] !== "string") continue;
      entries.set(index, {
        href: e["href"],
        pathname: e["pathname"],
        ...(typeof e["title"] === "string" ? { title: e["title"] } : {}),
      });
    }
  } catch {
    entries = new Map();
  }
}

function save(): void {
  try {
    const recent = [...entries].sort(([a], [b]) => a - b).slice(-MAX);
    sessionStorage.setItem(KEY, JSON.stringify(recent));
  } catch {
    // Private mode or a full quota: the trail lives in memory for this visit.
  }
}

/** Called after every resolved navigation (router.tsx). */
export function recordLocation(index: number, href: string, pathname: string): void {
  load();
  const old = entries.get(index);
  entries.set(index, {
    href,
    pathname,
    // A replace on the same screen (filters) keeps its title; a different screen starts untitled.
    ...(old?.pathname === pathname && old.title ? { title: old.title } : {}),
  });
  save();
}

/**
 * Called by PageHeader: the title the next screen's back control shows. May run before the router
 * reports the navigation, so it creates the entry when needed (recordLocation then keeps the title).
 */
export function setTitle(index: number, href: string, pathname: string, title: string): void {
  load();
  const e = entries.get(index);
  if (e?.pathname === pathname && e.title === title) return;
  entries.set(index, { href: e?.pathname === pathname ? e.href : href, pathname, title });
  save();
}

/** Where the browser's Back would land inside the app, if it is another signed-in screen. */
export function previousEntry(index: number, pathname: string): TrailEntry | undefined {
  load();
  const prev = entries.get(index - 1);
  if (!prev || prev.pathname === pathname || OUTSIDE.includes(prev.pathname)) return undefined;
  return prev;
}

/**
 * The last URL (with its list state) this tab showed for `pathname` at or before `index`, so a
 * breadcrumb to a list reopens it as the user left it.
 */
export function lastHrefFor(pathname: string, index: number): string | undefined {
  load();
  let best: [number, string] | undefined;
  for (const [i, e] of entries) {
    if (i <= index && e.pathname === pathname && (!best || i > best[0])) best = [i, e.href];
  }
  return best?.[1];
}

/** Tests only: forget everything. */
export function resetTrail(): void {
  entries = new Map();
  loaded = true;
  try {
    sessionStorage.removeItem(KEY);
  } catch {
    // nothing to forget
  }
}
