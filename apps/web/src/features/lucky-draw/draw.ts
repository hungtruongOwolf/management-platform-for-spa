import type { DrawResult, DrawStatus, Eligible, Gender, LuckyDrawDetail, Participant, Prize } from "./types";

/** Rules the screen reads off a program (spec lucky-draw §3). The server decides; these only show it. */

export const GENDER_LABEL: Record<Gender, string> = { female: "Nữ", male: "Nam" };
/** The prize's chip. */
export const ELIGIBLE_LABEL: Record<Eligible, string> = { all: "Cả nam và nữ", female: "Nữ", male: "Nam" };
/** The "Dành cho" choice of the prize form. */
export const ELIGIBLE_CHOICE: Record<Eligible, string> = { all: "Cả nam và nữ", female: "Chỉ nữ", male: "Chỉ nam" };
export const STATUS_LABEL: Record<DrawStatus, string> = {
  not_started: "Chưa quay",
  in_progress: "Đang quay",
  finished: "Đã xong",
};
export const STATUS_BADGE: Record<DrawStatus, string> = {
  not_started: "badge-info",
  in_progress: "badge-ok",
  finished: "badge-muted",
};

const byPosition = <T extends { position: number }>(items: readonly T[]) =>
  [...items].sort((a, b) => a.position - b.position);

/** Why a prize cannot be spun right now, or `null` when it can. */
export function prizeBlock(prize: Prize): string | null {
  if (prize.remaining <= 0) return "Hết suất";
  if (prize.eligible_count <= 0) return "Không còn người phù hợp";
  return null;
}

/** The prize the stage starts on: the first, in order, with a slot left and someone eligible (AC4.1). */
export function defaultPrize(prizes: readonly Prize[]): Prize | null {
  return byPosition(prizes).find((p) => prizeBlock(p) === null) ?? null;
}

/** The prize to spin: the one picked while it can still be spun, else the default. */
export function currentPrize(prizes: readonly Prize[], pickedId: string | null): Prize | null {
  const picked = prizes.find((p) => p.id === pickedId);
  return picked && prizeBlock(picked) === null ? picked : defaultPrize(prizes);
}

export interface Win {
  result: DrawResult;
  person: Participant | undefined;
  prize: Prize | undefined;
}

/** Every result with its person and prize, in spin order. */
export function wins(draw: LuckyDrawDetail): Win[] {
  const people = new Map(draw.participants.map((p) => [p.id, p]));
  const prizes = new Map(draw.prizes.map((p) => [p.id, p]));
  return draw.results.map((result) => ({
    result,
    person: people.get(result.participant_id),
    prize: prizes.get(result.prize_id),
  }));
}

export interface PrizeSummary {
  prize: Prize;
  /** Winners present, in spin order. */
  winners: Win[];
}

export interface DrawSummary {
  /** Results not forfeited. */
  awarded: number;
  /** Every prize, in its order. */
  prizes: PrizeSummary[];
  /** Forfeited results: the winner was absent. */
  absent: Win[];
}

/** "Tổng kết chương trình" (AC6.1): who won what, by prize, and who was absent. */
export function summaryByPrize(draw: LuckyDrawDetail): DrawSummary {
  const all = wins(draw);
  const present = all.filter((w) => w.result.forfeited_at === null);
  return {
    awarded: present.length,
    prizes: byPosition(draw.prizes).map((prize) => ({
      prize,
      winners: present.filter((w) => w.result.prize_id === prize.id),
    })),
    absent: all.filter((w) => w.result.forfeited_at !== null),
  };
}

export type PersonState = { kind: "waiting" } | { kind: "won"; prize: string } | { kind: "absent" };

/** One participant's status in the list (AC2.5). */
export function personState(person: Participant, results: ReadonlyMap<string, Win>): PersonState {
  const win = person.result_id === null ? undefined : results.get(person.result_id);
  if (!win) return { kind: "waiting" };
  if (win.result.forfeited_at !== null) return { kind: "absent" };
  return { kind: "won", prize: win.prize?.name ?? "" };
}

/** Vietnamese without diacritics, lower case, one space between words: "Lê  Thị Ánh" ~ "le thi anh". */
export function fold(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .replace(/[đĐ]/g, "d")
    .toLowerCase()
    .replace(/\s+/g, " ")
    .trim();
}

export type PeopleFilter = "all" | Gender | "won";

/** The participants a filter chip and the search box keep (accent-insensitive search). */
export function filterPeople(
  people: readonly Participant[],
  results: ReadonlyMap<string, Win>,
  filter: PeopleFilter,
  query: string,
): Participant[] {
  const q = fold(query);
  return people.filter((p) => {
    if (filter === "won" ? personState(p, results).kind !== "won" : filter !== "all" && p.gender !== filter) return false;
    return q === "" || fold(p.name).includes(q);
  });
}

/** The count next to each filter chip. */
export function filterCounts(people: readonly Participant[], results: ReadonlyMap<string, Win>): Record<PeopleFilter, number> {
  const counts: Record<PeopleFilter, number> = { all: people.length, female: 0, male: 0, won: 0 };
  for (const p of people) {
    counts[p.gender] += 1;
    if (personState(p, results).kind === "won") counts.won += 1;
  }
  return counts;
}

/** A prize with any result, forfeited included, keeps its audience and cannot be deleted (AC3.2–3.3). */
export function prizeDrawn(draw: LuckyDrawDetail, prizeId: string): boolean {
  return draw.results.some((r) => r.prize_id === prizeId);
}
