import type { QueryClient } from "@tanstack/react-query";

/**
 * Staff views computed from visits and sales (commission, served-day warnings, a technician's own
 * month, day and calendar). Not `["staff"]` as a whole: the desk's technician picker
 * (`["staff", "active"]`) is mounted on /quay and would refetch after every save for nothing.
 */
const STAFF_VIEWS_FROM_VISITS: unknown[][] = ["payroll", "attendance", "me", "me-day", "me-calendar"].map((view) => ["staff", view]);

/**
 * After any write that moves money, sessions or a visit (record / void a visit, sell / void /
 * cancel / extend a package): the management views computed from those events. Reports, the
 * package overview and the absent list are marked stale for every key; the customer's own history
 * and summary only for that customer (frontend.md "Invalidation sau mutation"). Inactive queries
 * refetch on their next mount; nothing extra runs on /quay where none of them is mounted.
 * Since the default staleTime is 30 s (lib/queryClient.ts), a view left out here would show the
 * old numbers on its next open, so the staff views computed from visits and sales are listed too.
 */
export function invalidateManagementViews(queryClient: QueryClient, customerId?: string): Promise<void> {
  const keys: unknown[][] = [["reports"], ["packages", "overview"], ["customers", "inactive"], ...STAFF_VIEWS_FROM_VISITS];
  if (customerId) keys.push(["customers", "history", customerId], ["customers", "summary", customerId]);
  else keys.push(["customers", "history"], ["customers", "summary"]);
  return Promise.all(keys.map((queryKey) => queryClient.invalidateQueries({ queryKey }))).then(() => undefined);
}
