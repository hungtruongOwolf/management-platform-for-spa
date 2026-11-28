import { describe, expect, it } from "vitest";
import { GENTLE, SECTORS, SPIN, SPIN_MS, randomSector, stopAngle } from "./angles";

/** CSS `cubic-bezier(x1, y1, x2, y2)` as progress at time `x` (0–1), by bisection on the curve. */
function curve(ease: string): (x: number) => number {
  const [x1 = 0, y1 = 0, x2 = 1, y2 = 1] = (/cubic-bezier\(([^)]+)\)/.exec(ease)?.[1] ?? "").split(",").map(Number);
  const point = (t: number, a: number, b: number) => 3 * (1 - t) ** 2 * t * a + 3 * (1 - t) * t ** 2 * b + t ** 3;
  return (x) => {
    let lo = 0;
    let hi = 1;
    for (let i = 0; i < 50; i += 1) {
      const mid = (lo + hi) / 2;
      if (point(mid, x1, x2) < x) lo = mid;
      else hi = mid;
    }
    return point((lo + hi) / 2, y1, y2);
  };
}

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

  it("lasts 6 s (EM); reduced motion is gentler, without blur, but still turns", () => {
    expect(SPIN_MS).toBe(6000);
    expect(SPIN.ms).toBe(SPIN_MS);
    expect(GENTLE.ms).toBe(SPIN_MS);
    expect(GENTLE.turns).toBeGreaterThanOrEqual(2);
    expect(GENTLE.turns).toBeLessThan(SPIN.turns);
    expect(SPIN.blur).toBe(true);
    expect(GENTLE.blur).toBe(false);
  });

  it("never jumps more than ~13° a frame at 60 Hz, and brakes for a long last turn (EM: “giật giật”)", () => {
    for (const timing of [SPIN, GENTLE]) {
      // The longest spin: the turns plus up to 359° to reach the sector.
      const total = timing.turns * 360 + 359;
      const frames = (timing.ms / 1000) * 60;
      const at = curve(timing.ease);
      const steps = Array.from({ length: frames }, (_, i) => (at((i + 1) / frames) - at(i / frames)) * total);
      expect(Math.max(...steps)).toBeLessThanOrEqual(13);
      // Suspense: the last full turn takes at least 3 s.
      const lastTurn = steps.reduceRight((acc, step, i) => (acc.deg < 360 ? { deg: acc.deg + step, from: i } : acc), { deg: 0, from: frames }).from;
      expect(((frames - lastTurn) / 60) * 1000).toBeGreaterThanOrEqual(3000);
    }
  });

  it("draws a sector from the crypto source", () => {
    const seen = new Set(Array.from({ length: 400 }, randomSector));
    for (const s of seen) expect(s).toBeGreaterThanOrEqual(0);
    expect(Math.max(...seen)).toBeLessThan(SECTORS);
    expect(seen.size).toBe(SECTORS);
  });
});
