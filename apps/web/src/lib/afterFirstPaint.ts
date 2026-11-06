import { useEffect, useState } from "react";

/** Upper bound on waiting for the browser's paint report, in case it never comes. */
export const PAINT_REPORT_TIMEOUT_MS = 1000;

/** rAF runs before the next paint; the timeout after it lands just after that frame is produced. */
function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      setTimeout(resolve, 0);
    });
  });
}

function contentfulPaintReported(): boolean {
  return performance.getEntriesByName("first-contentful-paint").length > 0;
}

/**
 * Resolves once the browser has *reported* the page's first contentful paint. A frame being
 * produced is not enough: the paint is presented later (≈ 0.2 s later under Lighthouse), and every
 * request or script that starts before the reported paint is counted in the simulated LCP
 * (frontend.md "Perf budget"). Resolves at once when the paint was already reported (a later
 * navigation inside the app) or the browser has no Paint Timing API (jsdom).
 */
function firstContentfulPaint(): Promise<void> {
  const paintTiming =
    typeof PerformanceObserver !== "undefined" &&
    PerformanceObserver.supportedEntryTypes.includes("paint");
  if (!paintTiming || contentfulPaintReported()) return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => {
      observer.disconnect();
      clearTimeout(timer);
      resolve();
    };
    const observer = new PerformanceObserver(() => {
      if (contentfulPaintReported()) done();
    });
    observer.observe({ type: "paint" });
    const timer = setTimeout(done, PAINT_REPORT_TIMEOUT_MS);
  });
}

/** Resolves once the current screen has been painted and the browser has reported that paint. */
export function afterFirstPaint(): Promise<void> {
  return nextFrame().then(firstContentfulPaint);
}

/**
 * True once the browser has painted the current screen and reported it. Gate secondary queries
 * and lazy chunks on it (`enabled`, or mount them only when true) so they do not compete with the
 * first paint of the primary task (e.g. the day's visit list on /quay vs the customer search box;
 * Lighthouse LCP budget, testing.md §4).
 */
export function useAfterFirstPaint(): boolean {
  const [painted, setPainted] = useState(false);
  useEffect(() => {
    let active = true;
    void afterFirstPaint().then(() => {
      if (active) setPainted(true);
    });
    return () => {
      active = false;
    };
  }, []);
  return painted;
}
