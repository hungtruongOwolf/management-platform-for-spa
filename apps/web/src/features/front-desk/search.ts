/**
 * URL state of /quay (lib/searchState.ts): which tab, and on the calendar which day and view, so a
 * reload or Back keeps the desk where it was. Defaults (record tab, today, day view) stay out of the URL.
 */
import { param, searchSchema } from "../../lib/searchState";

export const frontDeskSearch = searchSchema({
  /** "ghi" = record a visit (default, the walk-in fast path); "lich" = appointments calendar. */
  view: param.oneOf(["ghi", "lich"], "ghi"),
  /** Calendar day `YYYY-MM-DD`; "" = today. */
  ngay: param.day(),
  /** Calendar layout: day grid, week overview, or the list (default on phones). */
  kieu: param.oneOf(["ngay", "tuan", "ds"], "ngay"),
  /** "Sửa lượt": id of the visit to open in the cart (from the day list or a customer's history). */
  sua: param.text(40),
});
