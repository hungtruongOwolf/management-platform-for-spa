import { useEffectEvent, useLayoutEffect, useRef, type RefObject } from "react";
import diskUrl from "./assets/wheel-disk.webp";
import frameUrl from "./assets/wheel-frame.webp";
import { LAND_MS, RETURN_MS, angleOf, restAngle, stopAngle } from "./angles";

export type WheelPhase = "ready" | "spinning" | "landing" | "returning";

// A quick wind-up, then constant turns until the server answers (architecture/lucky-draw.md §4);
// the wind-up ends at the loop's speed, and the slow-down starts close to it.
const SPIN = "ld-wheel-turn 0.5s cubic-bezier(0.55, 0, 0.75, 0.5), ld-wheel-turn 0.25s linear 0.5s infinite";
const LAND = `transform ${String(LAND_MS)}ms cubic-bezier(0.14, 0.72, 0.12, 1)`;
const RETURN = `transform ${String(RETURN_MS)}ms cubic-bezier(0.2, 0.8, 0.2, 1)`;

const turn = (deg: number) => `rotate(${String(deg)}deg)`;

/**
 * The lotus wheel of the EM's design: the turning disk under the still gold frame (two images placed
 * by the original crop's ratios, `lucky-draw.css`). Not a control: the "Quay" button drives it.
 * `phase` comes from the spin (useSpin): it turns as soon as the button is pressed, slows down onto
 * `sector` once the server answered, and goes back where it was when the spin failed. `angleRef`
 * outlives the component, so the presentation's wheel starts where the page's one stopped.
 * `still` (prefers-reduced-motion): it never turns.
 */
export function Wheel({
  phase,
  sector,
  angleRef,
  still,
  onSettled,
}: {
  phase: WheelPhase;
  sector: number;
  angleRef: RefObject<number>;
  still: boolean;
  /** The slow-down (or the way back) ended. */
  onSettled: () => void;
}) {
  const disk = useRef<HTMLImageElement>(null);
  const settled = useEffectEvent(onSettled);

  useLayoutEffect(() => {
    const el = disk.current;
    if (!el) return;
    const style = el.style;
    if (still || phase === "ready") {
      style.animation = "";
      style.transition = "none";
      style.transform = turn(angleRef.current);
      return;
    }
    if (phase === "spinning") {
      style.transition = "none";
      style.transform = turn(angleRef.current);
      style.setProperty("--ld-from", `${String(angleRef.current)}deg`);
      style.animation = SPIN;
      return;
    }
    // Read where the turning disk is before the animation lets go of it.
    const now = angleOf(getComputedStyle(el).transform) ?? angleRef.current;
    style.animation = "";
    style.transition = "none";
    style.transform = turn(now);
    // Commit the start angle, so the transition runs from it.
    void el.getBoundingClientRect();
    const end = phase === "landing" ? stopAngle(now, sector) : restAngle(now, angleRef.current);
    if (phase === "landing") angleRef.current = end % 360;
    style.transition = phase === "landing" ? LAND : RETURN;
    style.transform = turn(end);
    const done = (e: TransitionEvent) => {
      if (e.target === el && e.propertyName === "transform") settled();
    };
    el.addEventListener("transitionend", done);
    return () => {
      el.removeEventListener("transitionend", done);
    };
  }, [phase, sector, still, angleRef]);

  return (
    <div className="ld-wheel" role="img" aria-label="Vòng quay tám cánh sen">
      <img ref={disk} className="ld-wheel-disk" src={diskUrl} alt="" width={376} height={376} draggable={false} decoding="async" />
      <img className="ld-wheel-frame" src={frameUrl} alt="" width={466} height={481} draggable={false} decoding="async" />
    </div>
  );
}
