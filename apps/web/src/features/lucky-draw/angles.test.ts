import { describe, expect, it } from "vitest";
import { GENTLE, SECTORS, SPIN, randomSector, stopAngle } from "./angles";

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

  it("reduced motion is gentler but still turns", () => {
    expect(GENTLE.turns).toBeGreaterThanOrEqual(2);
    expect(GENTLE.ms).toBeLessThan(SPIN.ms);
    expect(SPIN).toEqual({ turns: 6, ms: 4100, ease: "cubic-bezier(0.14, 0.72, 0.12, 1)" });
  });

  it("draws a sector from the crypto source", () => {
    const seen = new Set(Array.from({ length: 400 }, randomSector));
    for (const s of seen) expect(s).toBeGreaterThanOrEqual(0);
    expect(Math.max(...seen)).toBeLessThan(SECTORS);
    expect(seen.size).toBe(SECTORS);
  });
});
