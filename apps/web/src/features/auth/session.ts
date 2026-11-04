import { queryOptions, type QueryClient } from "@tanstack/react-query";
import type { MfaEnrollment } from "@vpyd/api-client";
import { z } from "zod";
import { ApiError, api } from "../../lib/api";

const tenantSchema = z.object({ id: z.string(), name: z.string() });

export const meSchema = z.object({
  user: z.object({ id: z.string(), full_name: z.string(), phone: z.string() }),
  active_tenant: tenantSchema.nullable(),
  tenants: z.array(tenantSchema),
  permissions: z.array(z.string()),
});
export type Me = z.infer<typeof meSchema>;

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
    return { state: "signed_in", me: meSchema.parse(await api("/me")) };
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

const loginResponseSchema = z.object({
  next: z.enum(["done", "mfa_verify", "mfa_enroll"]),
  me: meSchema.nullable().optional(),
});

/** Password step. Owners (and anyone with MFA on) come back in an `mfa_*` state. */
export async function login(phone: string, password: string): Promise<AuthState> {
  const reply = loginResponseSchema.parse(await api("/auth/login", { method: "POST", body: { phone, password } }));
  if (reply.next === "done") {
    if (!reply.me) throw new ApiError(500, "error");
    return { state: "signed_in", me: reply.me };
  }
  return { state: reply.next };
}

/** Completes a pending login with a TOTP code or a recovery code. */
export async function verifyMfa(code: string): Promise<Me> {
  return meSchema.parse(await api("/auth/mfa/verify", { method: "POST", body: { code } }));
}

const enrollmentSchema = z.object({ otpauth_uri: z.string(), manual_key: z.string() });

export async function startMfaEnrollment(): Promise<MfaEnrollment> {
  return enrollmentSchema.parse(await api("/auth/mfa/enroll/start", { method: "POST" }));
}

const enrolledSchema = z.object({ me: meSchema, recovery_codes: z.array(z.string()) });

/** Confirms the authenticator; returns the recovery codes, which the server never shows again. */
export async function confirmMfaEnrollment(code: string): Promise<{ me: Me; recoveryCodes: string[] }> {
  const reply = enrolledSchema.parse(await api("/auth/mfa/enroll/confirm", { method: "POST", body: { code } }));
  return { me: reply.me, recoveryCodes: reply.recovery_codes };
}

export function setAuthState(queryClient: QueryClient, auth: AuthState): void {
  queryClient.setQueryData(authQuery.queryKey, auth);
}

export async function logout(queryClient: QueryClient): Promise<void> {
  try {
    await api("/auth/logout", { method: "POST" });
  } finally {
    // Whatever the server said, forget everything cached for this user.
    queryClient.clear();
    setAuthState(queryClient, { state: "anonymous" });
  }
}

export function can(me: Me | null | undefined, permission: string): boolean {
  return me?.permissions.includes(permission) ?? false;
}
