/**
 * Runs once per `pnpm e2e`: migrates the E2E database and provisions a fresh spa through the same
 * CLI an operator uses (create-tenant, add-user, seed-catalog). Credentials are generated here,
 * passed to the CLI through the environment (never argv), and stored in the gitignored
 * e2e/.auth/run.json for the specs.
 */
import { expect, test as setup } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { randomBytes, randomInt } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { RunCredentials } from "../support/run";
import { signIn } from "../support/run";
import { ownerState, repoRoot, runFile, stack } from "../stack";

/**
 * The binary the `api` web server just built. Not `cargo run`: a different DATABASE_URL would make
 * cargo rebuild (sqlx macros track it) and Windows cannot replace the running app.exe.
 */
const appBinary = join(
  process.env["CARGO_TARGET_DIR"] ?? join(repoRoot, "apps", "api", "target"),
  "debug",
  process.platform === "win32" ? "app.exe" : "app",
);

function cli(args: string[], extraEnv: Record<string, string> = {}): void {
  execFileSync(appBinary, args, {
    cwd: repoRoot,
    env: {
      ...process.env,
      DATABASE_URL: stack.ownerDatabaseUrl(),
      RUST_LOG: "warn",
      ...extraEnv,
    },
    stdio: ["ignore", "ignore", "inherit"],
  });
}

setup("migrate and provision a fresh spa", () => {
  setup.setTimeout(10 * 60_000);
  const run: RunCredentials = {
    slug: `e2e-${Date.now().toString(36)}`,
    // A valid Vietnamese mobile number that no earlier run used.
    phone: `090${String(randomInt(0, 10_000_000)).padStart(7, "0")}`,
    password: randomBytes(18).toString("base64url"),
    fullName: "Chủ Spa E2E",
  };

  cli(["migrate"]);
  cli([
    "create-tenant",
    "--slug",
    run.slug,
    "--name",
    "Spa E2E",
    "--branch",
    "Cơ sở E2E",
  ]);
  cli(
    [
      "add-user",
      "--tenant",
      run.slug,
      "--phone",
      run.phone,
      "--name",
      run.fullName,
      "--role",
      "owner",
    ],
    {
      APP_NEW_USER_PASSWORD: run.password,
    },
  );
  cli(["seed-catalog", "--tenant", run.slug]);
  // Closer for the sell-package flow (sales.spec.ts).
  cli([
    "add-staff",
    "--tenant",
    run.slug,
    "--name",
    "KTV E2E",
    "--position",
    "Kỹ thuật viên",
  ]);

  mkdirSync(dirname(runFile), { recursive: true });
  writeFileSync(runFile, JSON.stringify(run, null, 2));
});

setup("sign the owner in once for the specs", async ({ page }) => {
  await signIn(page);
  await expect(page.getByRole("button", { name: "Khóa nhanh" })).toBeVisible();
  await page.context().storageState({ path: ownerState });
});
