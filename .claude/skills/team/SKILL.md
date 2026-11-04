---
name: team
description: Project status — use when the EM asks where the project stands, what is next, or which team skill to use.
---

# Team map

1. Read `memory/index.md`, `memory/wiki/overview.md`, and the last 10 entries of `memory/log.md`.
2. Determine the current stage from what exists in the wiki (no vision → `idea`; vision but no spec → `spec`; and so on down the table).
3. Reply to the EM in Vietnamese with: the current stage, what is done, what is blocking, and the one skill to run next with the reason.

| Stage | Skill | Role in a real team | Output in `memory/wiki/` |
|---|---|---|---|
| Discovery | `/idea` | PM + tech lead discovery session | `product/vision.md`, `personas.md`, `glossary.md` |
| Requirements | `/spec` | PM writes PRD | `product/specs/<slug>.md` |
| System design | `/architect` | Staff/principal engineer | `architecture/system.md`, `security/threat-model.md`, ADRs |
| Backend design | `/backend-design` | Backend lead | `architecture/backend.md`, `architecture/data.md`, `security/authz-matrix.md` |
| Frontend & UX | `/frontend-design` | Design engineer + UX | `design/design-system.md`, `architecture/frontend.md` |
| Planning | `/breakdown` | Tech lead sprint planning | `plans/<slug>.md` |
| Implementation | `/build` | Software engineer | code + tests, ticked plan |
| Testing | `/qa` | QA / SDET | test suites, coverage of acceptance criteria |
| Code review | `/pr-review` | Peer reviewer | review report |
| Security | `secure` (auto) or `/secure` | AppSec engineer | findings, `security/*` |
| Performance | `/perf` | Performance engineer | budgets, measurements |
| Infra & CI | `/devops` | DevOps / SRE | CI pipeline, Docker, observability |
| Release | `/ship` | Release manager | release checklist, changelog |
| Knowledge | `memory` (auto) | Tech writer | index, log, wiki pages |

Complementary installed skills worth reaching for: `/mattpocock-skills:grilling` (stress-test a plan), `/mattpocock-skills:prototype` (throwaway UI/logic prototype), `/mattpocock-skills:diagnosing-bugs` (hard bugs), `/mattpocock-skills:tdd`, `/code-review` and `/security-review` (built-in diff reviews).
