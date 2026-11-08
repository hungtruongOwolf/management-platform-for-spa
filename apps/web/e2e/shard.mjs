// Splits the desktop e2e specs into CI shards by measured duration instead of Playwright's
// `--shard`, which cuts by test count at file granularity (layout.spec.ts alone is ~110 s of the
// ~200 s suite, so `--shard=1/2` left one shard at 2.7 min and the other at 36 s).
//
//   node e2e/shard.mjs <index> <total>   → prints the spec paths of shard <index> (1-based)
//
// Every file in e2e/specs/ lands in exactly one shard; a spec missing from WEIGHTS still runs, with
// the median weight. Weights: seconds per file on CI (run 42189960683); refresh when a spec grows.
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const WEIGHTS = {
  "a11y.spec.ts": 5,
  "appointments.spec.ts": 16,
  "auth.spec.ts": 15,
  "customers.spec.ts": 4,
  "front-desk.spec.ts": 3,
  "head.spec.ts": 1,
  "layout.spec.ts": 110,
  "management.spec.ts": 12,
  "notes.spec.ts": 6,
  "onboarding.spec.ts": 6,
  "quay-upsell.spec.ts": 8,
  "sales.spec.ts": 3,
  "staff.spec.ts": 7,
  "technician.spec.ts": 8,
  "voids.spec.ts": 12,
};

export function shards(files, total) {
  const known = Object.values(WEIGHTS).sort((a, b) => a - b);
  const median = known[Math.floor(known.length / 2)] ?? 1;
  const weight = (file) => WEIGHTS[file] ?? median;
  // Longest first onto the lightest shard (LPT); ties broken by name so the split is stable.
  const sorted = [...files].sort((a, b) => weight(b) - weight(a) || a.localeCompare(b));
  const buckets = Array.from({ length: total }, () => ({ load: 0, files: [] }));
  for (const file of sorted) {
    const lightest = buckets.reduce((min, b) => (b.load < min.load ? b : min));
    lightest.files.push(file);
    lightest.load += weight(file);
  }
  return buckets.map((b) => b.files.sort());
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const index = Number(process.argv[2]);
  const total = Number(process.argv[3]);
  if (!Number.isInteger(index) || !Number.isInteger(total) || index < 1 || index > total) {
    console.error("usage: node e2e/shard.mjs <index> <total>");
    process.exit(2);
  }
  const dir = fileURLToPath(new URL("./specs/", import.meta.url));
  const files = readdirSync(dir).filter((f) => f.endsWith(".spec.ts"));
  const split = shards(files, total);
  if (split.flat().length !== files.length) throw new Error("shard split lost a spec");
  const mine = split[index - 1] ?? [];
  if (mine.length === 0) throw new Error(`shard ${String(index)} is empty: lower the shard count`);
  console.log(mine.map((f) => `e2e/specs/${f}`).join(" "));
}
