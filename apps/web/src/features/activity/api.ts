import type { ActivityPage } from "@vpyd/api-client";
import { infiniteQueryOptions, keepPreviousData } from "@tanstack/react-query";
import { api } from "../../lib/api";
import type { ActivitySearch } from "./search";

/** The API query of the screen's URL state (empty values left out: the API applies its defaults). */
export function activityParams(s: ActivitySearch): URLSearchParams {
  const p = new URLSearchParams();
  if (s.from) p.set("from", s.from);
  if (s.to) p.set("to", s.to);
  if (s.actor) p.set("actor", s.actor);
  if (s.kind) p.set("kind", s.kind);
  p.set("limit", "50");
  return p;
}

/** `GET /audit-log`: 50 a page, newest first; the previous list stays while filters change. */
export function activityQuery(s: ActivitySearch) {
  const params = activityParams(s);
  return infiniteQueryOptions({
    queryKey: ["audit-log", params.toString()],
    queryFn: async ({ pageParam }) => {
      const page = new URLSearchParams(params);
      if (pageParam) page.set("cursor", pageParam);
      return (await api(`/audit-log?${page.toString()}`)) as ActivityPage;
    },
    initialPageParam: "",
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    // Every write of the app adds an entry, so no mutation's invalidation list could cover it:
    // the screen refetches whenever it is opened (owner only, little traffic).
    staleTime: 0,
    placeholderData: keepPreviousData,
  });
}
