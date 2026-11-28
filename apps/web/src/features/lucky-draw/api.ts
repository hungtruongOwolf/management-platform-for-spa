import { queryOptions, type QueryClient } from "@tanstack/react-query";
import { api, ifMatch } from "../../lib/api";
import type {
  LuckyDrawDetail,
  LuckyDrawList,
  Participant,
  ParticipantWrite,
  Prize,
  PrizeWrite,
  SpinOutcome,
} from "./types";

/**
 * Every call of the lucky draw screens (architecture/lucky-draw.md §3). Each write answers with the
 * whole program (`LuckyDrawDetail`), which the screen puts in the cache as is: no refetch.
 */
export const LIST_KEY = ["lucky-draws"] as const;
export const drawKey = (id: string) => ["lucky-draws", id] as const;

const path = (id: string, ...rest: string[]) =>
  ["/lucky-draws", id, ...rest].map((p, i) => (i === 0 ? p : encodeURIComponent(p))).join("/");

export const drawsQuery = queryOptions({
  queryKey: LIST_KEY,
  queryFn: async () => ((await api("/lucky-draws")) as LuckyDrawList).items,
});

export function drawQuery(id: string) {
  return queryOptions({
    queryKey: drawKey(id),
    queryFn: async () => (await api(path(id))) as LuckyDrawDetail,
  });
}

/**
 * Puts a program the server just answered with into the cache, unless the cache already holds a
 * later version (answers can land out of order: a spin's answer is held until its wheel stops while
 * a person added meanwhile already came back). The list's counts are marked stale.
 */
export function putDraw(queryClient: QueryClient, draw: LuckyDrawDetail): void {
  queryClient.setQueryData<LuckyDrawDetail>(drawKey(draw.id), (old) => (old && old.version > draw.version ? old : draw));
  void queryClient.invalidateQueries({ queryKey: LIST_KEY, exact: true });
}

const send = async (route: string, method: string, body?: unknown, headers?: Record<string, string>) =>
  (await api(route, { method, ...(body === undefined ? {} : { body }), ...(headers ? { headers } : {}) })) as LuckyDrawDetail;

export const createDraw = (name: string) => send("/lucky-draws", "POST", { name });

export const renameDraw = (draw: Pick<LuckyDrawDetail, "id" | "version">, name: string) =>
  send(path(draw.id), "PUT", { name }, ifMatch(draw.version));

export async function deleteDraw(id: string): Promise<void> {
  await api(path(id), { method: "DELETE" });
}

/** 1–500 people in one transaction (the screen splits a longer paste into batches). */
export const addParticipants = (id: string, participants: ParticipantWrite[]) =>
  send(path(id, "participants"), "POST", { participants });

export const updateParticipant = (id: string, person: Pick<Participant, "id" | "version">, body: ParticipantWrite) =>
  send(path(id, "participants", person.id), "PUT", body, ifMatch(person.version));

export const deleteParticipant = (id: string, personId: string) => send(path(id, "participants", personId), "DELETE");

export const createPrize = (id: string, body: PrizeWrite) => send(path(id, "prizes"), "POST", body);

export const updatePrize = (id: string, prize: Pick<Prize, "id" | "version">, body: PrizeWrite) =>
  send(path(id, "prizes", prize.id), "PUT", body, ifMatch(prize.version));

export const deletePrize = (id: string, prizeId: string) => send(path(id, "prizes", prizeId), "DELETE");

/**
 * One spin: the server picks the winner. The same `key` sent again (a retry after a network error)
 * replays the first answer instead of drawing twice.
 */
export async function spin(id: string, prizeId: string, key: string): Promise<SpinOutcome> {
  return (await api(path(id, "spins"), {
    method: "POST",
    body: { prize_id: prizeId },
    headers: { "Idempotency-Key": key },
  })) as SpinOutcome;
}

export const forfeitResult = (id: string, resultId: string) => send(path(id, "results", resultId, "forfeit"), "POST");
