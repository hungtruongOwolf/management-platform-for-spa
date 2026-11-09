// Splits the desktop e2e specs into CI shards by measured duration instead of Playwright's
// `--shard`, which cuts by test count at file granularity (layout.spec.ts alone is ~160 s of the
// ~390 s suite).
//
//   node e2e/shard.mjs <index> <total>   → prints the spec paths of shard <index> (1-based)
//
// Every file in e2e/specs/ lands in exactly one shard; a spec missing from WEIGHTS still runs, with
// the median weight. Weights: seconds per file on CI (sum of the list reporter's test durations,
// run 42606122131, 2026-11-09); refresh when a spec grows or the shards drift apart.
//
// Specs of one shard share that shard's spa, so a spec sees what the specs before it created.
// ALONE specs get a shard to themselves: layout.spec.ts sweeps every screen at many widths and is
// written against an almost empty spa (next to front-desk/sales data it flagged a package name
// clipped on /quay at 1040 px, PR #118).
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const WEIGHTS = {
  "a11y.spec.ts": 7,
  "admin.spec.ts": 7,
  "appointments.spec.ts": 44,
  "auth.spec.ts": 31,
  "customers.spec.ts": 7,
  "front-desk.spec.ts": 6,
  "head.spec.ts": 1,
  "layout.spec.ts": 160,
  "management.spec.ts": 16,
  "notes.spec.ts": 7,
  "onboarding.spec.ts": 6,
  "quay-upsell.spec.ts": 31,
  "sales.spec.ts": 4,
  "staff.spec.ts": 30,
  "technician.spec.ts": 11,
  "voids.spec.ts": 22,
};

const ALONE = new Set(["layout.spec.ts"]);

export function shards(files, total) {
  const known = Object.values(WEIGHTS).sort((a, b) => a - b);
  const median = known[Math.floor(known.length / 2)] ?? 1;
  const weight = (file) => WEIGHTS[file] ?? median;
  // Longest first onto the lightest shard (LPT); ties broken by name so the split is stable.
  const sorted = [...files].sort((a, b) => weight(b) - weight(a) || a.localeCompare(b));
  const alone = sorted.filter((file) => ALONE.has(file));
  if (alone.length >= total) throw new Error(`${alone.join(", ")} run alone: use more than ${String(alone.length)} shards`);
  const buckets = alone.map((file) => ({ load: weight(file), files: [file], closed: true }));
  while (buckets.length < total) buckets.push({ load: 0, files: [], closed: false });
  for (const file of sorted.filter((f) => !ALONE.has(f))) {
    const lightest = buckets.filter((b) => !b.closed).reduce((min, b) => (b.load < min.load ? b : min));
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
