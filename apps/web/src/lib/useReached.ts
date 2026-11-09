import { useEffect, useState } from "react";

/** Longest delay `setTimeout` takes (2^31 − 1 ms ≈ 24.8 days); a later instant re-arms. */
const MAX_DELAY = 2_147_483_647;

/**
 * Whether the device clock has reached `at` (epoch ms). Re-renders once when it does: one timer
 * per instant, no polling. `null` = never. For UI that changes with the clock (a booking's start
 * passing, the spa closing); the server still decides.
 */
export function useReached(at: number | null): boolean {
  const [now, setNow] = useState(() => Date.now());
  const reached = at !== null && now >= at;
  useEffect(() => {
    if (at === null || reached) return;
    const t = setTimeout(
      () => {
        setNow(Date.now());
      },
      Math.min(Math.max(at - Date.now(), 0), MAX_DELAY),
    );
    return () => {
      clearTimeout(t);
    };
  }, [at, reached, now]);
  return reached;
}
