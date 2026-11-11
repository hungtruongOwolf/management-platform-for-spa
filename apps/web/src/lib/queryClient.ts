import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { ApiError } from "./api";

/**
 * How long a fetched answer counts as fresh unless a query says otherwise (perf page §4d). Each
 * refetch costs a trans-Pacific round trip (0.3–0.5 s from Vietnam), and with staleTime 0 every
 * mount and every return to the tab asked again. Writes in this browser invalidate what they
 * change explicitly (lib/invalidate.ts and each mutation); data that must never be served from
 * cache (package balances, payroll) sets `staleTime: 0` on its own query.
 */
export const DEFAULT_STALE_MS = 30_000;

/**
 * The app's client. `refetchOnWindowFocus` stays on, and TanStack Query only refetches stale queries on focus.
 * A 401 `auth.unauthenticated` from any request means the session ended (expired, revoked, signed
 * out elsewhere): `/me` is asked again, and the shell sends the person to /dang-nhap once it says so.
 * Any successful write may have added a line to some record's history (ADR-0022): every open one is
 * read again, from one place rather than from each mutation.
 */
export function createQueryClient(): QueryClient {
  const onError = (error: unknown) => {
    if (error instanceof ApiError && error.code === "auth.unauthenticated") void client.invalidateQueries({ queryKey: ["me"] });
  };
  // Not awaited: the cache awaits this hook before the mutation's own `onSuccess`.
  const onSuccess = () => {
    void client.invalidateQueries({ queryKey: ["record-history"] });
  };
  const client = new QueryClient({
    queryCache: new QueryCache({ onError }),
    mutationCache: new MutationCache({ onError, onSuccess }),
    defaultOptions: {
      queries: { retry: false, staleTime: DEFAULT_STALE_MS, refetchOnWindowFocus: true },
    },
  });
  return client;
}
