// "Sửa lượt" (spec R1-1 US6b): the cart pre-filled from a recorded visit, and the decision of how
// to save it. Changes that touch neither money nor sessions (technician, time, note) are a direct
// PATCH; anything else is one server-side correction (old visit voided + corrected one recorded).
// A lazy chunk: only loaded when the desk opens a correction (startup budget, frontend.md).
import type {
  CustomerSummary,
  PaymentCreate,
  Visit,
  VisitCorrected,
  VisitCorrection,
  VisitUpdate,
} from "@vpyd/api-client";
import type { QueryClient } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { api, ifMatch } from "../../lib/api";
import { customerQuery } from "../customers/api";
import {
  type CartLine,
  type DiscountDraft,
  type PaymentMode,
  discountBody,
  discountPreview,
  amountDue,
  timeOf,
  visitBody,
} from "./api";

/** Everything the cart needs to open a recorded visit for correction. */
export interface CorrectionStart {
  /** The visit as read with `GET /visits/{id}` (note included when the caller may read it). */
  visit: Visit;
  customer: CustomerSummary;
  /** Cart lines; keys are provisional (the page renumbers them). */
  lines: CartLine[];
  mode: PaymentMode;
  cashText: string;
  discount: DiscountDraft | null;
  /** "" = today (the page's convention); otherwise the visit's day. */
  serviceDay: string;
  serviceTime: string;
  note: string;
}

/** One cart line per session: the desk records quantities as repeated lines. */
export function cartFromVisit(v: Visit): CartLine[] {
  const lines: CartLine[] = [];
  for (const l of v.lines) {
    for (let i = 0; i < Math.max(1, l.qty); i++) {
      if (l.source === "package" && l.customer_package_item_id) {
        lines.push({
          key: lines.length,
          source: "package",
          serviceId: l.service_id,
          serviceName: l.service_name,
          categoryName: l.category_name,
          itemId: l.customer_package_item_id,
          packageName: l.package_name ?? "",
          staffId: l.staff_id,
          ...(l.expiry_override_reason
            ? { expiryOverrideReason: l.expiry_override_reason }
            : {}),
        });
      } else {
        const variant = l.price_variant;
        lines.push({
          key: lines.length,
          source: "one_off",
          serviceId: l.service_id,
          serviceName: l.service_name,
          categoryName: l.category_name,
          // Sent only for a customer without a recorded gender, as in a new visit.
          priceVariant:
            !v.customer_gender && (variant === "female" || variant === "male")
              ? variant
              : null,
          price: l.unit_price,
          staffId: l.staff_id,
          ...(l.referrer_staff_id ? { referrerId: l.referrer_staff_id } : {}),
        });
      }
    }
  }
  return lines;
}

/** How the visit was paid, as the payment picker shows it. */
export function paymentModeOf(v: Visit): {
  mode: PaymentMode;
  cashText: string;
} {
  const paid = v.payments.filter((p) => p.kind === "payment");
  const cash = paid
    .filter((p) => p.method === "cash")
    .reduce((s, p) => s + p.amount, 0);
  const transfer = paid
    .filter((p) => p.method === "bank_transfer")
    .reduce((s, p) => s + p.amount, 0);
  if (cash > 0 && transfer > 0)
    return { mode: "mixed", cashText: String(cash) };
  return { mode: transfer > 0 ? "bank_transfer" : "cash", cashText: "" };
}

/** The visit's discount as an amount draft (the server stored the amount, not the percent). */
export function discountOf(v: Visit): DiscountDraft | null {
  if (v.discount_amount <= 0) return null;
  return {
    kind: "amount",
    value: String(v.discount_amount),
    reason: v.discount_reason ?? "",
  };
}

/** "2026-11-05" of an instant in the spa's zone. */
function spaDay(iso: string): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Ho_Chi_Minh",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(iso));
}

/** Reads the visit (with its note when allowed) and its customer, and builds the cart. */
export async function loadCorrection(
  queryClient: QueryClient,
  visitId: string,
  today: string,
): Promise<CorrectionStart> {
  const visit = (await api(`/visits/${visitId}`)) as Visit;
  const customer = await queryClient.query(customerQuery(visit.customer_id));
  const { mode, cashText } = paymentModeOf(visit);
  const day = spaDay(visit.check_in_at);
  return {
    visit,
    customer: {
      id: customer.id,
      full_name: customer.full_name,
      named: customer.named,
      phone: customer.phone ?? null,
      gender: customer.gender ?? null,
    },
    lines: cartFromVisit(visit),
    mode,
    cashText,
    discount: discountOf(visit),
    serviceDay: day === today ? "" : day,
    serviceTime: timeOf(visit.check_in_at),
    note: typeof visit.note === "string" ? visit.note : "",
  };
}

/** The cart as the desk left it. */
export interface CorrectionDraft {
  customerId: string;
  lines: CartLine[];
  payments: PaymentCreate[];
  discount: DiscountDraft | null;
  /** The service instant picked (null = unchanged). */
  checkInAt: string | null;
  note: string;
  /** The desk may write notes (otherwise the old note is kept untouched). */
  canWriteNote: boolean;
  reason: string;
}

export type CorrectionPlan =
  | { kind: "none" }
  | { kind: "direct"; patch: VisitUpdate }
  | { kind: "correct"; body: VisitCorrection; difference: number };

function samePayments(a: PaymentCreate[], b: PaymentCreate[]): boolean {
  const key = (ps: PaymentCreate[]) =>
    ps
      .filter((p) => p.amount > 0)
      .map((p) => `${p.method}:${String(p.amount)}`)
      .sort()
      .join("|");
  return key(a) === key(b);
}

/**
 * How to save the cart over `old`: nothing, a direct edit (technician / time / note only), or a
 * correction (anything touching money or sessions: lines, prices, discount, payments, customer).
 */
export function planCorrection(
  old: Visit,
  oldNote: string,
  d: CorrectionDraft,
): CorrectionPlan {
  const due = amountDue(d.lines);
  const discount = discountPreview(due, d.discount);
  const oldPaid: PaymentCreate[] = old.payments
    .filter((p) => p.kind === "payment")
    .map((p) => ({ method: p.method, amount: p.amount }));
  const sameLines =
    d.lines.length === old.lines.length &&
    d.lines.every((l, i) => {
      const o = old.lines[i];
      if (
        !o ||
        o.qty !== 1 ||
        o.service_id !== l.serviceId ||
        o.source !== l.source
      )
        return false;
      return l.source === "package"
        ? o.customer_package_item_id === l.itemId &&
            (l.expiryOverrideReason ?? null) ===
              (o.expiry_override_reason ?? null)
        : o.unit_price === l.price &&
            // The referrer earns the upsell credit: changing it re-records the visit.
            (l.referrerId || null) === (o.referrer_staff_id ?? null);
    });
  const sameDiscount =
    discount === old.discount_amount &&
    (discount === 0 ||
      (d.discount?.reason.trim() ?? "") === (old.discount_reason ?? ""));
  const moneySame =
    d.customerId === old.customer_id &&
    sameLines &&
    sameDiscount &&
    samePayments(d.payments, oldPaid);
  const noteChanged = d.canWriteNote && d.note.trim() !== oldNote.trim();
  // The desk picks minutes; the recorded time has seconds.
  const minute = (iso: string) => Math.floor(new Date(iso).getTime() / 60_000);
  const timeChanged =
    d.checkInAt !== null && minute(d.checkInAt) !== minute(old.check_in_at);

  if (moneySame) {
    const staff = d.lines.flatMap((l, i) => {
      const o = old.lines[i];
      return o && o.staff_id !== l.staffId
        ? [{ id: o.id, staff_id: l.staffId }]
        : [];
    });
    if (staff.length === 0 && !noteChanged && !timeChanged)
      return { kind: "none" };
    return {
      kind: "direct",
      patch: {
        ...(staff.length ? { lines: staff } : {}),
        ...(timeChanged && d.checkInAt ? { check_in_at: d.checkInAt } : {}),
        ...(noteChanged
          ? { note: d.note.trim() === "" ? null : d.note.trim() }
          : {}),
      },
    };
  }
  const visit = visitBody(d.customerId, d.lines, d.payments, d.note, {
    discount: due > 0 ? d.discount : null,
    checkInAt: d.checkInAt,
  });
  // Absent note = the server copies the old one (it never has to come through the browser).
  const { note, ...rest } = visit;
  return {
    kind: "correct",
    difference: due - discount - old.total,
    body: {
      ...rest,
      ...(due > 0 && d.discount ? { discount: discountBody(d.discount) } : {}),
      ...(noteChanged ? { note } : {}),
      reason: d.reason.trim(),
    },
  };
}

/** "Thu thêm 50.000đ" / "Hoàn 50.000đ" / "Không đổi tiền". */
export function differenceText(difference: number): string {
  if (difference > 0) return `Thu thêm ${formatVnd(difference)}`;
  if (difference < 0) return `Hoàn ${formatVnd(-difference)}`;
  return "Không đổi tiền";
}

export interface CorrectionSaved {
  visit: Visit;
  /** Null for a direct edit (no money moved). */
  difference: number | null;
  /** "Đã sửa lượt của Hà · Thu thêm 30.000đ" for the toast. */
  summary: string;
}

function summaryOf(visit: Visit, difference: number | null): string {
  return `Đã sửa lượt của ${visit.customer_name} · ${difference === null ? "không đổi tiền, buổi" : differenceText(difference)}`;
}

/** Sends the plan; `key` stays the same across retries so a resend never corrects twice. */
export async function saveCorrection(
  old: Visit,
  plan: Exclude<CorrectionPlan, { kind: "none" }>,
  key: string,
): Promise<CorrectionSaved> {
  if (plan.kind === "direct") {
    const visit = (await api(`/visits/${old.id}`, {
      method: "PATCH",
      body: plan.patch,
      headers: ifMatch(old.version),
    })) as Visit;
    return { visit, difference: null, summary: summaryOf(visit, null) };
  }
  const done = (await api(`/visits/${old.id}/correct`, {
    method: "POST",
    body: plan.body,
    headers: { ...ifMatch(old.version), "Idempotency-Key": key },
  })) as VisitCorrected;
  return {
    visit: done.visit,
    difference: done.difference,
    summary: summaryOf(done.visit, done.difference),
  };
}
