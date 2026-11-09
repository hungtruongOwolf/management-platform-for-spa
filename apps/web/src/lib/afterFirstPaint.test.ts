import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type Module = typeof import("./afterFirstPaint");

/**
 * A fake Performance Timeline. `paint()` reports first-contentful-paint; `lcp(t)` reports a
 * largest-contentful-paint candidate painted at `t` (buffered entries replay on observe).
 */
function fakeTimeline(supported: string[]) {
  let fcp = false;
  const lcps: number[] = [];
  const observers = new Map<object, { type: string; cb: (list: { getEntries: () => { startTime: number }[] }) => void }>();
  const entries = (times: number[]) => ({ getEntries: () => times.map((startTime) => ({ startTime })) });
  class FakeObserver {
    static supportedEntryTypes = supported;
    constructor(private readonly callback: (list: { getEntries: () => { startTime: number }[] }) => void) {}
    observe({ type, buffered }: { type: string; buffered?: boolean }) {
      observers.set(this, { type, cb: this.callback });
      if (type === "largest-contentful-paint" && buffered && lcps.length > 0) this.callback(entries(lcps));
    }
    disconnect() {
      observers.delete(this);
    }
  }
  vi.stubGlobal("PerformanceObserver", FakeObserver);
  vi.spyOn(performance, "getEntriesByName").mockImplementation((name: string) =>
    fcp && name === "first-contentful-paint" ? ([{}] as PerformanceEntryList) : [],
  );
  return {
    paint() {
      fcp = true;
      for (const o of [...observers.values()]) if (o.type === "paint") o.cb(entries([]));
    },
    lcp(t: number) {
      lcps.push(t);
      for (const o of [...observers.values()]) if (o.type === "largest-contentful-paint") o.cb(entries([t]));
    },
    observing: () => observers.size,
  };
}

async function settled(p: Promise<void>): Promise<boolean> {
  let done = false;
  void p.then(() => {
    done = true;
  });
  await vi.advanceTimersByTimeAsync(0);
  return done;
}

describe("afterFirstPaint", () => {
  let mod: Module;
  beforeEach(async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame"] });
    // The module remembers that the page load's paint was reported: a fresh copy per test.
    vi.resetModules();
    mod = await import("./afterFirstPaint");
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("waits for a paint reported after the screen rendered, not for an earlier frame (shell, skeleton)", async () => {
    const t = fakeTimeline(["paint", "largest-contentful-paint"]);
    t.paint();
    t.lcp(80); // the skeleton's paint, before the screen rendered at 100
    const p = mod.afterFirstPaint(100);
    await vi.advanceTimersByTimeAsync(50);
    expect(await settled(p)).toBe(false);
    t.lcp(190); // the screen's own paint
    expect(await settled(p)).toBe(true);
    expect(t.observing()).toBe(0);
  });

  it("after the load's paint, a later screen (in-app navigation) only waits a frame", async () => {
    const t = fakeTimeline(["paint", "largest-contentful-paint"]);
    t.paint();
    const first = mod.afterFirstPaint(100);
    await vi.advanceTimersByTimeAsync(20);
    t.lcp(150);
    expect(await settled(first)).toBe(true);
    // After user input the browser reports no more LCP: no wait for one.
    const later = mod.afterFirstPaint(5000);
    await vi.advanceTimersByTimeAsync(20);
    expect(await settled(later)).toBe(true);
  });

  it("gives up after PAINT_REPORT_TIMEOUT_MS when no paint is ever reported", async () => {
    const t = fakeTimeline(["paint", "largest-contentful-paint"]);
    const p = mod.afterFirstPaint(100);
    await vi.advanceTimersByTimeAsync(mod.PAINT_REPORT_TIMEOUT_MS - 100);
    expect(await settled(p)).toBe(false);
    await vi.advanceTimersByTimeAsync(200);
    expect(await settled(p)).toBe(true);
    expect(t.observing()).toBe(0);
  });

  it("without LCP reporting, waits for the reported first contentful paint", async () => {
    const t = fakeTimeline(["paint"]);
    const p = mod.afterFirstPaint();
    await vi.advanceTimersByTimeAsync(50);
    expect(await settled(p)).toBe(false);
    t.paint();
    expect(await settled(p)).toBe(true);
  });

  it("resolves right after a frame without any Paint Timing API (jsdom)", async () => {
    const p = mod.afterFirstPaint();
    await vi.advanceTimersByTimeAsync(20);
    expect(await settled(p)).toBe(true);
  });
});
