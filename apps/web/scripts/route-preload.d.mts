import type { Plugin } from "vite";

/** Matches the generated table in the emitted script; group 1 is its JSON. */
export const ROUTE_TABLE_PATTERN: RegExp;
/** File name prefix of the emitted script, so tools can find it in index.html. */
export const SCRIPT_PREFIX: string;
/** `routes`: path → the module of its screen (path ending), e.g. { "/quay": "/src/features/front-desk/FrontDeskPage.tsx" }. */
export function routePreload(routes: Readonly<Record<string, string>>): Plugin;
/** `build.modulePreload.resolveDependencies` without the files a lazy import's importer already loaded. */
export function loadedDeps(): {
  plugin: Plugin;
  resolveDependencies: (
    filename: string,
    deps: string[],
    context: { hostId: string; hostType: "html" | "js" },
  ) => string[];
};
