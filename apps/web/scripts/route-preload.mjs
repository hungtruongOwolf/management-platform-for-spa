// Vite build plugin: starts the landing screen's own chunk next to the startup chunk.
//
// Some first screens (/quay, home) are lazy route chunks, so the startup chunk every screen pays
// stays small (frontend.md "Perf budget"). Left alone, the browser would fetch such a screen's chunk
// only after the startup chunk has downloaded and run: one round trip more before its first paint.
// This plugin emits a tiny module script, placed in <head> before the entry, that looks at the URL
// and adds <link rel="modulepreload"> for that screen's chunk and the chunks it imports. Both
// downloads then run side by side. The CSP stays `script-src 'self'`: the script is a file.
//
// `startup-budget.mjs` reads the same table back (ROUTE_TABLE_PATTERN) and gates every screen's
// critical path: startup + its own chunks.
import { createHash } from "node:crypto";

/** Matches the generated table in the emitted script; group 1 is its JSON. */
export const ROUTE_TABLE_PATTERN = /\/\*routes\*\/(\{.*?\})\/\*end\*\//s;

/** File name prefix of the emitted script, so tools can find it in index.html. */
export const SCRIPT_PREFIX = "assets/route-preload-";

/** Every file `chunk` needs before it runs: itself and its static imports, recursively. */
export function staticClosure(bundle, fileName, seen = new Set()) {
  if (seen.has(fileName)) return seen;
  seen.add(fileName);
  const chunk = bundle[fileName];
  if (chunk?.type === "chunk") {
    for (const dep of chunk.imports) staticClosure(bundle, dep, seen);
  }
  return seen;
}

/** The chunk that holds `moduleFile` (a path ending, e.g. "/src/app/HomePage.tsx"). */
function chunkOf(bundle, moduleFile) {
  const norm = (id) => id.replace(/\\/g, "/");
  return Object.values(bundle).find(
    (c) =>
      c.type === "chunk" &&
      (norm(c.facadeModuleId ?? "").endsWith(moduleFile) || c.moduleIds.some((id) => norm(id).endsWith(moduleFile))),
  );
}

/**
 * path → files to preload when the app opens on that path: the screen's chunk, the chunks it
 * imports and their CSS, minus what the startup (entry + its imports) already loads.
 */
export function routeTable(bundle, routes, base = "/") {
  const entry = Object.values(bundle).find((c) => c.type === "chunk" && c.isEntry);
  if (!entry) throw new Error("route-preload: no entry chunk");
  const startup = staticClosure(bundle, entry.fileName);
  const table = {};
  for (const [path, moduleFile] of Object.entries(routes)) {
    const chunk = chunkOf(bundle, moduleFile);
    if (!chunk) throw new Error(`route-preload: no chunk holds ${moduleFile} (${path})`);
    if (startup.has(chunk.fileName)) throw new Error(`route-preload: ${moduleFile} is in the startup chunk`);
    const files = [];
    for (const file of staticClosure(bundle, chunk.fileName)) {
      if (startup.has(file)) continue;
      files.push(base + file);
      for (const css of bundle[file]?.viteMetadata?.importedCss ?? []) {
        if (!files.includes(base + css)) files.push(base + css);
      }
    }
    table[path] = files;
  }
  return table;
}

/** The emitted script: tiny, no imports; `crossOrigin = ""` matches Vite's own preload links. */
export function scriptSource(table) {
  return (
    `const r=/*routes*/${JSON.stringify(table)}/*end*/;\n` +
    `for(const h of r[location.pathname.replace(/(.)\\/+$/,"$1")]||[]){` +
    `const l=document.createElement("link"),c=h.endsWith(".css");` +
    `l.rel=c?"preload":"modulepreload";if(c)l.as="style";l.crossOrigin="";l.href=h;document.head.append(l)}\n`
  );
}

/** `routes`: path → the module of its screen (path ending), e.g. { "/quay": "/src/features/front-desk/FrontDeskPage.tsx" }. */
export function routePreload(routes) {
  let base = "/";
  return {
    name: "vpyd:route-preload",
    apply: "build",
    enforce: "post",
    configResolved(config) {
      base = config.base;
    },
    generateBundle(_options, bundle) {
      const html = bundle["index.html"];
      if (html?.type !== "asset") this.error("route-preload: index.html is not in the bundle");
      const source = scriptSource(routeTable(bundle, routes, base));
      const hash = createHash("sha256").update(source).digest("base64url").slice(0, 8);
      const fileName = `${SCRIPT_PREFIX}${hash}.js`;
      this.emitFile({ type: "asset", fileName, source });
      const tag = `<script type="module" async crossorigin src="${base}${fileName}"></script>\n`;
      const htmlSource = String(html.source);
      const entryTag = htmlSource.search(/<script type="module"/);
      if (entryTag < 0) this.error("route-preload: no module script in index.html");
      // Before the entry: the browser starts it first and runs it as soon as it arrives.
      html.source = htmlSource.slice(0, entryTag) + tag + "    " + htmlSource.slice(entryTag);
    },
  };
}
