/**
 * Where the E2E stack runs. Ports differ from `pnpm dev:*` so a dev session and an E2E run can
 * coexist; the database is dedicated (`vpyd_e2e`) so E2E data never mixes with dev data.
 *
 * Connection strings come from the environment (CI) or are derived from the repo's gitignored
 * `.env` by swapping the database name. Nothing secret is written here.
 */
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parseEnv } from "node:util";

export const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));
export const webRoot = fileURLToPath(new URL("../", import.meta.url));
export const runFile = fileURLToPath(
  new URL("./.auth/run.json", import.meta.url),
);
/** Browser storage (session cookie) of the signed-in owner, saved by the setup project. */
export const ownerState = fileURLToPath(
  new URL("./.auth/owner.json", import.meta.url),
);

const E2E_DATABASE = "vpyd_e2e";

function dotenv(): Record<string, string | undefined> {
  const path = `${repoRoot}.env`;
  return existsSync(path) ? parseEnv(readFileSync(path, "utf8")) : {};
}

function withDatabase(
  url: string | undefined,
  name: string,
): string | undefined {
  if (!url) return undefined;
  const parsed = new URL(url);
  parsed.pathname = `/${name}`;
  return parsed.toString();
}

function required(name: string, value: string | undefined): string {
  if (!value) {
    throw new Error(
      `${name} is not set and cannot be derived from .env — see memory/wiki/conventions/testing.md`,
    );
  }
  return value;
}

const local = dotenv();
const apiPort = Number(process.env["E2E_API_PORT"] ?? 8091);
const webPort = Number(process.env["E2E_WEB_PORT"] ?? 5181);

export const stack = {
  apiPort,
  webPort,
  apiOrigin: `http://127.0.0.1:${String(apiPort)}`,
  webOrigin: `http://localhost:${String(webPort)}`,
  /** Schema owner: migrations and provisioning CLI. */
  ownerDatabaseUrl: () =>
    required(
      "E2E_DATABASE_URL",
      process.env["E2E_DATABASE_URL"] ??
        withDatabase(local["DATABASE_URL"], E2E_DATABASE),
    ),
  /** Runtime role app_rw: the API server. */
  appDatabaseUrl: () =>
    required(
      "E2E_APP_DATABASE_URL",
      process.env["E2E_APP_DATABASE_URL"] ??
        withDatabase(local["APP_DATABASE_URL"], E2E_DATABASE),
    ),
  /** Field-encryption key for this run only: every run provisions a fresh spa, so nothing outlives it. */
  dataKey: () =>
    process.env["E2E_DATA_KEY"] ?? randomBytes(32).toString("base64"),
};
