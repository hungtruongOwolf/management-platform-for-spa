import { defineConfig, devices } from "@playwright/test";
import { repoRoot, stack, webRoot } from "./e2e/stack";

const ci = Boolean(process.env["CI"]);
// A prebuilt API binary (the local merge gate passes the one its `cargo test` just built): no
// second `cargo run` build in another feature set.
const apiBin = process.env["E2E_API_BIN"];

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
      command: apiBin
        ? `${JSON.stringify(apiBin)} serve`
        : "cargo run --manifest-path apps/api/Cargo.toml -p app -- serve",
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
        // One browser on 127.0.0.1 drives the whole suite: the production per-IP limit (5 rps,
        // burst 60) made /me answer 429 mid-run. A test stack limit, accepted by the API only with
        // APP_ENV=development (config.rs); /auth/* keeps its own strict limit.
        APP_RATE_PER_SECOND: "200",
        APP_RATE_BURST: "2000",
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
