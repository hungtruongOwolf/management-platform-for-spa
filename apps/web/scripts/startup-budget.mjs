// Startup JS budget gate (frontend.md "Perf budget", ADR-0019 D21).
// Startup JS = the entry <script type="module"> PLUS every <link rel="modulepreload"> in
// dist/index.html: the browser fetches and parses all of them before the first screen, so a
// helper chunk split off the entry (e.g. rolldown-runtime) counts just like the entry does.
// Size = gzip level 9 of each file, summed, in KiB. Run after `vite build`.
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

export const BUDGET_KIB = 125;

/** Every script the browser loads before the first screen, in document order. */
export function startupScripts(html) {
  const paths = [];
  for (const [tag] of html.matchAll(/<(?:script|link)\b[^>]*>/gi)) {
    const isEntry = /^<script\b/i.test(tag) && /\btype\s*=\s*["']module["']/i.test(tag);
    const isPreload = /^<link\b/i.test(tag) && /\brel\s*=\s*["']modulepreload["']/i.test(tag);
    if (!isEntry && !isPreload) continue;
    const url = /\b(?:src|href)\s*=\s*["']([^"']+)["']/i.exec(tag)?.[1];
    if (url && !/^(?:[a-z]+:)?\/\//i.test(url) && !paths.includes(url)) paths.push(url);
  }
  return paths;
}

export function gzipKib(bytes) {
  return gzipSync(bytes, { level: 9 }).length / 1024;
}

/** Measures dist/: one row per startup script plus the total and whether it fits the budget. */
export function measure(distDir, budgetKib = BUDGET_KIB) {
  const html = readFileSync(join(distDir, "index.html"), "utf8");
  const rows = startupScripts(html).map((url) => ({
    file: url,
    kib: gzipKib(readFileSync(join(distDir, url.replace(/^\//, "").split(/[?#]/)[0]))),
  }));
  if (rows.length === 0) throw new Error("no module script in dist/index.html: was the build run?");
  const total = rows.reduce((sum, r) => sum + r.kib, 0);
  return { rows, total, ok: total <= budgetKib };
}

if (process.argv[1] && resolve(fileURLToPath(import.meta.url)) === resolve(process.argv[1])) {
  const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
  const { rows, total, ok } = measure(dist);
  for (const r of rows) console.log(`${r.kib.toFixed(2).padStart(8)} KiB  ${r.file}`);
  console.log(`${total.toFixed(2).padStart(8)} KiB  startup JS (entry + modulepreload, gzip -9), budget ${BUDGET_KIB} KiB`);
  if (!ok) {
    console.error(`✗ startup JS over budget by ${(total - BUDGET_KIB).toFixed(2)} KiB (frontend.md "Perf budget")`);
    process.exit(1);
  }
}
