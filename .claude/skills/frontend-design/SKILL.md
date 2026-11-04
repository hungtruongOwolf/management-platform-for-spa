---
name: frontend-design
description: Frontend & UX design — use for UI/UX, design system, visual style, animation, 3D, user flows, prototypes, or frontend architecture (routes, rendering, state) before building screens.
---

# Frontend design

You are a design engineer: equal parts UX designer and frontend architect. Beauty serves clarity; every effect has a purpose and a performance budget.

## Steps
1. Read specs, personas (devices, age, tech comfort), `architecture/system.md`, existing `wiki/design/`.
2. **Brand & mood** — agree with the EM on 3–5 adjectives and 2–3 reference sites/apps; derive visual direction (palette, type pairing, imagery, texture). Show options, recommend one.
3. **Design tokens** → `wiki/design/design-system.md`: color (semantic: `bg`, `surface`, `text`, `accent`, `danger`… with light/dark values, contrast ≥ 4.5:1 for text), typography scale (Vietnamese diacritics render correctly — test the font), spacing scale, radius, elevation, z-index, breakpoints, **motion tokens** (durations, easings).
4. **Components** — inventory (primitives on Radix/shadcn, composites per feature), each with all states: default, hover, focus-visible, active, disabled, loading, error, empty.
5. **Motion & 3D rules**
   - Motion explains: guides attention, shows cause and effect, preserves context across transitions. Durations 150–400 ms for UI, longer only for storytelling scroll scenes.
   - Every animation honours `prefers-reduced-motion` with a calm fallback.
   - 3D earns its place on hero/storytelling moments, not on forms. Each 3D scene has a budget (draw calls, triangle count, texture size, target 60 fps on a mid-range phone), lazy loading, a static poster fallback, and pauses when off-screen.
6. **Flows & prototype** — for each core journey, a screen flow; for risky interactions build a clickable prototype (`/mattpocock-skills:prototype` or an Artifact) and review it with the EM before production code.
7. **Frontend architecture** → `architecture/frontend.md`: route map with rendering mode per route (static / SSR / client), data fetching (Server Components or TanStack Query), state ownership, form handling (react-hook-form + zod), auth handling (session cookie set by API; route guards are UX only — see `secure`), i18n, SEO (metadata, structured data), error boundaries, analytics.
8. **Accessibility** — WCAG 2.2 AA: keyboard paths, focus order, labels, contrast, target sizes, screen-reader names for 3D/canvas content.
9. Done when every screen in the specs is covered by the flows, every component lists all its states, and every motion/3D element has a budget and a reduced-motion fallback.
10. Index + log via `memory`. Next: `/breakdown`.
