/** URL state of /khach-hang (lib/searchState.ts): the search box, so Back returns to the same hits. */
import { param, searchSchema } from "../../lib/searchState";

/** Mirrors the server's bound on `q` (crm::service::MAX_QUERY_CHARS). */
export const customersSearch = searchSchema({
  q: param.text(100),
});
