# Wyrd UI Working Map

Paths are relative to `crates/wyrd/wyrd-server/wyrd-ui`.

## Brand (`brand/`)

- `DESIGN.md`: doctrine. `palette.json`: canonical tokens. `components.json`: component
  contracts. `gen-theme.mjs`: emits every token projection and the mark SVGs.
- `renders/`: the styleguide and product mocks.

## Component package (`packages/ui/src/lib`, published as `wyrd-ai-ui`)

- `components/*.svelte`: every reusable component, exported from `index.ts`.
- `schemas.ts`: Zod input schemas, the view-document schema, and the layout schema.
  `scripts/catalog.ts` derives `catalog.json` (and `problems.json` from the Rust error
  catalog) from them.
- `contract.ts`: the read contract (`Executor`, `Problem`, `wyrdProblem`) shared by
  loaders, executors, and components. Domain types come from `wyrd-ai` wire types.
- `loaders.ts`: portable loaders. They plan reads, run them through an executor, and
  shape the props. No Node, browser, or credential.
- `node.ts`: server-only executor over the `wyrd-ai` SDK. `mcp.ts`: the MCP App
  executor over `App.callServerTool`.
- `view.ts` and `WyrdView.svelte`: render a view document; `mountView` serves non-Svelte
  hosts.
- `format.ts`: display formatting. `theme.css`: generated tokens.
- `src/app/main.ts`: the standalone MCP App bundle (`wyrd-view.html`).

`examples/ui-app` is a second consumer of the package; keep its contract working.

## Host app (`src/`)

- `hooks.server.ts`: per-request session handling and the browser side of the
  authorization server.
- `lib/server/`: server-only code. `auth/` (sessions), `upstream.ts` (the internal
  server origin), `problem.ts` (safe problem projection), `form.ts`, `routing/`,
  `development.ts` (local sign-in, unavailable in production builds).
- `lib/theme.svelte.ts`: the light/dark mode state and its cookie.
- `lib/cards-host.ts`, `lib/observe-host.ts`: URL state for the Cards and Observe pages.
- `lib/ui-fixtures.ts`: synthetic component data for `_gallery` and the tests.
- `routes/t/[tenantKey]/`: tenant pages (`cards`, `cards/detail`, `observe`, `query`,
  `settings`). `login`, `signup`, and `reset` use `+page@.svelte` to escape the
  tenant shell. `ui/load/+server.ts` loads one catalog panel for the browser session.
- `routes/_gallery`: the dev component gallery.

## Where a change goes

- A reusable visual or data component goes in the package, never in `src/lib/components`.
- A component used only by one page stays beside that route.
- Code that must hold a credential or reach the backend goes in `src/lib/server/` or a
  `+page.server.ts`/`+server.ts`.
- A type stays with its feature until a second feature needs it. Wire types come from
  `wyrd-ai`; never redeclare them.
