# Latency scripts

Re-run the measurements of `memory/wiki/architecture/performance-latency-2026-11.md`. Node ≥ 22, no dependencies.

| Script | Measures | Credentials |
|---|---|---|
| `endpoints.mjs [samples]` | Signed-in p50/p95 per endpoint + the API's `Server-Timing` (server total, db time, SQL statements per request) | demo manager password |
| `gaps.mjs [seconds…]` | First request after an idle gap (cold start) | none |
| `globalping.mjs <http\|ping> <host> [path]` | Real probes in Vietnam via Globalping | none (public endpoints only, never cookies) |

Credentials are read at run time from `.env.staging` at the repo root (gitignored), or the file named by `PERF_ENV_FILE`, or the environment: `STAGING_MANAGER_LOGIN_PASSWORD`, optional `PERF_PHONE` (default demo manager `0905000002`) and `PERF_ORIGIN` (default the staging URL). Nothing is printed or passed on argv, and the session is logged out at the end. Use a manager: owners need TOTP.

Against a local stack: `PERF_ORIGIN=http://localhost:5173` does not work with HTTP/2 (Vite is h1); point the scripts at a deployed origin.

The API reports its own time in every response (except `/auth/*`):
`Server-Timing: db;dur=0.72;desc="14 statements", total;dur=3.10`. DevTools → Network → Timing shows it too.
