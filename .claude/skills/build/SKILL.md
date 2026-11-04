---
name: build
description: Implementation — use whenever writing or changing production code (feature, fix, refactor): test-first, clean code per project standards, verified before done.
---

# Build

Implement exactly one ticket from `memory/wiki/plans/`. The output is production-quality code a reviewer approves on first pass.

## Steps
1. Read the ticket, the spec criteria it links, the relevant `architecture/*` pages, and `memory/wiki/conventions/coding-standards.md`.
2. **Impact analysis**: build the impact list per [impact-analysis.md](impact-analysis.md) (callers, cross-boundary dependents, stale tests, docs). Restate the plan from it: files to touch, public interfaces, tests to write, update, or delete. Raise any conflict with the spec or design before coding.
3. Create the branch (`feat/<slug>`).
4. **Red → green → refactor** for each behaviour: write the failing test, make it pass with the simplest code, refactor while green. Include the ticket's negative authz and validation tests.
5. Apply the `secure` skill when the ticket touches auth, data, uploads, payments, or media.
6. Verify — all must pass, and paste the summary:
   - Backend: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
   - Frontend: `pnpm lint`, `pnpm typecheck`, `pnpm test`
   - E2E for the slice when it has a UI path.
7. Re-run the impact check (step 3 of impact-analysis.md): every impact-list item is resolved, and no leftovers of renamed or removed names remain.
8. Self-review the diff against `.claude/skills/pr-review/SKILL.md` and fix what it finds.
9. Done when every acceptance criterion of the ticket has a passing test, every impact-list item is resolved, all workspace-wide checks are green, and the self-review has no open blocker/major.
10. Tick the ticket in the plan; record via `memory`. Suggest a Conventional Commit message; commit only when the EM asks.
