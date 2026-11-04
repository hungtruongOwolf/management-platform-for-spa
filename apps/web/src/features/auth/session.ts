import { queryOptions, type QueryClient } from "@tanstack/react-query";
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

/** Current user, or null when not logged in. UX only: the server re-checks every request. */
export const meQuery = queryOptions({
  queryKey: ["me"],
  queryFn: async (): Promise<Me | null> => {
    try {
      return meSchema.parse(await api("/me"));
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) return null;
      throw error;
    }
  },
  staleTime: 60_000,
});

export async function login(phone: string, password: string): Promise<Me> {
  return meSchema.parse(await api("/auth/login", { method: "POST", body: { phone, password } }));
}

export async function logout(queryClient: QueryClient): Promise<void> {
  try {
    await api("/auth/logout", { method: "POST" });
  } finally {
    // Whatever the server said, forget everything cached for this user.
    queryClient.clear();
    queryClient.setQueryData(meQuery.queryKey, null);
  }
}

export function can(me: Me | null | undefined, permission: string): boolean {
  return me?.permissions.includes(permission) ?? false;
}
