import { describe, expect, it } from "vitest";
import { SECTORS, angleOf, randomSector, restAngle, stopAngle } from "./angles";

describe("wheel angles", () => {
  it("stops a few turns on, at the middle of the sector", () => {
    for (const from of [0, 37, 359.5, 1234]) {
      for (let sector = 0; sector < SECTORS; sector += 1) {
        const end = stopAngle(from, sector);
        expect(end - from).toBeGreaterThanOrEqual(3 * 360);
        expect(end - from).toBeLessThan(4 * 360);
        expect(((end % 360) + 360) % 360).toBeCloseTo(sector * 45 + 22.5);
      }
    }
  });

  it("goes back to where it rested, forwards", () => {
    expect(restAngle(100, 40)).toBe(400);
    expect(restAngle(40, 40)).toBe(40);
    expect(restAngle(730, 10) % 360).toBe(10);
  });

  it("reads a rotation back from a computed transform", () => {
    const rad = (deg: number) => (deg * Math.PI) / 180;
    const matrix = (deg: number) => `matrix(${String(Math.cos(rad(deg)))}, ${String(Math.sin(rad(deg)))}, 0, 0, 0, 0)`;
    expect(angleOf(matrix(90))).toBeCloseTo(90);
    expect(angleOf(matrix(-45))).toBeCloseTo(315);
    expect(angleOf("none")).toBeNull();
    expect(angleOf("")).toBeNull();
  });

  it("draws a sector from the crypto source", () => {
    const seen = new Set(Array.from({ length: 400 }, randomSector));
    for (const s of seen) expect(s).toBeGreaterThanOrEqual(0);
    expect(Math.max(...seen)).toBeLessThan(SECTORS);
    expect(seen.size).toBe(SECTORS);
  });
});
