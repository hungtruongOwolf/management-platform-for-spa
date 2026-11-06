// Local mirror of the CI checks that `cargo test` / lint don't cover, so they fail before a push:
//   0. contract files are present and parseable (an interrupted api:spec once committed an empty file)
//   1. contract drift: openapi.json and the generated TS client must match the code
//   2. cargo-deny (licenses, advisories, bans) — skipped with a hint when not installed
import { execSync, spawnSync } from "node:child_process";

const run = (cmd) => execSync(cmd, { stdio: "inherit" });

run("node scripts/contract-files.mjs");
run("pnpm -s api:gen");
const drift = spawnSync(
  "git",
  ["diff", "--exit-code", "--stat", "apps/api/openapi.json", "packages/api-client/src/schema.ts"],
  { stdio: "inherit" },
);
if (drift.status !== 0) {
  console.error("✗ contract drift: commit the regenerated openapi.json / schema.ts");
  process.exit(1);
}
console.log("✓ contract in sync");

const deny = spawnSync("cargo", ["deny", "--version"], { stdio: "ignore" });
if (deny.status !== 0) {
  console.warn("! cargo-deny not installed — skipped (CI runs it). Install: cargo install --locked cargo-deny");
} else {
  run("cargo deny --manifest-path apps/api/Cargo.toml check");
  console.log("✓ cargo-deny");
}
