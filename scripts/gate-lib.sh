# shellcheck shell=bash
# Change classification and step planning for scripts/merge-local-gate.sh. Pure functions over a
# list of changed paths (no git, no network), so scripts/gate-lib.test.sh can test them directly.
#
#   gate_classify <path>            → space-separated tokens for one path
#   gate_plan <crate graph> <paths> → sets the PLAN_* variables below
#
# Tokens:
#   docs            memory/, docs, *.md, prototypes, .claude, editor/git meta: nothing to execute
#   gate            the gate's own scripts: run the gate self-test
#   infra           deploy/CI config the gate cannot exercise (Dockerfile, .github, vercel.json)
#   rust:all        migrations, kernel, app, workspace Cargo.toml/lock, toolchain, unknown api file
#   rust:<crate>    apps/api/crates/<crate>/**
#   sqlx            apps/api/.sqlx (only widens to rust:all when no crate changed with it)
#   deny            cargo-deny inputs (deny.toml, Cargo.lock, Cargo.toml)
#   spec            apps/api/openapi.json or the scripts that write it: regenerate from code
#   client          packages/api-client: regenerate the TS client from the committed openapi.json
#   web:<feature>   apps/web/src/features/<feature>/** (source); web:_static = public/, index.html
#   webtest         a *.test.ts(x) file anywhere under apps/web/src (vitest only, no e2e)
#   e2e:<spec>      apps/web/e2e/specs/<spec>.spec.ts
#   shared          everything else under apps/web, packages/ui, root package.json/lockfile/configs
#   full            unknown path or a change that touches everything: run the whole gate
#
# Plan (strings, "" = skip):
#   PLAN_RUST       "" | "workspace" | "<crate> <crate> …" (cargo -p list, reverse dependents included)
#   PLAN_CONTRACT   "" | "types" (client from committed spec) | "regen" (spec from code, then client)
#   PLAN_DENY       "" | 1
#   PLAN_TYPECHECK  "" | 1
#   PLAN_VITEST     "" | "changed" | "full"
#   PLAN_UI_TEST    "" | 1
#   PLAN_SELFTEST   "" | 1
#   PLAN_E2E        "" | "targeted" | "full";  PLAN_E2E_SPECS: spec basenames when targeted
#   PLAN_FULL       1 when a `full` token forced everything
#   PLAN_NOTES      human-readable reasons, one per line
#   PLAN_UNKNOWN    unknown paths, one per line
#   PLAN_WIDE       paths that widened the plan (shared web, whole workspace, contract)

# Feature folder → e2e specs that drive its screens. layout + a11y run with every targeted e2e
# (they sweep every screen). Keep in sync when a spec starts covering another feature.
gate_feature_specs() {
  case "$1" in
    appointments) echo "appointments" ;;
    auth) echo "auth" ;;
    catalog) echo "auth" ;;
    corrections) echo "voids" ;;
    customers) echo "customers management sales voids" ;;
    front-desk) echo "front-desk appointments quay-upsell voids" ;;
    notes) echo "notes" ;;
    reports) echo "management" ;;
    sales) echo "sales quay-upsell" ;;
    staff) echo "staff technician onboarding" ;;
    _static) echo "auth" ;;
    *) echo "" ;;
  esac
}

gate_classify() {
  local path="$1" rest crate feature
  case "$path" in
    # Gate scripts first: their names end in .sh/.mjs but they must not fall into the docs/shared rules.
    scripts/merge-local-gate.sh | scripts/gate-lib.sh | scripts/gate-lib.test.sh | scripts/merge-when-green.sh)
      echo "gate"; return ;;
    scripts/api-spec.mjs | scripts/contract-files.mjs | scripts/check-extra.mjs)
      echo "spec"; return ;;
    *.md) echo "docs"; return ;;
    memory/* | docs/* | "docs for reference/"* | prototypes/* | .claude/* | scripts/brand/*) echo "docs"; return ;;
    .gitignore | .gitattributes | .editorconfig | .env.example | LICENSE) echo "docs"; return ;;

    apps/api/.sqlx/*) echo "sqlx"; return ;;
    apps/api/openapi.json) echo "spec"; return ;;
    apps/api/deny.toml) echo "deny"; return ;;
    apps/api/Cargo.toml | apps/api/Cargo.lock) echo "rust:all deny"; return ;;
    apps/api/Dockerfile | apps/api/.dockerignore) echo "infra"; return ;;
    apps/api/migrations/* | apps/api/rust-toolchain.toml) echo "rust:all"; return ;;
    apps/api/crates/*/*)
      rest="${path#apps/api/crates/}"
      crate="${rest%%/*}"
      case "$crate" in
        kernel | app) echo "rust:all" ;;
        *) echo "rust:$crate" ;;
      esac
      return ;;
    apps/api/*) echo "rust:all"; return ;;

    packages/api-client/*) echo "client"; return ;;
    packages/ui/*) echo "shared"; return ;;

    # A test file anywhere in the web source only feeds vitest (its import graph decides what runs).
    apps/web/src/*.test.ts | apps/web/src/*.test.tsx) echo "webtest"; return ;;
    apps/web/src/features/*/*)
      rest="${path#apps/web/src/features/}"
      echo "web:${rest%%/*}"; return ;;
    # Static assets and the HTML entry: no unit graph; a targeted e2e still loads every screen.
    apps/web/public/* | apps/web/index.html) echo "web:_static"; return ;;
    apps/web/e2e/specs/*.spec.ts)
      rest="${path#apps/web/e2e/specs/}"
      echo "e2e:${rest%.spec.ts}"; return ;;
    apps/web/*) echo "shared"; return ;;

    package.json | pnpm-lock.yaml | pnpm-workspace.yaml | tsconfig.base.json | eslint.config.js | .npmrc)
      echo "shared"; return ;;

    .github/* | vercel.json | .vercelignore | .dockerignore) echo "infra"; return ;;
    # Dev database roles and compose file feed the sqlx tests and the e2e stack.
    infra/postgres-init/* | infra/docker-compose.dev.yml) echo "full"; return ;;
    infra/*) echo "infra"; return ;;
  esac
  echo "full"
}

# Reverse-dependency closure over the workspace crate graph.
#   $1: graph, one line per crate: "<crate> <dep> <dep> …" (path dependencies, dev-deps included)
#   $2: space-separated starting crates
# Prints the closure, sorted, space-separated.
gate_reverse_closure() {
  local graph="$1" selected=" $2 " changed=1 crate deps dep
  while [ "$changed" = 1 ]; do
    changed=0
    while read -r crate deps; do
      [ -n "$crate" ] || continue
      case "$selected" in *" $crate "*) continue ;; esac
      for dep in $deps; do
        case "$selected" in
          *" $dep "*) selected="$selected$crate "; changed=1; break ;;
        esac
      done
    done <<< "$graph"
  done
  # shellcheck disable=SC2086 # word splitting is the point
  printf '%s\n' $selected | sort -u | tr '\n' ' ' | sed 's/ $//'
}

# $1: crate graph (see gate_reverse_closure), $2: changed paths (newline-separated),
# $3: e2e mode (auto | on | off), $4: 1 to force everything (--full).
gate_plan() {
  local graph="$1" paths="$2" e2e_mode="${3:-auto}" force="${4:-}"
  local path token tokens all=" "
  PLAN_UNKNOWN="" PLAN_WIDE=""
  while IFS= read -r path; do
    [ -n "$path" ] || continue
    tokens=$(gate_classify "$path")
    [ "$tokens" = full ] && PLAN_UNKNOWN="$PLAN_UNKNOWN$path"$'\n'
    case " $tokens " in
      *" shared "* | *" rust:all "* | *" spec "*) PLAN_WIDE="$PLAN_WIDE$path ($tokens)"$'\n' ;;
    esac
    for token in $tokens; do
      case "$all" in *" $token "*) ;; *) all="$all$token " ;; esac
    done
  done <<< "$paths"

  has() { case "$all" in *" $1 "*) return 0 ;; esac; return 1; }
  has_prefix() { case "$all" in *" $1"*) return 0 ;; esac; return 1; }

  PLAN_FULL="" PLAN_RUST="" PLAN_CONTRACT="" PLAN_DENY="" PLAN_TYPECHECK="" PLAN_VITEST=""
  PLAN_UI_TEST="" PLAN_SELFTEST="" PLAN_E2E="" PLAN_E2E_SPECS="" PLAN_NOTES="" PLAN_TOKENS="${all# }"
  PLAN_TOKENS="${PLAN_TOKENS% }"
  note() { PLAN_NOTES="$PLAN_NOTES$1"$'\n'; }

  if [ "$force" = 1 ] || has full; then
    PLAN_FULL=1
    [ "$force" = 1 ] && note "--full: every step" || note "unknown or all-affecting path: every step (fail safe)"
    PLAN_RUST=workspace PLAN_CONTRACT=regen PLAN_DENY=1 PLAN_TYPECHECK=1 PLAN_VITEST=full
    PLAN_UI_TEST=1 PLAN_SELFTEST=1
    if [ "$e2e_mode" = off ] && [ "$force" != 1 ]; then
      note "e2e skipped (--no-e2e)"
    else
      PLAN_E2E=full
    fi
    return 0
  fi

  # Rust: changed crates + reverse dependents; migrations/kernel/app/workspace files → workspace.
  local crates="" crate
  for token in $all; do
    case "$token" in rust:all) ;; rust:*) crates="$crates ${token#rust:}" ;; esac
  done
  if has rust:all; then
    PLAN_RUST=workspace
  elif [ -n "$crates" ]; then
    PLAN_RUST=$(gate_reverse_closure "$graph" "${crates# }")
  elif has sqlx; then
    PLAN_RUST=workspace
    note ".sqlx changed without a crate: whole workspace"
  fi
  if [ -n "$PLAN_RUST" ]; then
    # A crate the graph does not know (new crate, renamed folder) → whole workspace.
    for crate in $crates; do
      grep -q "^$crate\( \|$\)" <<< "$graph" || { PLAN_RUST=workspace; note "crate '$crate' not in the workspace graph: whole workspace"; }
    done
  fi
  has deny && PLAN_DENY=1

  # Contract: Rust or spec changes → regenerate from code; client-only → regenerate the client.
  if [ -n "$PLAN_RUST" ] || has spec; then
    PLAN_CONTRACT=regen
  elif has client; then
    PLAN_CONTRACT=types
  fi

  # Web.
  local web=""
  if has shared || has client || has_prefix web: || has webtest || has_prefix e2e: || [ -n "$PLAN_CONTRACT" ]; then
    PLAN_TYPECHECK=1
  fi
  if has shared; then
    PLAN_VITEST=full PLAN_UI_TEST=1
  elif has_prefix web: || has webtest || has client; then
    PLAN_VITEST=changed
  fi
  has gate && PLAN_SELFTEST=1
  has infra && note "deploy/CI config changed: not exercised by this gate (Docker build, workflows, Vercel)"

  # E2E. auto: on when app code changed (web source, shared web, Rust, spec) or a spec changed.
  for token in $all; do
    case "$token" in web:* | e2e:* | shared | spec) web=1 ;; esac
  done
  [ -n "$PLAN_RUST" ] && web=1
  case "$e2e_mode" in
    off) [ -n "$web" ] && note "e2e skipped (--no-e2e)"; return 0 ;;
    on) PLAN_E2E=full; note "--e2e: full e2e suite"; return 0 ;;
  esac
  [ -n "$web" ] || return 0
  if has shared || has spec || [ -n "$PLAN_RUST" ]; then
    PLAN_E2E=full
    return 0
  fi
  local specs=" " spec feature
  for token in $all; do
    case "$token" in
      web:*)
        feature="${token#web:}"
        local mapped
        mapped=$(gate_feature_specs "$feature")
        if [ -z "$mapped" ]; then
          PLAN_E2E=full
          note "feature '$feature' has no e2e mapping: full e2e"
          return 0
        fi
        for spec in $mapped; do specs="$specs$spec "; done ;;
      e2e:*) specs="$specs${token#e2e:} " ;;
    esac
  done
  specs="${specs}layout a11y"
  PLAN_E2E=targeted
  # shellcheck disable=SC2086
  PLAN_E2E_SPECS=$(printf '%s\n' $specs | sort -u | tr '\n' ' ' | sed 's/ $//')
}

# Human-readable plan (after gate_plan).
gate_print_plan() {
  local rust_desc="skip" e2e_desc="skip"
  case "$PLAN_RUST" in
    "") ;;
    workspace) rust_desc="fmt + clippy + cargo test --workspace" ;;
    *) rust_desc="fmt + clippy + cargo test -p ${PLAN_RUST// / -p }" ;;
  esac
  case "$PLAN_E2E" in
    full) e2e_desc="full suite" ;;
    targeted) e2e_desc="setup + ${PLAN_E2E_SPECS}" ;;
  esac
  echo "== plan  (tokens: ${PLAN_TOKENS:-none})"
  printf '   %-15s %s\n' \
    contract-files "always (cheap parse check)" \
    lint "always" \
    typecheck "$([ -n "$PLAN_TYPECHECK" ] && echo run || echo skip)" \
    vitest "${PLAN_VITEST:-skip}" \
    ui-test "$([ -n "$PLAN_UI_TEST" ] && echo run || echo skip)" \
    rust "$rust_desc" \
    contract "${PLAN_CONTRACT:-skip}" \
    cargo-deny "$([ -n "$PLAN_DENY" ] && echo "run (if installed)" || echo skip)" \
    self-test "$([ -n "$PLAN_SELFTEST" ] && echo run || echo skip)" \
    e2e "$e2e_desc"
  if [ -n "$PLAN_UNKNOWN" ]; then
    echo "   unknown paths (forced a full run):"
    printf '%s' "$PLAN_UNKNOWN" | sed 's/^/     /'
  fi
  if [ -n "$PLAN_WIDE" ]; then
    echo "   widened by (shared web / whole workspace / contract):"
    printf '%s' "$PLAN_WIDE" | head -n 8 | sed 's/^/     /'
    [ "$(printf '%s' "$PLAN_WIDE" | grep -c .)" -le 8 ] || echo "     …"
  fi
  [ -z "$PLAN_NOTES" ] || printf '%s' "$PLAN_NOTES" | sed 's/^/   note: /'
}
