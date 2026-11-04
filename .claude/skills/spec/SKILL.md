---
name: spec
description: PRD writing — use when a feature needs requirements, user stories, or acceptance criteria before design or code, or when the EM describes what a feature should do.
---

# Spec (PRD)

Input: a feature name from the EM. Output: `memory/wiki/product/specs/<slug>.md`, precise enough that a different engineer could build and test it without asking you.

## Steps
1. Read `wiki/product/vision.md`, `personas.md`, `glossary.md`, and any existing spec it relates to.
2. Ask the EM only about gaps that change behaviour; propose a default for each.
3. Write the spec with these sections:
   - **Goal** — one paragraph, linked to the vision metric.
   - **Personas & roles** involved.
   - **User stories** — `As a <persona>, I want <capability>, so that <outcome>.`
   - **Acceptance criteria** per story, Given/When/Then, including **negative and abuse paths** (unauthenticated, wrong role, someone else's resource, invalid input, duplicate submit, network failure).
   - **UX notes** — key screens and states (loading / empty / error / success), link to design pages if any.
   - **Data** — entities touched, personal/sensitive fields, retention.
   - **Non-functional** — performance budget, security & privacy requirements, accessibility (WCAG 2.2 AA), i18n (vi first), supported devices.
   - **Out of scope** — explicit.
   - **Open questions** — owner and due date.
   - **Status**: draft → approved.
4. Done when every story has at least one positive and one negative acceptance criterion, and each criterion is testable by an automated test.
5. Ask the EM to approve; mark `Status: approved`. Index + log via `memory`.
6. Next: `/architect` (first feature or architectural change) or `/breakdown` (feature fits existing architecture).
