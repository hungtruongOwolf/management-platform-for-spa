// The unsaved cart survives a reload or a 401 (spec R1-1 §10.8 Q-U8): kept in sessionStorage per
// signed-in user and customer, so another account on the same tab never sees it and closing the
// tab forgets it. Only what the page already shows goes in (services, technicians, the choices
// made); the session note (health data) never does. Cleared when the visit is saved, the cart is
// abandoned, or the user signs out. Every storage access is wrapped: a private window or blocked
// storage simply means no draft.

import type { Appointment, CustomerSummary } from "@vpyd/api-client";
import type {
  CartLine,
  DiscountDraft,
  PackageSaleDraft,
  PaymentMode,
} from "./api";

const PREFIX = "vpyd.cart.v1:";
/** A draft older than this is stale (the day has moved on, prices may have changed). */
export const DRAFT_MAX_AGE_MS = 12 * 60 * 60 * 1000;

interface Stored {
  savedAt: number;
  draft: object;
}

function keyOf(userId: string, customerId: string): string {
  return `${PREFIX}${userId}:${customerId}`;
}

/** Drops the fields that must never be stored (the session note is health data). */
function sanitize(draft: object): object {
  const copy: Record<string, unknown> = { ...draft };
  delete copy["note"];
  return copy;
}

export function saveDraft(
  userId: string,
  customerId: string,
  draft: object,
  now: number = Date.now(),
): void {
  try {
    const value: Stored = { savedAt: now, draft: sanitize(draft) };
    sessionStorage.setItem(keyOf(userId, customerId), JSON.stringify(value));
  } catch {
    // Storage full or blocked: the cart still works, it just will not survive a reload.
  }
}

/**
 * The draft of this user and customer, if one was saved recently enough. Untyped on purpose: the
 * caller checks its shape (an older build may have written it).
 */
export function loadDraft(
  userId: string,
  customerId: string,
  now: number = Date.now(),
): unknown {
  try {
    const raw = sessionStorage.getItem(keyOf(userId, customerId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { savedAt?: unknown; draft?: unknown };
    if (
      typeof parsed.savedAt !== "number" ||
      typeof parsed.draft !== "object" ||
      parsed.draft === null ||
      now - parsed.savedAt > DRAFT_MAX_AGE_MS
    ) {
      sessionStorage.removeItem(keyOf(userId, customerId));
      return null;
    }
    return parsed.draft;
  } catch {
    return null;
  }
}

export function clearDraft(userId: string, customerId: string): void {
  try {
    sessionStorage.removeItem(keyOf(userId, customerId));
  } catch {
    // nothing to clear
  }
}

/** The customer of this user's most recently saved draft (to reopen the cart after a reload). */
export function latestDraftCustomer(
  userId: string,
  now: number = Date.now(),
): string | null {
  try {
    const mine = `${PREFIX}${userId}:`;
    let best: { id: string; at: number } | null = null;
    for (let i = 0; i < sessionStorage.length; i++) {
      const k = sessionStorage.key(i);
      if (!k?.startsWith(mine)) continue;
      const parsed = JSON.parse(sessionStorage.getItem(k) ?? "null") as {
        savedAt?: unknown;
      } | null;
      const at = parsed?.savedAt;
      if (typeof at !== "number" || now - at > DRAFT_MAX_AGE_MS) continue;
      if (!best || at > best.at) best = { id: k.slice(mine.length), at };
    }
    return best?.id ?? null;
  } catch {
    return null;
  }
}

/** What the /quay cart keeps of an unsaved visit (no session note: health data). */
export interface CartDraft {
  v: 1;
  customer: CustomerSummary;
  lines: CartLine[];
  sales: PackageSaleDraft[];
  mode: PaymentMode;
  cashText: string;
  discount: DiscountDraft | null;
  serviceDay: string;
  serviceTime: string;
  companions: string[];
  appointment: Appointment | null;
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null;
const isStr = (v: unknown): v is string => typeof v === "string";

/**
 * A stored draft back as a cart, or null when it does not have the shape this build writes (an
 * older build, a hand-edited value). Lines are checked one by one: a broken line drops the draft.
 */
export function restoreCart(raw: unknown): CartDraft | null {
  if (!isObj(raw) || raw["v"] !== 1) return null;
  const c = raw["customer"];
  if (!isObj(c) || !isStr(c["id"]) || !isStr(c["full_name"])) return null;
  const lines = raw["lines"];
  const sales = raw["sales"];
  if (!Array.isArray(lines) || !Array.isArray(sales)) return null;
  const okLine = (l: unknown) =>
    isObj(l) &&
    typeof l["key"] === "number" &&
    isStr(l["serviceId"]) &&
    isStr(l["serviceName"]) &&
    isStr(l["staffId"]) &&
    ((l["source"] === "package" && isStr(l["itemId"])) ||
      (l["source"] === "one_off" && typeof l["price"] === "number"));
  const okSale = (s: unknown) =>
    isObj(s) &&
    isStr(s["ref"]) &&
    isStr(s["closer"]) &&
    isStr(s["giftNote"]) &&
    isStr(s["discountText"]) &&
    isStr(s["discountReason"]) &&
    typeof s["useToday"] === "boolean";
  if (!lines.every(okLine) || !sales.every(okSale)) return null;
  const mode = raw["mode"];
  const companions = raw["companions"];
  return {
    ...(raw as unknown as CartDraft),
    mode:
      mode === "bank_transfer" || mode === "mixed" || mode === "cash"
        ? mode
        : "cash",
    cashText: isStr(raw["cashText"]) ? raw["cashText"] : "",
    serviceDay: isStr(raw["serviceDay"]) ? raw["serviceDay"] : "",
    serviceTime: isStr(raw["serviceTime"]) ? raw["serviceTime"] : "",
    companions: Array.isArray(companions) ? companions.filter(isStr) : [],
    appointment: isObj(raw["appointment"])
      ? (raw["appointment"] as unknown as Appointment)
      : null,
    discount: isObj(raw["discount"])
      ? (raw["discount"] as unknown as DiscountDraft)
      : null,
  };
}

/** On sign-out: every cart draft of every user in this tab. */
export function clearAllDrafts(): void {
  try {
    const keys: string[] = [];
    for (let i = 0; i < sessionStorage.length; i++) {
      const k = sessionStorage.key(i);
      if (k?.startsWith(PREFIX)) keys.push(k);
    }
    for (const k of keys) sessionStorage.removeItem(k);
  } catch {
    // nothing to clear
  }
}
