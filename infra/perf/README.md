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

## Database review scripts (`db/`)

Evidence for `memory/wiki/architecture/db-review-2026-11.md`. Docker + psql in the `postgres:17-alpine` image (the staging major), Python ≥ 3.10 for the generator. No cargo build needed.

| Script | Does | Where |
|---|---|---|
| `db/staging-readonly.sh <file.sql>` | Runs one SQL file inside `BEGIN READ ONLY … ROLLBACK` against staging. `STAGING_OWNER_URL` is read from `.env.staging` (or `PERF_ENV_FILE`) into the environment and passed with `docker run -e PGURL`: never on argv, never printed | staging, read-only |
| `db/staging-stats.sql` | Server settings, table/index sizes, scans, writes, HOT updates | any |
| `db/staging-statements.sql` | `pg_stat_statements` of `app_rw`: most called, most time, most buffers | staging |
| `db/staging-payloads.sql` | Sizes only (audit per action, idempotency bodies, sessions, ciphertext, row width) | staging |
| `db/staging-ledger-explain.sql` | Plan of the month ledger (SELECT only) | staging |
| `db/invariants.sql` | 11 checks of the copies the schema stores on purpose; zero rows = clean | any (schema owner) |
| `db/seed-busy-spa.sql` | Three years of a busy spa into a **throwaway** database (~400 MB, a few minutes). `-v days=… -v customers=…` | local only |
| `db/explain_hot.py` | Writes a psql script that `EXPLAIN (ANALYZE, BUFFERS)`s 40 hot statements verbatim from `apps/api/.sqlx`, prepared, as `app_rw` with the RLS tenant set and staging's planner settings | local only |
| `db/experiments.sql` | The comparisons behind the findings (trigram owner vs `app_rw`, history rewrite, generic plans, one-statement auth draft, index drops in a rolled-back transaction) | local only |

```sh
# Local: throwaway copy, migrate, seed, measure, drop.
docker exec vpyd-dev-postgres-1 psql -U vpyd -d postgres -c "CREATE DATABASE vpyd_dbreview"
for f in apps/api/migrations/*.sql; do docker exec -i vpyd-dev-postgres-1 psql -q -U vpyd -d vpyd_dbreview -v ON_ERROR_STOP=1 -1 < "$f"; done
docker exec -i vpyd-dev-postgres-1 psql -U vpyd -d vpyd_dbreview -X < infra/perf/db/seed-busy-spa.sql
python -I infra/perf/db/explain_hot.py > /tmp/explain.sql
docker exec -i vpyd-dev-postgres-1 psql -U vpyd -d vpyd_dbreview -X < /tmp/explain.sql
docker exec -i vpyd-dev-postgres-1 psql -U vpyd -d vpyd_dbreview -X < infra/perf/db/experiments.sql
docker exec vpyd-dev-postgres-1 psql -U vpyd -d postgres -c "DROP DATABASE vpyd_dbreview"

# Staging, read-only.
PERF_ENV_FILE=/path/to/.env.staging bash infra/perf/db/staging-readonly.sh infra/perf/db/staging-stats.sql
```

The seed keeps every constraint and trigger, but `sessions_remaining` is synthetic: invariant 5 reports rows on a seeded database, by design. A case of `explain_hot.py` prints a NOTE instead of a plan when its statement changed (new sqlx hash).
