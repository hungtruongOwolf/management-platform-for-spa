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

/** One eased turn from the press to the stop, as the prototype: +6 turns in 4.1 s. */
export const SPIN: Turn = { turns: 6, ms: 4100, ease: "cubic-bezier(0.14, 0.72, 0.12, 1)" };
/**
 * `prefers-reduced-motion`: still a spin the room can see (EM: the spin is the content of a draw),
 * gentler: two turns, a shorter and softer ease-in-out.
 */
export const GENTLE: Turn = { turns: 2, ms: 2500, ease: "cubic-bezier(0.33, 0, 0.2, 1)" };

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
