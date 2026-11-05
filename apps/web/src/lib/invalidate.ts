import type { QueryClient } from "@tanstack/react-query";

/**
 * After any write that moves money, sessions or a visit (record / void a visit, sell / void /
 * cancel / extend a package): the management views computed from those events. Reports, the
 * package overview and the absent list are marked stale for every key; the customer's own history
 * and summary only for that customer (frontend.md "Invalidation sau mutation"). Inactive queries
 * refetch on their next mount; nothing extra runs on /quay where none of them is mounted.
 */
export function invalidateManagementViews(queryClient: QueryClient, customerId?: string): Promise<void> {
  const keys: unknown[][] = [["reports"], ["packages", "overview"], ["customers", "inactive"]];
  if (customerId) keys.push(["customers", "history", customerId], ["customers", "summary", customerId]);
  else keys.push(["customers", "history"], ["customers", "summary"]);
  return Promise.all(keys.map((queryKey) => queryClient.invalidateQueries({ queryKey }))).then(() => undefined);
}
