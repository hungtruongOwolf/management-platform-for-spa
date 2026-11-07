import type { StaffProfile } from "./api";
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

/**
 * Decision C reads the login link, not the position: a "Quản lý" profile without a login counts
 * as a technician when someone records for them. The roster says so instead of letting it surprise.
 */
export function managerWithoutLogin(p: Pick<StaffProfile, "position" | "login_linked" | "status">): boolean {
  return p.status === "active" && !p.login_linked && /quản lý/i.test(p.position);
}
