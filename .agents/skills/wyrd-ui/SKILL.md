---
name: wyrd-ui
description: "Use this repo-level skill when building, editing, debugging, styling, or extending the Wyrd UI in `crates/wyrd/wyrd-server/wyrd-ui`: the `wyrd-ai-ui` component package, the SvelteKit host app, Svelte 5 components, routes and load functions, Tailwind v4 styling, theme tokens, view documents, loaders and executors, and any request mentioning Wyrd UI, Evidence Thread, dark mode, or Wyrd frontend patterns. Wyrd UI is the styling source of truth, not the product or workflow source of truth."
---

# Wyrd UI

Use this skill before touching anything under `crates/wyrd/wyrd-server/wyrd-ui/`.
Precedence: the user's instructions, then current code, then `brand/DESIGN.md`, then
this skill.

## Product boundary

The Rust server owns durable behavior; the UI is one client among MCP, CLI, HTTP, and
the SDKs. Never add a workflow only the UI can perform, and never re-derive
server-owned meaning: status, judgments, thresholds, lineage, sorting, pagination,
aggregation. The UI owns presentation only: display formatting (`wyrd-ai-ui`'s
`format.ts`), widths, relative time, truncation, verdict→token mapping. Every path is
tenant-scoped; the browser never holds a credential.

## Two packages

| Path | What it is | Put here |
|---|---|---|
| `packages/ui` (`wyrd-ai-ui`) | Publishable components, Zod input schemas, `catalog.json`, view documents (`WyrdView`), portable loaders, the Node and MCP App executors, `theme.css` | Reusable components and anything another host (MCP App, `examples/ui-app`) renders |
| `src/` (host app) | SvelteKit BFF: sign-in and sessions, tenant routing, the `t/[tenantKey]/…` pages, URL state, the `_gallery` dev page | Routes, server-only code, page composition, host URL state |

Map and conventions: `references/wyrd-working-map.md`. Server/client split, errors,
and auth: `references/wyrd-sveltekit-architecture.md`.

## Styling: Evidence Thread

Authority is `brand/`: read `DESIGN.md` first, then `palette.json` and
`components.json`; `renders/styleguide.html` is the rendered reference.

- **Tokens only.** `var(--token)` or Tailwind utilities that resolve through
  `brand/theme.css`. No hex in components, no inline styles that bypass tokens.
- **Modes.** The root layout sets `data-mode="light|dark"` (`src/lib/theme.svelte.ts`,
  persisted in the `wyrd-mode` cookie). Light and dark swap token values, never
  structure.
- **Semantics.** `--declare` is Declaration, links, selection, focus, and the primary
  action. `--observe` is observed fact. `--evidence` is retained Evidence only, never an
  accent, hover, or secondary action. `--ok`/`--warn`/`--danger` mean state and always
  pair with a glyph or word; chart series carry a dash and marker, not color alone.
- **Geometry.** 1px rules (`--border`, `--border-soft`), 2px radius (`--r`). No
  shadows, glow, gradients, scanlines, or lift/sink motion.
- **Type.** Familjen Grotesk (`--font-sans`) for interface and reading; Fragment Mono
  (`--font-mono`) only for machine-shaped values.
- **Host controls.** `.app-control` and `.app-input` (`src/app.css`) match the approved
  mock's buttons: 1px rule, 32px, 13px sans. Add `.mono` to an input that holds a machine
  value (key, issuer, client id, claim, tenant key). Fonts ship via `@fontsource`; never
  import a font service. No UI component library: Tailwind v4 plus tokens.
- **Generated, never hand-edited:** `brand/theme.css`, `packages/ui/src/lib/theme.css`,
  `docs/src/styles/wyrd-tokens.css`, the mark SVGs. Change `palette.json`, run
  `pnpm tokens`. `palette.json`'s `pairs` block is a contrast gate in both modes.

## Adding or changing a component

1. Contract in `brand/components.json` (`status: "spec"`).
2. Component in `packages/ui/src/lib/components/`, exported from `index.ts`. Data
   components render from typed props alone; fetching belongs to loaders.
3. If hosts can place it in a view document, add its input schema to
   `packages/ui/src/lib/schemas.ts` and a loader in `loaders.ts`, then run
   `pnpm --filter wyrd-ai-ui catalog` (regenerates `catalog.json` and `problems.json`).
4. Add a fixture in `src/lib/ui-fixtures.ts` so `_gallery` and the tests render it.
5. Flip the contract to `built`, set `catalog` to match `catalog.json`, and add a test.
   `brand/gen-theme.test.mjs` fails if the contracts, exports, and catalog disagree.

## Frontend rules

- Dense, scannable, task-oriented. One dominant work region per page; progressive
  disclosure; names before IDs (`DESIGN.md` § Workbench composition).
- Server loads by default. Browser-called reads go through host `+server.ts` routes
  that use the session cookie (for example `t/[tenantKey]/ui/load`), never the backend
  directly.
- State users share, reload, or go Back to lives in the URL (`cards-host.ts`,
  `observe-host.ts`). Page through bounded results from the server; never fetch all
  pages to sort or filter in the browser.
- Svelte 5 runes: `$derived` for pure values, `$effect` only for external side effects,
  `.svelte.ts` modules only for state several components share. Strict TypeScript;
  `unknown` at boundaries, then narrow with the Zod schemas.
- Empty, loading, error, unauthorized, and unavailable states are distinct and
  reachable. Errors are stable problems (code, title, remediation), not raw messages.
- Accessibility: semantic elements, keyboard reach, visible focus in both modes, dense
  data in a labeled horizontal scroll region rather than crushed.
- No new dependency unless the user asks.

## Verification

Narrowest first: `references/wyrd-testing-verification.md`. The minimum for any change
under `wyrd-ui/` is `mise run ui:check`, the same lane CI's `ui` job runs. It builds the
`wyrd-ai` addon, then runs `pnpm check` and `pnpm test`. Add `mise run codegen:check`
when touching `palette.json`, `schemas.ts`, or the error catalog. For visual work,
check both modes in `/_gallery` and the affected route.

## Handoff

Report the visible outcome, the changed route or component, verification status, and
any material limitation.
