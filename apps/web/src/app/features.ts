import { can, type Me } from "../features/auth/session";

/**
 * UX gate of the management views (reports, package overview, absent customers, customer
 * history): the API's `report.read`. Money inside them needs `report.revenue.read` and is simply
 * absent from the replies otherwise. The server decides both; this only hides dead ends.
 */
export function canSeeManagement(me: Me | null | undefined): boolean {
  return can(me, "report.read");
}
