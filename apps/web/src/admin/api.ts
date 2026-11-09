/**
 * The platform console's API (`/api/v1/admin/*`, ADR-0018). Its own cookie and session: nothing
 * here touches the spa's `/me` or its query cache keys (all keys start with "admin").
 */
import type { components } from "@vpyd/api-client";
import { api } from "../lib/api";

type Schemas = components["schemas"];
export type OperatorMe = Schemas["OperatorMe"];
export type OperatorLogin = Schemas["OperatorLogin"];
export type OperatorEnrollment = Schemas["OperatorEnrollment"];
export type OperatorEnrolled = Schemas["OperatorEnrolled"];
export type TenantRow = Schemas["TenantRow"];
export type CreatedTenant = Schemas["CreatedTenant"];
export type NewTenant = Schemas["NewTenant"];
export type StatusChanged = Schemas["StatusChanged"];
export type PlatformAuditPage = Schemas["PlatformAuditPage"];
export type TenantFeature = Schemas["TenantFeature"];
export type PermissionInfo = Schemas["PermissionInfo"];
export type RoleGrants = Schemas["RoleGrants"];
export type AccountRow = Schemas["AccountRow"];
export type SessionsRevoked = Schemas["SessionsRevoked"];
export type PasswordReset = Schemas["PasswordReset"];
export type SpaAuditPage = Schemas["SpaAuditPage"];

export type AccountAction = "lock" | "unlock" | "reset-mfa" | "revoke-sessions";

export const adminKeys = {
  me: ["admin", "me"] as const,
  tenants: ["admin", "tenants"] as const,
  tenant: (id: string) => ["admin", "tenants", id] as const,
  audit: ["admin", "audit"] as const,
  features: (id: string) => ["admin", "tenants", id, "features"] as const,
  roles: (id: string) => ["admin", "tenants", id, "roles"] as const,
  spaAudit: (id: string) => ["admin", "tenants", id, "audit"] as const,
  permissions: ["admin", "permissions"] as const,
  templates: ["admin", "role-templates"] as const,
  accounts: (q: string) => ["admin", "accounts", q] as const,
};

export const adminApi = {
  me: () => api("/admin/me") as Promise<OperatorMe>,
  login: (username: string, password: string) =>
    api("/admin/auth/login", { method: "POST", body: { username, password } }) as Promise<OperatorLogin>,
  verify: (code: string) => api("/admin/auth/mfa/verify", { method: "POST", body: { code } }),
  enrollStart: () => api("/admin/auth/mfa/enroll/start", { method: "POST" }) as Promise<OperatorEnrollment>,
  enrollConfirm: (code: string) =>
    api("/admin/auth/mfa/enroll/confirm", { method: "POST", body: { code } }) as Promise<OperatorEnrolled>,
  reauth: (password: string, code: string) =>
    api("/admin/auth/reauth", { method: "POST", body: { password, code } }),
  logout: () => api("/admin/auth/logout", { method: "POST" }),
  tenants: () => api("/admin/tenants") as Promise<TenantRow[]>,
  tenant: (id: string) => api(`/admin/tenants/${encodeURIComponent(id)}`) as Promise<TenantRow>,
  createTenant: (body: NewTenant) =>
    api("/admin/tenants", { method: "POST", body }) as Promise<CreatedTenant>,
  setStatus: (id: string, suspend: boolean, reason: string) =>
    api(`/admin/tenants/${encodeURIComponent(id)}/${suspend ? "suspend" : "unsuspend"}`, {
      method: "POST",
      body: { reason },
    }) as Promise<StatusChanged>,
  features: (id: string) =>
    api(`/admin/tenants/${encodeURIComponent(id)}/features`) as Promise<TenantFeature[]>,
  setFeature: (id: string, key: string, enabled: boolean, reason: string) =>
    api(`/admin/tenants/${encodeURIComponent(id)}/features/${encodeURIComponent(key)}`, {
      method: "PUT",
      body: { enabled, reason },
    }) as Promise<TenantFeature>,
  permissions: () => api("/admin/permissions") as Promise<PermissionInfo[]>,
  templates: () => api("/admin/role-templates") as Promise<RoleGrants[]>,
  setTemplate: (key: string, permissions: string[], reason: string) =>
    api(`/admin/role-templates/${encodeURIComponent(key)}`, { method: "PUT", body: { permissions, reason } }) as Promise<RoleGrants>,
  roles: (id: string) => api(`/admin/tenants/${encodeURIComponent(id)}/roles`) as Promise<RoleGrants[]>,
  setRole: (id: string, key: string, permissions: string[], reason: string) =>
    api(`/admin/tenants/${encodeURIComponent(id)}/roles/${encodeURIComponent(key)}`, {
      method: "PUT",
      body: { permissions, reason },
    }) as Promise<RoleGrants>,
  resetRole: (id: string, key: string, reason: string) =>
    api(`/admin/tenants/${encodeURIComponent(id)}/roles/${encodeURIComponent(key)}/reset`, {
      method: "POST",
      body: { reason },
    }) as Promise<RoleGrants>,
  spaAudit: (id: string, before?: string) =>
    api(
      `/admin/tenants/${encodeURIComponent(id)}/audit?limit=50${before ? `&before=${encodeURIComponent(before)}` : ""}`,
    ) as Promise<SpaAuditPage>,
  accounts: (q: string) => api(`/admin/accounts?q=${encodeURIComponent(q)}`) as Promise<AccountRow[]>,
  accountAction: (id: string, action: AccountAction, reason: string) =>
    api(`/admin/accounts/${encodeURIComponent(id)}/${action}`, { method: "POST", body: { reason } }) as Promise<SessionsRevoked>,
  resetPassword: (id: string, reason: string) =>
    api(`/admin/accounts/${encodeURIComponent(id)}/reset-password`, { method: "POST", body: { reason } }) as Promise<PasswordReset>,
  audit: (before?: string) =>
    api(`/admin/audit?limit=50${before ? `&before=${encodeURIComponent(before)}` : ""}`) as Promise<PlatformAuditPage>,
};

/** "12 thg 11, 2026 09:30" in the browser's own zone (operators are not tied to one spa's zone). */
export function adminDateTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Intl.DateTimeFormat("vi-VN", { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso));
}
