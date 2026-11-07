import type { NoteEntry, NoteKind, NotesPage } from "@vpyd/api-client";
import { infiniteQueryOptions, keepPreviousData } from "@tanstack/react-query";
import { api } from "../../lib/api";
import type { NotesSearch } from "./search";

export type { NoteEntry, NoteKind, NotesPage };

/** The API query of the screen's URL state (empty values left out: the API applies its defaults). */
export function notesParams(s: NotesSearch): URLSearchParams {
  const p = new URLSearchParams();
  if (s.from) p.set("from", s.from);
  if (s.to) p.set("to", s.to);
  if (s.kind) p.set("kind", s.kind);
  if (s.staff) p.set("staff_id", s.staff);
  if (s.customer) p.set("customer_id", s.customer);
  if (s.q) p.set("q", s.q);
  p.set("limit", "30");
  return p;
}

/** `GET /notes`: 30 a page, newest first; the previous list stays while filters change. */
export function notesQuery(s: NotesSearch) {
  const params = notesParams(s);
  return infiniteQueryOptions({
    queryKey: ["notes", params.toString()],
    queryFn: async ({ pageParam }) => {
      const page = new URLSearchParams(params);
      if (pageParam) page.set("cursor", pageParam);
      return (await api(`/notes?${page.toString()}`)) as NotesPage;
    },
    initialPageParam: "",
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    staleTime: 30_000,
    placeholderData: keepPreviousData,
  });
}
