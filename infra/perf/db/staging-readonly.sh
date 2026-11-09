#!/usr/bin/env bash
# Runs one SQL file against the staging database inside a READ ONLY transaction that is rolled
# back. Catalog/stat queries and EXPLAIN of SELECTs only: never DDL, writes, VACUUM or ANALYZE.
#
#   bash infra/perf/db/staging-readonly.sh infra/perf/db/staging-stats.sql
#
# The connection string is read from STAGING_OWNER_URL in the env file (PERF_ENV_FILE, default
# `.env.staging` at the repo root) into the environment and handed to the container with
# `-e PGURL`: it never appears in argv, in output or in git.
set -euo pipefail

sql_file="${1:?usage: staging-readonly.sh <file.sql>}"
env_file="${PERF_ENV_FILE:-$(git rev-parse --show-toplevel)/.env.staging}"

PGURL="$({ grep -E '^STAGING_OWNER_URL=' "$env_file" || true; } | head -n 1 | cut -d= -f2- | sed -E "s/^[\"']//; s/[\"']\$//")"
if [ -z "$PGURL" ]; then
  echo "STAGING_OWNER_URL missing in $env_file" >&2
  exit 1
fi
export PGURL

docker run --rm -i -e PGURL postgres:16-alpine sh -c \
  'psql "$PGURL" -X -q -v ON_ERROR_STOP=1 -P pager=off \
     -c "SET default_transaction_read_only = on" -c "BEGIN READ ONLY" -f /dev/stdin -c "ROLLBACK"' \
  < "$sql_file"
