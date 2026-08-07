---
name: wyrd-ui
description: "Use this repo-level skill when building, editing, debugging, styling, or extending the Wyrd SvelteKit UI in `crates/wyrd/wyrd-server/wyrd-ui`. Trigger for Svelte 5 components, SvelteKit routes and load functions, Tailwind v4 or Skeleton styling, theme work, frontend UX, visual changes, and any request mentioning Wyrd UI, wyrd-theme.css, brutalism, dark mode, or Wyrd frontend patterns. Wyrd UI is the styling source of truth, not the product or workflow source of truth."
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

Wyrd is brutalist in both light and dark modes. Same geometry across modes; palette and atmosphere change.

- Light mode: parchment, rune-purple, ink-black brutalism. White card surfaces, 2px black borders, hard-offset black shadows, semantic card colors.
- Dark mode: brutalist phosphor terminal. Same 2px borders and hard-offset shadows, rendered in dim phosphor green against near-black surfaces with CRT atmosphere.

The dark mode is not just the light UI with darker colors. It is the same physical brutalist system rendered as vintage terminal hardware.

## Non-Negotiable Styling Rules

### Zero rounded corners

Use `border-radius: 0` for Wyrd UI surfaces and controls. Do not add inline radii or component-library defaults that fight this. Use true circles only for elements that must be circular, such as avatars or status dots.

### Hard-offset shadows only

Use solid, zero-blur shadows:

```css
box-shadow: 3px 3px 0 0 var(--shadow);
box-shadow: 6px 6px 0 0 var(--shadow);
box-shadow: 10px 10px 0 0 var(--shadow);
```

Do not use blurred shadows, glassmorphism, soft elevation, or `filter: drop-shadow(...)`.

### Two-pixel borders

Use 2px borders for normal UI, 3px for major hero or feature containers, and 2px dashed dividers inside cards. Avoid gray, hairline, or low-contrast borders.

### Tokens over raw colors

Use Wyrd theme tokens and Tailwind classes that resolve through the Wyrd theme. Avoid hardcoded hex values in components unless editing the theme itself.

## Required References

For any styling or visual change, read:

- `references/wyrd-light-mode-style-guide.md`
- `references/wyrd-dark-mode-style-guide.md`
- `references/wyrd-theme.css`

`references/wyrd-theme.css` is a **generated artifact**, not a hand-authored
file. Its color/token layer (the `[data-theme='wyrd'].theme-light` /
`.theme-dark` blocks) is emitted from `crates/wyrd/wyrd-server/wyrd-ui/brand/palette.json`
by `brand/gen-theme.mjs`; the brutalist utility classes below the token blocks
(`.neo-card`, `.neo-btn`, `.mono-tag`, `.pixel-text`, `.card-tag--*`,
`.status-dot*`, and the zero-radius overrides) are ported forward and reference
those canonical tokens via `var(--*)`. Do not edit it by hand — change
`palette.json` (for token values) or the `codexUtilities` block in
`gen-theme.mjs` (for the utilities), then run `pnpm tokens`. A CI drift lock
(`gen-theme.mjs --check`) fails if the file diverges from the palette. The old
oklch fork — glow (`.neo-glow`, `--neo-glow-color`), gradient helpers,
`--retro-*`, `.grain`, and the CRT scanline/vignette/phosphor `text-shadow`
layers — has been removed; do not reintroduce it (it violates the Never rules).

Also inspect the current app files:

- `crates/wyrd/wyrd-server/wyrd-ui/src/app.css`
- `crates/wyrd/wyrd-server/wyrd-ui/src/routes/+layout.svelte`
- `crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.svelte`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/brand-skill.md`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/theme.css`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/palette.json`

## Standalone References

Load these only when relevant. They are Wyrd-native and should be enough to use this skill without any outside repository.

- Feature boundary and code navigation: `references/wyrd-working-map.md`
- SvelteKit routing, server routes, load functions, invalidation, and auth/session boundaries: `references/wyrd-sveltekit-architecture.md`
- Svelte 5 runes, snippets, effects, `.svelte.ts` modules, and TypeScript patterns: `references/wyrd-svelte5-patterns.md`
- Large tables, trace-style views, files, charts, pagination, virtualization, search/filtering, and payload budgets: `references/wyrd-data-performance.md`
- Developer workflow UX, filters, drilldowns, empty/loading/error states: `references/wyrd-developer-ux.md`
- Tailwind v4, Skeleton, token usage, dark mode, icons, semantic colors: `references/wyrd-theming.md`
- Vitest, Svelte Testing Library, build checks, accessibility, and performance verification: `references/wyrd-testing-verification.md`

## Frontend Rules

- Work inside the existing SvelteKit app instead of treating this as a generic Svelte project.
- Keep the app dense, scannable, and task-oriented. Wyrd is a developer tool, not a marketing site.
- Prefer existing local patterns and package choices before adding abstractions or dependencies.
- Keep data fetching server-side by default when server boundaries exist. Prefer SvelteKit server load functions and route handlers over browser-direct backend calls.
- Use Svelte 5 runes deliberately: `$derived` for pure derived state, `$effect` for external side effects.
- Do not add new UI libraries unless the user explicitly asks.
- Use icons from the existing enabled icon stack when available; do not invent custom inline icons for common actions.
- Preserve accessibility basics: semantic elements, keyboard behavior, visible focus, useful empty/loading/error states, and responsive layouts.

## Plan Remediation

When `$wyrd-implement-plan` resumes UI work after review, require an active
orchestrator-owned `Remediation revision RR<N>` in the canonical task. Treat
the original task plus that revision as the executable assignment. It must be
decision-complete at the same density as the original task: exact correction,
owners and symbols, interfaces, consequential control flow, UI states and edge
behavior, tests and critical assertions, verification, allowed/prohibited
scope, escalation boundaries, and finding closure. Reviewer findings, a diff,
or a desired visual outcome alone are not implementation instructions. Stop
before editing rather than choosing a material fix that the revision omits.

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
