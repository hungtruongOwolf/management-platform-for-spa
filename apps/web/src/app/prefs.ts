/**
 * Per-device conveniences kept in localStorage: colour theme, the collapsed sidebar and the
 * report chart's previous-period line. Storage can
 * be missing or throw (private mode, blocked site data), so every access falls back to a default.
 */

export type Theme = "system" | "light" | "dark";

const THEME_KEY = "vpyd.theme";
const SIDEBAR_KEY = "vpyd.sidebar";
const PREVIOUS_LINE_KEY = "vpyd.report.previousLine";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // A preference that cannot be saved still applies for this visit.
  }
}

/** The theme chosen during this visit, so it shows even when storage refuses to keep it. */
let chosen: Theme | null = null;

export function readTheme(): Theme {
  if (chosen) return chosen;
  const t = read(THEME_KEY);
  return t === "light" || t === "dark" ? t : "system";
}

const themeListeners = new Set<() => void>();

/**
 * The browser chrome follows the app's theme too (ADR-0019 D20). index.html has one
 * `theme-color` per colour scheme (`media`); a chosen theme gives every one of them that theme's
 * colour, "system" puts each back to its own. The colours stay in index.html (`data-color` keeps
 * the original).
 */
function syncThemeColor(theme: Theme, doc: Document): void {
  const metas = [...doc.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')];
  const own = (m: HTMLMetaElement) => (m.dataset["color"] ??= m.content);
  for (const m of metas) own(m);
  const pick = metas.find((m) => m.media.includes(theme));
  const color = theme === "system" || !pick ? null : own(pick);
  for (const m of metas) m.content = color ?? own(m);
}

/** Applies a theme (tokens.css reads `data-theme`) and remembers it on this device. */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  if (theme === "system") delete root.dataset["theme"];
  else root.dataset["theme"] = theme;
  syncThemeColor(theme, root.ownerDocument);
  write(THEME_KEY, theme === "system" ? null : theme);
  chosen = theme;
  for (const notify of themeListeners) notify();
}

/** For useSyncExternalStore: every picker (account menu, settings) shows the one stored value. */
export function subscribeTheme(listener: () => void): () => void {
  themeListeners.add(listener);
  return () => {
    themeListeners.delete(listener);
  };
}

export function readSidebarCollapsed(): boolean {
  return read(SIDEBAR_KEY) === "collapsed";
}

export function saveSidebarCollapsed(collapsed: boolean): void {
  write(SIDEBAR_KEY, collapsed ? "collapsed" : null);
}

/** The "Kỳ trước" line on the report's trend chart: off unless this viewer turned it on. */
export function readPreviousLine(): boolean {
  return read(PREVIOUS_LINE_KEY) === "on";
}

export function savePreviousLine(on: boolean): void {
  write(PREVIOUS_LINE_KEY, on ? "on" : null);
}
