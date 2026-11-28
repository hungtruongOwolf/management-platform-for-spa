import { useEffectEvent, useLayoutEffect, useRef, type RefObject } from "react";
import blurUrl from "./assets/wheel-disk-blur.webp";
import diskUrl from "./assets/wheel-disk.webp";
import frameUrl from "./assets/wheel-frame.webp";

/** One press of "Quay": the disk turns from `from` to `end` (degrees) in `ms`, eased by `ease`. */
export interface WheelTurn {
  id: number;
  from: number;
  end: number;
  ms: number;
  ease: string;
  /** Fade the motion blur out over the turn (not with reduced motion). */
  blur: boolean;
  /** `performance.now()` at the press: a wheel mounted later shows the stop, it does not replay. */
  at: number;
}

const turn = (deg: number) => `rotate(${String(deg)}deg)`;
/** A wheel that mounts this long after the press (the presentation opening) skips the turn. */
const LATE_MS = 200;

/**
 * The lotus wheel of the EM's design: the turning disk under the still gold frame (two images placed
 * by the original crop's ratios, `lucky-draw.css`). Not a control: the "Quay" button drives it.
 * Each press is one eased CSS transition to a sector chosen at the press, like the prototype. It
 * always turns, reduced motion included (EM: the spin is the content of a draw; useSpin picks the
 * gentler timing then): the transition is set `!important` inline, above the app-wide rule that
 * strips transform transitions for `prefers-reduced-motion`.
 *
 * While it is fast, a rotational motion blur of the disk art (scripts/wheel-blur.py) lies on the
 * disk, turning with it, and fades out as the wheel slows (`ld-wheel-blur`, opacity only, so it
 * runs on the compositor like the turn). Both run off the main thread: nothing here touches the DOM
 * between the press and the stop.
 */
export function Wheel({
  spin,
  angleRef,
  onStopped,
}: {
  spin: WheelTurn | null;
  /** The disk's resting angle, kept by useSpin: every wheel on screen starts from it. */
  angleRef: RefObject<number>;
  /** The transition of turn `id` ended. */
  onStopped: (id: number) => void;
}) {
  const disk = useRef<HTMLDivElement>(null);
  const blur = useRef<HTMLImageElement>(null);
  const started = useRef(0);
  const stopped = useEffectEvent(onStopped);

  useLayoutEffect(() => {
    const el = disk.current;
    if (!el) return;
    if (!spin) {
      el.style.setProperty("transition", "none", "important");
      el.style.transform = turn(angleRef.current);
      return;
    }
    if (started.current !== spin.id) {
      started.current = spin.id;
      el.style.setProperty("transition", "none", "important");
      const smear = blur.current;
      smear?.style.removeProperty("animation");
      if (performance.now() - spin.at > LATE_MS) {
        el.style.transform = turn(spin.end);
      } else {
        el.style.transform = turn(spin.from);
        // Commit the start angle (and the blur's reset), so both run from the press.
        void el.getBoundingClientRect();
        el.style.setProperty("transition", `transform ${String(spin.ms)}ms ${spin.ease}`, "important");
        el.style.transform = turn(spin.end);
        if (spin.blur) smear?.style.setProperty("animation", `ld-wheel-blur ${String(spin.ms)}ms linear both`);
      }
    }
    const id = spin.id;
    const done = (e: TransitionEvent) => {
      if (e.target === el && e.propertyName === "transform") stopped(id);
    };
    el.addEventListener("transitionend", done);
    return () => {
      el.removeEventListener("transitionend", done);
    };
  }, [spin, angleRef]);

  return (
    <div className="ld-wheel" role="img" aria-label="Vòng quay tám cánh sen">
      <div ref={disk} className="ld-wheel-disk">
        <img src={diskUrl} alt="" width={376} height={376} draggable={false} decoding="async" />
        <img ref={blur} className="ld-wheel-blur" src={blurUrl} alt="" width={376} height={376} draggable={false} decoding="async" />
      </div>
      <img className="ld-wheel-frame" src={frameUrl} alt="" width={466} height={481} draggable={false} decoding="async" />
    </div>
  );
}
