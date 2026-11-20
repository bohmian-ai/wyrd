# Wyrd UI Testing and Verification

Run `pnpm` commands from `crates/wyrd/wyrd-server/wyrd-ui`. `check`, `test`, `build`,
and `dev` build `wyrd-ai-ui` first and need the `wyrd-ai` addon built (`mise run ts:build`).

| Changed | Run |
|---|---|
| Any `.svelte` or `.ts` file | `mise run ui:check` (the CI `ui` job) |
| Fast iteration, addon already built | `pnpm check`, `pnpm test` |
| A single test | `pnpm exec vitest run src/lib/<file>.test.ts -t '<name>'` |
| `brand/palette.json` or `gen-theme.mjs` | `pnpm tokens`, then `mise run codegen:check` |
| `packages/ui/src/lib/schemas.ts` or the Rust error catalog | `pnpm --filter wyrd-ai-ui catalog`, then `mise run codegen:check` |
| Sign-in, sessions, tenant routing, BFF routes | `mise run test:identity:journey:suite` (needs Postgres, Keycloak, Dex) |
| Production BFF journey against Keycloak and Dex | `mise run test:identity:ui` |
| Real-server UI tests (`*.integration.test.ts`) | see Tests below |
| Bundle or adapter | `pnpm build` |

## Tests

- Shared components are tested in `src/lib/ui-library.test.ts`, pages in
  `src/lib/ui-pages.test.ts` (vitest, jsdom, Testing Library). Component data comes from
  `ui-fixtures.ts`; don't invent shapes.
- `ui-library.integration.test.ts` (hosts and built artifacts) and
  `ui-pages.integration.test.ts` (production BFF in Chromium) run against a real server
  and are skipped unless `WYRD_UI_INTEGRATION=1`. They need `mise run ts:build`,
  `ts:build:testing` and `build:server` first. Run from the repository root:

  ```bash
  mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 \
    pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '<name>'
  ```
- Node-lifetime behavior is tested in TypeScript. A user-facing capability also needs
  its journey test (see `TESTING.md`).

## Manual visual check

`mise run dev:frontend`, then open `/_gallery` and the affected route in both modes.
For a page with an approved mock, compare as `SKILL.md` § Matching an approved mock
says. Confirm:

- text fits, focus is visible, and no layout shifts when counts or labels change
- empty, loading, error, and unauthorized states are reachable
- 1px rules and 2px radius, no elevation, no color-only meaning
- a narrow viewport scrolls dense data instead of clipping it
