#!/usr/bin/env bash
# Tests for scripts/gate-lib.sh (classification + plan). No git, no build: `bash scripts/gate-lib.test.sh`.
set -uo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/gate-lib.sh"

fails=0 checks=0
eq() { # <label> <expected> <actual>
  checks=$((checks + 1))
  if [ "$2" != "$3" ]; then fails=$((fails + 1)); printf 'FAIL %s\n  expected: %s\n  actual:   %s\n' "$1" "$2" "$3"; fi
}

# The real workspace graph (path deps incl. dev-deps), as merge-local-gate.sh builds it.
GRAPH='app catalog crm hr identity kernel operations org packages reporting sales
catalog identity kernel
crm identity kernel
hr identity kernel org
identity kernel
kernel
operations catalog crm identity kernel org packages sales
org identity kernel
packages catalog crm identity kernel
reporting crm identity kernel packages
sales catalog crm identity kernel org packages'

plan() { gate_plan "$GRAPH" "$1" "${2:-auto}" "${3:-}"; }
summary() { echo "rust=$PLAN_RUST contract=$PLAN_CONTRACT tc=$PLAN_TYPECHECK vitest=$PLAN_VITEST ui=$PLAN_UI_TEST e2e=$PLAN_E2E specs=$PLAN_E2E_SPECS full=$PLAN_FULL"; }

# --- classification
eq "md anywhere is docs"        docs        "$(gate_classify packages/api-client/README.md)"
eq "memory is docs"             docs        "$(gate_classify memory/wiki/overview.md)"
eq "raw note is docs"           docs        "$(gate_classify memory/raw/x.txt)"
eq "kernel is all"              rust:all    "$(gate_classify apps/api/crates/kernel/src/lib.rs)"
eq "app is all"                 rust:all    "$(gate_classify apps/api/crates/app/tests/staff.rs)"
eq "crate"                      rust:sales  "$(gate_classify apps/api/crates/sales/src/paper.rs)"
eq "migration"                  rust:all    "$(gate_classify apps/api/migrations/0018_paper_book_sales.sql)"
eq "lockfile also deny"         "rust:all deny" "$(gate_classify apps/api/Cargo.lock)"
eq "sqlx"                       sqlx        "$(gate_classify apps/api/.sqlx/query-abc.json)"
eq "openapi"                    spec        "$(gate_classify apps/api/openapi.json)"
eq "unknown api file"           rust:all    "$(gate_classify apps/api/certs/ca.pem)"
eq "api client"                 client      "$(gate_classify packages/api-client/src/schema.ts)"
eq "ui kit is shared"           shared      "$(gate_classify packages/ui/src/tokens/tokens.css)"
eq "feature source"             web:staff   "$(gate_classify apps/web/src/features/staff/PayrollTab.tsx)"
eq "feature test"               webtest     "$(gate_classify apps/web/src/features/staff/Staff.test.tsx)"
eq "shell test is a test"       webtest     "$(gate_classify apps/web/src/app/icons.test.ts)"
eq "component test is a test"   webtest     "$(gate_classify apps/web/src/components/RowMenu.test.tsx)"
eq "test setup is shared"       shared      "$(gate_classify apps/web/src/test/setup.ts)"
eq "public asset"               web:_static "$(gate_classify apps/web/public/favicon.svg)"
eq "html entry"                 web:_static "$(gate_classify apps/web/index.html)"
eq "e2e spec"                   e2e:staff   "$(gate_classify apps/web/e2e/specs/staff.spec.ts)"
eq "e2e support is shared"      shared      "$(gate_classify apps/web/e2e/support/run.ts)"
eq "e2e setup is shared"        shared      "$(gate_classify apps/web/e2e/setup/provision.setup.ts)"
eq "components are shared"      shared      "$(gate_classify apps/web/src/components/ResponsiveTable.tsx)"
eq "shell is shared"            shared      "$(gate_classify apps/web/src/app/AppShell.tsx)"
eq "lib is shared"              shared      "$(gate_classify apps/web/src/lib/format.ts)"
eq "css is shared"              shared      "$(gate_classify apps/web/src/styles.css)"
eq "playwright config"          shared      "$(gate_classify apps/web/playwright.config.ts)"
eq "root lockfile"              shared      "$(gate_classify pnpm-lock.yaml)"
eq "workflow is infra"          infra       "$(gate_classify .github/workflows/ci.yml)"
eq "dockerfile is infra"        infra       "$(gate_classify apps/api/Dockerfile)"
eq "db roles are full"          full        "$(gate_classify infra/postgres-init/01-roles.sql)"
eq "gate script"                gate        "$(gate_classify scripts/merge-local-gate.sh)"
eq "spec writer"                spec        "$(gate_classify scripts/api-spec.mjs)"
eq "unknown root file"          full        "$(gate_classify Makefile)"
eq "unknown script"             full        "$(gate_classify scripts/new-tool.mjs)"

# --- reverse closure
eq "sales dependents" "app operations sales"            "$(gate_reverse_closure "$GRAPH" sales)"
eq "hr dependents"    "app hr"                          "$(gate_reverse_closure "$GRAPH" hr)"
eq "crm dependents"   "app crm operations packages reporting sales" "$(gate_reverse_closure "$GRAPH" crm)"

# --- plans
plan $'memory/log.md\nmemory/wiki/overview.md'
eq "docs only" "rust= contract= tc= vitest= ui= e2e= specs= full=" "$(summary)"

plan $'apps/web/src/features/staff/PayrollTab.tsx\napps/web/src/features/staff/Staff.test.tsx'
eq "feature only" "rust= contract= tc=1 vitest=changed ui= e2e=targeted specs=a11y layout onboarding staff technician full=" "$(summary)"

plan 'apps/web/src/features/staff/Staff.test.tsx'
eq "feature test only: no e2e" "rust= contract= tc=1 vitest=changed ui= e2e= specs= full=" "$(summary)"

plan $'apps/web/src/features/customers/CustomerForm.tsx\napps/web/e2e/specs/auth.spec.ts'
eq "customers + a spec" "rust= contract= tc=1 vitest=changed ui= e2e=targeted specs=a11y auth customers layout management sales voids full=" "$(summary)"

plan $'apps/web/src/features/staff/PayrollTab.tsx\napps/web/src/components/ResponsiveTable.tsx'
eq "shared web" "rust= contract= tc=1 vitest=full ui=1 e2e=full specs= full=" "$(summary)"

plan $'apps/web/public/favicon.svg
apps/web/src/app/icons.test.ts'
eq "favicon + its test" "rust= contract= tc=1 vitest=changed ui= e2e=targeted specs=a11y auth layout full=" "$(summary)"

plan 'apps/web/src/features/notes/NotesPage.tsx'
eq "notes feature" "rust= contract= tc=1 vitest=changed ui= e2e=targeted specs=a11y layout notes full=" "$(summary)"

plan 'apps/web/src/features/newthing/Page.tsx'
eq "unmapped feature: full e2e" "rust= contract= tc=1 vitest=changed ui= e2e=full specs= full=" "$(summary)"

plan 'packages/api-client/src/index.ts'
eq "client only: cheap contract" "rust= contract=types tc=1 vitest=changed ui= e2e= specs= full=" "$(summary)"

plan 'apps/api/crates/hr/src/payroll.rs'
eq "one crate" "rust=app hr contract=regen tc=1 vitest= ui= e2e=full specs= full=" "$(summary)"

plan $'apps/api/crates/sales/src/paper.rs\napps/api/.sqlx/query-1.json'
eq "crate + sqlx stays narrow" "rust=app operations sales contract=regen tc=1 vitest= ui= e2e=full specs= full=" "$(summary)"

plan 'apps/api/.sqlx/query-1.json'
eq "sqlx alone: workspace" "workspace" "$PLAN_RUST"

plan 'apps/api/crates/brandnew/src/lib.rs'
eq "crate not in graph: workspace" "workspace" "$PLAN_RUST"

plan $'apps/api/migrations/0019_x.sql\napps/api/crates/sales/src/x.rs'
eq "migration: workspace" "workspace" "$PLAN_RUST"

plan 'apps/api/openapi.json'
eq "spec without rust: regen + full e2e" "rust= contract=regen tc=1 vitest= ui= e2e=full specs= full=" "$(summary)"

plan 'Makefile'
eq "unknown: full" "rust=workspace contract=regen tc=1 vitest=full ui=1 e2e=full specs= full=1" "$(summary)"
eq "unknown listed" "Makefile" "${PLAN_UNKNOWN%$'\n'}"

plan 'memory/log.md' auto 1
eq "--full on docs" "rust=workspace contract=regen tc=1 vitest=full ui=1 e2e=full specs= full=1" "$(summary)"

plan 'apps/web/src/features/staff/PayrollTab.tsx' off
eq "--no-e2e" "" "$PLAN_E2E"

plan 'memory/log.md' on
eq "--e2e forces the full suite" "full" "$PLAN_E2E"

plan 'scripts/merge-local-gate.sh'
eq "gate change: self-test only" "1 " "$PLAN_SELFTEST $PLAN_E2E"

plan '.github/workflows/ci.yml'
eq "infra: nothing to run, noted" "rust= contract= tc= vitest= ui= e2e= specs= full=" "$(summary)"
case "$PLAN_NOTES" in *"not exercised"*) eq "infra note" ok ok ;; *) eq "infra note" ok missing ;; esac

echo "gate-lib: $((checks - fails))/$checks passed"
[ "$fails" -eq 0 ]
