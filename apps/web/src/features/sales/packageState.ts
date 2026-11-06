// Own module (not sales/api.ts): only lazy screens use it, so it stays out of the startup chunk.

/** The one state a package shows (design-system.md "Quy tắc" 7): never two chips on one package. */
export type PackageState = "active" | "ending" | "expired" | "exhausted" | "cancelled";

/** "Sắp hết": an active package with this many sessions or fewer left… */
export const ENDING_SESSIONS = 2;
/** …or expiring within this many days (the same 30-day window as the "Sắp hết hạn" filter). */
export const ENDING_DAYS = 30;

export const PACKAGE_STATE: Record<PackageState, { label: string; badge: string }> = {
  active: { label: "Đang dùng", badge: "badge-ok" },
  ending: { label: "Sắp hết", badge: "badge-warn" },
  expired: { label: "Hết hạn", badge: "badge-warn" },
  exhausted: { label: "Hết buổi", badge: "badge-muted" },
  cancelled: { label: "Đã hủy", badge: "badge-danger" },
};

interface PackageLike {
  status: "active" | "expired" | "exhausted" | "cancelled";
  sessions_remaining: number;
  expires_on?: string | null;
}

/** Whole days from `today` to `iso` (both YYYY-MM-DD, spa calendar days). */
export function daysBetween(today: string, iso: string): number {
  return Math.round((Date.parse(`${iso}T00:00:00Z`) - Date.parse(`${today}T00:00:00Z`)) / 86_400_000);
}

/**
 * The server's status, plus "Sắp hết" for an active package close to its end (few sessions left
 * or the expiry near). The server stays the authority on what may be done with it.
 */
export function packageState(p: PackageLike, today: string): PackageState {
  if (p.status !== "active") return p.status;
  if (p.sessions_remaining <= ENDING_SESSIONS) return "ending";
  if (p.expires_on && daysBetween(today, p.expires_on) <= ENDING_DAYS) return "ending";
  return "active";
}

/** Closed packages (used up, cancelled) wait behind a disclosure and offer no action. */
export function isClosedPackage(p: PackageLike): boolean {
  return p.status === "cancelled" || p.status === "exhausted" || p.sessions_remaining <= 0;
}
