---
name: wyrd-ui
description: "Use this repo-level skill when building, editing, debugging, styling, or extending the Wyrd SvelteKit UI in `crates/wyrd/wyrd-server/wyrd-ui`. Trigger for Svelte 5 components, SvelteKit routes and load functions, Tailwind v4 or Skeleton styling, theme work, frontend UX, visual changes, and any request mentioning Wyrd UI, wyrd-theme.css, Evidence Thread, dark mode, or Wyrd frontend patterns. Wyrd UI is the styling source of truth, not the product or workflow source of truth."
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

Wyrd UI uses **Evidence Thread**, the one design system shared with the documentation
site: scientific provenance rendered flat and ruled on a neutral mineral canvas. Light and
dark are two complete renderings of the same structure; only token values change.

The authority is `crates/wyrd/wyrd-server/wyrd-ui/brand/`: read `DESIGN.md` first, then
`palette.json` and `components.json`. `renders/styleguide.html` is the rendered reference.

## Non-Negotiable Styling Rules

- **Tokens only.** Use `var(--token)` or the Tailwind utilities that resolve through
  `brand/theme.css`. Never hardcode a hex in a component.
- **Semantics.** `--declare` (indigo) is Declaration, links, selection, focus, and the primary
  action; `--observe` (green) is observed fact; `--evidence` (lime) is retained Evidence
  only — never an accent, hover, or secondary action; `--ok`/`--warn`/`--danger` keep status
  meaning and always pair with a glyph or word.
- **Geometry.** 1px rules (`--border`, `--border-soft`), 2px radius (`--r`), no decorative
  elevation: no hard-offset or blurred shadows, glow, gradients, scanlines, or lift/sink
  motion.
- **Type.** Familjen Grotesk (`--font-sans`) for interface and reading; Fragment Mono
  (`--font-mono`) only for machine-shaped values. Both ship locally; never import a font
  service.
- **Mark.** The Wyrd mark SVGs are generated from the palette; never redraw them.

`references/wyrd-theme.css` is a **generated artifact**: the same tokens under the
`[data-theme='wyrd'].theme-light` / `.theme-dark` selectors, emitted from
`brand/palette.json` by `brand/gen-theme.mjs`. Do not edit any generated projection by
hand — change `palette.json`, run `pnpm tokens`, then `mise run skills:sync`.
`mise run check:tokens` fails on drift.

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

For visual work, inspect both light and dark behavior when theme support exists. Check that text fits, controls are reachable, and the flat ruled geometry is intact.

## Handoff

Report only the visible outcome, changed route or component, verification
status, and any material limitation. Do not narrate implementation steps or
repeat details visible in the diff.
