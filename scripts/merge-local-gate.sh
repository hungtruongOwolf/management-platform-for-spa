#!/usr/bin/env bash
# Merge gate while GitHub Actions is disabled (minutes exhausted, EM 2026-11-05): runs the CI checks
# locally on the PR head merged with origin/main, then squash-merges. Use instead of
# merge-when-green.sh until the CI workflow is re-enabled (`gh workflow enable CI`).
#
#   scripts/merge-local-gate.sh <pr-number> [--e2e]
#
# The temporary worktree lives on the scratch/temp drive (not the exFAT project drive). Secrets come
# from the repo's .env (dev database for the sqlx tests); nothing is printed.
set -euo pipefail
# The repo sits on exFAT, which records no file ownership; git's safe.directory check would refuse
# the temp worktree. Trust it for this process only (no global config change).
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0='*'
pr="${1:?usage: merge-local-gate.sh <pr-number> [--e2e]}"
e2e="${2:-}"
repo_root=$(git rev-parse --show-toplevel)
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)

state=$(gh pr view "$pr" --json state,mergeable -q '.state + " " + .mergeable')
[ "${state%% *}" = OPEN ] || { echo "PR #$pr is not open ($state)"; exit 1; }
head=$(gh pr view "$pr" --json headRefName -q .headRefName)

work_base="${TMPDIR:-${TEMP:-/tmp}}/vpyd-gate"
mkdir -p "$work_base"
wt="$work_base/pr-$pr"
git -C "$repo_root" fetch -q origin "$head" main
git -C "$repo_root" worktree remove --force "$wt" 2>/dev/null || true
git -C "$repo_root" worktree add -q --detach "$wt" "origin/$head"
cleanup() { git -C "$repo_root" worktree remove --force "$wt" 2>/dev/null || true; }
trap cleanup EXIT

cd "$wt"
# Test what main will look like after the merge.
git -c user.name=gate -c user.email=gate@localhost merge -q --no-edit origin/main \
  || { echo "PR #$pr conflicts with main — merge origin/main into $head first"; exit 1; }

set -a; . "$repo_root/.env"; set +a
export SQLX_OFFLINE=true CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="${GATE_CARGO_TARGET_DIR:-$work_base/target}"

echo "== install";   pnpm install --frozen-lockfile >/dev/null
echo "== api";       pnpm -s check:api
echo "== lint";      pnpm -s lint
echo "== typecheck"; pnpm -s typecheck
echo "== test";      pnpm -s --filter web exec vitest run --no-file-parallelism
pnpm -s -r --filter '!web' test
echo "== extra";     pnpm -s check:extra
if [ "$e2e" = "--e2e" ]; then
  echo "== e2e";     pnpm -s e2e
fi

gh pr merge "$pr" --squash
gh api -X DELETE "repos/$repo/git/refs/heads/$head" >/dev/null 2>&1 \
  && echo "deleted remote branch $head" || echo "note: remote branch $head not deleted"
echo "PR #$pr merged (local gate${e2e:+ + e2e})"
