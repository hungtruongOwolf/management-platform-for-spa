import type {
  CustomerPackage,
  PaymentCreate,
  Sale,
  SaleCreate,
  Staff,
} from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api } from "../../lib/api";

/** Active staff for the closer picker. */
export const activeStaffQuery = queryOptions({
  queryKey: ["staff", "active"],
  queryFn: async () => (await api("/staff")) as Staff[],
  staleTime: 5 * 60_000,
});

export function customerPackagesQuery(customerId: string) {
  return queryOptions({
    queryKey: ["customer-packages", customerId],
    queryFn: async () =>
      (await api(`/customers/${customerId}/packages`)) as CustomerPackage[],
    // Sessions left decide what the desk deducts, and another device may have used one: asked
    // again on every open. 5 s (not 0) only so a route prefetch and the mount right after share one request.
    staleTime: 5_000,
  });
}

/** The key is created once per dialog: a double click or a retry after a network error replays the same sale. */
export async function createSale(
  body: SaleCreate,
  idempotencyKey: string,
): Promise<Sale> {
  return (await api("/sales", {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as Sale;
}

/** Manager cap (D10); the server enforces it, this only stops a doomed submit early. */
export const MANAGER_DISCOUNT_MAX_PCT = 10;

export function exceedsCap(
  subtotal: number,
  discount: number,
  unlimited: boolean,
): boolean {
  return !unlimited && discount * 100 > subtotal * MANAGER_DISCOUNT_MAX_PCT;
}

export type PaymentMode = "cash" | "bank_transfer" | "mixed";

/** Payments for a full collection (R1 has no debt): one method, or cash part + the rest by transfer. */
export function paymentsFor(
  total: number,
  mode: PaymentMode,
  cashPart: number,
): PaymentCreate[] {
  if (total === 0) return [];
  if (mode === "mixed") {
    return [
      { method: "cash", amount: cashPart },
      { method: "bank_transfer", amount: total - cashPart },
    ];
  }
  return [{ method: mode, amount: total }];
}

/** "2026-11-04" → "04/11/2026". */
export function formatDate(iso: string): string {
  const [y, m, d] = iso.split("-");
  return y && m && d ? `${d}/${m}/${y}` : iso;
}

export const STATUS_LABEL: Record<CustomerPackage["status"], string> = {
  active: "Đang dùng",
  expired: "Hết hạn",
  exhausted: "Hết buổi",
  cancelled: "Đã hủy",
};

/** Badge colours per status (design-system.md "Badge"): active reads calm, the rest warn. */
export const STATUS_BADGE: Record<CustomerPackage["status"], string> = {
  active: "bg-success-soft text-success",
  expired: "bg-warning-bg text-warning-text",
  exhausted: "bg-surface-2 text-text-muted",
  cancelled: "bg-danger-soft text-danger",
};

/** Digits only ("1.500.000" → 1500000); NaN when there are none. */
export function parseAmount(text: string): number {
  const digits = text.replace(/[.\s,đ]/g, "");
  return /^\d+$/.test(digits) ? Number(digits) : Number.NaN;
}
