---
name: breakdown
description: Planning — use when an approved spec/design must become tickets, or the EM asks for a plan, sprint, task list, or order of work.
---

# Breakdown

Turn a spec into **vertical slices**: each slice cuts through every layer (DB → API → UI) and delivers something demoable. The first slice is a tracer bullet — the thinnest end-to-end path that proves the architecture.

## Steps
1. Read the spec, `architecture/*`, `security/authz-matrix.md`, `design/*`.
2. Slice. Each ticket in `memory/wiki/plans/<slug>.md`:
   ```
   - [ ] S1 <title>  (size: S|M)
     Goal: …
     Acceptance: links to spec criteria it satisfies
     Touches: crates/modules/routes/components
     Tests: unit / integration / e2e cases, incl. negative authz cases
     Security: abuse cases + controls (or "none — reason")
     Depends on: S0
   ```
3. Order by dependency first, then by risk (riskiest unknowns early).
4. Size: every ticket S (≤ half a day) or M (≤ 1–2 days). Split any L.
5. Done when every acceptance criterion in the spec is covered by at least one ticket, and no ticket is L.
6. Show the plan to the EM; index + log via `memory`. Next: `/build S1`.
