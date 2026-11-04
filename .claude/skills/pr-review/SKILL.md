---
name: pr-review
description: Code review — use when the EM asks to review code, a diff, branch, or PR, and as the self-review step before calling any implementation done.
---

# PR review

Review as the senior engineer who will be paged at 3 a.m. if this breaks. Kind to the author, relentless on the code.

## Steps
1. Get the diff (`git diff <base>...HEAD`, or the files named by the EM) and the spec/plan ticket it implements.
2. Read every changed line plus enough surrounding code to understand callers and callees. Independently rebuild the blast radius per `.claude/skills/build/impact-analysis.md` (LSP find-references on each changed symbol, plus grep for strings) and look for **unchanged** files that should have changed: callers relying on the old behaviour, stale tests and fixtures, OpenAPI/client, migrations, permissions, docs.
3. Check each axis:
   - **Correctness** — logic, edge cases, error paths, concurrency, transactions, time zones.
   - **Impact** — every dependent of a changed symbol is updated or provably unaffected; deleted code leaves no tests, fixtures, routes, permissions, or docs behind.
   - **Spec match** — every acceptance criterion of the ticket implemented; nothing extra unrequested.
   - **Security** — apply the `secure` checklists to anything touching auth, data, input, media; authz in the service layer; identity from session only.
   - **Tests** — negative paths present; tests would fail if the code were wrong.
   - **Design** — module boundaries, deep interfaces, naming, duplication, consistency with `architecture/*`.
   - **Standards** — every rule in `memory/wiki/conventions/coding-standards.md`.
   - **Performance** — N+1 queries, missing indexes, unbounded lists, re-renders, bundle size, blocking calls in async.
   - **Operability** — logs, metrics, migrations safe on a live DB, config documented.
4. Report findings, most severe first:
   ```
   [BLOCKER|MAJOR|MINOR|NIT] path/to/file.rs:42 — problem
     Why: concrete failure scenario
     Fix: suggested change
   ```
   Close with a verdict: **Approve / Approve with nits / Request changes**, and one line on what the PR does well.
5. Done when every changed file has been read and every axis has been checked against it.
