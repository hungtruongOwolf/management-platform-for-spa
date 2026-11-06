import { afterEach, describe, expect, it, vi } from "vitest";
import { PAINT_REPORT_TIMEOUT_MS, afterFirstPaint } from "./afterFirstPaint";

// A fake Paint Timing API: the browser reports first-contentful-paint when `report()` is called.
function fakePaintTiming() {
  let reported = false;
  const observers: (() => void)[] = [];
  class FakeObserver {
    static supportedEntryTypes = ["paint", "largest-contentful-paint"];
    constructor(private readonly callback: () => void) {}
    observe() {
      observers.push(this.callback);
    }
    disconnect() {
      observers.splice(observers.indexOf(this.callback), 1);
    }
  }
  vi.stubGlobal("PerformanceObserver", FakeObserver);
  vi.spyOn(performance, "getEntriesByName").mockImplementation((name: string) =>
    reported && name === "first-contentful-paint" ? ([{}] as PerformanceEntryList) : [],
  );
  return {
    report() {
      reported = true;
      for (const cb of [...observers]) cb();
    },
    observing: () => observers.length,
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
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("waits for the browser to report the first contentful paint, not just for a frame", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame"] });
    const paint = fakePaintTiming();
    const p = afterFirstPaint();
    await vi.advanceTimersByTimeAsync(50); // frames go by, the paint is not reported yet
    expect(await settled(p)).toBe(false);
    paint.report();
    expect(await settled(p)).toBe(true);
    expect(paint.observing()).toBe(0);
  });

  it("resolves right after a frame when the paint was already reported (in-app navigation)", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame"] });
    const paint = fakePaintTiming();
    paint.report();
    const p = afterFirstPaint();
    await vi.advanceTimersByTimeAsync(20);
    expect(await settled(p)).toBe(true);
  });

  it("gives up waiting after PAINT_REPORT_TIMEOUT_MS when no paint is ever reported", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame"] });
    const paint = fakePaintTiming();
    const p = afterFirstPaint();
    await vi.advanceTimersByTimeAsync(PAINT_REPORT_TIMEOUT_MS - 100);
    expect(await settled(p)).toBe(false);
    await vi.advanceTimersByTimeAsync(200);
    expect(await settled(p)).toBe(true);
    expect(paint.observing()).toBe(0);
  });
});
