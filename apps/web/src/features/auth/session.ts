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
  | { state: "signed_in"; me: Me };

export type MfaStep = "mfa_verify" | "mfa_enroll";

const MFA_STEP_BY_CODE: Record<string, MfaStep> = {
  "auth.mfa_required": "mfa_verify",
  "auth.mfa_enrollment_required": "mfa_enroll",
};

async function fetchAuthState(): Promise<AuthState> {
  try {
    return { state: "signed_in", me: parseMe(await api("/me")) };
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      const step = MFA_STEP_BY_CODE[error.code];
      return step ? { state: step } : { state: "anonymous" };
    }
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

const LOGIN_NEXT = ["done", "mfa_verify", "mfa_enroll"] as const;

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
