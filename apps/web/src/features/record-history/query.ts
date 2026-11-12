// "Ai ghi, ai sửa" on each record (ADR-0022), the part every screen imports: the request and who
// may ask for it. Deliberately tiny and free of UI and words (design record-history §1.1): the
// screens import it to start the request while the lazy chunk with the component is still loading.
import type { RecordHistory } from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { can, type Me } from "../auth/session";

/**
 * A record whose history a screen opens, by the route it is asked through. A sale or a refund is
 * re-anchored by the server (a one-off sale's history is its visit's, a package sale's and a
 * refund's is the package's), so the reply's `record` may be another kind. A cell of a staff book
 * is a person and a day; the spa's settings are one of each.
 */
export type RecordRef = RecordIdRef | StaffDayRef | SettingsRef;

/** A record named by its id: every kind but a staff-book cell and the settings. */
export interface RecordIdRef {
  type:
    | "visit"
    | "sale"
    | "refund"
    | "customer_package"
    | "customer"
    | "appointment"
    | "adjustment"
    | "service"
    | "package_plan"
    | "service_category";
  id: string;
}

/** One person's cell of one staff book on one day (`YYYY-MM-DD`). */
export interface StaffDayRef {
  type: "attendance" | "daily_review";
  staffId: string;
  date: string;
}

/** The spa's payroll rules or its calendar's "số khách tối đa": one per spa, no id. */
export interface SettingsRef {
  type: "payroll_rules" | "appointment_settings";
}

const isStaffDay = (ref: RecordRef): ref is StaffDayRef => ref.type === "attendance" || ref.type === "daily_review";

/**
 * One record's history. Never served from cache alone (`staleTime: 0`): it is asked **once** per
 * open (`LazyRecordTrace` prefetches it, `RecordTrace` does not ask again for a reply of the same
 * open, its `openedAt`), and every write in this browser invalidates `["record-history"]`
 * (lib/queryClient.ts). The route is the type with a dash (`customer_package` → `customer-package`).
 */
export function recordHistoryQuery(ref: RecordRef) {
  const route = ref.type.replace("_", "-");
  const [key, path] = isStaffDay(ref)
    ? [
        [ref.staffId, ref.date],
        `/record-history/staff-day/${encodeURIComponent(ref.staffId)}/${encodeURIComponent(ref.date)}?book=${ref.type}`,
      ]
    : "id" in ref
      ? [[ref.id], `/record-history/${route}/${encodeURIComponent(ref.id)}`]
      : [[], `/record-history/settings/${route}`];
  return queryOptions({
    queryKey: ["record-history", ref.type, ...key],
    queryFn: async () => (await api(path)) as RecordHistory,
    staleTime: 0,
  });
}

/**
 * What the record's history ends up being about, for the read right of that kind (spec D7, AC-H4):
 * `visit` = a visit or a one-off sale made in one; `package` = a package, a package sale, a refund;
 * a booking, a staff-book cell, a bonus / penalty, the price list, the settings: their screen's own
 * right.
 */
export type TraceAnchor =
  | "visit"
  | "package"
  | "customer"
  | "appointment"
  | "attendance"
  | "daily_review"
  | "adjustment"
  | "catalog"
  | "payroll_rules"
  | "appointment_settings";

/**
 * Whether to offer the history at all (UX only: the server checks the same and answers 403). The
 * history right, the right of the route (`sale.read` when asked through a sale or a refund), then
 * the read right of the kind the record re-anchors to: the chunk is never loaded and no request is
 * sent only to be refused.
 */
export function canTrace(me: Me | null | undefined, anchor: TraceAnchor, through?: "sale" | "refund"): boolean {
  if (!can(me, "record.history.read")) return false;
  if (through && !can(me, "sale.read")) return false;
  switch (anchor) {
    case "visit":
      return can(me, "visit.read");
    case "package":
    case "customer":
      return can(me, "customer.read");
    case "appointment":
      return can(me, "appointment.manage");
    case "attendance":
      return can(me, "attendance.record");
    case "daily_review":
    case "adjustment":
      return can(me, "evaluation.record");
    case "catalog":
      return can(me, "catalog.manage");
    case "payroll_rules":
      return can(me, "payroll.read");
    case "appointment_settings":
      return can(me, "appointment.settings");
  }
}
