/**
 * URL state of /vong-quay/$drawId (lib/searchState.ts): the side column's tab, the participants'
 * filter chip and search.
 * The screen parses it itself, so the schema stays in its lazy chunk (no `validateSearch`).
 */
import { param, searchSchema } from "../../lib/searchState";
import type { PeopleFilter } from "./draw";

export const PEOPLE_FILTERS = ["all", "female", "male", "won"] as const satisfies readonly PeopleFilter[];

/** The side column's tabs: prizes (the default), winners, participants. */
export const SIDE_TABS = ["giai", "trung", "nguoi"] as const;
export type SideTab = (typeof SIDE_TABS)[number];

/** ?tab=nguoi&loc=female&q=lan — defaults (prizes, everyone, no search) stay out of the URL. */
export const drawSearch = searchSchema({
  tab: param.oneOf<SideTab>(SIDE_TABS, "giai"),
  loc: param.oneOf<PeopleFilter>(PEOPLE_FILTERS, "all"),
  q: param.text(60),
});
