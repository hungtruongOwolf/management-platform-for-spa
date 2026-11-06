#!/usr/bin/env bash
# Tests for scripts/ci-changes.sh (which CI jobs a change needs). Pure bash: `bash scripts/ci-changes.test.sh`.
set -uo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/ci-changes.sh"

fails=0 checks=0
eq() { # <label> <expected> <actual>
  checks=$((checks + 1))
  if [ "$2" != "$3" ]; then fails=$((fails + 1)); printf 'FAIL %s\n  expected: %s\n  actual:   %s\n' "$1" "$2" "$3"; fi
}
scope() { ci_scope "$(printf '%s\n' "$@")"; }

eq "empty list runs everything" full "$(ci_scope "")"

if declare -F gate_classify >/dev/null; then
  eq "docs only"                docs "$(scope memory/log.md memory/wiki/conventions/testing.md CLAUDE.md)"
  eq "md inside the web app"    docs "$(scope apps/web/README.md)"
  eq "web feature"              web  "$(scope apps/web/src/features/sales/SellSheet.tsx memory/log.md)"
  eq "web test"                 web  "$(scope apps/web/src/lib/afterFirstPaint.test.ts)"
  eq "e2e spec"                 web  "$(scope apps/web/e2e/specs/voids.spec.ts)"
  eq "shared web config"        web  "$(scope apps/web/vite.config.ts)"
  eq "ui package"               web  "$(scope packages/ui/src/Button.tsx)"
  eq "generated TS client"      web  "$(scope packages/api-client/src/schema.ts)"
  eq "root lockfile"            web  "$(scope pnpm-lock.yaml)"
  eq "rust crate"               full "$(scope apps/api/crates/sales/src/paper.rs)"
  eq "migration"                full "$(scope apps/api/migrations/0019_calendar_timeline.sql)"
  eq "sqlx offline data"        full "$(scope apps/api/.sqlx/query-abc.json)"
  eq "openapi spec"             full "$(scope apps/api/openapi.json)"
  eq "cargo-deny config"        full "$(scope apps/api/deny.toml)"
  eq "workflow"                 full "$(scope .github/workflows/ci.yml)"
  eq "api Dockerfile"           full "$(scope apps/api/Dockerfile)"
  eq "dev db roles"             full "$(scope infra/postgres-init/01-roles.sql)"
  eq "gate scripts"             full "$(scope scripts/gate-lib.sh)"
  eq "this script"              full "$(scope scripts/ci-changes.sh)"
  eq "unknown root file"        full "$(scope some-new-tool.toml)"
  eq "web plus rust is full"    full "$(scope apps/web/src/features/sales/SellSheet.tsx apps/api/crates/sales/src/paper.rs)"
else
  eq "no gate-lib: docs run everything" full "$(scope memory/log.md)"
  eq "no gate-lib: web runs everything" full "$(scope apps/web/src/main.tsx)"
fi

echo "ci-changes tests: $((checks - fails))/$checks passed"
[ "$fails" = 0 ]
