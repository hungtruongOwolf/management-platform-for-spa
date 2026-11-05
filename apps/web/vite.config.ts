import { fileURLToPath } from "node:url";
import { configDefaults, defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const uiSrc = fileURLToPath(new URL("../../packages/ui/src", import.meta.url));
const apiClientSrc = fileURLToPath(new URL("../../packages/api-client/src", import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    // Workspace packages are aliased rather than linked: the repo lives on an exFAT drive without symlinks.
    alias: [
      { find: /^@vpyd\/ui\/tokens\.css$/, replacement: `${uiSrc}/tokens/tokens.css` },
      { find: /^@vpyd\/ui$/, replacement: `${uiSrc}/index.ts` },
      { find: /^@vpyd\/api-client$/, replacement: `${apiClientSrc}/index.ts` },
    ],
  },
  build: {
    rolldownOptions: {
      output: {
        // Everything the first screen needs in one chunk: left alone, the bundler splits modules
        // shared with lazy screens into ~10 small preloaded chunks, and each one gzips on its own
        // (≈ +2 KB on the startup budget, frontend.md "Perf budget"). Lazy screens stay separate.
        codeSplitting: { groups: [{ name: "app", tags: ["$initial"] }] },
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
