/**
 * Geometry and timing of the wheel. Where it stops is decoration only: the server picks the winner
 * (AC4.2, ADR-0025), so the sector is drawn here, at the press, like the EM's prototype.
 */
export const SECTORS = 8;

export interface Turn {
  /** Full turns before the wheel settles on the sector. */
  turns: number;
  ms: number;
  ease: string;
}

/**
 * How long one spin lasts, press to stop (EM 2026-11-28: "tầm 6s thôi", up from the 4.1 s of the
 * prototype). The one number to tune; both timings below use it.
 */
export const SPIN_MS = 6000;

/**
 * One eased turn from the press to the stop. 9 turns keep the opening speed of the 6-turn, 4.1 s
 * prototype (the curve's first slope ≈ 5.1 × the average speed); the ease-out ends on a long, slow
 * tail, the suspense of the last turns.
 */
export const SPIN: Turn = { turns: 9, ms: SPIN_MS, ease: "cubic-bezier(0.13, 0.66, 0.04, 1)" };
/**
 * `prefers-reduced-motion`: still a spin the room can see (EM: the spin is the content of a draw),
 * as long, gentler: three turns, a soft start and the same slow end.
 */
export const GENTLE: Turn = { turns: 3, ms: SPIN_MS, ease: "cubic-bezier(0.3, 0.15, 0.1, 1)" };

/** A uniform sector (8 divides 2³², so the modulo has no bias). */
export function randomSector(): number {
  return (crypto.getRandomValues(new Uint32Array(1))[0] ?? 0) % SECTORS;
}

/** The angle a spin ends on: `turns` full turns past `from`, then on to the middle of `sector`. */
export function stopAngle(from: number, sector: number, turns: number): number {
  const step = 360 / SECTORS;
  const target = sector * step + step / 2;
  const base = from + turns * 360;
  return base + ((((target - base) % 360) + 360) % 360);
}
