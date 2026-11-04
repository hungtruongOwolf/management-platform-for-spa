import { defineConfig, devices } from "@playwright/test";
import { repoRoot, stack, webRoot } from "./e2e/stack";

const ci = Boolean(process.env["CI"]);

/**
 * E2E against the real stack: Rust API (`cargo run -p app`) on a dedicated database + Vite dev
 * server proxying /api to it. The `setup` project migrates and provisions a fresh spa per run.
 * Guide: memory/wiki/conventions/testing.md.
 */
export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results",
  // One worker: specs share one spa and the per-IP auth rate limit (10 logins burst).
  workers: 1,
  fullyParallel: false,
  forbidOnly: ci,
  retries: ci ? 1 : 0,
  timeout: 30_000,
  expect: { timeout: 10_000 },
  reporter: ci
    ? [
        ["github"],
        ["html", { open: "never", outputFolder: "playwright-report" }],
      ]
    : [
        ["list"],
        ["html", { open: "never", outputFolder: "playwright-report" }],
      ],
  use: {
    baseURL: stack.webOrigin,
    locale: "vi-VN",
    timezoneId: "Asia/Ho_Chi_Minh",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    { name: "setup", testMatch: /setup\/.*\.setup\.ts/ },
    {
      name: "desktop",
      testMatch: /specs\/.*\.spec\.ts/,
      dependencies: ["setup"],
      use: { ...devices["Desktop Chrome"] },
    },
    {
      name: "perf",
      testMatch: /perf\/.*\.spec\.ts/,
      dependencies: ["setup"],
    },
  ],
  webServer: [
    {
      name: "api",
      command: "cargo run --manifest-path apps/api/Cargo.toml -p app -- serve",
      cwd: repoRoot,
      url: `${stack.apiOrigin}/api/v1/health`,
      // A cold `cargo build` takes minutes; later runs start in seconds.
      timeout: 15 * 60_000,
      reuseExistingServer: !ci,
      stdout: "ignore",
      stderr: "pipe",
      env: {
        APP_ENV: "development",
        APP_BIND_ADDR: `127.0.0.1:${String(stack.apiPort)}`,
        APP_DATABASE_URL: stack.appDatabaseUrl(),
        APP_PUBLIC_ORIGIN: stack.webOrigin,
        APP_DATA_KEY: stack.dataKey(),
        RUST_LOG: process.env["RUST_LOG"] ?? "warn",
      },
    },
    {
      name: "web",
      command: `pnpm exec vite --port ${String(stack.webPort)} --strictPort`,
      cwd: webRoot,
      url: stack.webOrigin,
      timeout: 120_000,
      reuseExistingServer: !ci,
      env: { VPYD_API_ORIGIN: stack.apiOrigin },
    },
  ],
});
