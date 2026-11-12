// Startup JS budget gate (frontend.md "Perf budget", ADR-0019 D21).
//
// Startup JS = every <script type="module"> PLUS every <link rel="modulepreload"> in
// dist/index.html: the browser fetches and parses all of them before the first screen, so a
// helper chunk split off the entry (e.g. rolldown-runtime) counts just like the entry does.
//
// First screens that are lazy chunks (/quay, home: src/app/screens.ts) are fetched next to the
// startup by the route-preload script (scripts/route-preload.mjs) when the app opens on them, so
// their critical path is startup + their own chunks. Each such path is gated on the same budget:
// moving code from the startup chunk into a screen's chunk only helps the other screens.
//
// Size = gzip level 9 of each file, summed, in KiB. Run after `vite build`.
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { ROUTE_TABLE_PATTERN, SCRIPT_PREFIX } from "./route-preload.mjs";

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

/** path → files the route-preload script starts for it (empty when the build has no such script). */
export function routeFiles(distDir, scripts) {
  const script = scripts.find((url) => url.replace(/^\//, "").startsWith(SCRIPT_PREFIX));
  if (!script) return {};
  const table = ROUTE_TABLE_PATTERN.exec(readFileSync(fileOf(distDir, script), "utf8"))?.[1];
  if (!table) throw new Error(`${script} has no route table: was it built by scripts/route-preload.mjs?`);
  return JSON.parse(table);
}

export function gzipKib(bytes) {
  return gzipSync(bytes, { level: 9 }).length / 1024;
}

function fileOf(distDir, url) {
  return join(distDir, url.replace(/^\//, "").split(/[?#]/)[0]);
}

function rowsOf(distDir, urls) {
  return urls.map((url) => ({ file: url, kib: gzipKib(readFileSync(fileOf(distDir, url))) }));
}

const sum = (rows) => rows.reduce((total, r) => total + r.kib, 0);

/**
 * Measures dist/: the startup scripts and their total, then for each first screen with its own
 * chunk, its extra files and the total of its critical path. `ok` = every total fits the budget.
 */
export function measure(distDir, budgetKib = BUDGET_KIB) {
  const html = readFileSync(join(distDir, "index.html"), "utf8");
  const scripts = startupScripts(html);
  const rows = rowsOf(distDir, scripts);
  if (rows.length === 0) throw new Error("no module script in dist/index.html: was the build run?");
  const total = sum(rows);
  const routes = Object.entries(routeFiles(distDir, scripts)).map(([path, urls]) => {
    // JS only: a screen's CSS is not part of the JS budget.
    const extra = rowsOf(distDir, urls.filter((u) => !u.endsWith(".css")));
    return { path, rows: extra, total: total + sum(extra) };
  });
  const ok = total <= budgetKib && routes.every((r) => r.total <= budgetKib);
  return { rows, total, routes, ok };
}

if (process.argv[1] && resolve(fileURLToPath(import.meta.url)) === resolve(process.argv[1])) {
  const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
  const { rows, total, routes, ok } = measure(dist);
  const line = (kib, text) => `${kib.toFixed(2).padStart(8)} KiB  ${text}`;
  for (const r of rows) console.log(line(r.kib, r.file));
  console.log(line(total, `startup JS, every screen (entry + modulepreload, gzip -9), budget ${BUDGET_KIB} KiB`));
  for (const route of routes) {
    console.log("");
    for (const r of route.rows) console.log(line(r.kib, `  + ${r.file}`));
    console.log(line(route.total, `${route.path}: startup + its own chunks, budget ${BUDGET_KIB} KiB`));
  }
  if (!ok) {
    const over = [{ path: "startup", total }, ...routes].filter((r) => r.total > BUDGET_KIB);
    for (const r of over)
      console.error(`✗ ${r.path} JS over budget by ${(r.total - BUDGET_KIB).toFixed(2)} KiB (frontend.md "Perf budget")`);
    process.exit(1);
  }
}
