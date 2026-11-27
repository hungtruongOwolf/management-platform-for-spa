import type { DrawResult, LuckyDrawDetail, Participant, Prize } from "./types";

/** Test data shaped exactly like the contract (architecture/lucky-draw.md §3). */
export function person(id: string, name: string, gender: Participant["gender"], position: number, result_id: string | null = null): Participant {
  return { id, name, gender, position, version: 1, result_id };
}

export function prize(id: string, name: string, over: Partial<Prize> = {}): Prize {
  return {
    id,
    name,
    description: null,
    eligible: "all",
    quantity: 1,
    awarded: 0,
    remaining: 1,
    eligible_count: 1,
    position: 0,
    version: 1,
    ...over,
  };
}

export function result(id: string, seq: number, prize_id: string, participant_id: string, forfeited = false): DrawResult {
  return {
    id,
    seq,
    prize_id,
    participant_id,
    drawn_at: `2026-11-20T0${String(seq)}:00:00Z`,
    forfeited_at: forfeited ? "2026-11-20T09:30:00Z" : null,
  };
}

export function detail(over: Partial<LuckyDrawDetail> = {}): LuckyDrawDetail {
  return {
    id: "d1",
    name: "Tri ân 20/11",
    created_at: "2026-11-19T03:00:00Z",
    version: 1,
    status: "not_started",
    finished: false,
    participants: [],
    prizes: [],
    results: [],
    ...over,
  };
}
