import { useEffect, useState } from "react";

/**
 * True once the browser has painted the current screen. Gate secondary queries on it (`enabled`) so
 * a large response does not compete with the first paint of the primary task (e.g. the day's visit
 * list on /quay vs the customer search box; Lighthouse LCP budget, testing.md §4).
 */
export function useAfterFirstPaint(): boolean {
  const [painted, setPainted] = useState(false);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    // rAF runs before the next paint; the timeout after it lands just after that paint.
    const frame = requestAnimationFrame(() => {
      timer = setTimeout(() => {
        setPainted(true);
      }, 0);
    });
    return () => {
      cancelAnimationFrame(frame);
      if (timer !== undefined) clearTimeout(timer);
    };
  }, []);
  return painted;
}
