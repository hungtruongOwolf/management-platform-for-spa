import { QueryClient } from "@tanstack/react-query";

/**
 * How long a fetched answer counts as fresh unless a query says otherwise (perf page §4d). Each
 * refetch costs a trans-Pacific round trip (0.3–0.5 s from Vietnam), and with staleTime 0 every
 * mount and every return to the tab asked again. Writes in this browser invalidate what they
 * change explicitly (lib/invalidate.ts and each mutation); data that must never be served from
 * cache (package balances, payroll) sets `staleTime: 0` on its own query.
 */
export const DEFAULT_STALE_MS = 30_000;

/** The app's client. `refetchOnWindowFocus` stays on, and TanStack Query only refetches stale queries on focus. */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: DEFAULT_STALE_MS, refetchOnWindowFocus: true },
    },
  });
}
