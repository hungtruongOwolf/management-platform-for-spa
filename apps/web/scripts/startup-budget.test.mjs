import { describe, expect, it } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { scriptSource } from "./route-preload.mjs";
import { gzipKib, measure, startupScripts } from "./startup-budget.mjs";

const html = `<!doctype html><html><head>
  <script type="module" crossorigin src="/assets/index-abc.js"></script>
  <link rel="modulepreload" crossorigin href="/assets/rolldown-runtime-x.js">
  <link rel="preload" href="/api/v1/me" as="fetch" crossorigin="anonymous" />
  <link rel="stylesheet" crossorigin href="/assets/index-abc.css">
  <link rel="icon" href="data:," data-placeholder />
</head><body><div id="root"></div></body></html>`;

// Random bytes do not compress, so each file's gzip size is about its raw size.
const noise = (length, seed) => Buffer.from(Array.from({ length }, (_, i) => (i * seed) % 251));

describe("startup JS budget", () => {
  it("counts the entry and every modulepreload, nothing else", () => {
    expect(startupScripts(html)).toEqual(["/assets/index-abc.js", "/assets/rolldown-runtime-x.js"]);
  });

  it("sums the gzip size of every startup script against the budget", () => {
    const dist = mkdtempSync(join(tmpdir(), "budget-"));
    mkdirSync(join(dist, "assets"));
    writeFileSync(join(dist, "index.html"), html);
    const entry = noise(4096, 7919);
    const runtime = noise(2048, 104729);
    writeFileSync(join(dist, "assets/index-abc.js"), entry);
    writeFileSync(join(dist, "assets/rolldown-runtime-x.js"), runtime);

    const expected = gzipKib(entry) + gzipKib(runtime);
    const fits = measure(dist, expected);
    expect(fits.rows.map((r) => r.file)).toEqual(["/assets/index-abc.js", "/assets/rolldown-runtime-x.js"]);
    expect(fits.total).toBeCloseTo(expected, 6);
    expect(fits.routes).toEqual([]);
    expect(fits.ok).toBe(true);
    // The entry alone would fit; the preloaded runtime is what tips it over.
    expect(measure(dist, gzipKib(entry) + gzipKib(runtime) / 2).ok).toBe(false);
  });

  it("also gates each screen the route-preload script starts: startup + its own JS", () => {
    const dist = mkdtempSync(join(tmpdir(), "budget-"));
    mkdirSync(join(dist, "assets"));
    const preload = scriptSource({ "/quay": ["Desk-q.js", "Desk-q.css"], "/": ["Home-h.js"] }, "/assets/");
    writeFileSync(
      join(dist, "index.html"),
      html.replace("<script", '<script type="module" async crossorigin src="/assets/route-preload-z.js"></script>\n<script'),
    );
    const entry = noise(4096, 7919);
    const desk = noise(1024, 104729);
    const home = noise(256, 7919);
    writeFileSync(join(dist, "assets/route-preload-z.js"), preload);
    writeFileSync(join(dist, "assets/index-abc.js"), entry);
    writeFileSync(join(dist, "assets/rolldown-runtime-x.js"), "");
    writeFileSync(join(dist, "assets/Desk-q.js"), desk);
    writeFileSync(join(dist, "assets/Desk-q.css"), noise(4096, 13));
    writeFileSync(join(dist, "assets/Home-h.js"), home);

    const startup = gzipKib(Buffer.from(preload)) + gzipKib(entry) + gzipKib(Buffer.alloc(0));
    const fits = measure(dist, startup + gzipKib(desk));
    expect(fits.total).toBeCloseTo(startup, 6);
    // CSS is not JS: the screen's stylesheet is left out of its total.
    expect(fits.routes.map((r) => [r.path, r.rows.map((x) => x.file)])).toEqual([
      ["/quay", ["/assets/Desk-q.js"]],
      ["/", ["/assets/Home-h.js"]],
    ]);
    expect(fits.routes[0]?.total).toBeCloseTo(startup + gzipKib(desk), 6);
    expect(fits.ok).toBe(true);
    // Startup alone fits; the front desk's own chunk is what tips its path over.
    expect(measure(dist, startup + gzipKib(desk) / 2).ok).toBe(false);
  });
});
