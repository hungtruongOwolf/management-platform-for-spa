// Lighthouse CI for pages behind a login (testing.md §4). Starts the built API on the E2E database
// and `vite preview` (proxying /api to it), signs in the manager the E2E setup provisioned (managers
// have no MFA), and runs `lhci autorun` with that session as a Cookie header. The cookie goes through
// a temporary config file, never argv; it belongs to a throwaway spa of this run.
//
// Run after `pnpm e2e` (needs e2e/.auth/run.json and apps/api/target/debug/app).
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseEnv } from "node:util";

const webRoot = fileURLToPath(new URL("../", import.meta.url));
const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));
const apiPort = Number(process.env["LHCI_API_PORT"] ?? 8093);
const webPort = 4173;
const webOrigin = `http://localhost:${String(webPort)}`;

function databaseUrl() {
  if (process.env["E2E_APP_DATABASE_URL"]) return process.env["E2E_APP_DATABASE_URL"];
  const envFile = join(repoRoot, ".env");
  const local = existsSync(envFile) ? parseEnv(readFileSync(envFile, "utf8")) : {};
  if (!local["APP_DATABASE_URL"]) throw new Error("E2E_APP_DATABASE_URL is not set and .env has no APP_DATABASE_URL");
  const url = new URL(local["APP_DATABASE_URL"]);
  url.pathname = `/${process.env["E2E_DATABASE"] ?? "vpyd_e2e"}`;
  return url.toString();
}

async function waitFor(url, ms) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`timed out waiting for ${url}`);
}

const run = JSON.parse(readFileSync(join(webRoot, "e2e", ".auth", "run.json"), "utf8"));
if (!run.managerPhone || !run.managerPassword) throw new Error("run.json has no manager: run `pnpm e2e` first");

const binary = join(
  process.env["CARGO_TARGET_DIR"] ?? join(repoRoot, "apps", "api", "target"),
  "debug",
  process.platform === "win32" ? "app.exe" : "app",
);
const children = [];
const tmp = mkdtempSync(join(tmpdir(), "vpyd-lhci-"));
let code;
try {
  children.push(
    spawn(binary, ["serve"], {
      cwd: repoRoot,
      stdio: ["ignore", "ignore", "inherit"],
      env: {
        ...process.env,
        APP_ENV: "development",
        APP_BIND_ADDR: `127.0.0.1:${String(apiPort)}`,
        APP_DATABASE_URL: databaseUrl(),
        APP_PUBLIC_ORIGIN: webOrigin,
        APP_DATA_KEY: randomBytes(32).toString("base64"),
        RUST_LOG: "warn",
      },
    }),
  );
  children.push(
    spawn("pnpm", ["exec", "vite", "preview", "--port", String(webPort), "--strictPort"], {
      cwd: webRoot,
      shell: process.platform === "win32",
      stdio: "ignore",
      env: { ...process.env, VPYD_API_ORIGIN: `http://127.0.0.1:${String(apiPort)}` },
    }),
  );
  await waitFor(`http://127.0.0.1:${String(apiPort)}/api/v1/health`, 60_000);
  await waitFor(`${webOrigin}/dang-nhap`, 60_000);

  const login = await fetch(`${webOrigin}/api/v1/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: webOrigin },
    body: JSON.stringify({ phone: run.managerPhone, password: run.managerPassword }),
  });
  const cookie = (login.headers.get("set-cookie") ?? "").split(";")[0];
  if (login.status !== 200 || !cookie) throw new Error(`manager login failed: ${String(login.status)}`);

  // Two passes, because extra headers apply to every URL of a run: the login page as an anonymous
  // visitor sees it, then the signed-in pages with the manager's cookie.
  const base = JSON.parse(readFileSync(join(webRoot, "lighthouserc.json"), "utf8"));
  const passes = [
    { name: "anonymous", urls: base.ci.collect.anonymousUrls, headers: undefined },
    { name: "signed-in", urls: base.ci.collect.signedInUrls, headers: { Cookie: cookie } },
  ];
  code = 0;
  for (const pass of passes) {
    const ci = structuredClone(base.ci);
    delete ci.collect.anonymousUrls;
    delete ci.collect.signedInUrls;
    ci.collect.url = pass.urls.map((path) => `${webOrigin}${path}`);
    if (pass.headers) ci.collect.settings = { ...ci.collect.settings, extraHeaders: pass.headers };
    const config = join(tmp, `lighthouserc-${pass.name}.json`);
    writeFileSync(config, JSON.stringify({ ci }), { mode: 0o600 });
    const status =
      spawnSync("pnpm", ["dlx", "@lhci/cli@0.15.1", "autorun", `--config=${config}`], {
        cwd: webRoot,
        shell: process.platform === "win32",
        stdio: "inherit",
      }).status ?? 1;
    if (status !== 0) code = status;
  }
} finally {
  for (const child of children) child.kill();
  rmSync(tmp, { recursive: true, force: true });
}
process.exit(code ?? 1);
