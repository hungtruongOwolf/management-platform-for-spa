#!/usr/bin/env bash
# Fallback merge gate for when GitHub Actions cannot run (it was disabled 2026-11-05 when a private
# repo ran out of minutes; CI is back since 2026-11-06 and the normal path is merge-when-green.sh).
# Runs the CI checks locally on the PR head merged with origin/main, then squash-merges. It does not
# cover Docker builds, gitleaks or Lighthouse. Change-aware: the steps that
# run depend on which paths the PR changes (rules in scripts/gate-lib.sh, runbook in
# memory/wiki/runbooks/local-merge-gate.md).
#
#   scripts/merge-local-gate.sh [options] <pr>
#   scripts/merge-local-gate.sh [options] --batch <pr> <pr> …   one gate on main + every PR, in order
#
#   --dry-run   classify and print the plan; no worktree, no build, no merge
#   --full      every step and the full e2e suite (required before a staging / Vercel deploy)
#   --e2e       full e2e suite regardless of what changed
#   --no-e2e    no e2e
#   (default)   e2e runs when app code changed: targeted specs for feature-only web changes, the full
#               suite when Rust, the contract, migrations or shared web code changed
#
# Env: GATE_WORK_BASE (default $TMPDIR/vpyd-gate), CARGO_BUILD_JOBS (default 2),
#      GATE_CARGO_INCREMENTAL (default 1), GATE_E2E_API_PORT / GATE_E2E_WEB_PORT (8191 / 5291).
#
# Secrets come from the repo's .env (dev database for the sqlx tests); nothing is printed.
set -euo pipefail
# The repo sits on exFAT, which records no file ownership; git's safe.directory check would refuse
# the gate worktree. Trust it for this process only (no global config change).
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0='*'

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/gate-lib.sh
. "$script_dir/gate-lib.sh"

usage() { sed -n '9,17p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 2; }
dry_run="" full="" e2e_mode=auto batch="" prs=()
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    --full) full=1 ;;
    --e2e) e2e_mode=on ;;
    --no-e2e) e2e_mode=off ;;
    --batch) batch=1 ;;
    -h | --help) usage ;;
    *[!0-9]* | "") echo "unknown argument: $arg"; usage ;;
    *) prs+=("$arg") ;;
  esac
done
[ "${#prs[@]}" -ge 1 ] || usage
if [ -z "$batch" ] && [ "${#prs[@]}" -gt 1 ]; then echo "several PRs need --batch"; exit 2; fi
if [ -n "$batch" ] && [ "${#prs[@]}" -lt 2 ]; then echo "--batch needs at least two PRs"; exit 2; fi

repo_root=$(git rev-parse --show-toplevel)
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
git -C "$repo_root" fetch -q origin main
base=$(git -C "$repo_root" rev-parse origin/main)

# ---------------------------------------------------------------- compose the tree under test
# The merge commit is built without a worktree (merge-tree + commit-tree), so the gate worktree is
# only ever checked out once per run: files that did not change keep their mtime and cargo reuses
# their build.
heads=() head_refs=() stage_trees=()
cur="$base"
for pr in "${prs[@]}"; do
  info=$(gh pr view "$pr" --json state,headRefName,headRefOid,mergeCommit \
    -q '.state + " " + .headRefName + " " + .headRefOid + " " + (.mergeCommit.oid // "-")')
  read -r state head_ref head_oid merge_oid <<< "$info"
  if [ "$state" != OPEN ]; then
    if [ -n "$dry_run" ] && [ "$state" = MERGED ] && [ -z "$batch" ]; then
      # Dry run of a merged PR: classify its squash commit against its parent.
      git -C "$repo_root" fetch -q origin "$merge_oid" 2>/dev/null || true
      base=$(git -C "$repo_root" rev-parse "$merge_oid^")
      cur="$merge_oid"
      echo "PR #$pr is merged: classifying its squash commit ${merge_oid:0:9} against its parent"
      heads+=("$merge_oid") head_refs+=("$head_ref")
      continue
    fi
    echo "PR #$pr is not open ($state)"; exit 1
  fi
  git -C "$repo_root" fetch -q origin "refs/pull/$pr/head"
  fetched=$(git -C "$repo_root" rev-parse FETCH_HEAD)
  [ "$fetched" = "$head_oid" ] || { echo "PR #$pr head moved while fetching ($fetched ≠ $head_oid); rerun"; exit 1; }
  if ! merged_out=$(git -C "$repo_root" merge-tree --write-tree --name-only "$cur" "$head_oid"); then
    echo "PR #$pr conflicts with main${batch:+ + the PRs before it} — merge origin/main into $head_ref first"
    # Output: tree id, the conflicted paths, a blank line, then merge messages.
    printf '%s\n' "$merged_out" | sed -n '2,/^$/p' | sed '/^$/d; s/^/   conflict: /'
    exit 1
  fi
  tree="${merged_out%%$'\n'*}"
  cur=$(git -C "$repo_root" -c user.name=gate -c user.email=gate@localhost \
    commit-tree "$tree" -p "$cur" -p "$head_oid" -m "gate: merge #$pr ($head_ref)")
  heads+=("$head_oid") head_refs+=("$head_ref") stage_trees+=("$tree")
done

changed=$(git -C "$repo_root" diff --name-only "$base" "$cur")
crate_graph=""
for dir in $(git -C "$repo_root" ls-tree --name-only "$cur" apps/api/crates/); do
  name="${dir##*/}"
  # `|| true`: a crate without path dependencies (kernel) makes grep exit 1.
  deps=$(git -C "$repo_root" show "$cur:$dir/Cargo.toml" 2>/dev/null \
    | grep -oE 'path *= *"\.\./[A-Za-z0-9_-]+"' | sed -E 's/.*\.\.\/([A-Za-z0-9_-]+)"/\1/' | sort -u | tr '\n' ' ' || true)
  crate_graph="$crate_graph$name $deps"$'\n'
done

gate_plan "$crate_graph" "$changed" "$e2e_mode" "$full"
echo "== gate: PR ${prs[*]/#/#} on main ${base:0:9}  ($(printf '%s\n' "$changed" | grep -c . || true) changed paths)"
gate_print_plan
if [ -n "$dry_run" ]; then
  echo "== dry run: nothing built, nothing merged"
  exit 0
fi

# ---------------------------------------------------------------- worktree + environment
work_base="${GATE_WORK_BASE:-${TMPDIR:-${TEMP:-/tmp}}/vpyd-gate}"
mkdir -p "$work_base"
lock="$work_base/lock"
mkdir "$lock" 2>/dev/null || { echo "another gate holds $lock (remove it if no gate is running)"; exit 1; }

times_log="$work_base/gate-times.log"
step_names=() step_secs=()
gate_start=$(date +%s)
result=fail
finish() {
  local total=$(( $(date +%s) - gate_start )) i line=""
  echo "== timings"
  for i in "${!step_names[@]}"; do
    printf '   %-14s %5ss\n' "${step_names[$i]}" "${step_secs[$i]}"
    line="$line ${step_names[$i]}=${step_secs[$i]}s"
  done
  printf '   %-14s %5ss  (%s)\n' total "$total" "$result"
  echo "$(date '+%Y-%m-%dT%H:%M:%S') prs=${prs[*]} result=$result total=${total}s rust=${PLAN_RUST:-none} vitest=${PLAN_VITEST:-none} e2e=${PLAN_E2E:-none}${full:+ full}$line" >> "$times_log"
  rmdir "$lock" 2>/dev/null || true
}
trap finish EXIT

step() {
  local name="$1" start; shift
  echo "== $name"
  start=$(date +%s)
  if "$@"; then
    step_names+=("$name") step_secs+=("$(( $(date +%s) - start ))")
  else
    step_names+=("$name(FAILED)") step_secs+=("$(( $(date +%s) - start ))")
    exit 1
  fi
}

# One stable worktree path for every gate: cargo fingerprints hold absolute paths and source mtimes,
# so a fixed path checked out in place (only changed files rewritten) lets cargo reuse earlier builds.
wt="$work_base/wt"
git -C "$repo_root" worktree prune
if git -C "$wt" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git -C "$wt" checkout -q --detach -f "$cur"
  # node_modules survives (pnpm install is then a quick verify); everything else untracked goes.
  git -C "$wt" clean -q -ffdx -e node_modules
else
  rm -rf "$wt"
  git -C "$repo_root" worktree add -q --detach "$wt" "$cur"
fi
cd "$wt"

# The e2e stack and the sqlx tests read the repo's .env (gitignored, so absent from the worktree).
cp "$repo_root/.env" "$wt/.env"
set -a; . "$repo_root/.env"; set +a
# sqlx::test registers and cleans up its `_sqlx_test_*` databases per DATABASE_URL database, and a
# run drops every leftover one it finds there; a gate run sharing the dev database with agents'
# test runs would drop theirs (and they ours). The gate gets its own registry database, and its own
# e2e database and ports so it never reuses an agent's e2e server.
docker exec vpyd-dev-postgres-1 createdb -U vpyd vpyd_gate >/dev/null 2>&1 || true
export DATABASE_URL="${DATABASE_URL%/*}/vpyd_gate"
export SQLX_OFFLINE=true
# Incremental pays off now that path and mtimes are stable (only the changed crates recompile);
# the old per-PR worktree path made every incremental cache miss.
export CARGO_INCREMENTAL="${GATE_CARGO_INCREMENTAL:-1}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_TARGET_DIR="${GATE_CARGO_TARGET_DIR:-$work_base/target}"
manifest=apps/api/Cargo.toml
app_bin="$CARGO_TARGET_DIR/debug/app"
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) app_bin="$app_bin.exe" ;; esac
command -v cygpath >/dev/null && app_bin=$(cygpath -m "$app_bin")

# ---------------------------------------------------------------- steps (cheap first)
step contract-files node scripts/contract-files.mjs
step install pnpm install --frozen-lockfile --reporter=silent
step lint pnpm -s lint
[ -n "$PLAN_TYPECHECK" ] && step typecheck pnpm -s typecheck
case "$PLAN_VITEST" in
  full) step vitest pnpm -s --filter web exec vitest run --no-file-parallelism ;;
  # Tests whose import graph reaches a changed file (vitest --changed, since the tested base).
  changed) step vitest pnpm -s --filter web exec vitest run --no-file-parallelism --changed "$base" --passWithNoTests ;;
esac
# Root's `test` is `pnpm -r test`, so a `!web` filter would rerun web through the root.
[ -n "$PLAN_UI_TEST" ] && step ui-test pnpm -s --filter @vpyd/ui test
[ -n "$PLAN_SELFTEST" ] && step self-test bash scripts/gate-lib.test.sh

app_built=""
if [ -n "$PLAN_RUST" ]; then
  if [ "$PLAN_RUST" = workspace ]; then pkgs=(--workspace); else
    pkgs=(); for crate in $PLAN_RUST; do pkgs+=(-p "$crate"); done
  fi
  step rust-fmt cargo fmt --manifest-path "$manifest" --all --check
  step clippy cargo clippy --manifest-path "$manifest" "${pkgs[@]}" --all-targets -- -D warnings
  step rust-test cargo test --manifest-path "$manifest" "${pkgs[@]}"
  # `cargo test` of `app` builds the `app` binary for its integration tests (same profile, test
  # feature set): reuse it for the contract and e2e instead of a second build without kernel/testing.
  case " $PLAN_RUST " in *" app "* | " workspace ") [ -f "$app_bin" ] && app_built=1 ;; esac
fi
if [ -z "$app_built" ] && { [ "$PLAN_CONTRACT" = regen ] || [ -n "$PLAN_E2E" ]; }; then
  step api-build cargo build --manifest-path "$manifest" -p app
fi

contract_diff() { git diff --exit-code --stat -- "$@" || { echo "✗ contract drift: commit the regenerated $*"; return 1; }; }
case "$PLAN_CONTRACT" in
  regen)
    step contract sh -c 'API_SPEC_BIN="$1" node scripts/api-spec.mjs && pnpm -s api:types' _ "$app_bin"
    step contract-diff contract_diff apps/api/openapi.json packages/api-client/src/schema.ts ;;
  types)
    step contract pnpm -s api:types
    step contract-diff contract_diff packages/api-client/src/schema.ts ;;
esac
if [ -n "$PLAN_DENY" ]; then
  if cargo deny --version >/dev/null 2>&1; then
    step cargo-deny cargo deny --manifest-path "$manifest" check
  else
    echo "! cargo-deny not installed — skipped. Install: cargo install --locked cargo-deny"
  fi
fi

if [ -n "$PLAN_E2E" ]; then
  docker exec vpyd-dev-postgres-1 createdb -U vpyd vpyd_gate_e2e >/dev/null 2>&1 || true
  export E2E_DATABASE_URL="${DATABASE_URL%/*}/vpyd_gate_e2e"
  export E2E_APP_DATABASE_URL="${APP_DATABASE_URL%/*}/vpyd_gate_e2e"
  export E2E_API_PORT="${GATE_E2E_API_PORT:-8191}" E2E_WEB_PORT="${GATE_E2E_WEB_PORT:-5291}"
  export E2E_API_BIN="$app_bin"
  if [ "$PLAN_E2E" = full ]; then
    step e2e pnpm -s e2e
  else
    e2e_args=(e2e/setup/provision.setup.ts)
    for spec in $PLAN_E2E_SPECS; do e2e_args+=("e2e/specs/$spec.spec.ts"); done
    step e2e pnpm -s --filter web exec playwright test --project=setup --project=desktop "${e2e_args[@]}"
  fi
fi

# ---------------------------------------------------------------- merge
# Merge only what was tested: main must not have moved, each PR head must be the tested one, and
# after each squash main must hold exactly the tree the gate built for that stage.
wait_mergeable() {
  local pr="$1" state i
  for i in $(seq 1 40); do
    state=$(gh pr view "$pr" --json mergeable -q .mergeable)
    [ "$state" != UNKNOWN ] && break
    sleep 3
  done
  [ "$state" = MERGEABLE ] || { echo "PR #$pr is $state on GitHub — stopping; merged so far: ${merged[*]:-none}"; exit 1; }
}
merged=()
expected_main="$base"
for i in "${!prs[@]}"; do
  pr="${prs[$i]}"
  git -C "$repo_root" fetch -q origin main
  main_now=$(git -C "$repo_root" rev-parse origin/main)
  if [ "$i" -eq 0 ]; then
    [ "$main_now" = "$base" ] || { echo "main moved since the gate started (${base:0:9} → ${main_now:0:9}); rerun the gate"; exit 1; }
  else
    [ "$(git -C "$repo_root" rev-parse "$main_now^{tree}")" = "${stage_trees[$((i - 1))]}" ] \
      && [ "$(git -C "$repo_root" rev-parse "$main_now^")" = "$expected_main" ] \
      || { echo "main after #${prs[$((i - 1))]} is not the tree the gate tested; stopping (merged: ${merged[*]})"; exit 1; }
    wait_mergeable "$pr"
  fi
  gh pr merge "$pr" --squash --match-head-commit "${heads[$i]}"
  merged+=("#$pr")
  gh api -X DELETE "repos/$repo/git/refs/heads/${head_refs[$i]}" >/dev/null 2>&1 \
    && echo "deleted remote branch ${head_refs[$i]}" || echo "note: remote branch ${head_refs[$i]} not deleted"
  expected_main="$main_now"
done
git -C "$repo_root" fetch -q origin main
if [ "$(git -C "$repo_root" rev-parse "origin/main^{tree}")" != "${stage_trees[-1]}" ]; then
  echo "! warning: main's tree differs from the tested tree — check for a concurrent merge"
fi
result=ok
echo "merged ${merged[*]} (local gate: rust=${PLAN_RUST:-skip} vitest=${PLAN_VITEST:-skip} e2e=${PLAN_E2E:-skip}${full:+, --full})"
