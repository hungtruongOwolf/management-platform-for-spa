import type {
  CustomerPackage,
  PackageCancel,
  PackageExtend,
  Sale,
  Visit,
} from "@vpyd/api-client";
import { api, ifMatch } from "../../lib/api";

/**
 * Corrections (US6, US7). Every call takes the key of the form that issued it: a double click
 * or a retry after a network error (or after re-entering the password) replays the same action.
 */
export async function voidVisit(
  id: string,
  reason: string,
  idempotencyKey: string,
): Promise<Visit> {
  return (await api(`/visits/${id}/void`, {
    method: "POST",
    body: { reason },
    headers: { "Idempotency-Key": idempotencyKey },
  })) as Visit;
}

export async function voidSale(
  id: string,
  reason: string,
  idempotencyKey: string,
): Promise<Sale> {
  return (await api(`/sales/${id}/void`, {
    method: "POST",
    body: { reason },
    headers: { "Idempotency-Key": idempotencyKey },
  })) as Sale;
}

export async function extendPackage(
  pkg: Pick<CustomerPackage, "id" | "version">,
  body: PackageExtend,
  idempotencyKey: string,
): Promise<CustomerPackage> {
  return (await api(`/customer-packages/${pkg.id}/extend`, {
    method: "POST",
    body,
    headers: { ...ifMatch(pkg.version), "Idempotency-Key": idempotencyKey },
  })) as CustomerPackage;
}

export async function cancelPackage(
  id: string,
  body: PackageCancel,
  idempotencyKey: string,
): Promise<CustomerPackage> {
  return (await api(`/customer-packages/${id}/cancel`, {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as CustomerPackage;
}

/** A package sale can be voided only while none of its sessions was used (AC6.4). */
export function isUnused(p: CustomerPackage): boolean {
  return p.items.every((i) => i.sessions_remaining === i.sessions_total);
}

/** "YYYY-MM-DD" of `date` + `months` (clamped to the month's last day), for the extend form. */
export function addMonths(iso: string, months: number): string {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) return iso;
  const target = new Date(Date.UTC(y, m - 1 + months, 1));
  const last = new Date(
    Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0),
  ).getUTCDate();
  target.setUTCDate(Math.min(d, last));
  return target.toISOString().slice(0, 10);
}

/** Today in the spa's time zone, "YYYY-MM-DD". */
export function spaToday(now: Date = new Date()): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Ho_Chi_Minh",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(now);
}
