// `pnpm api:spec`: writes apps/api/openapi.json from the code without ever leaving it empty.
// `cargo run … > openapi.json` truncated the file before cargo finished; an interrupted run left an
// empty file that got committed (PRs #44, #45). Here the output is captured, validated, written to a
// temp file next to the target and renamed into place only when it is a real OpenAPI document.
//
// API_SPEC_BIN=<path to a built app binary> skips `cargo run` (the merge gate reuses the binary its
// `cargo test` just built instead of compiling a second feature set).
import { spawnSync } from "node:child_process";
import { renameSync, rmSync, writeFileSync } from "node:fs";
import { OPENAPI_PATH, openApiProblem } from "./contract-files.mjs";

const bin = process.env["API_SPEC_BIN"];
const [command, args] = bin
  ? [bin, ["openapi"]]
  : ["cargo", ["run", "-q", "--manifest-path", "apps/api/Cargo.toml", "-p", "app", "--", "openapi"]];

const result = spawnSync(command, args, {
  stdio: ["ignore", "pipe", "inherit"],
  maxBuffer: 64 * 1024 * 1024,
});
if (result.error) {
  console.error(`✗ api:spec: could not run ${command}: ${result.error.message}`);
  process.exit(1);
}
if (result.status !== 0) {
  console.error(`✗ api:spec: ${command} exited with ${String(result.status ?? result.signal)}; ${OPENAPI_PATH} left unchanged`);
  process.exit(1);
}
const problem = openApiProblem(result.stdout.toString("utf8"));
if (problem) {
  console.error(`✗ api:spec: generated document rejected: ${problem}; ${OPENAPI_PATH} left unchanged`);
  process.exit(1);
}

const temp = `${OPENAPI_PATH}.tmp-${String(process.pid)}`;
try {
  writeFileSync(temp, result.stdout);
  renameSync(temp, OPENAPI_PATH);
} catch (error) {
  rmSync(temp, { force: true });
  throw error;
}
