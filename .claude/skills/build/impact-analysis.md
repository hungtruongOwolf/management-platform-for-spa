# Impact analysis — the blast radius of every change

A change is done only when everything that depends on it has been found, updated, and re-verified. The edited lines are the smallest part of the job. Run this **before** coding (to plan) and **again after** (to confirm), for every add, change, rename, move, and delete.

## 1. Map the blast radius (before editing)
For each symbol you will touch (function, type, struct field, enum variant, endpoint, DB column/table, config key, permission string, event name, UI component, route):
- **Code references:** use LSP *find references* / *go to implementation* (rust-analyzer, TypeScript server) as the authoritative source. Grep for the name as a backstop for strings LSP cannot see: SQL, serde renames (`#[serde(rename)]`), JSON keys, route paths, permission strings, env vars, i18n keys, test fixtures.
- **Cross-boundary dependents** that no compiler sees:
  - API contract: `openapi.yaml`, the generated TS client, and every frontend caller of that endpoint.
  - Database: migrations, `sqlx` queries (`query!` macros + `.sqlx` offline data), indexes, constraints, seed data, backfill needs.
  - Authorization: permission strings, `security/authz-matrix.md`, authz tests.
  - Policies/config: `CompensationPolicy` fields and existing versions that still reference the old shape.
  - Background jobs, outbox event consumers, reminders, reports that read the data.
  - Docs: specs, `architecture/*`, glossary, ADRs that describe the old behaviour.
- **Tests:** every test that exercises the symbol directly or through a caller, including tests that will now be **stale** (asserting the old behaviour) and fixtures/snapshots that encode the old shape.

Write the result as an **impact list** in your plan:
```
Impact list for <change>
- Code:   path:line — caller — needs update / unaffected (reason)
- API/DB: …
- Tests:  update X, delete Y (obsolete because …), add Z (new behaviour / regression)
- Docs:   …
```

## 2. Change by kind
- **Add:** wire it everywhere it must appear: router, permission seed, authz matrix, OpenAPI, client, nav/UI, reminders, reports, docs. An unreachable feature counts as unfinished.
- **Change behaviour:** update every caller that relied on the old behaviour; rewrite the tests that asserted it. Never weaken a test just to make it pass. If a test's expectation is now wrong, say why in the diff.
- **Rename / move:** use LSP rename where available, then grep for string occurrences (SQL, serde, routes, docs).
- **Delete:** prove it is unused (zero references + no string/route/DB usage), then delete the code, its tests, fixtures, permission strings, docs, and feature flags together. Leave no dead code behind. Columns and tables are dropped only through an expand → contract migration.
- **Public contract change** (API field, DB column, event payload): prefer additive changes (new optional field, new endpoint version). Breaking changes need an explicit migration path in the plan.

## 3. Verify (after editing)
- Full checks, not just the touched module: `cargo check --workspace --all-targets`, `cargo clippy`, `cargo test --workspace`, `pnpm typecheck`, `pnpm test` across all packages, regenerate the API client and typecheck the frontend against it, e2e for affected journeys.
- Re-run find-references on every changed symbol and compare against the impact list: each item is updated, deleted, or marked unaffected with a reason.
- Grep for leftovers of removed or renamed names (0 hits outside the changelog).

Done when every item on the impact list is resolved, all workspace-wide checks are green, and zero leftovers remain.
