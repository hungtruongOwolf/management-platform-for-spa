import { describe, expect, it } from "vitest";
import { GENTLE, SECTORS, SPIN, SPIN_MS, randomSector, stopAngle } from "./angles";

describe("wheel angles", () => {
  it("stops after the given turns, at the middle of the sector, as the prototype", () => {
    for (const from of [0, 37, 359.5, 1234]) {
      for (const turns of [SPIN.turns, GENTLE.turns]) {
        for (let sector = 0; sector < SECTORS; sector += 1) {
          const end = stopAngle(from, sector, turns);
          expect(end - from).toBeGreaterThanOrEqual(turns * 360);
          expect(end - from).toBeLessThan((turns + 1) * 360);
          expect(((end % 360) + 360) % 360).toBeCloseTo(sector * 45 + 22.5);
        }
      }
    }
  });

  it("lasts three times the prototype's 4.1 s, at its opening speed; reduced motion is gentler but still turns", () => {
    expect(SPIN_MS).toBe(3 * 4100);
    expect(SPIN.ms).toBe(SPIN_MS);
    expect(GENTLE.ms).toBe(SPIN_MS);
    expect(GENTLE.turns).toBeGreaterThanOrEqual(2);
    expect(GENTLE.turns).toBeLessThan(SPIN.turns);
    // Opening speed (deg/ms) = the curve's first slope (y1 / x1) × the average speed.
    const opening = (turns: number, ms: number, x1: number, y1: number) => ((y1 / x1) * turns * 360) / ms;
    const prototype = opening(6, 4100, 0.14, 0.72);
    expect(opening(SPIN.turns, SPIN.ms, 0.12, 0.65)).toBeGreaterThan(prototype * 0.9);
    expect(opening(SPIN.turns, SPIN.ms, 0.12, 0.65)).toBeLessThan(prototype * 1.1);
    expect(SPIN.ease).toBe("cubic-bezier(0.12, 0.65, 0.04, 1)");
  });

  it("draws a sector from the crypto source", () => {
    const seen = new Set(Array.from({ length: 400 }, randomSector));
    for (const s of seen) expect(s).toBeGreaterThanOrEqual(0);
    expect(Math.max(...seen)).toBeLessThan(SECTORS);
    expect(seen.size).toBe(SECTORS);
  });
});
