---
name: perf
description: Performance — use when something is slow, when setting performance budgets, or when optimizing API latency, database queries, page load, bundle size, animation or 3D frame rate.
---

# Perf

Measure first. A fix without a before/after number is a guess.

## Default budgets (override per spec)
- API: p95 < 200 ms, p99 < 500 ms for reads at expected load; no endpoint without a timeout.
- Web (mobile, 4G, mid-range device): LCP < 2.5 s, INP < 200 ms, CLS < 0.1; initial JS < 200 KB gzip on content pages.
- 3D scenes: 60 fps target / 30 fps floor on a mid-range phone; < 100 draw calls; textures KTX2/Basis, geometry Draco or meshopt.

## Steps
1. Define the budget and the scenario (endpoint + load, or page + device profile).
2. Measure the baseline:
   - Backend: `k6` load, `tracing` spans, `cargo flamegraph`, Postgres `EXPLAIN (ANALYZE, BUFFERS)`, `pg_stat_statements`.
   - Frontend: Lighthouse, Chrome Performance panel, React Profiler, bundle analyzer.
   - 3D: `r3f-perf` / `stats.js`, renderer info (draw calls, triangles, textures).
3. Identify the single largest contributor; fix it. Typical wins: index or query rewrite, N+1 removal, caching (with explicit invalidation), pagination, streaming SSR, code-splitting, image formats (AVIF/WebP, responsive sizes), font subsetting (Vietnamese subset), instancing, `frameloop="demand"`, clamped `dpr`, pausing off-screen canvases.
4. Re-measure with the same scenario; keep the change only if it moves the number.
5. Done when the scenario meets its budget, with before/after numbers recorded.
6. Record numbers and changes via `memory`.
