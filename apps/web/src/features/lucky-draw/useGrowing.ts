import { useCallback, useEffect, useState } from "react";

/**
 * A long list drawn in steps as its panel scrolls: the first `step` rows at once, `step` more each
 * time the end comes within `margin` of the panel's edge. No "Hiện thêm" button (EM 2026-11-28:
 * the list scrolls inside its panel); the point is a fast first paint: 2.000 people drawn at once
 * took ~340 ms on a desktop (dev build), several times that on a slow counter laptop.
 *
 * `sentinel` goes on an empty element after the last drawn row, present only while rows are left
 * (keyed by `shown.length`, so each step observes a fresh element). A new `resetKey` (another
 * filter or search) starts again from the first step. Without IntersectionObserver every row is
 * drawn.
 */
export function useGrowing<T>(items: readonly T[], resetKey: string, step = 150, margin = "600px") {
  const [limit, setLimit] = useState(step);
  const [seenKey, setSeenKey] = useState(resetKey);
  if (seenKey !== resetKey) {
    setSeenKey(resetKey);
    setLimit(step);
  }
  const [node, setNode] = useState<HTMLElement | null>(null);
  const sentinel = useCallback((el: HTMLElement | null) => {
    setNode(el);
  }, []);
  const observable = typeof IntersectionObserver !== "undefined";

  useEffect(() => {
    if (!node || !observable) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) setLimit((n) => n + step);
      },
      { root: node.closest(".ld-scroll"), rootMargin: margin },
    );
    io.observe(node);
    return () => {
      io.disconnect();
    };
  }, [node, observable, step, margin]);

  const shown = observable ? items.slice(0, limit) : items;
  return { shown, more: shown.length < items.length, sentinel };
}
