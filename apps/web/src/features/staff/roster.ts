import type { StaffAccount, StaffProfile } from "./api";
import { ROSTER_FILTERS, type RosterFilter } from "./search";

export { ROSTER_FILTERS, type RosterFilter };

export const ROSTER_FILTER_LABEL: Record<RosterFilter, string> = {
  "": "Đang làm",
  left: "Đã nghỉ",
  all: "Tất cả",
};

export function inFilter(p: StaffProfile, filter: RosterFilter): boolean {
  if (filter === "all") return true;
  return filter === "left" ? p.status === "left" : p.status === "active";
}

/**
 * "Làm bao lâu", from the server's tenure (summed over the employment periods): "2 năm 3 tháng",
 * "5 tháng", "12 ngày"; `null` = a start date was never recorded.
 */
export function tenureLabel(p: Pick<StaffProfile, "tenure_days" | "tenure_months">): string {
  const months = p.tenure_months;
  const days = p.tenure_days;
  if (months == null || days == null) return "Chưa rõ ngày vào làm";
  if (months < 1) return days > 0 ? `${String(days)} ngày` : "Chưa bắt đầu";
  const years = Math.floor(months / 12);
  const rest = months % 12;
  if (years === 0) return `${String(rest)} tháng`;
  return rest === 0 ? `${String(years)} năm` : `${String(years)} năm ${String(rest)} tháng`;
}

/** Positions offered when adding someone (free text stays possible: the spa names its own roles). */
export const POSITIONS = ["Kỹ thuật viên", "Quản lý", "Thu ngân", "Lễ tân", "Học viên"] as const;

/** A working profile with no login yet: counts as a technician; the owner is nudged to give one. */
export function withoutLogin(p: Pick<StaffProfile, "login_linked" | "status">): boolean {
  return p.status === "active" && !p.login_linked;
}

/**
 * A manager login (its role records for others without managing staff) linked to no profile: it
 * records nothing until the owner links it (`staff.link_required`, EM 2026-11-08).
 */
export function managerLoginWithoutProfile(a: Pick<StaffAccount, "is_manager" | "staff_id">): boolean {
  return a.is_manager && !a.staff_id;
}

/**
 * A login with no profile that is not the owner's (ADR-0016: logins are made from a profile, so
 * only older, CLI-made logins are like this): the owner links each once.
 */
export function legacyLogin(a: Pick<StaffAccount, "manages_staff" | "staff_id">): boolean {
  return !a.manages_staff && !a.staff_id;
}
