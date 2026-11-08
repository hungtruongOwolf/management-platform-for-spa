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

/**
 * Whether a position names a manager, whatever the case, accents or Unicode form ("Quản lý",
 * "quan ly"): the same rule as the server's `org::profile::is_manager_position`, for warnings only.
 */
export function isManagerPosition(position: string): boolean {
  return position
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/[đĐ]/g, "d")
    .toLowerCase()
    .replace(/\s+/g, " ")
    .includes("quan ly");
}

/**
 * F19 (live test 2026-11-08): decision C needs to know which profile is a manager's own, and only
 * the link between the profile and the login says so. Until the owner links it, a "Quản lý"
 * profile is the owner's to record (the server refuses a manager) and the manager cannot read
 * their own month. The roster and the daily book warn the owner.
 */
export function managerWithoutLogin(p: Pick<StaffProfile, "position" | "login_linked" | "status">): boolean {
  return p.status === "active" && !p.login_linked && isManagerPosition(p.position);
}

/** A manager login (records for others, does not manage staff) not linked to any profile yet. */
export function managerLoginWithoutProfile(a: Pick<StaffAccount, "is_manager" | "staff_id">): boolean {
  return a.is_manager && !a.staff_id;
}
