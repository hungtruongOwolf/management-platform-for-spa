---
name: ship
description: Release — use when preparing to deploy or release to staging/production, writing a changelog, or verifying launch readiness.
---

# Ship

## Steps
1. **Readiness** — each item verified, not assumed:
   - [ ] All tickets in scope ticked in `memory/wiki/plans/`; acceptance criteria tests green in CI.
   - [ ] `secure` run on every change touching auth, data, payments, uploads, media; no open Critical/High.
   - [ ] `/pr-review` approved for every merged PR.
   - [ ] Migrations safe on live data and tested against a production-like snapshot.
   - [ ] Performance budgets met for changed journeys.
   - [ ] Feature flags set for risky features; kill switch known.
   - [ ] Rollback steps written for this release.
2. **Changelog & version** — generate from Conventional Commits; SemVer; user-facing release notes in Vietnamese.
3. **Deploy** — staging → smoke test → production (with EM approval).
4. **Smoke test production** — core journeys, login, a protected-content access check from a non-entitled account (must be denied).
5. **Monitor** for the first hours: error rate, latency, auth failures, 4xx/5xx spikes. Roll back on any regression past threshold.
6. Done when production smoke tests pass and metrics are stable through the monitoring window.
7. Record the release in `memory/log.md` and update `overview.md` via `memory`.
