import { can, type Me } from "../features/auth/session";
import { canSeeManagement } from "./features";

/**
 * Who may open each screen: one rule for the nav (what is shown) and the route guards (what opens
 * by URL). UX only: every API behind these screens answers 403 on its own.
 */
export const mayOpen = {
  frontDesk: (me: Me | null) => can(me, "visit.record"),
  customers: (me: Me | null) => can(me, "customer.read"),
  management: (me: Me | null) => canSeeManagement(me),
  catalog: (me: Me | null) => can(me, "catalog.read"),
  /** Keeps the staff books (attendance, reviews, payroll, profiles). */
  staffBooks: (me: Me | null) =>
    ["attendance.record", "evaluation.record", "payroll.read", "staff.manage"].some((p) => can(me, p)),
  staff: (me: Me | null) => mayOpen.staffBooks(me) || mayOpen.myPay(me),
  /** "Công và lương của tôi": a working member, or a former one (ADR-0014, past months only). */
  myPay: (me: Me | null) => can(me, "staff.read_own") || can(me, "staff.read_own_past"),
  myWork: (me: Me | null) => can(me, "staff.read_own"),
};

/** A route's guard refused the signed-in role: the error page shows "Không có quyền". */
export class ForbiddenRoute extends Error {
  constructor() {
    super("forbidden route");
    this.name = "ForbiddenRoute";
  }
}
