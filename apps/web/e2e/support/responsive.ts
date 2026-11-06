import type { Page } from "@playwright/test";

/**
 * Responsive layout checks (EM 2026-11-05 round 3: "tăng giảm kích thước trang … đừng có bị lỗi").
 * Run inside the page at one viewport; each returns human-readable problems (empty = fine).
 */

/** The page scrolls sideways by this many pixels (0 = fits). */
export function sidewaysOverflow(page: Page): Promise<number> {
  return page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
}

/**
 * Sibling boxes that overlap: for every element of the app frame, its visible, in-flow, non-inline
 * children must not intersect each other (beyond 1px of anti-aliasing). Absolutely positioned
 * decoration, sr-only text, inline runs (which wrap across lines) and SVG internals are skipped.
 */
export function overlappingSiblings(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const describe = (el: Element) =>
      `<${el.tagName.toLowerCase()} class="${(el.getAttribute("class") ?? "").slice(0, 50)}"> ${el.textContent.trim().slice(0, 30)}`;
    const box = (el: Element): DOMRect | null => {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.display === "inline" || cs.display === "contents") return null;
      // <colgroup>/<col> only carry widths; their box spans the whole table by definition.
      if (cs.display === "table-column-group" || cs.display === "table-column") return null;
      if (cs.visibility === "hidden" || Number(cs.opacity) === 0) return null;
      if (cs.position === "absolute" || cs.position === "fixed") return null;
      // A bottom action bar (StickyActions) floats over the content it scrolls past, by design.
      if (cs.position === "sticky" && el.classList.contains("sticky-actions")) return null;
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0 ? r : null;
    };
    const parents = document.querySelectorAll(".shell, .shell *:not(svg, svg *)");
    for (const parent of Array.from(parents)) {
      const kids = Array.from(parent.children)
        .map((k) => [k, box(k)] as const)
        .filter((pair): pair is readonly [Element, DOMRect] => pair[1] !== null);
      for (let i = 0; i < kids.length; i++) {
        for (let j = i + 1; j < kids.length; j++) {
          const [a, ra] = kids[i] as readonly [Element, DOMRect];
          const [b, rb] = kids[j] as readonly [Element, DOMRect];
          const w = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left);
          const h = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
          if (w > 1 && h > 1) out.push(`${describe(a)}  ⨯  ${describe(b)} (${String(Math.round(w))}×${String(Math.round(h))}px)`);
        }
      }
    }
    return out;
  });
}

/**
 * Text that is cut off: an element that clips its overflow (hidden/clip), has its own text, and
 * whose content is larger than its box — unless it ends in an ellipsis on purpose (`truncate`).
 */
export function clippedText(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    for (const el of Array.from(document.querySelectorAll<HTMLElement>(".shell *:not(svg, svg *)"))) {
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden") continue;
      const clipsX = cs.overflowX === "hidden" || cs.overflowX === "clip";
      const clipsY = cs.overflowY === "hidden" || cs.overflowY === "clip";
      if (!clipsX && !clipsY) continue;
      if (cs.textOverflow === "ellipsis") continue;
      // sr-only: clipped by design.
      if (el.getBoundingClientRect().width <= 1) continue;
      const ownText = Array.from(el.childNodes).some((n) => n.nodeType === Node.TEXT_NODE && (n.textContent ?? "").trim() !== "");
      if (!ownText) continue;
      if ((clipsX && el.scrollWidth > el.clientWidth + 1) || (clipsY && el.scrollHeight > el.clientHeight + 1)) {
        out.push(`<${el.tagName.toLowerCase()} class="${(el.getAttribute("class") ?? "").slice(0, 50)}"> ${el.textContent.trim().slice(0, 40)}`);
      }
    }
    return out;
  });
}

/**
 * Focus rings that cover other text (EM round 3: the search box's ring hid its label). Focuses a
 * sample of controls with keyboard modality, measures the outline (width + positive offset) around
 * each, and reports text of other elements inside that ring.
 */
export async function focusRingsOverText(page: Page, max = 16): Promise<string[]> {
  await page.keyboard.press("Shift");
  return page.evaluate((limit) => {
    const out: string[] = [];
    const controls = Array.from(
      document.querySelectorAll<HTMLElement>(
        "main input:not([type=hidden]), main select, main textarea, main button, main a[href], .sidebar a[href], .topbar button",
      ),
    ).filter((el) => {
      const r = el.getBoundingClientRect();
      if (r.width <= 1 || r.height <= 1 || getComputedStyle(el).visibility === "hidden") return false;
      // Only controls the user can see right now (not under the phone tab bar or below the fold).
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return hit !== null && el.contains(hit);
    });
    const step = Math.max(1, controls.length / limit);
    const sample = Array.from({ length: Math.min(limit, controls.length) }, (_, i) => controls[Math.floor(i * step)]).filter(
      (el): el is HTMLElement => el !== undefined,
    );
    const texts: { node: Text; rects: DOMRect[] }[] = [];
    const walker = document.createTreeWalker(document.querySelector(".shell") ?? document.body, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const t = n as Text;
      if (t.textContent.trim() === "") continue;
      const range = document.createRange();
      range.selectNodeContents(t);
      const rects = Array.from(range.getClientRects()).filter((r) => r.width > 1 && r.height > 1);
      if (rects.length) texts.push({ node: t, rects });
    }
    for (const el of sample) {
      el.focus({ preventScroll: true });
      if (document.activeElement !== el) continue;
      const cs = getComputedStyle(el);
      if (cs.outlineStyle === "none" || parseFloat(cs.outlineWidth) === 0) continue;
      // The ring's outer edge sits offset + width outside the border (an inset ring: ≤ 0, nothing outside).
      const reach = Math.max(0, parseFloat(cs.outlineWidth) + parseFloat(cs.outlineOffset));
      const r = el.getBoundingClientRect();
      const ring = { left: r.left - reach, right: r.right + reach, top: r.top - reach, bottom: r.bottom + reach };
      for (const { node, rects } of texts) {
        if (el.contains(node)) continue;
        const owner = node.parentElement;
        if (!owner || owner.closest(".sr-only")) continue;
        for (const t of rects) {
          // Only the band between the control's border and the ring's outer edge is "ring".
          const inRing = t.right > ring.left + 0.5 && t.left < ring.right - 0.5 && t.bottom > ring.top + 0.5 && t.top < ring.bottom - 0.5;
          const inControl = t.left >= r.left && t.right <= r.right && t.top >= r.top && t.bottom <= r.bottom;
          if (inRing && !inControl) {
            out.push(`ring of "${(el.getAttribute("aria-label") ?? el.textContent).trim().slice(0, 30)}" covers "${node.textContent.trim().slice(0, 30)}"`);
            break;
          }
        }
      }
      el.blur();
    }
    return [...new Set(out)];
  }, max);
}
