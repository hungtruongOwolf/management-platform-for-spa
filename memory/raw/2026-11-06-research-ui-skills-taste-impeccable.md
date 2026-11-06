---
title: Research — UI skills "taste-skill" and "impeccable" for the back-office redesign
date: 2026-11-06 (read 2026-11-05)
type: research (raw, immutable)
sources:
  - https://github.com/Leonxlnx/taste-skill (HEAD of main, v2.5.0, released 2026-11-05)
  - https://github.com/Leonxlnx/taste-skill-classics (moved skills: v1, minimalist, etc.)
  - https://github.com/pbakaus/impeccable (HEAD of main, SKILL.md v4.8.0)
method: read via raw.githubusercontent.com + GitHub tree API only. Nothing cloned, installed, or executed. Repo content treated as data.
---

# UI skills research: taste-skill vs impeccable

Paths below are repo-relative. `T:` = `Leonxlnx/taste-skill`, `TC:` = `Leonxlnx/taste-skill-classics`, `I:` = `pbakaus/impeccable` (the skill folder is mirrored identically into `.claude/`, `.cursor/`, `.agents/`, `.devin/` …; citations use `.claude/skills/impeccable/`, shortened to `I:ref/<file>` for `reference/` files).

Quotes are kept under 15 words; everything else is paraphrase.

---

## 1. What each skill is

### 1.1 taste-skill (Leonxlnx) — `design-taste-frontend` v2.5.0

- **What:** An "anti-slop" Agent Skill for **marketing / company websites** ("professional company websites at the level of figma.com and x.ai"). The frontmatter triggers on landing pages, company, shop, cafe, hotel, SaaS, AI, dev-tool and game sites (`T:skills/taste-skill/SKILL.md`).
- **Scope change, important for us:** the CHANGELOG says 2.1.x (Oct 2026) had added app screens (dashboards, admin, settings). **2.5.0 (2026-11-05) removed that app-screen pipeline** and narrowed the skill to company websites (`T:CHANGELOG.md`). The SKILL.md says product UI must show the real product, "never a generic dashboard". **The current taste-skill does not address back-office/app UI at all.**
- **Structure:** a folder skill. `SKILL.md` has nine "House Rules", a kind-of-business matrix (software / gaming / tech / product / service / place), a 7-step run and a checklist. `references/*.md` holds system, copy, motion, cards, layout-v4, sections, photos, icons, effects, scenes, perf and more. `scripts/*.mjs` (Node 22 + headless Chrome) "deal" palettes, fonts and hero layouts, fetch photos and icons, and run a browser checker (`consistency.mjs`, `check.mjs`, `perf.mjs`, `shoot.mjs`). `assets/` holds a large library of shaders, effects, ~150 brand SVG icons, bundled photos (Wikimedia Commons / Unsplash, each with its own licence), UI fragments and a starter HTML file.
- **How it's used:** `npx skills add … --skill design-taste-frontend`, a Claude Code plugin marketplace, or `install.sh`. The agent copies `assets/` into the project, starts from `assets/starter.html`, picks tokens with the scripts ("deal, never choose by taste") and must pass the checker at 1440/1024/768/390 px.
- **License:** MIT, © 2026 Leonxlnx (`T:LICENSE`). Bundled photos and brand icons carry third-party licences/trademarks (`T:README.md`, `T:skills/taste-skill/references/photos.md`).
- **Classics repo** (`TC:`, MIT, © 2026 Leonxlnx): `taste-skill-v1` (dial-driven: DESIGN_VARIANCE 8, MOTION_INTENSITY 6, VISUAL_DENSITY 4), `taste-skill-v2-experimental`, `minimalist-skill`, `soft-skill`, `brutalist-skill`, `gpt-tasteskill`, `stitch-skill`. These are opt-in (`disable-model-invocation: true`). v1 is the version most people mean by "taste-skill", and it does mention dashboards (`TC:skills/taste-skill-v1/SKILL.md`).

### 1.2 impeccable (Paul Bakaus) — v4.8.0

- **What:** "A design vocabulary for coding agents." It is one skill with ~24 named sub-commands, plus a deterministic **anti-pattern detector with 62 rules** that needs no LLM (`I:README.md`). It started from Anthropic's `frontend-design` skill (`I:README.md` § Attribution).
- **Scope:** explicitly covers websites, landing pages, **dashboards, app UI, components**, native iOS/Android and design systems (`I:.claude/skills/impeccable/SKILL.md` frontmatter).
- **Key concept: modes**, which are chosen per surface by its success condition (`SKILL.md` § Modes):
  - **Persuade**: landing, marketing, pricing.
  - **Operate**: app UI, dashboards, admin; "scanability and consistency come first".
  - **Read**: docs and articles.
  - **Experience**: portfolios and galleries.
  - Ambition scales with the mode. In Operate it shows as precision and restraint.
- **Structure:** `SKILL.md` (router + principles) → `reference/*.md`:
  - one playbook per command: shape, critique, audit, polish, harden, clarify, adapt, distill, quieter, bolder, colorize, typeset, layout, animate, delight, overdrive, onboard, optimize, live, generate, init, document, extract;
  - mode files: `mode-operate.md`, `mode-persuade.md`, `mode-read.md`, plus `operate.md`;
  - the mandatory `craft-floor.md`, `new-work.md`, and native `ios.md` / `android.md`.
  - `scripts/` holds the `impeccable` launcher (context loader + detector), a live-mode browser overlay, a font index and telemetry. `.claude/agents/*` holds sub-agents (finish-reviewer, documenter, asset-producer, manual-edit-applier), and there are design hooks that scan edited UI files.
- **Project artifacts it writes:** `PRODUCT.md` (via `init`) and `DESIGN.md` (via `document`), plus a `.impeccable/` state folder.
- **How it's used:** `npx impeccable install`, then `/impeccable init`, then `/impeccable <command> <target>` (for example `critique the onboarding flow`). Detector: `npx impeccable detect` (exit code 2 = findings) or a Chrome extension.
- **Operational caveats** (`I:README.md`): telemetry is **on by default**: it fetches visual directions from impeccable.style and records anonymous choices (off with `DO_NOT_TRACK=1`). Hooks can run even when the model's commands are denied. Live mode can run the `package.json` validation script and, in some configurations, a CLI agent "with approvals and sandboxing disabled".
- **License:** Apache-2.0, © 2025 Paul Bakaus (`I:LICENSE`). No root NOTICE file (404).

---

## 2. Digest of the actual rules

### 2.1 impeccable

**Craft floor (applies to every UI edit; `I:ref/craft-floor.md`)**
- Contrast: body and placeholder text ≥ 4.5:1, large text ≥ 3:1. Secondary text on coloured surfaces is tinted from the hue, "never gray".
- Shadows need a vertical offset and a soft blur. A zero-offset coloured halo counts as decoration.
- Spacing: tight within groups, generous between groups, more space above a heading than below it.
- Type: body measure 65–75ch, display ≤ 6rem, tracking ≥ −0.04em, balanced headings, obvious scale steps. Run the real copy at every breakpoint.
- Motion: one authored moment, exponential ease-out, starting from an already-visible default state.
- States: hover, disabled, loading, error, empty, plus visible keyboard focus.
- Theme the browser defaults too: selection, caret, scrollbars, focus ring, underline offset, tabular numerals. The file calls this the "cheapest signal" that a page was built.
- Copy uses the product's own language. Errors name the problem and the recovery.
- **Refuse list** (category defaults, which the brief can earn back):
  - same-size icon+heading+text card grids as the page structure; **nested cards are always wrong**;
  - the hero-metric template (big number, small label, supporting stats, accent);
  - **kicker/eyebrow above a heading: a hard ban, "no brief earns it back"**;
  - section numbers 01/02/03;
  - a modal where neither interruption nor protected focus is needed;
  - gradient text; decorative glass or blur;
  - **coloured `border-left/right` wider than 1px on cards, list items, callouts or alerts**;
  - hard offset shadows;
  - sparklines and progress rings standing in for content;
  - monospace as a costume;
  - emoji or unicode glyphs as icons;
  - choosing light or dark by category rather than by the use scene.

**Operate mode (`I:ref/operate.md`, `I:ref/mode-operate.md`)**
- Product slop test: "Familiarity is a feature". The failure mode is strangeness without purpose, not flatness. The tool should disappear into the task.
- Type:
  - one family is often enough, with no display/body pairing needed;
  - **fixed rem scale, not fluid**;
  - **ratio 1.125–1.2** between steps;
  - prose 65–75ch; tables at 120ch+ are acceptable.
- Colour:
  - Restrained by default (tinted neutrals + one accent); one surface may go Committed;
  - standard state vocabulary: hover, focus, active, disabled, selected, loading, error, warning, success, info;
  - **accent only for primary actions, current selection and state**, never decoration;
  - a second neutral layer for sidebars, toolbars and panels.
- Components:
  - **all 7 states** (default, hover, focus, active, disabled, loading, error) on every interactive component, "Don't ship with half of these";
  - skeletons rather than mid-content spinners;
  - empty states teach the interface;
  - one consistent affordance vocabulary across screens;
  - overlays escape `overflow` containers via `<dialog>`, the popover API, `position: fixed` or a portal.
- Motion: **150–250 ms**, state only ("Motion conveys state, not decoration"), no orchestrated page-load sequences.
- Avoid:
  - display fonts in labels, buttons or data;
  - reinvented scrollbars or controls;
  - full-saturation accents on inactive states;
  - modal-first design ("Modals are usually laziness").
- Allowed: system fonts, standard navigation (top bar + side nav, breadcrumbs, tabs, command palette), high density, "consistency over surprise".
- A brand "world" contributes only **type, palette, density and one signature move**. It never supplies layout, navigation or controls. Red marks critical and amber marks degraded. "Costumes" are banned: function-key bars, instrument bezels, ledger paper as a table (`mode-operate.md`).

**Typography (`I:ref/typeset.md`)**
- Fewest roles and families. Hierarchy comes from size, weight, space and tone together.
- Body floor 16px unless a dense role justifies less. Prose measure 45–75ch.
- Light-on-dark text gets slightly more leading, a touch more tracking, and sometimes +1 weight.
- Tabular numerals wherever content benefits.
- `text-wrap: balance` on headings, `pretty` on body. Use paragraph spacing or indent, not both.
- Load only the weights in use, with metric-compatible fallbacks.
- **Reflex-font list** (Persuade/Experience only; Operate may use workhorse faces; `I:ref/new-work.md`): Fraunces, Playfair Display, Cormorant, **Lora**, Crimson, Newsreader, Syne, Space Grotesk/Mono, IBM Plex, Inter-as-display, DM Sans/Serif, Outfit, Plus Jakarta Sans, Instrument Sans.

**Layout (`I:ref/layout.md`)**
- A documented spacing scale with a 4-unit base preferred. Group by proximity before containers. Use `gap` for sibling rhythm. "Variation is not a goal by itself."
- Squint test for hierarchy. DOM, focus and visual order must match.
- Operate mode needs predictable structure and stable density. Responsive changes are structural (reorder, collapse, reflow).

**Colour (`I:ref/colorize.md`, `I:ref/new-work.md`)**
- Colour is defined by role (canvas, text, action, focus, selection, borders, status, data), not as a "swatch bag".
- Strategies: Restrained / Committed (30–60% of the surface) / Full palette / Drenched.
- In Operate, colour encodes action, selection, status and wayfinding, and "rarity gives accents their force".
- OKLCH for new palettes. Reduce chroma near white and black.
- Tint neutrals only when the brand hue creates real cohesion.
- Dark mode is designed separately, not inverted. Prefer explicit colours over chained translucent overlays.
- Never rely on colour alone. Contrast: text 4.5:1, large text 3:1, controls/icons/focus 3:1.
- **Calibration "AI looks" to avoid unless the brief calls for them:**
  - **"Warm cream ground, high-contrast serif, terracotta or red accent"**;
  - near-black + neon glow;
  - broadsheet hairlines + italic serif + mono labels;
  - neo-brutalist.
- "The brief wins": pinned palettes and fonts are honoured even against these warnings (`SKILL.md` § How to Design).

**Motion (`I:ref/animate.md`)**
- Durations: 100–150 ms feedback, 150–300 ms state change, 300–500 ms overlay/view, 500–800 ms authored entrance. Exits are faster than entrances.
- Easing `cubic-bezier(0.16,1,0.3,1)`; no bounce or elastic.
- Never animate layout properties.
- Every animation needs a `prefers-reduced-motion` path. Reduced motion means fewer and gentler, not necessarily none. `audit.md` flags a global `0.01ms` kill that removes useful feedback.
- Banned as a core idea: fade-and-rise, hover lift, parallax, scroll reveals, staggering every section.

**Hardening / states / forms (`I:ref/harden.md`, `I:ref/clarify.md`)**
- Test extremes: names of 100+ characters, emoji, large numbers, 1000+ rows, offline, slow, 4xx/5xx, ten rapid submit clicks.
- `min-width: 0` on flex and grid children. Truncate, clamp or wrap deliberately.
- 16px input text on mobile (iOS zooms focused inputs under 16px). Test at 200% zoom.
- Use `Intl` for dates, numbers and currency.
- Errors:
  - map 400 → inline validation, 401 → login, 403 → permission message, 404 → not-found, 429 → rate limit, 500 → generic + support;
  - an error says what failed, why, and how to recover; don't lead with internal codes.
- Forms:
  - persistent visible labels ("Placeholders are examples, not labels");
  - inline errors near the field, announced accessibly;
  - preserve input after an error;
  - disable submit while submitting.
- Empty states distinguish first use / no results / filtered / no permission / failure. Loading names the real operation.
- Destructive actions name the object and its consequence. "Prefer undo over confirmation when recovery is safe." Confirm buttons repeat the verb, never Yes/OK.
- Permissions distinguish no-view, no-edit and read-only, and explain why.
- Debounce search ~300 ms. Paginate or virtualize large data.

**Responsive (`I:ref/adapt.md`)**
- Breakpoints 320–767 / 768–1023 / 1024+. Touch targets 44×44. Mobile body text ≥ 16px.
- Tables → cards on mobile. Navigation goes drawer (mobile) → compact (tablet) → full (desktop); bottom navigation for apps.
- Use `pointer`/`hover` media queries; never rely on hover for function. Use safe-area insets.

**Simplify (`I:ref/distill.md`)**
- One primary goal and one primary action per view.
- 1–2 colours + neutrals, one family, 3–4 sizes, 2–3 weights. Remove borders, shadows and backgrounds that don't serve hierarchy.
- Skip cards when spacing does the job. Prefer inline editing over modals.

**Review tooling**
- `I:ref/audit.md`: five dimensions scored 0–4 (/20): a11y, performance, theming, responsive, implementation integrity. Severity P0 blocking, P1 major/WCAG AA, P2 minor, P3 polish. Touch target 44×44, with WCAG 2.5.8 minimum 24×24.
- `I:ref/critique.md`:
  - Nielsen's 10 heuristics, each scored 0–4 (/40);
  - cognitive-load checklist (≤ 4 items per group, ≤ 4 visible options, working memory ≤ 4);
  - personas Alex (power user), Jordan (first-timer), Sam (a11y), Riley (stress tester), Casey (mobile);
  - "would a user contact support?" means P1+.
- `I:ref/polish.md` triage order: broken tasks → missing states → flow/hierarchy/drift → visual/motion → cleanup.

**Detector, the 62 rules** (`I:tests/oracle/golden/detect-list-rules-json.json`; 31 "slop" + 31 "quality").
- Slop: side-tab, border-accent-on-rounded, overused-font, flat-type-hierarchy (< 1.25× heading/body), gradient-text, ai-color-palette, **cream-palette**, nested-cards, monotonous-spacing, bounce-easing, pulsing-dot, dark-glow, radial-halo, marquee, icon-tile-stack, hero-eyebrow-chip, **kicker-above-heading**, numbered-section-labels, em-dash-overuse, marketing-buzzword, oversized-h1, extreme-negative-tracking, gpt-thin-border-wide-shadow, codex-grid-background, …
- Quality: low-contrast, gray-on-color, line-length (> ~80ch), tight-leading (< 1.3), tiny-text (< 12px), undersized-ui-text (< 11px), all-caps-body, wide-tracking (> 0.05em body), skipped-heading, heading-rhythm, text-overflow, clipped-overflow-container, design-system-font/color/radius/font-size (off DESIGN.md), content-hidden-at-rest, …

### 2.2 taste-skill v2.5 (marketing-site rules)

- **System** (`T:skills/taste-skill/references/system.md`):
  - spacing 4/8/12/16/24/32/48/64/96/128; 12-column grid with a 24px gap;
  - body text ≥ 16px, line-height 1.5–1.8, ink ≥ 7:1; muted ≥ 4.5:1;
  - "Never 14px paragraphs"; nothing under 13px; labels 13px/500, **never tracked capitals or small caps**; sentence case; lines ≤ 62 characters; one size per element kind;
  - interface 90–95% neutral; one accent at most, under 3% of a screen outside the primary button, focus and links; ink is never `#000`; **never violet/lavender** (OKLCH hue 270–330 is remapped);
  - buttons 44px; exactly one filled button per view; **gradient, ghost, slanted or shadowed buttons banned**;
  - fixed nav that never changes on scroll (no blur, shrink or hide);
  - forms: fields 48px, labels 13px, text 16px, ≤ 6 fields, validation on submit, "No fake success states or modals".
- **Cards** (`…/references/cards.md`):
  - hairline border at 8–14% ink, no shadow at rest; padding ≥ 24px;
  - **two nesting levels max**; one height per row;
  - only linked cards lift (2px); fragment text never under 11px.
- **Motion** (`…/references/motion.md`):
  - tokens: hover in 140 ms, out 200 ms, press 90 ms, panel 200 ms; easing `cubic-bezier(.2,.8,.2,1)`; no bounce or overshoot;
  - lift 2px, press 1px + scale .985;
  - nothing animates height, max-height or padding;
  - non-tech pages: hover layer only, with nothing moving on load, by time or on scroll;
  - bans: scroll reveals, parallax, tilt, magnetic buttons, carousels, timed tabs, marquees, glitter, scrambled text;
  - anything still moving after 5 s gets a pause button; reduced motion shows the finished frame.
- **Copy** (`…/references/copy.md`):
  - buttons are a verb + object, 1–5 words; never "Learn more", "Discover", "Explore", "Click here";
  - headings contain a noun, with no questions or colon headings and no exclamation marks;
  - filler words banned (seamless, effortless, unlock, elevate, robust, powerful);
  - invent no reviews, statistics or customers; no placeholders or "sample" tags.
- SKILL.md House Rules (photos lead, one section per screen, shaders only for tech, address redaction) and the hero/section machinery are landing-page-only.

### 2.3 taste-skill classics (for completeness; `TC:`)

- **v1** (`TC:skills/taste-skill-v1/SKILL.md`):
  - Tailwind-first; one global icon `strokeWidth`; emoji banned;
  - avoid Inter (prefer Geist / Satoshi / Cabinet Grotesk); **"serif fonts are banned for dashboards"**;
  - one accent with saturation < 80%; no purple/blue "AI" look; Zinc/Slate neutrals; no `#000`;
  - at density > 7, cards are replaced by `border-t` / `divide-y`, with monospace numbers;
  - loading = skeletons, plus empty and inline error states; `:active` feedback;
  - forms: label above, error below, `gap-2`;
  - AI tells: neon glows, gradient text, "3 equal cards" rows, John Doe, 99.99%, default shadcn.
  - **But** v1 also pushes motion intensity 6, spring physics, magnetic buttons, perpetual shimmer/pulse and a "bento" SaaS dashboard paradigm with `rounded-[2.5rem]`. All of that is wrong for an operational tool.
- **minimalist-skill** (`TC:skills/minimalist-skill/SKILL.md`): editorial monochrome, 1px `#EAEAEA` borders, radius ≤ 12, near-zero shadows, uppercase pastel pill badges; bans Inter/Roboto/Open Sans and Lucide/Feather icons. It has no table rules and includes scroll-entry reveals.

---

## 3. Agreement and conflict

**Agree (strong signal; both sources):**
- **No nested cards** (I:craft-floor, I:distill; T:cards "two levels only"). Use spacing/dividers rather than boxes when density is high (I:distill; TC:v1).
- **No gradient text, glow, glassmorphism-as-decoration, neon "AI palette"** (I:craft-floor + detector; T:system; TC:v1).
- **One accent, used for action/selection/state only** (I:operate "accent only for primary actions…"; T:system one accent < 3% of screen).
- **Reduced-motion path for everything; transform/opacity only; no layout-property animation; no bounce** (I:animate; T:motion; TC:v1).
- **No scroll reveals / staggered fade-ins / parallax / carousels / marquees** (I:animate + detector `marquee`; T:motion "Never").
- **Sentence case; no tracked-caps labels or eyebrows** (I:craft-floor ban + detector `kicker-above-heading`, `all-caps-body`; T:system "never tracked capitals or small caps").
- **Body ≥ 16px on web/mobile inputs; contrast ≥ 4.5:1; 44px targets** (I:typeset/harden/adapt; T:system).
- **Skeletons over spinners; real empty/error/loading states; labels above inputs, errors inline** (I:operate/harden/clarify; TC:v1).
- **Real content only, no placeholder names or fake stats; verb+object buttons; filler-word bans** (I:clarify + detector `marketing-buzzword`; T:copy).
- **Coloured side-stripe accents are a tell**: impeccable `side-tab` / `border-left > 1px`; taste does not name it but bans "colored bars behind words".

**Conflict / different emphasis:**
| Topic | impeccable | taste-skill | Resolution for us |
|---|---|---|---|
| Scope | Has an explicit **Operate** mode for apps | v2.5 is websites only; v1 treats dashboards as "bento" showpieces | Follow impeccable Operate; use taste only for visual hygiene bans |
| Motion amount | Operate: 150–250 ms, state only | v2.5 non-tech: hover layer only; v1: intensity 6, springs, perpetual loops | impeccable Operate + taste v2.5 hover tokens; reject v1 motion |
| Hover lift | Bans "hover lift" as the *core idea* | Linked cards lift 2px (standard) | Small lift on clickable cards is OK as feedback, not as decoration |
| Fonts | Operate: system/workhorse faces fine; Lora is on the reflex list for Persuade only | v1: no serif in dashboards; minimalist allows serif headings | Keep Be Vietnam Pro for all UI; limit Lora to brand moments (see §4) |
| Fluid type | Operate: fixed rem scale | Uses `clamp()` for headings on sites | Fixed scale in-app |
| Cream ground | Detector `cream-palette` + the "cream + serif + red accent" calibration look; "the brief wins" | No cream ban; warm near-black ink recommended | EM pinned cream/maroon/gold from the printed menu, so it stays. Know that it reads as a known AI look and differentiate by craft, not by changing brand |
| Modals | "Modals are usually laziness"; inline first | "No fake success states or modals" (site forms) | Keep bottom sheets for focused mobile tasks (M3/HIG); prefer inline editing on desktop |
| Button style | Doesn't ban gradients on buttons explicitly (bans decoration generally) | Gradient/ghost/shadowed buttons banned | Reconsider the "lacquer" gradient + `shadow-primary` on primary buttons |
| Mechanism | LLM guidance + deterministic detector + hooks + telemetry | Deterministic "dealing" scripts + headless checker, heavy asset library | Neither runtime is needed; we already have our own Playwright layout checks |

---

## 4. Applying this to Viên Phúc Y Đường (dense operational back-office)

Context: front desk (`/quay`), customer list/profile, appointment calendar, packages, reports, staff/payroll. Users are on the app 8–10 h/day on desktop, tablet and phone. Current system: `memory/wiki/design/design-system.md` (cream `#FAF4E8` ground, white cards, maroon chrome sidebar, gold accent, Be Vietnam Pro UI + Lora page titles, 15/22 base). **Surface mode = Operate** for every in-app screen; login is borderline (still Operate: a task, not persuasion).

### 4.1 Prioritised checklist (fits)

**P0: correctness and accessibility (keep or verify)**
1. Every interactive component has all 7 states, plus empty and permission states for data components (I:operate; our design-system already mandates this, so audit for gaps in newer screens: appointments calendar, staff/payroll).
2. Contrast pairs ≥ 4.5 / 3:1 in both themes. We already enforce this via `contrast.test.ts`, so extend it to new tokens only.
3. Input text ≥ 16px on phones (iOS zoom). **Our base is 15px**, so verify that inputs are 16px under 640px (I:harden; T:system).
4. Errors map by status code: 401 → login, 403 → permission message (no-view vs read-only), 404 → not-found, 409 → conflict explanation (I:harden). Error copy says what failed + how to recover; no raw codes (I:clarify).
5. Destructive actions (cancel package, refund, close payroll): name the object + consequence; the confirm button repeats the verb ("Hủy gói", not "OK"); undo where safe (I:clarify). This already matches ReasonDialog, so check the wording.
6. Double-submit protection and preserved input on error for every form (I:harden).
7. Overlays escape `overflow` containers (dropdowns inside `.table-wrap`, calendar popovers) via `<dialog>`/popover/portal (I:operate; detector `clipped-overflow-container`).
8. Stress data: 100-character customer names, 1000+ rows, ten rapid clicks, offline (I:harden). Our e2e layout sweep covers overflow; add a long-data seed for calendar and payroll.

**P1: Operate-mode hierarchy and density**
9. **Type scale ratio 1.125–1.2, fixed (no clamp)** (I:operate). Ours (12/13/15/17/20/24/30) is roughly 1.13–1.25, so it fits. Keep it fixed and don't add fluid headings.
10. **Remove uppercase tracked eyebrows above h1** (`.eyebrow`, 12px, 0.14em). This is the one hard ban both skills share (I:craft-floor "no brief earns it back"; detector `kicker-above-heading`; T:system). The breadcrumb + h1 already carry the location. Sidebar group labels (11px caps) are navigation, not a kicker, but consider sentence-case 12–13px muted labels.
11. **Replace coloured left stripes on content items**: the package row's "gold left border" is a `side-tab` tell (I:craft-floor `border-left > 1px`). Status is already carried by the badge + "Còn X/Y buổi" bar, so drop the stripe or use a full 1px tinted border. The selected-nav 3px bar is a navigation indicator: acceptable but borderline. A full-row tint + icon colour is enough.
12. **Accent = action, selection, state only** (I:operate). Audit decorative gold: corner ornaments (`.ornament`), the dot texture on `bg` (`--texture` 22px), gold rules, the gold icon circles on StatCards. Keep the brand in the **chrome** (sidebar/header) as the single "signature move" (I:mode-operate) and pull decorative gold out of the work area.
13. **StatCard row = the hero-metric template** (big number + small label + icon circle + delta). For a front-desk "today" summary it is legitimate content, but drop the icon tiles (detector `icon-tile-stack`), keep number + label + delta text, and make each number link to its filtered list.
14. **No nested cards**: check the customer profile (card → package rows styled as cards?), reports and payroll. Use `divide-y` rows inside one card (I:distill; T:cards; TC:v1 density > 7).
15. **Tables**: tabular numerals (done), right-aligned numbers (done), sticky header (done), sortable headers as standard controls, 120ch+ fine. Row height ~48px is fine for touch tablets; offer no "compact" mode unless asked. Keep the second neutral layer (sidebar/toolbars) distinct from content (I:operate).
16. **Calendar** (appointments): standard controls only, with no "costume" (I:mode-operate bans instrument/ledger costumes). Red = conflict/critical, amber = warning, used only where meaningful (I:mode-operate). Show one real state worth seeing (overbooked slot, empty day with next action).
17. **Empty states teach**: "no customers yet → Ghi lượt khách", "no results for filter → clear filter", "no permission → ask owner". Distinguish the five empty kinds (I:clarify). Our EmptyState uses the logo + one sentence; add the next action per kind.
18. **Motion budget 150–250 ms, state-only** (I:operate). Our 120/200/320 ms tokens fit. Questionable items:
    - `CountUp` on StatCards is decorative (it delays reading the number); I:operate says no load choreography. Recommend removing it or limiting it to value *changes*.
    - `.page-enter` fade+6px on every navigation: acceptable as continuity, but drop it if it feels like "watching it load".
    - Skeleton shimmer: fine.
    - Hover lift on `.card-link`: keep at 2px as feedback.
19. **Reduced motion**: we currently kill *all* transitions. I:audit flags a global `0.01ms` kill that removes useful feedback. Keep opacity/colour state changes under `reduce`.
20. **Copy**: verb + object buttons ("Bán gói", "Ghi lượt khách"), no "Xem thêm" generic links, labels persistent (not placeholder-only), consistent terminology (we have a glossary), no marketing filler (T:copy; I:clarify).
21. **Theme browser defaults**: `::selection` (done via accent-soft), caret colour, scrollbars in chrome/sidebar, `accent-color` (done), underline offset (I:craft-floor).

**P2: brand vs. "AI look" calibration**
22. Cream ground + serif (Lora) + red/maroon accent is literally impeccable's first listed "common AI look" and trips the `cream-palette` detector rule. **Do not change the brand** (EM pinned it from the printed menu; "the brief wins"). Reduce the generic read instead:
    - limit Lora to the logo/login/welcome band, and set page titles in Be Vietnam Pro 600 (I:operate "one family"; TC:v1 "no serif in dashboards"); **needs EM decision**;
    - drop the 22px dot texture and the corner ornaments in the work area;
    - keep the chrome rich, and let the work area be quiet with sharp hierarchy.
23. Primary-button "lacquer" gradient + coloured shadow: taste bans gradient/shadowed buttons, and impeccable treats zero-offset coloured halos as decoration. Recommend a flat maroon fill with a 1-step darker hover.
24. Radius consistency: one radius scale (ours 6/10/14/full) and one card style (T:system; detector `design-system-radius`).

**P3: mobile (already largely aligned)**
25. Tables → cards, filters → sheet, bottom tab bar, 44px targets, safe-area insets, no hover-only, `pointer` media queries (I:adapt matches our round-4 rules). Gap: check landscape phones/tablets at the front desk (I:adapt).

### 4.2 Rules NOT to apply here (marketing/landing-page only)

- taste-skill v2.5 in its entirety as a *process*: kind-of-business dealing, the starter HTML, "one section per screen", hero layouts, photos-lead, real brand-icon walls, shader grounds, scroll scenes and moments, the footer/credits line, address redaction, word budgets per screen, ≤ 2 hero actions, the newsletter rule.
- taste "one filled button per view": in a dense app a view can legitimately have one primary per *region* (cart pay button + dialog primary). Use one primary per task area instead.
- taste "FAQ answers always open, no accordion" and "≤ 6 form fields": site-contact-form rules. Our customer and payroll forms are longer; use grouping + sticky actions instead.
- taste fixed-nav "nothing else on the page is sticky": we need sticky table headers, sticky actions and the tab bar.
- impeccable Persuade/Experience guidance: reflex-font bans for display type, Committed/Drenched colour, the "one authored moment" focal entrance (500–800 ms), `bolder` / `overdrive` / `delight` commands, the 7-candidate "world" concept rounds and comps, live-mode variant cycling.
- taste v1 creative arsenal: magnetic buttons, spring physics, perpetual pulse/shimmer, staggered reveals, bento grids at `rounded-[2.5rem]`, liquid glass, asymmetric/high-variance layouts, "no 3 equal cards" (a 3–4-up stat row is fine in an app).
- minimalist-skill: uppercase pastel pill badges, scroll-entry reveals, low-opacity background imagery, the Lucide/Feather ban (irrelevant; we use our own sprite).
- The detector rules that target heroes (`oversized-h1`, `hero-eyebrow-chip`, `blinking-cursor`, `radial-halo`, `marquee`) are mostly non-issues for us. Treat `cream-palette` as a known, accepted brand exception.

---

## 5. Vendoring as a project skill: recommendation only (nothing copied)

**taste-skill:** do **not** vendor.
- MIT licence is compatible: keep the copyright + licence text with any copy.
- But v2.5 is website-only and depends on Node scripts, a headless browser and a large asset library. The bundled photos (Wikimedia/Unsplash) and brand SVGs carry **separate third-party licences and trademarks**, so copying `assets/` is a licensing liability.
- The useful parts (§2.2 system/motion/copy bans) fit in ~15 lines of our own `design-system.md`, with attribution.
- The classics v1 contains motion/dashboard guidance that contradicts our Operate needs.

**impeccable:** vendoring a **curated, text-only subset** is legally fine and useful, with conditions.
- Apache-2.0 is compatible with a private repo. Section 4 obligations:
  1. include a copy of the Apache-2.0 licence;
  2. keep the copyright notice (© 2025 Paul Bakaus);
  3. **mark modified files** with a "changed by …" note;
  4. no NOTICE file exists upstream, so there is no NOTICE to carry; add our own attribution line.
- Candidate files (markdown only), for example under `.claude/skills/frontend-design/reference/impeccable/`:
  - `craft-floor.md`, `operate.md`, `mode-operate.md`;
  - `harden.md`, `clarify.md`, `adapt.md`, `distill.md`, `polish.md`;
  - `audit.md` and `critique.md` (as review checklists; edit out script calls);
  - `typeset.md`, `layout.md`, `colorize.md`, `animate.md`.
- Pin to a commit SHA and record it in the copied header.
- **Do not vendor:**
  - `scripts/` (launcher binary, live-mode browser injection, telemetry to impeccable.style, font index);
  - `.claude/agents/*`, hooks, `.claude/settings.json`;
  - the `SKILL.md` router: it expects `PRODUCT.md`/`DESIGN.md` and the `impeccable context` launcher.
  - This is a **supply-chain and autonomy concern**: hooks run even when commands are denied, and live mode can launch an agent with sandboxing disabled. Running `npx impeccable detect` in CI would need a separate `secure`/`devops` review, a pinned version and `DO_NOT_TRACK=1`. Our existing Playwright layout + contrast tests already cover much of the "quality" half of the detector.
- **Simplest alternative (recommended first step):** don't copy files at all. Fold the applicable rules from §4.1 into `memory/wiki/design/design-system.md` and the `frontend-design` / `pr-review` skills as our own checklist, citing impeccable (Apache-2.0) and taste-skill (MIT) as sources. Reusing ideas and paraphrased rules has no redistribution obligations. Vendor the markdown only if the team wants the agent to run impeccable-style `critique` / `audit` passes verbatim.
- Any vendoring is a lasting decision, so it would warrant an ADR in `memory/wiki/decisions/`.

### Open questions for EM
1. Keep Lora for page titles, or move it to brand-only spots (logo, login, welcome band)?
2. OK to remove uppercase eyebrows, corner ornaments and the dot texture from the work area (brand stays in the maroon chrome)?
3. OK to drop `CountUp` and the gradient "lacquer" on primary buttons?
4. Vendor impeccable's reference markdown (Apache-2.0, text-only, pinned SHA), or just fold the rules into our own design-system page?
