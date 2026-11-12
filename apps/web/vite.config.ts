import { fileURLToPath } from "node:url";
import { configDefaults, defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { routePreload } from "./scripts/route-preload.mjs";

/**
 * First screens that are lazy chunks (src/app/screens.ts): opening the app on one of these paths
 * fetches the screen's chunk next to the startup chunk (scripts/route-preload.mjs). A path missing
 * here still works, one round trip later; `pnpm budget` gates each listed path's critical path.
 */
export const ROUTE_PRELOAD = {
  "/quay": "/src/features/front-desk/FrontDeskPage.tsx",
  "/": "/src/app/HomePage.tsx",
};

const uiSrc = fileURLToPath(new URL("../../packages/ui/src", import.meta.url));
const apiClientSrc = fileURLToPath(new URL("../../packages/api-client/src", import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss(), routePreload(ROUTE_PRELOAD)],
  resolve: {
    // Workspace packages are aliased rather than linked: the repo lives on an exFAT drive without symlinks.
    alias: [
      { find: /^@vpyd\/ui\/tokens\.css$/, replacement: `${uiSrc}/tokens/tokens.css` },
      { find: /^@vpyd\/ui$/, replacement: `${uiSrc}/index.ts` },
      { find: /^@vpyd\/api-client$/, replacement: `${apiClientSrc}/index.ts` },
      // @tanstack/react-store imports the React 17 shim of useSyncExternalStore; React 19 has the
      // hook built in (the shim only hands it over), so the shim's fallback stays out of the
      // startup chunk (frontend.md "Perf budget").
      {
        find: /^use-sync-external-store\/shim\/with-selector(?:\.js)?$/,
        replacement: "use-sync-external-store/with-selector.js",
      },
    ],
  },
  build: {
    // Every browser the spa uses honours <link rel="modulepreload"> natively (Safari 17+); an older
    // one still loads the module through its import, just without the head start. Dropping the
    // polyfill takes 0.25 KiB off the startup JS (frontend.md "Perf budget", ADR-0019 D21).
    modulePreload: { polyfill: false },
    rolldownOptions: {
      output: {
        // Everything the first screen needs in one chunk: left alone, the bundler splits modules
        // shared with lazy screens into ~10 small preloaded chunks, and each one gzips on its own
        // (≈ +2 KB on the startup budget, frontend.md "Perf budget"). Lazy screens stay separate.
        codeSplitting: {
          groups: [
            { name: "app", tags: ["$initial"] },
            {
              name: "FrontDeskPage",
              test: /src[\\/](features[\\/](front-desk[\\/](FrontDeskPage|api|Tip)|appointments[\\/]complete|customers[\\/](CustomerSearch|api)|sales[\\/]api)|lib[\\/]invalidate|app[\\/]CustomerName)\.tsx?$/,
              includeDependenciesRecursively: false,
            },
            {
              name: "common",
              test: /(react-query[\\/]build[\\/]modern[\\/](useQuery|useMutation)|query-core[\\/]build[\\/]modern[\\/]mutationObserver)\.js$|packages[\\/]ui[\\/]src[\\/]format[\\/]money\.ts$|src[\\/]lib[\\/]errors\.ts$/,
              includeDependenciesRecursively: false,
            },
          ],
        },
      },
    },
  },
  // Dev: the SPA and API share one origin through this proxy, mirroring production (Caddy).
  // VPYD_API_ORIGIN lets the E2E stack run its own API on another port.
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": { target: process.env["VPYD_API_ORIGIN"] ?? "http://127.0.0.1:8080", changeOrigin: false },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    // Playwright specs live in e2e/ and run with `pnpm e2e`, not Vitest.
    exclude: [...configDefaults.exclude, "e2e/**"],
  },
});
