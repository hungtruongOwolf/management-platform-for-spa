import { expect, type Locator, type Page } from "@playwright/test";

/**
 * Layout stability (EM 2026-11-05: "khi bấm ra hiệu ứng ảnh hưởng luôn các phần khác"). Hover and
 * press may only use transform / opacity / box-shadow / outline, so no other element may move.
 * These helpers record the boxes of every visible element on the page, interact with one control,
 * and report the elements (outside the control itself) whose box changed.
 */

export interface Shift {
  element: string;
  before: string;
  after: string;
}

type Boxes = Record<string, string>;

/** Tags every visible element with a stable index and returns its rounded box. */
async function boxes(page: Page): Promise<Boxes> {
  return page.evaluate(() => {
    const out: Record<string, string> = {};
    let i = 0;
    for (const el of Array.from(document.querySelectorAll<HTMLElement>("main *, header *, nav *, aside *"))) {
      // The front desk's day clock re-renders every second (its digits change width): that is the
      // clock ticking, not a hover side effect (CI flake on main after #56, 13:00–21:00 spa time).
      if (el.closest(".day-clock")) continue;
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const id = el.dataset["layoutId"] ?? String(i++);
      el.dataset["layoutId"] = id;
      out[id] = [r.x, r.y, r.width, r.height].map((n) => Math.round(n * 2) / 2).join(",");
    }
    return out;
  });
}

/** Ids of the control and everything inside it: its own transform legitimately moves them. */
async function ownIds(target: Locator): Promise<Set<string>> {
  const ids = await target.evaluate((el) =>
    [el, ...Array.from(el.querySelectorAll<HTMLElement>("*"))]
      .map((e) => (e as HTMLElement).dataset["layoutId"])
      .filter((v): v is string => v !== undefined),
  );
  return new Set(ids);
}

async function describe(page: Page, id: string): Promise<string> {
  return page.evaluate((key) => {
    const el = document.querySelector<HTMLElement>(`[data-layout-id="${key}"]`);
    if (!el) return key;
    const text = (el.getAttribute("aria-label") ?? el.textContent).trim().slice(0, 40);
    return `<${el.tagName.toLowerCase()} class="${(el.getAttribute("class") ?? "").slice(0, 60)}"> ${text}`;
  }, id);
}

function diff(before: Boxes, after: Boxes, skip: Set<string>): string[] {
  return Object.keys(before).filter((id) => !skip.has(id) && after[id] !== undefined && after[id] !== before[id]);
}

/**
 * Hovers, then presses (mouse down, move away, release: no click fires) one control and returns
 * every element that moved or resized meanwhile.
 */
export async function shiftsOnInteraction(page: Page, target: Locator): Promise<Shift[]> {
  await target.scrollIntoViewIfNeeded();
  await page.mouse.move(0, 0);
  const before = await boxes(page);
  const skip = await ownIds(target);
  const box = await target.boundingBox();
  if (!box) return [];
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  const hovered = await boxes(page);
  await page.mouse.down();
  const pressed = await boxes(page);
  await page.mouse.move(0, 0);
  await page.mouse.up();
  const moved = new Set([...diff(before, hovered, skip), ...diff(before, pressed, skip)]);
  const out: Shift[] = [];
  for (const id of moved) {
    out.push({ element: await describe(page, id), before: before[id] ?? "", after: pressed[id] ?? hovered[id] ?? "" });
  }
  return out;
}

/** A sample of the visible interactive controls in the main area and the app chrome. */
export function interactiveSample(page: Page, max = 12): Promise<Locator[]> {
  return page
    .locator("main button:visible, main a:visible, aside a:visible, header button:visible, main [role=option]:visible")
    .all()
    .then((all) => {
      if (all.length <= max) return all;
      const step = all.length / max;
      return Array.from({ length: max }, (_, i) => all[Math.floor(i * step)]).filter((l): l is Locator => l !== undefined);
    });
}

/** Two animation frames: whatever a resize or a state change scheduled is laid out and painted. */
export async function nextPaint(page: Page): Promise<void> {
  await page.evaluate(
    () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => { resolve(); }))),
  );
}

/**
 * The screen has finished loading: no skeleton or "Đang tải…" placeholder, nothing `aria-busy`,
 * web fonts in, then one painted frame. Waits on what the page shows, not on the network going
 * quiet (`networkidle`: slow, flaky, discouraged by Playwright) or on a fixed delay.
 */
export async function settled(page: Page): Promise<void> {
  await expect(page.locator(".skeleton, [role='status'][aria-label^='Đang'], [aria-busy='true']")).toHaveCount(0);
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await nextPaint(page);
}
