// Vite build plugins for the startup critical path (frontend.md "Perf budget").
//
// routePreload: starts the landing screen's own chunk next to the startup chunk.
// Some first screens (/quay, home) are lazy route chunks, so the startup chunk every screen pays
// stays small. Left alone, the browser would fetch such a screen's chunk only after the startup
// chunk has downloaded and run: one round trip more before its first paint. This plugin emits a
// tiny module script, placed in <head> before the entry, that looks at the URL and adds
// <link rel="modulepreload"> for that screen's chunk and the chunks it imports. Both downloads then
// run side by side. The CSP stays `script-src 'self'`: the script is a file, not inline code.
// `startup-budget.mjs` reads the same table back (ROUTE_TABLE_PATTERN) and gates every screen's
// critical path: startup + its own chunks.
//
// loadedDeps: a lazy import preloads the chunks it needs, but not those its importer already
// loaded (the importer's own static imports, e.g. the startup chunk): their entries were dead
// weight in every chunk's preload table.
import { createHash } from "node:crypto";

/** Matches the generated table in the emitted script; group 1 is its JSON. */
export const ROUTE_TABLE_PATTERN = /\/\*routes\*\/(\{.*?\})\/\*end\*\//s;

/** File name prefix of the emitted script, so tools can find it in index.html. */
export const SCRIPT_PREFIX = "assets/route-preload-";

/** Every file `fileName` needs before it runs: itself and its static imports, recursively. */
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
 * imports and their CSS, minus what the startup (entry + its imports) already loads. Names are
 * relative to `dir` (the script's own folder), where every one of them must be.
 */
export function routeTable(bundle, routes, dir = "assets/") {
  const entry = Object.values(bundle).find((c) => c.type === "chunk" && c.isEntry);
  if (!entry) throw new Error("route-preload: no entry chunk");
  const startup = staticClosure(bundle, entry.fileName);
  const table = {};
  const name = (file) => {
    if (!file.startsWith(dir)) throw new Error(`route-preload: ${file} is not in ${dir}`);
    return file.slice(dir.length);
  };
  for (const [path, moduleFile] of Object.entries(routes)) {
    const chunk = chunkOf(bundle, moduleFile);
    if (!chunk) throw new Error(`route-preload: no chunk holds ${moduleFile} (${path})`);
    if (startup.has(chunk.fileName)) throw new Error(`route-preload: ${moduleFile} is in the startup chunk`);
    const files = [];
    for (const file of staticClosure(bundle, chunk.fileName)) {
      if (startup.has(file)) continue;
      files.push(name(file));
      for (const css of bundle[file]?.viteMetadata?.importedCss ?? []) {
        if (!startup.has(css) && !files.includes(name(css))) files.push(name(css));
      }
    }
    table[path] = files;
  }
  return table;
}

/**
 * The emitted script: tiny, no imports. `href` is `url` + name (`url` = the script's folder as the
 * page sees it); `crossOrigin = ""` matches Vite's own preload links, so the browser reuses them.
 */
export function scriptSource(table, url) {
  return (
    `const r=/*routes*/${JSON.stringify(table)}/*end*/;\n` +
    `for(const f of r[location.pathname.replace(/(.)\\/+$/,"$1")]||[]){` +
    `const l=document.createElement("link"),c=f.endsWith(".css");` +
    `l.rel=c?"preload":"modulepreload";if(c)l.as="style";l.crossOrigin="";l.href=${JSON.stringify(url)}+f;document.head.append(l)}\n`
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
      const dir = SCRIPT_PREFIX.slice(0, SCRIPT_PREFIX.lastIndexOf("/") + 1);
      const source = scriptSource(routeTable(bundle, routes, dir), base + dir);
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

/**
 * `build.modulePreload.resolveDependencies` + the plugin that lets it see the bundle: a lazy
 * import's preload list without the files its importer already loaded.
 */
export function loadedDeps() {
  let bundle = null;
  return {
    plugin: {
      name: "vpyd:loaded-deps",
      apply: "build",
      // Before Vite writes the preload lists (its generateBundle), keep the bundle at hand.
      generateBundle: {
        order: "pre",
        handler(_options, b) {
          bundle = b;
        },
      },
    },
    resolveDependencies(filename, deps, { hostId, hostType }) {
      if (hostType !== "js" || !bundle?.[hostId]) return deps;
      const loaded = staticClosure(bundle, hostId);
      return deps.filter((dep) => dep === filename || !loaded.has(dep));
    },
  };
}
