import { describe, expect, it } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gzipKib, measure, startupScripts } from "./startup-budget.mjs";

const html = `<!doctype html><html><head>
  <script type="module" crossorigin src="/assets/index-abc.js"></script>
  <link rel="modulepreload" crossorigin href="/assets/rolldown-runtime-x.js">
  <link rel="preload" href="/api/v1/me" as="fetch" crossorigin="anonymous" />
  <link rel="stylesheet" crossorigin href="/assets/index-abc.css">
  <link rel="icon" href="data:," data-placeholder />
</head><body><div id="root"></div></body></html>`;

describe("startup JS budget", () => {
  it("counts the entry and every modulepreload, nothing else", () => {
    expect(startupScripts(html)).toEqual(["/assets/index-abc.js", "/assets/rolldown-runtime-x.js"]);
  });

  it("sums the gzip size of every startup script against the budget", () => {
    const dist = mkdtempSync(join(tmpdir(), "budget-"));
    mkdirSync(join(dist, "assets"));
    writeFileSync(join(dist, "index.html"), html);
    // Random bytes do not compress, so each file's gzip size is about its raw size.
    const entry = Buffer.from(Array.from({ length: 4096 }, (_, i) => (i * 7919) % 251));
    const runtime = Buffer.from(Array.from({ length: 2048 }, (_, i) => (i * 104729) % 241));
    writeFileSync(join(dist, "assets/index-abc.js"), entry);
    writeFileSync(join(dist, "assets/rolldown-runtime-x.js"), runtime);

    const expected = gzipKib(entry) + gzipKib(runtime);
    const fits = measure(dist, expected);
    expect(fits.rows.map((r) => r.file)).toEqual(["/assets/index-abc.js", "/assets/rolldown-runtime-x.js"]);
    expect(fits.total).toBeCloseTo(expected, 6);
    expect(fits.ok).toBe(true);
    // The entry alone would fit; the preloaded runtime is what tips it over.
    expect(measure(dist, gzipKib(entry) + gzipKib(runtime) / 2).ok).toBe(false);
  });
});
