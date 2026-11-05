import { useSyncExternalStore } from "react";

/** Phone layout below this width (design-system § Breakpoints `sm`). */
export const PHONE_MAX = 639.98;

/**
 * Live `matchMedia` result. Without `matchMedia` (jsdom, very old browsers) it answers `false`, so
 * tests and the fallback render the desktop layout.
 */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      if (typeof window.matchMedia !== "function") return () => undefined;
      const mq = window.matchMedia(query);
      mq.addEventListener("change", onChange);
      return () => {
        mq.removeEventListener("change", onChange);
      };
    },
    () => typeof window.matchMedia === "function" && window.matchMedia(query).matches,
  );
}

/** `true` on phones (< 640 px): lists render as cards, forms as bottom sheets. */
export function usePhone(): boolean {
  return useMediaQuery(`(max-width: ${String(PHONE_MAX)}px)`);
}
