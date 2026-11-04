import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
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
  // Dev: the SPA and API share one origin through this proxy, mirroring production (Caddy).
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": { target: "http://127.0.0.1:8080", changeOrigin: false },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
});
