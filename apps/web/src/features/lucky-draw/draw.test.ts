import { describe, expect, it } from "vitest";
import { currentPrize, defaultPrize, filterCounts, filterPeople, fold, personState, prizeBlock, summaryByPrize, wins } from "./draw";
import { detail, person, prize, result } from "./fixtures";

describe("defaultPrize (AC4.1)", () => {
  it("starts on the first prize, by position, with a slot left and someone eligible", () => {
    const prizes = [
      prize("c", "Ba", { position: 2 }),
      prize("a", "Một", { position: 0, remaining: 0, awarded: 1 }),
      prize("b", "Hai", { position: 1, eligible_count: 0 }),
      prize("d", "Bốn", { position: 3 }),
    ];
    expect(defaultPrize(prizes)?.id).toBe("c");
    expect(defaultPrize([])).toBeNull();
    expect(defaultPrize([prize("a", "Một", { remaining: 0 })])).toBeNull();
  });

  it("keeps the prize the owner picked while it can be spun, else falls back", () => {
    const one = prize("a", "Một", { position: 0 });
    const two = prize("b", "Hai", { position: 1 });
    const prizes = [one, two];
    expect(currentPrize(prizes, "b")?.id).toBe("b");
    expect(currentPrize([one, { ...two, remaining: 0 }], "b")?.id).toBe("a");
    expect(currentPrize(prizes, "gone")?.id).toBe("a");
  });

  it("says why a prize cannot be spun", () => {
    expect(prizeBlock(prize("a", "A", { remaining: 0, eligible_count: 0 }))).toBe("Hết suất");
    expect(prizeBlock(prize("a", "A", { eligible_count: 0 }))).toBe("Không còn người phù hợp");
    expect(prizeBlock(prize("a", "A"))).toBeNull();
  });
});

const DRAW = detail({
  status: "finished",
  finished: true,
  participants: [
    person("p1", "Lan", "female", 0, "r2"),
    person("p2", "Minh", "male", 1, "r1"),
    person("p3", "Hà", "female", 2, "r3"),
    person("p4", "Bảo", "male", 3),
  ],
  prizes: [
    prize("g2", "Cổ vai gáy", { position: 1, quantity: 3, awarded: 1, remaining: 2, eligible_count: 0, eligible: "female" }),
    prize("g1", "Gội đầu", { position: 0, quantity: 2, awarded: 1, remaining: 1, eligible_count: 1 }),
  ],
  results: [result("r1", 1, "g1", "p2"), result("r2", 2, "g2", "p1", true), result("r3", 3, "g2", "p3")],
});

describe("summaryByPrize (AC6.1)", () => {
  it("groups the winners present by prize in prize order, spin order within, absent apart", () => {
    const s = summaryByPrize(DRAW);
    expect(s.awarded).toBe(2);
    expect(s.prizes.map((p) => [p.prize.name, p.winners.map((w) => w.person?.name)])).toEqual([
      ["Gội đầu", ["Minh"]],
      ["Cổ vai gáy", ["Hà"]],
    ]);
    expect(s.absent.map((w) => [w.person?.name, w.prize?.name])).toEqual([["Lan", "Cổ vai gáy"]]);
  });
});

describe("participants list (AC2.5)", () => {
  const results = new Map(wins(DRAW).map((w) => [w.result.id, w]));

  it("tells each person's state", () => {
    const state = (name: string) => {
      const p = DRAW.participants.find((x) => x.name === name);
      return p && personState(p, results);
    };
    expect(state("Minh")).toEqual({ kind: "won", prize: "Gội đầu" });
    expect(state("Lan")).toEqual({ kind: "absent" });
    expect(state("Bảo")).toEqual({ kind: "waiting" });
  });

  it("filters by chip and searches without accents", () => {
    const names = (f: Parameters<typeof filterPeople>[2], q = "") => filterPeople(DRAW.participants, results, f, q).map((p) => p.name);
    expect(names("all")).toEqual(["Lan", "Minh", "Hà", "Bảo"]);
    expect(names("female")).toEqual(["Lan", "Hà"]);
    expect(names("male")).toEqual(["Minh", "Bảo"]);
    // "Đã trúng" = winners present; the absent are only under the other chips.
    expect(names("won")).toEqual(["Minh", "Hà"]);
    expect(names("all", "bao")).toEqual(["Bảo"]);
    expect(names("female", " HA ")).toEqual(["Hà"]);
    expect(filterCounts(DRAW.participants, results)).toEqual({ all: 4, female: 2, male: 2, won: 2 });
  });

  it("folds Vietnamese for search", () => {
    expect(fold("  Đặng   Thị Ánh ")).toBe("dang thi anh");
  });
});
