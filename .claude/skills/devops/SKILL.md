---
name: devops
description: DevOps — use for repo setup, local dev environment, Docker, CI/CD, deployment, environments, secrets, monitoring/logging, or backups.
---

# DevOps

Goal: every merge to `main` is tested, scanned, and deployable with one action; production is observable and recoverable.

## Steps
1. Read `architecture/system.md` and the relevant ADRs (hosting, stack).
2. **Repo layout** (monorepo): `apps/web` (frontend), `apps/api` (Cargo workspace), `packages/` (shared TS: API client generated from OpenAPI, UI kit), `infra/`, `memory/`. Root `README` with one-command local setup.
3. **Local dev** — `docker compose` for Postgres, Redis, object storage (MinIO), mail catcher; seed script; `.env.example` documented, real `.env` git-ignored.
4. **Containers** — multi-stage Dockerfiles; distroless or slim runtime; non-root user; pinned base images.
5. **CI (GitHub Actions)** on every PR: format, lint, typecheck, unit + integration tests (Postgres service), build, `cargo audit`/`cargo deny`, `pnpm audit`/osv-scanner, `gitleaks`, SAST (CodeQL or Semgrep), Playwright smoke on preview build. Required checks block merge.
6. **CD** — staging auto-deploys from `main`; production on tagged release with manual approval; migrations run as a separate step before the new version takes traffic; rollback documented and tested.
7. **Secrets** — platform secret store; least-privilege credentials per environment; rotation procedure.
8. **Observability** — structured JSON logs (`tracing`), OpenTelemetry traces across web → API → DB, RED metrics per endpoint, uptime checks, alerts routed to the EM with runbooks. Error tracking (e.g. Sentry) on both apps with PII scrubbing.
9. **Backups** — automated daily Postgres backups + PITR, encrypted, restore tested monthly.
10. Done when a fresh clone reaches a running local stack with one documented command, and a PR cannot merge with a failing check.
11. Record via `memory` (ADR for hosting choice).
