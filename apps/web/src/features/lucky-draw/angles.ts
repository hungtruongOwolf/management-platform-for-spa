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
  /** Lay the rotational motion blur over the disk while it is fast (lucky-draw.css `ld-wheel-blur`). */
  blur: boolean;
}

/**
 * How long one spin lasts, press to stop (EM 2026-11-28: "tầm 6s thôi", up from the 4.1 s of the
 * prototype). The one number to tune; both timings below use it.
 */
export const SPIN_MS = 6000;

/**
 * One eased turn from the press to the stop (EM 2026-11-28: "bị giật giật"). The wheel is now big
 * (~516 px on a 1920 screen, ~790 px on the presentation), so the speed is set per frame: at most
 * ~13 degrees a frame at 60 Hz (≤ 780°/s, measured by angles.test.ts for the longest spin, 5 turns +
 * 359°). The prototype's 9 turns opened at ~55°/frame: on an 8-sector wheel (45° a sector) that reads
 * as a strobing, juddering disk. 5 turns, kicked to full speed at once, holding it for about 2 s,
 * then a long brake: the last turn takes ~3.2 s, the suspense. The motion blur covers the fast part.
 */
export const SPIN: Turn = { turns: 5, ms: SPIN_MS, ease: "cubic-bezier(0.45, 0.95, 0.5, 1)", blur: true };
/**
 * `prefers-reduced-motion`: still a spin the room can see (EM: the spin is the content of a draw),
 * as long, gentler: three turns, a soft start and the same slow end, no motion blur.
 */
export const GENTLE: Turn = { turns: 3, ms: SPIN_MS, ease: "cubic-bezier(0.3, 0.15, 0.1, 1)", blur: false };

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
