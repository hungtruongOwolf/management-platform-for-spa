#!/usr/bin/env bash
# Merge a PR only after every CI check has run and passed. The repo is private on GitHub Free, so
# branch protection is unavailable — this script is the merge gate. Agents and humans use it instead
# of calling `gh pr merge` directly.
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
gh pr merge "$pr" --squash --delete-branch
echo "PR #$pr merged"
