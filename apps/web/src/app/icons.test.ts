import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Browsers keep favicons far past normal caching (EM round 4: "logo trên thanh bar new tab … chưa
// đúng"), so every icon URL carries the generator's ICON_VERSION; a new mark needs a new version.
const read = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");
const html = read("../../index.html");
const manifest = read("../../public/manifest.webmanifest");
const version = /^ICON_VERSION = (\d+)$/m.exec(read("../../../../scripts/brand/build_logo_assets.py"))?.[1];

describe("browser icons", () => {
  it("links the tab mark (svg + ico + png fallbacks), apple-touch and the manifest", () => {
    for (const href of ["/favicon.svg", "/favicon.ico", "/favicon-32.png", "/favicon-16.png", "/apple-touch-icon.png"]) {
      expect(html).toContain(`href="${href}?v=`);
    }
    expect(html).toContain('rel="manifest" href="/manifest.webmanifest"');
  });

  it("cache-busts every icon with the generator's ICON_VERSION", () => {
    expect(version).toBeDefined();
    const versions = [...html.matchAll(/(?:favicon[^"]*|apple-touch-icon\.png)\?v=(\d+)/g), ...manifest.matchAll(/\.png\?v=(\d+)/g)].map((m) => m[1]);
    expect(versions.length).toBe(8);
    expect(new Set(versions)).toEqual(new Set([version]));
  });
});
