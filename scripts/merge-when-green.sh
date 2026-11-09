#!/usr/bin/env bash
# Merge a PR only after every CI check has run and passed. `main` has no branch protection (none was
# available while the repo was private on GitHub Free), so this script is the merge gate. Agents and
# humans use it instead of calling `gh pr merge` directly.
#   scripts/merge-when-green.sh <pr-number>
set -euo pipefail
pr="${1:?usage: merge-when-green.sh <pr-number>}"

state=$(gh pr view "$pr" --json state,mergeable -q '.state + " " + .mergeable')
case "$state" in
  "OPEN MERGEABLE") ;;
  "OPEN UNKNOWN") sleep 10; state=$(gh pr view "$pr" --json mergeable -q .mergeable)
                  [ "$state" = MERGEABLE ] || { echo "PR #$pr not mergeable ($state)"; exit 1; } ;;
  *) echo "PR #$pr not mergeable ($state)"; exit 1 ;;
esac

# Right after a push GitHub reports no checks for a while; `--watch` would then exit at once.
for _ in $(seq 1 40); do
  [ -n "$(gh pr checks "$pr" 2>/dev/null || true)" ] && break
  sleep 15
done
[ -n "$(gh pr checks "$pr" 2>/dev/null || true)" ] || { echo "PR #$pr: no checks appeared after 10 min"; exit 1; }

gh pr checks "$pr" --watch --interval 30 >/dev/null 2>&1 || true
results=$(gh pr checks "$pr" | awk -F'\t' '{print $1": "$2}')
echo "$results"
if echo "$results" | grep -qvE ': (pass|skipping)$'; then
  echo "PR #$pr: not all checks passed — not merging"
  exit 1
fi
# CI skips jobs by change scope (scripts/ci-changes.sh), so "skipping" is normal; `CI ok` is the
# job that checks every job which ran passed. Without it, the run did not finish.
if ! echo "$results" | grep -qx 'CI ok: pass'; then
  echo "PR #$pr: no passing 'CI ok' check — not merging"
  exit 1
fi
# Delete the head branch through the API, not `--delete-branch`: that flag also checks out the base
# branch locally, which fails when `main` is checked out in another worktree and then skips the
# remote delete too (stale branches piled up on GitHub, 2026-11-05).
head=$(gh pr view "$pr" --json headRefName -q .headRefName)
gh pr merge "$pr" --squash
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
gh api -X DELETE "repos/$repo/git/refs/heads/$head" >/dev/null 2>&1   && echo "deleted remote branch $head" || echo "note: remote branch $head not deleted"
echo "PR #$pr merged"
