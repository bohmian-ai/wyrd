---
name: wyrd-ui
description: "Use this repo-level skill when building, editing, debugging, styling, or extending the Wyrd SvelteKit UI in `crates/wyrd/wyrd-server/wyrd-ui`. Trigger for Svelte 5 components, SvelteKit routes and load functions, Tailwind v4 styling, theme/token work, frontend UX, visual changes, and any request mentioning Wyrd UI, the design system, brand tokens, brutalism, dark mode, or Wyrd frontend patterns. Wyrd UI is the styling source of truth, not the product or workflow source of truth."
---

# Wyrd UI

This is the canonical repo-level skill for Wyrd frontend work. Use it before touching files under `crates/wyrd/wyrd-server/wyrd-ui/`.

The skill is standalone. Do not rely on external repositories, deprecated product docs, or a user's local filesystem outside this repo. Use the references bundled here.

## Product Boundary

Wyrd follows a language-agnostic client/server model. The Rust server owns
durable behavior and core logic; contracts live on the API wire through typed
schemas, HTTP/MCP payloads, generated docs, and stable errors so any language
can implement a client.

The SvelteKit UI is a supported developer client and the styling source of
truth. It is not the product or workflow source of truth. Do not add behavior
that can only be performed through the UI, and do not duplicate server-owned
registry, storage, policy, audit, tenancy, relationship, status, or evaluation
logic in frontend code.

Wyrd is agent-first and headless. MCP, CLI, HTTP, generated schemas, stable
errors, and machine-readable docs are primary surfaces. UI work must preserve
those headless paths and must respect both self-hosted and cloud SaaS tenant
separation.

## Precedence

Apply rules in this order:

1. Direct user instructions for this task.
2. Current Wyrd repository code under `crates/wyrd/wyrd-server/wyrd-ui/`.
3. Wyrd UI styling and brand guidance in this skill.
4. Bundled Wyrd references for architecture, Svelte, data, UX, and verification patterns.

Do not import product-specific naming, routes, client wrappers, visual tokens, or examples from older systems. If a pattern is useful, port the idea into Wyrd terms and Wyrd paths before using it.

## What Wyrd UI Is

Wyrd is brutalist in both light and dark modes. Same geometry across modes (5px radius, 2px ink borders, hard-offset zero-blur shadows); only the palette changes.

- Light mode: parchment background, white card surfaces, ink borders, brand violet (rune) accent, acid lime for primary action, calm near-black text (`#14141a`).
- Dark mode: flat near-black surfaces, the same borders and hard-offset shadows, brand violet + lime accents, calm off-white text (`#d6d6dc`). **No CRT, scanlines, vignette, glow, or phosphor** — dark mode is the same system on darker surfaces, nothing more.

The canonical specification lives in the app's `brand/` directory — read those first (see Required References). They override anything in this skill if they ever disagree.

## Non-Negotiable Styling Rules

### 5px radius — locked

Use `border-radius: var(--r)` (5px) for Wyrd UI surfaces and controls. Not zero, not larger. Do not add inline radii or component-library defaults that fight this. Use true circles only for elements that must be circular, such as avatars or status dots.

### Hard-offset shadows only

Use solid, zero-blur shadows in the altitude dial (quiet / raised / loud):

```css
box-shadow: 3px 3px 0 0 var(--shadow);   /* quiet — workbench default */
box-shadow: 6px 6px 0 0 var(--shadow);   /* raised — panels, stat blocks */
box-shadow: 10px 10px 0 0 var(--shadow); /* loud — hero only */
```

Do not use blurred shadows, glassmorphism, soft elevation, or `filter: drop-shadow(...)`.

### Two-pixel borders

Use 2px borders for normal UI, 3px for major hero or feature containers, and 2px dashed dividers inside cards. Always `var(--border)` — never gray, hairline, or low-contrast.

### Tokens over raw colors

Use Wyrd tokens (`var(--surface)`, `var(--text)`, …) or the generated Tailwind utilities (`bg-surface`, `text-text`, `border-border`). Never hardcode hex in a component. Tokens are defined in `brand/palette.json` and generated into `brand/theme.css` via `pnpm tokens` — edit the JSON, never the generated CSS.

## Required References

The design system is **locked in the app's `brand/` directory** — that is the canonical
source of truth and overrides this skill. For any styling or visual change, read these first:

- `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md` — rules doctrine (geometry, altitude, the Line, signal rules)
- `crates/wyrd/wyrd-server/wyrd-ui/brand/palette.json` — canonical tokens (edit here, then `pnpm tokens`)
- `crates/wyrd/wyrd-server/wyrd-ui/brand/components.json` — per-component contracts (built + specced)
- `crates/wyrd/wyrd-server/wyrd-ui/brand/wyrd-ui-source-of-truth.html` — the rendered visual reference

The bundled `references/wyrd-light-mode-style-guide.md`, `references/wyrd-dark-mode-style-guide.md`,
and `references/wyrd-theme.css` are portable summaries; if they ever disagree with `brand/`, `brand/` wins.

Also inspect the current app files:

- `crates/wyrd/wyrd-server/wyrd-ui/src/app.css` (imports `brand/theme.css`)
- `crates/wyrd/wyrd-server/wyrd-ui/brand/theme.css` (GENERATED — do not hand-edit)
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/components/` (built primitives + `ModeProvider`)
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/registry.ts` (name → component bridge)
- `crates/wyrd/wyrd-server/wyrd-ui/src/routes/styleguide/+page.svelte` (living catalog, light + dark)
- `crates/wyrd/wyrd-server/wyrd-ui/src/routes/+layout.svelte`

## Standalone References

Load these only when relevant. They are Wyrd-native and should be enough to use this skill without any outside repository.

- Feature boundary and code navigation: `references/wyrd-working-map.md`
- SvelteKit routing, server routes, load functions, invalidation, and auth/session boundaries: `references/wyrd-sveltekit-architecture.md`
- Svelte 5 runes, snippets, effects, `.svelte.ts` modules, and TypeScript patterns: `references/wyrd-svelte5-patterns.md`
- Large tables, trace-style views, files, charts, pagination, virtualization, search/filtering, and payload budgets: `references/wyrd-data-performance.md`
- Developer workflow UX, filters, drilldowns, empty/loading/error states: `references/wyrd-developer-ux.md`
- Tailwind v4, token usage, data-mode dark mode, semantic colors: `references/wyrd-theming.md`
- Vitest, Svelte Testing Library, build checks, accessibility, and performance verification: `references/wyrd-testing-verification.md`

## Frontend Rules

- Work inside the existing SvelteKit app instead of treating this as a generic Svelte project.
- Keep the app dense, scannable, and task-oriented. Wyrd is a developer tool, not a marketing site.
- Prefer existing local patterns and package choices before adding abstractions or dependencies. Styling is bespoke tokens + Tailwind v4 — there is no Skeleton/component library to lean on.
- Keep data fetching server-side by default when server boundaries exist. Prefer SvelteKit server load functions and route handlers over browser-direct backend calls.
- Use Svelte 5 runes deliberately: `$derived` for pure derived state, `$effect` for external side effects.
- Do not add new UI libraries unless the user explicitly asks.
- Use icons from the existing enabled icon stack when available; do not invent custom inline icons for common actions.
- Preserve accessibility basics: semantic elements, keyboard behavior, visible focus, useful empty/loading/error states, and responsive layouts.

## Editing Order

When making UI changes, inspect in this order:

1. Route entrypoint: `+page.svelte`, `+page.ts`, `+page.server.ts`, or `+layout.*`.
2. Feature component under `src/lib/components/...`, if present.
3. Server helper or route handler under `src/lib/server/...` or `src/routes/api/...`, if present.
4. Theme and brand files.
5. Shared route constants, schemas, stores, and local feature types.

The Wyrd UI is intentionally small right now. If a referenced directory does not exist yet, create it only when the feature needs that boundary.

## Verification

Run checks from `crates/wyrd/wyrd-server/wyrd-ui` when relevant:

```bash
pnpm build
pnpm check
```

For visual work, inspect both light and dark behavior when theme support exists. Check that text fits, controls are reachable, and brutalist geometry is intact.
