// First request after an idle gap (perf page §2e): catches cold starts and rebuilt edge paths.
// Public endpoint only, no credentials.  node infra/perf/gaps.mjs [gapsInSeconds…]
import { loadEnv, origin, sleep } from "./lib.mjs";

const url = `${origin(loadEnv())}/api/v1/health`;
const gaps = process.argv.slice(2).map(Number).filter((n) => n > 0);
const once = async () => {
  const t = performance.now();
  const r = await fetch(url);
  await r.text();
  return Math.round(performance.now() - t);
};
await once();
await once();
for (const gap of gaps.length ? gaps : [2, 5, 10, 20, 30, 45, 60, 90, 120]) {
  await sleep(gap * 1000);
  const first = await once();
  const next = await once();
  console.log(`${new Date().toISOString()} gap ${gap}s: first ${first} ms, next ${next} ms`);
}
