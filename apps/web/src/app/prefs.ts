/**
 * Per-device conveniences kept in localStorage: colour theme and the collapsed sidebar. Storage can
 * be missing or throw (private mode, blocked site data), so every access falls back to a default.
 */

export type Theme = "system" | "light" | "dark";

const THEME_KEY = "vpyd.theme";
const SIDEBAR_KEY = "vpyd.sidebar";

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

export function readTheme(): Theme {
  const t = read(THEME_KEY);
  return t === "light" || t === "dark" ? t : "system";
}

/** Applies a theme (tokens.css reads `data-theme`) and remembers it on this device. */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  if (theme === "system") delete root.dataset["theme"];
  else root.dataset["theme"] = theme;
  write(THEME_KEY, theme === "system" ? null : theme);
}

export function readSidebarCollapsed(): boolean {
  return read(SIDEBAR_KEY) === "collapsed";
}

export function saveSidebarCollapsed(collapsed: boolean): void {
  write(SIDEBAR_KEY, collapsed ? "collapsed" : null);
}
