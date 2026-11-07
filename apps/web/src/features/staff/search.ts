import { param, searchSchema, type Parser } from "../../lib/searchState";

/** Nhân sự filter: "" = Đang làm (default), `left` = Đã nghỉ, `all` = Tất cả. */
export const ROSTER_FILTERS = ["", "left", "all"] as const;
export type RosterFilter = (typeof ROSTER_FILTERS)[number];

export const STAFF_TABS = ["day", "month", "evaluation", "payroll", "profiles"] as const;
export type StaffTab = (typeof STAFF_TABS)[number];

/** Round-4 links (`?tab=attendance|adjustments`) open the tab that now holds that content. */
const LEGACY_TAB: Partial<Record<string, StaffTab>> = { attendance: "day", adjustments: "month" };

const tab: Parser<StaffTab> = (raw) => (typeof raw === "string" && LEGACY_TAB[raw]) || param.oneOf(STAFF_TABS, "day")(raw);

/** `YYYY-MM` (2020–2100); "" = the current month. */
const month: Parser<string> = (raw) =>
  typeof raw === "string" && /^20[2-9]\d-(0[1-9]|1[0-2])$/.test(raw) ? raw : "";

/** /nhan-vien: tab, month and the day of the daily book, all in the URL (ADR-0007). */
export const staffSearch = searchSchema({
  tab,
  month,
  day: param.day(),
  /** Nhân sự filter: "" = Đang làm, `left` = Đã nghỉ, `all` = Tất cả. */
  roster: param.oneOf(ROSTER_FILTERS, ""),
});

/** /khach-cua-toi: the calendar's day ("" = today) and view ("" = the device's default). */
export const myWorkSearch = searchSchema({
  ngay: param.day(),
  kieu: param.oneOf(["", "ngay", "tuan", "ds"], ""),
});
