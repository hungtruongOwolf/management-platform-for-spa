import { param, searchSchema, type Parser } from "../../lib/searchState";

export const STAFF_TABS = ["attendance", "evaluation", "adjustments", "payroll", "profiles"] as const;
export type StaffTab = (typeof STAFF_TABS)[number];

/** `YYYY-MM` (2020–2100); "" = the current month. */
const month: Parser<string> = (raw) =>
  typeof raw === "string" && /^20[2-9]\d-(0[1-9]|1[0-2])$/.test(raw) ? raw : "";

/** /nhan-vien: tab, month and the day of the phone day-list, all in the URL (ADR-0007). */
export const staffSearch = searchSchema({
  tab: param.oneOf(STAFF_TABS, "attendance"),
  month,
  day: param.day(),
});
