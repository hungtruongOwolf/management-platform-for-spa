import type {
  Customer,
  CustomerCreate,
  CustomerSummary,
  CustomerUpdate,
  PhoneCheck,
} from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api, ifMatch } from "../../lib/api";

/** ≥ 3 digits or ≥ 2 letters, mirroring the server rule (AC2.1); shorter queries are not sent. */
export function isSearchable(q: string): boolean {
  const t = q.trim();
  if (t === "") return false;
  const digits = t.replace(/\D/g, "");
  const phoneLike = /^[\d\s.+()-]+$/.test(t);
  return phoneLike ? digits.replace(/^0/, "").length >= 3 : t.length >= 2;
}

export function searchQuery(q: string) {
  const term = q.trim();
  return queryOptions({
    queryKey: ["customers", "search", term],
    queryFn: async () =>
      (await api(
        `/customers?q=${encodeURIComponent(term)}`,
      )) as CustomerSummary[],
    staleTime: 30_000,
  });
}

export const recentQuery = queryOptions({
  queryKey: ["customers", "recent"],
  queryFn: async () => (await api("/customers")) as CustomerSummary[],
  staleTime: 30_000,
});

export function customerQuery(id: string) {
  return queryOptions({
    queryKey: ["customers", "one", id],
    queryFn: async () => (await api(`/customers/${id}`)) as Customer,
  });
}

export async function createCustomer(body: CustomerCreate): Promise<Customer> {
  return (await api("/customers", { method: "POST", body })) as Customer;
}

export async function updateCustomer(
  id: string,
  version: number,
  body: CustomerUpdate,
): Promise<Customer> {
  return (await api(`/customers/${id}`, {
    method: "PUT",
    body,
    headers: ifMatch(version),
  })) as Customer;
}

export async function setHealthNote(
  id: string,
  version: number,
  healthNote: string | null,
): Promise<Customer> {
  return (await api(`/customers/${id}/health-note`, {
    method: "PUT",
    body: { health_note: healthNote },
    headers: ifMatch(version),
  })) as Customer;
}

/** Digits a typed phone carries, the way the server normalises it ("+84 905…" → "0905…"). */
export function phoneDigits(typed: string): string {
  const d = typed.replace(/\D/g, "");
  return d.startsWith("84") && d.length === 11 ? `0${d.slice(2)}` : d;
}

/** Only complete-looking numbers are checked: 10 digits from 0 (the API says `valid: false` otherwise). */
export function isCompletePhone(typed: string): boolean {
  return /^0\d{9}$/.test(phoneDigits(typed));
}

/** `GET /customers/phone-check?phone=` while the number is typed (EM round 3 item 13). */
export function phoneCheckQuery(digits: string) {
  return queryOptions({
    queryKey: ["customers", "phone-check", digits],
    queryFn: async () => (await api(`/customers/phone-check?phone=${encodeURIComponent(digits)}`)) as PhoneCheck,
    staleTime: 30_000,
    enabled: isCompletePhone(digits),
  });
}

/** Wait between the last keystroke and the check, so a number typed fast costs one request. */
export const PHONE_CHECK_DEBOUNCE_MS = 400;

export const GENDER_LABEL: Record<string, string> = {
  female: "Nữ",
  male: "Nam",
};
