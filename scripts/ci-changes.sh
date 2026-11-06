#!/usr/bin/env bash
# Decides which CI jobs a change needs. Reuses gate_classify from scripts/gate-lib.sh so the local
# merge gate and CI share one set of path buckets. Fails safe: anything it cannot classify runs everything.
#
#   ci_scope <changed paths, newline-separated>  → prints "docs" | "web" | "full"
#     docs  only docs/meta changed: secret scan only
#     web   web app, shared TS packages or the TS client changed: web, e2e, Lighthouse, images;
#           no Rust tests/clippy/deny (the API binary is still built for e2e and the contract check)
#     full  Rust, migrations, OpenAPI spec, CI/deploy config, gate scripts, unknown paths: everything
#
# CLI (used by .github/workflows/ci.yml):
#   scripts/ci-changes.sh < paths        prints "scope=<scope>" lines for $GITHUB_OUTPUT
# shellcheck shell=bash

ci_lib="$(dirname "${BASH_SOURCE[0]}")/gate-lib.sh"
# shellcheck source=/dev/null
[ -f "$ci_lib" ] && . "$ci_lib"

ci_scope() {
  local paths="$1" path tokens token web="" any=""
  # Without the shared classifier there is no safe way to skip anything.
  declare -F gate_classify >/dev/null || { echo full; return; }
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    any=1
    tokens=$(gate_classify "$path")
    for token in $tokens; do
      case "$token" in
        docs) ;;
        client | shared | webtest | web:* | e2e:*) web=1 ;;
        # rust:*, sqlx, deny, spec, infra (.github, Dockerfiles), gate (CI depends on it), full, unknown
        *) echo full; return ;;
      esac
    done
  done <<< "$paths"
  # An empty change list means we could not read it: run everything.
  if [ -z "$any" ]; then echo full; elif [ -n "$web" ]; then echo web; else echo docs; fi
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  echo "scope=$(ci_scope "$(cat)")"
fi
