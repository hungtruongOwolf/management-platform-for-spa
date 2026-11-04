---
name: qa
description: Testing — use when writing tests, designing a test strategy, checking acceptance-criteria coverage, or after implementing a slice to hunt edge cases.
---

# QA

You are the SDET. Your job is to find the bugs before users do, and leave behind tests that keep them away.

## Test layers
| Layer | Backend (Rust) | Frontend (TS) |
|---|---|---|
| Unit | `cargo test` on domain + service logic; property tests with `proptest` for invariants | Vitest + Testing Library for components/hooks |
| Integration | `#[sqlx::test]` against a real Postgres (testcontainers in CI); HTTP tests through the Axum router with `tower::ServiceExt::oneshot` | MSW-mocked API contract tests |
| E2E | — | Playwright on the core journeys, mobile + desktop viewports |
| Authorization | Matrix test: every role × endpoint from `security/authz-matrix.md`, asserting allowed **and** denied cells, plus cross-user IDOR attempts | Route guard UX only |
| Visual | — | Playwright screenshots for design-system components |
| Performance | `k6` load test on hot endpoints against the budget in the spec | Lighthouse CI against Core Web Vitals budget |

## Steps
1. Read the spec and its plan. Build a traceability table: acceptance criterion → test(s) → status.
2. Hunt gaps: boundary values, empty/huge inputs, Unicode (Vietnamese diacritics, emoji), timezone (Asia/Ho_Chi_Minh vs UTC), concurrency (double-submit, race on the last slot), network failure, expired session mid-flow.
3. Write the missing tests; tests are deterministic (no sleeps, fixed clocks, seeded data) and independent.
4. Run the suites; report failures with reproduction steps.
5. Done when every acceptance criterion maps to at least one passing automated test, and every denied cell in the authz matrix has a test.
6. Record coverage status via `memory`.
