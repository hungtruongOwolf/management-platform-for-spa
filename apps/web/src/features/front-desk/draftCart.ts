// The unsaved cart survives a reload or a 401 (spec R1-1 §10.8 Q-U8): kept in sessionStorage per
// signed-in user and customer, so another account on the same tab never sees it and closing the
// tab forgets it. Only what the page already shows goes in (services, technicians, the choices
// made); the session note (health data) never does. Cleared when the visit is saved, the cart is
// abandoned, or the user signs out. Every storage access is wrapped: a private window or blocked
// storage simply means no draft.

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
