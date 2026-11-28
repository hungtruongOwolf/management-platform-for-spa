/**
 * Geometry of the wheel. Where it stops is decoration only: the server has already picked the
 * winner (AC4.2); the sector is drawn here so the stop looks different every time.
 */
export const SECTORS = 8;
/** The slow-down after the server answered (architecture/lucky-draw.md §4: ~4 s). */
export const LAND_MS = 4000;
/** Back to where it started after a failed spin (AC4.6). */
export const RETURN_MS = 700;
/** Full turns of the slow-down before it settles on the sector. */
const TURNS = 3;

/** A uniform sector (8 divides 2³², so the modulo has no bias). */
export function randomSector(): number {
  return (crypto.getRandomValues(new Uint32Array(1))[0] ?? 0) % SECTORS;
}

/** The angle the slow-down ends on: a few turns past `from`, at the middle of `sector`. */
export function stopAngle(from: number, sector: number): number {
  const step = 360 / SECTORS;
  const target = sector * step + step / 2;
  const base = from + TURNS * 360;
  return base + ((((target - base) % 360) + 360) % 360);
}

/** The first angle at or after `from` that shows the wheel as it was at `home`. */
export function restAngle(from: number, home: number): number {
  return from + ((((home - from) % 360) + 360) % 360);
}

/** Degrees of a computed `transform` (`matrix(a, b, …)` of a rotation); `null` without one. */
export function angleOf(transform: string): number | null {
  const m = /^matrix\(([^,]+),([^,]+)/.exec(transform.replace(/\s/g, ""));
  if (!m) return null;
  const deg = (Math.atan2(Number(m[2]), Number(m[1])) * 180) / Math.PI;
  return Number.isFinite(deg) ? (deg + 360) % 360 : null;
}
