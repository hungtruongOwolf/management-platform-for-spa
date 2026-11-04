import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const cssLoaded = vi.fn();

// fonts.ts keeps a module-level "already requested" flag: load a fresh copy per test, with a
// fresh mock of the stylesheet that records when it is imported.
async function freshFonts() {
  vi.resetModules();
  vi.doMock("../fonts.css", () => {
    cssLoaded();
    return {};
  });
  return import("./fonts");
}

describe("web-font loading policy", () => {
  beforeEach(() => {
    localStorage.clear();
    cssLoaded.mockClear();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("cold load: nothing at startup, fonts shortly after the first screen, then remembered", async () => {
    const fonts = await freshFonts();
    expect(fonts.loadFontsIfCached()).toBe(false);

    fonts.loadFontsAfterFirstScreen();
    expect(cssLoaded).not.toHaveBeenCalled();
    await vi.runAllTimersAsync();
    await vi.dynamicImportSettled();
    expect(cssLoaded).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem("vpyd.fonts-cached")).toBe("1");
  });

  it("warm load: fonts are requested at startup, once", async () => {
    localStorage.setItem("vpyd.fonts-cached", "1");
    const fonts = await freshFonts();
    expect(fonts.loadFontsIfCached()).toBe(true);
    fonts.loadFontsAfterFirstScreen();
    await vi.runAllTimersAsync();
    await vi.dynamicImportSettled();
    expect(cssLoaded).toHaveBeenCalledTimes(1);
  });
});
