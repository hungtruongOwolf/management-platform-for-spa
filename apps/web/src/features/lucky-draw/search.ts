/**
 * URL state of /vong-quay/$drawId (lib/searchState.ts): the participants' filter chip and search.
 * The screen parses it itself, so the schema stays in its lazy chunk (no `validateSearch`).
 */
import { param, searchSchema } from "../../lib/searchState";
import type { PeopleFilter } from "./draw";

export const PEOPLE_FILTERS = ["all", "female", "male", "won"] as const satisfies readonly PeopleFilter[];

/** ?loc=female&q=lan — defaults (everyone, no search) stay out of the URL. */
export const drawSearch = searchSchema({
  loc: param.oneOf<PeopleFilter>(PEOPLE_FILTERS, "all"),
  q: param.text(60),
});
