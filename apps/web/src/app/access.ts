import { can, hasFeature, type Me } from "../features/auth/session";
import { canSeeManagement } from "./features";

/**
 * Who may open each screen: one rule for the nav (what is shown) and the route guards (what opens
 * by URL). UX only: every API behind these screens answers 403 on its own.
 */
export const mayOpen = {
  frontDesk: (me: Me | null) => can(me, "visit.record"),
  customers: (me: Me | null) => can(me, "customer.read"),
  /** Management views, plus what their API checks (`GET /packages`: customer.read). */
  packages: (me: Me | null) => canSeeManagement(me) && can(me, "customer.read"),
  /** `GET /customers/inactive`: customer.read + visit.read. */
  inactive: (me: Me | null) => canSeeManagement(me) && can(me, "customer.read") && can(me, "visit.read"),
  /** "Báo cáo kinh doanh" and "Nhật ký ghi chú": features a spa may have off (ADR-0018). */
  reports: (me: Me | null) => canSeeManagement(me) && hasFeature(me, "reports"),
  notes: (me: Me | null) => canSeeManagement(me) && hasFeature(me, "notes_journal"),
  /** "Nhật ký thao tác" (ADR-0021): the owner's audit trail, `audit.read` only (owner-only grant). */
  activity: (me: Me | null) => can(me, "audit.read"),
  catalog: (me: Me | null) => can(me, "catalog.read"),
  /** Keeps the staff books (attendance, reviews, payroll, profiles). */
  staffBooks: (me: Me | null) =>
    ["attendance.record", "evaluation.record", "payroll.read", "staff.manage"].some((p) => can(me, p)),
  staff: (me: Me | null) => mayOpen.staffBooks(me) || mayOpen.myPay(me),
  /** "Công và lương của tôi": a working member, or a former one (ADR-0014, past months only). */
  myPay: (me: Me | null) => can(me, "staff.read_own") || can(me, "staff.read_own_past"),
  /** Its only API, `GET /me/appointments`, belongs to the appointments feature. */
  myWork: (me: Me | null) => can(me, "staff.read_own") && hasFeature(me, "appointments"),
};

/** A route's guard refused the signed-in role: the error page shows "Không có quyền". */
export class ForbiddenRoute extends Error {
  constructor() {
    super("forbidden route");
    this.name = "ForbiddenRoute";
  }
}
