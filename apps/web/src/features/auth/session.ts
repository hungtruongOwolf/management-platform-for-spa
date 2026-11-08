import { queryOptions, type QueryClient } from "@tanstack/react-query";
import type { MfaEnrollment } from "@vpyd/api-client";
import { ApiError, api } from "../../lib/api";

export interface Tenant {
  id: string;
  name: string;
}

export interface Me {
  user: { id: string; full_name: string; phone: string };
  active_tenant: Tenant | null;
  tenants: Tenant[];
  permissions: string[];
}

// Responses are checked by hand rather than with zod: this module is on every page's critical path,
// and zod would put ~20 KB gzip in front of the first paint (frontend.md, perf budget). The types
// come from the API contract; these guards only reject a reply that breaks it.

type Json = Record<string, unknown>;

function contractError(): ApiError {
  return new ApiError(500, "error");
}

function obj(value: unknown): Json {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw contractError();
  return value as Json;
}

function str(value: unknown): string {
  if (typeof value !== "string") throw contractError();
  return value;
}

function list<T>(value: unknown, item: (v: unknown) => T): T[] {
  if (!Array.isArray(value)) throw contractError();
  return value.map(item);
}

function tenant(value: unknown): Tenant {
  const t = obj(value);
  return { id: str(t["id"]), name: str(t["name"]) };
}

/** Checks a `Me` payload from the API; throws `ApiError(500)` when it does not match the contract. */
export function parseMe(value: unknown): Me {
  const me = obj(value);
  const user = obj(me["user"]);
  return {
    user: { id: str(user["id"]), full_name: str(user["full_name"]), phone: str(user["phone"]) },
    active_tenant: me["active_tenant"] === null ? null : tenant(me["active_tenant"]),
    tenants: list(me["tenants"], tenant),
    permissions: list(me["permissions"], str),
  };
}

/**
 * Where the browser's session stands. `mfa_*` = password accepted, second factor still owed:
 * the server refuses that session everywhere except the MFA endpoints and logout.
 */
export type AuthState =
  | { state: "anonymous" }
  | { state: "mfa_verify" }
  | { state: "mfa_enroll" }
  /** Signed in with a temporary password (ADR-0016): set a new one before anything else. */
  | { state: "password_change" }
  | { state: "signed_in"; me: Me };

export type MfaStep = "mfa_verify" | "mfa_enroll";

const MFA_STEP_BY_CODE: Record<string, MfaStep> = {
  "auth.mfa_required": "mfa_verify",
  "auth.mfa_enrollment_required": "mfa_enroll",
};

/**
 * A busy API (429 rate limit, 503) is not a broken session: `/me` is asked again with backoff
 * (0.5 → 1 → 2 → 4 s, or the server's `Retry-After`, at most 8 s) before the error page. Every
 * screen waits on `/me`, so one burst of requests must not crash the whole app. While it waits,
 * `authRetry.get()` is true and the pending screen says "Đang thử lại…".
 */
export const ME_RETRY_DELAYS_MS = [500, 1000, 2000, 4000] as const;
const RETRY_AFTER_CAP_MS = 8000;

let retrying = false;
const retryListeners = new Set<() => void>();
function setRetrying(value: boolean): void {
  if (retrying === value) return;
  retrying = value;
  for (const listener of retryListeners) listener();
}
/** For `useSyncExternalStore`: whether `/me` is being retried after a 429/503 right now. */
export const authRetry = {
  subscribe: (listener: () => void): (() => void) => {
    retryListeners.add(listener);
    return () => {
      retryListeners.delete(listener);
    };
  },
  get: (): boolean => retrying,
};

function isBusy(error: unknown): error is ApiError {
  return error instanceof ApiError && (error.status === 429 || error.status === 503);
}

async function fetchMe(): Promise<unknown> {
  for (let attempt = 0; ; attempt++) {
    try {
      const reply = await api("/me");
      setRetrying(false);
      return reply;
    } catch (error) {
      const delay = ME_RETRY_DELAYS_MS[attempt];
      if (!isBusy(error) || delay === undefined) {
        setRetrying(false);
        throw error;
      }
      setRetrying(true);
      const wait = error.retryAfter === undefined ? delay : Math.min(error.retryAfter * 1000, RETRY_AFTER_CAP_MS);
      await new Promise((resolve) => setTimeout(resolve, wait));
    }
  }
}

async function fetchAuthState(): Promise<AuthState> {
  try {
    return { state: "signed_in", me: parseMe(await fetchMe()) };
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      const step = MFA_STEP_BY_CODE[error.code];
      return step ? { state: step } : { state: "anonymous" };
    }
    if (error instanceof ApiError && error.code === "auth.password_change_required") return { state: "password_change" };
    throw error;
  }
}

/** Session state for route guards. UX only: the server re-checks every request. */
export const authQuery = queryOptions({
  queryKey: ["me"],
  queryFn: fetchAuthState,
  staleTime: 60_000,
});

/** Current user, or null unless fully signed in (same cache entry as `authQuery`). */
export const meQuery = queryOptions({
  queryKey: ["me"],
  queryFn: fetchAuthState,
  staleTime: 60_000,
  select: (auth: AuthState): Me | null => (auth.state === "signed_in" ? auth.me : null),
});

const LOGIN_NEXT = ["done", "mfa_verify", "mfa_enroll", "password_change"] as const;

/** Password step. Owners (and anyone with MFA on) come back in an `mfa_*` state. */
export async function login(phone: string, password: string): Promise<AuthState> {
  const reply = obj(await api("/auth/login", { method: "POST", body: { phone, password } }));
  const next = LOGIN_NEXT.find((n) => n === reply["next"]);
  if (!next) throw contractError();
  if (next === "done") {
    if (reply["me"] === null || reply["me"] === undefined) throw contractError();
    return { state: "signed_in", me: parseMe(reply["me"]) };
  }
  return { state: next };
}

/** Completes a pending login with a TOTP code or a recovery code. */
export async function verifyMfa(code: string): Promise<Me> {
  return parseMe(await api("/auth/mfa/verify", { method: "POST", body: { code } }));
}

export async function startMfaEnrollment(): Promise<MfaEnrollment> {
  const reply = obj(await api("/auth/mfa/enroll/start", { method: "POST" }));
  return { otpauth_uri: str(reply["otpauth_uri"]), manual_key: str(reply["manual_key"]) };
}

/** Confirms the authenticator; returns the recovery codes, which the server never shows again. */
export async function confirmMfaEnrollment(code: string): Promise<{ me: Me; recoveryCodes: string[] }> {
  const reply = obj(await api("/auth/mfa/enroll/confirm", { method: "POST", body: { code } }));
  return { me: parseMe(reply["me"]), recoveryCodes: list(reply["recovery_codes"], str) };
}

export function setAuthState(queryClient: QueryClient, auth: AuthState): void {
  queryClient.setQueryData(authQuery.queryKey, auth);
}

export async function logout(queryClient: QueryClient): Promise<void> {
  try {
    await api("/auth/logout", { method: "POST" });
  } finally {
    // Whatever the server said, forget everything cached for this user, unsaved carts included
    // (loaded on demand: the draft module is not part of the startup chunk).
    queryClient.clear();
    try {
      (await import("../front-desk/draftCart")).clearAllDrafts();
    } catch {
      // The tab closes the storage anyway; signing out must not fail over it.
    }
    setAuthState(queryClient, { state: "anonymous" });
  }
}

export function can(me: Me | null | undefined, permission: string): boolean {
  return me?.permissions.includes(permission) ?? false;
}
