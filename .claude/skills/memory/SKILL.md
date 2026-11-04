---
name: memory
description: Project memory wiki in memory/ — ingest a new source, answer a question from project knowledge, lint the wiki, or record decisions and progress after meaningful work.
---

# Memory (LLM Wiki)

`memory/raw/` is immutable truth, `memory/wiki/` is compiled knowledge you own, `memory/index.md` is the catalog, `memory/log.md` is append-only history. The wiki is a **compounding artifact**: every operation leaves it more accurate and better linked than before.

## Page conventions
- Frontmatter: `title`, `updated: YYYY-MM-DD`, `sources: [raw/...]`.
- Vietnamese prose, English technical terms. Link related pages with relative markdown links.
- One concept per page, co-located: its definition, rules, and caveats under one heading. Update an existing page before creating a new one.
- A claim that came from a source cites it in `sources`. Unverified claims are marked `(chưa xác minh)`.

## ingest — a new source arrives (EM message with requirements, meeting notes, a doc, research output)
1. Save it verbatim (or a faithful summary when it is chat) as `memory/raw/YYYY-MM-DD-<slug>.md`.
2. List every wiki page the source touches; update each one, create pages for genuinely new concepts. Flag contradictions with existing pages inline as `> ⚠️ Mâu thuẫn:` and raise them with the EM.
3. Update `memory/index.md` and append to `memory/log.md`.
Done when every fact in the source is either reflected in a wiki page or deliberately judged irrelevant.

## query — a question about the project
1. Read `memory/index.md`, then the relevant pages; follow links one hop.
2. Answer with citations to wiki pages. If the wiki lacks the answer, say so rather than guessing.
3. If the answer is a reusable synthesis (a comparison, an analysis), file it as a new wiki page and index it.

## lint — on request, or every ~10 log entries
Check: contradictions between pages; pages whose `updated` is older than newer sources on the same topic; orphan pages missing from the index; index lines pointing at missing pages; concepts mentioned on several pages without their own page; ADRs whose status no longer matches reality. Fix what is mechanical; list the rest for the EM.

## record — after meaningful work (a decision, a finished slice, a design change)
1. Update the affected wiki pages (spec status, plan checkboxes, architecture changes, new ADR).
2. Update `overview.md` stage if it moved.
3. Append to `memory/log.md`: `## [YYYY-MM-DD] record | <title>` + 1–5 bullets, ending with the next step.
