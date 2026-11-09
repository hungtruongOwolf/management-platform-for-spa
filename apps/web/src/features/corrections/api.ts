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

/**
 * Show "Sửa lượt" for a visit of `businessDate`? Mirrors the server (UX only; it decides):
 * `visit.record` + `record.void_same_day`, and an earlier day needs `record.void_closed_day`.
 * Only the newest version of a visit, never a voided one.
 */
export function mayCorrectVisit(
  permissions: readonly string[],
  visit: { business_date: string; status?: string; voided?: boolean },
  today: string,
): boolean {
  const has = (p: string) => permissions.includes(p);
  const current = visit.status
    ? visit.status === "completed"
    : visit.voided === false;
  return (
    current &&
    has("visit.record") &&
    has("record.void_same_day") &&
    (visit.business_date >= today || has("record.void_closed_day"))
  );
}

/**
 * "Thêm / Sửa ghi chú gói" (F4) on an open package sale, UX only (`PATCH /sales/{id}` decides):
 * `package.sell` + `customer.health_notes.write`, and the health read too so the desk sees the note
 * it replaces; a day before today needs `record.void_closed_day` (the owner).
 */
export function mayEditSaleNote(
  permissions: readonly string[],
  sale: { business_date: string; voided: boolean },
  today: string,
): boolean {
  const has = (p: string) => permissions.includes(p);
  return (
    !sale.voided &&
    has("package.sell") &&
    has("customer.health_notes.write") &&
    has("customer.health_notes.read") &&
    (sale.business_date >= today || has("record.void_closed_day"))
  );
}
