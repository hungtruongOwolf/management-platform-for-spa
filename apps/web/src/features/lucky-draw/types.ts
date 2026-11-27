/**
 * Shapes of the lucky draw API (architecture/lucky-draw.md §3, the contract between the backend and
 * this screen). Written by hand while the API is not on `main`; they switch to the generated
 * `@vpyd/api-client` exports when it lands, under these same names.
 */
export type DrawStatus = "not_started" | "in_progress" | "finished";
export type Gender = "male" | "female";
export type Eligible = "all" | Gender;

export interface LuckyDrawSummary {
  id: string;
  name: string;
  created_at: string;
  participant_count: number;
  /** Sum of every prize's quantity. */
  prize_slots: number;
  /** Results not forfeited. */
  awarded: number;
  status: DrawStatus;
}

export interface LuckyDrawList {
  items: LuckyDrawSummary[];
}

export interface Participant {
  id: string;
  name: string;
  gender: Gender;
  position: number;
  version: number;
  /** Their result (forfeited or not): someone with a result is out of every later spin. */
  result_id: string | null;
}

export interface Prize {
  id: string;
  name: string;
  description: string | null;
  eligible: Eligible;
  quantity: number;
  awarded: number;
  remaining: number;
  /** People still eligible for it. */
  eligible_count: number;
  position: number;
  version: number;
}

export interface DrawResult {
  id: string;
  seq: number;
  prize_id: string;
  participant_id: string;
  drawn_at: string;
  forfeited_at: string | null;
}

export interface LuckyDrawDetail {
  id: string;
  name: string;
  created_at: string;
  version: number;
  status: DrawStatus;
  finished: boolean;
  /** In `position` order. */
  participants: Participant[];
  prizes: Prize[];
  /** In `seq` order. */
  results: DrawResult[];
}

export interface SpinOutcome {
  result: Omit<DrawResult, "forfeited_at">;
  draw: LuckyDrawDetail;
}

export interface ParticipantWrite {
  name: string;
  gender: Gender;
}

export interface PrizeWrite {
  name: string;
  description?: string | null;
  eligible: Eligible;
  quantity: number;
}
