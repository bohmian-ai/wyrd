---
id: TASK-003-R3-identity
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 13
parent_task: TASK-003
remediates: [rebase onto flat Card identity]
---

# Project flat Card identity in the shared UI

Execute with `$wyrd-implement` under approved revision 13.
Original task `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`.
UI-relative paths below begin at `crates/wyrd/wyrd-server/wyrd-ui`.

## Cause

The UI branch was rebased onto the flat Card identity change, which removes
`space` from `CardRef`, Card `Metadata` and `ListCardsRequest` across the spec,
server, Rust/Python/TypeScript SDKs and MCP. Card routes now refuse a `space`
key with 400 `WYRD_SPEC_400_VALIDATION`. The shared UI still projects `space`:

- `packages/ui/src/lib/loaders.ts:207` passes the resolved space to `cards.list`
  (svelte-check: unknown property on `ListCardsRequest`, `Metadata`).
- `packages/ui/src/lib/components/CardHeader.svelte:29,84` builds and renders a
  spaced reference (svelte-check: unknown property on `CardRef`, `Metadata`).
- `packages/ui/src/lib/schemas.ts:39` accepts `space` in the component CardRef
  input; `packages/ui/src/lib/catalog.json` advertises it in three schemas.
- `packages/ui/src/lib/format.ts:50` `fmtCardRef` and `CardRefChip.svelte`
  render `space/name@version`.
- Fixtures and assertions carry it: `src/lib/ui-fixtures.ts:47,51,54`,
  `src/lib/ui-library.test.ts:346`,
  `src/lib/ui-library.integration.test.ts:149,281`.

## Required outcome

Remove `space` from every UI CardRef projection, schema, catalog entry,
formatter, component, loader request, fixture and assertion. The component
input schema must refuse or not advertise `space` consistently with the
generated `CardRef` (no silent acceptance that is then dropped). Identity
renders without a space segment. Do not add an alias, compatibility key or
fallback. Card reads stay exact (`kind`, `name`, `version`, optional `uid`).

Preserve all closed TASK-003 findings and the pending TASK-003-R3 and
TASK-027-R3 scopes (`plot.ts`, `WyrdView.svelte`); do not touch those files.

## Proof

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
```

Plus the existing UI real-server integration lane that runs
`src/lib/ui-library.integration.test.ts` (find its owning `mise` task), which
now exercises the flat routes, and `mise run check:examples` if any example
input carries `space`.

## Implementation evidence

Owner reuse: `CardRefSchema` (`schemas.ts`) stays the one component CardRef
schema; it is `.strict()`, so dropping the `space` key makes the input refuse
`space` and `scripts/catalog.ts` regenerates `catalog.json` without it. No new
helper; `CardHeader` reuses `fmtCardRef` for the identity line.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| No `space` in loader requests | `packages/ui/src/lib/loaders.ts` `loadCardHeader` lists by `kind`+`name` | `pnpm check` 0 errors; `ui-library.test.ts` paged listing; integration `TASK-003: Identical authorized reads` (live `cards.list` on flat routes) | PASS |
| Component input refuses `space`, catalog does not advertise it | `schemas.ts` `CardRefSchema`; regenerated `catalog.json` | `validatePanel('CardHeader', { cardRef: { ...card, space } })` → `WYRD_SPEC_400_VALIDATION` naming `space`; `node packages/ui/scripts/catalog.ts --check` | PASS |
| Identity renders without a space segment | `format.ts` `fmtCardRef`; `CardHeader.svelte`; `CardRefChip.svelte` | `ui-library.test.ts` `TASK-003: Validated portable rendering` (`ranker@1.4.2`, `Model ranker@1.4.2`) | PASS |
| Fixtures, assertions and example inputs carry no `space` | `src/lib/ui-fixtures.ts`, `ui-library.test.ts`, `ui-library.integration.test.ts`, `examples/ui-app/src/panels.ts` | example `svelte-check` 0 errors; `git grep -nw space` over UI code and the example returns only CSS | PASS |
| `plot.ts`, `WyrdView.svelte` untouched | diff | `git diff --stat` | PASS |

Commands:

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` — PASS (0 errors, both projects).
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` — PASS (40/40).
- `mise exec -- node packages/ui/scripts/catalog.ts --check` (in the UI dir) — PASS.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app check` — PASS. `check:examples` builds only Rust examples, so it does not cover `examples/ui-app`.
- `git diff --check` — PASS.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts'`
  (after `mise run build:server` and `mise run ts:build:testing`) — FAIL 1/2:
  `TASK-003: Identical authorized reads` passes; `TASK-003: Actual four-host foundation proof` fails.

Diagnosis of the four-host failure (outside this remediation's identity scope):

- **Symptom:** `ui-library.integration.test.ts:223` waits for `[data-panel="latency"] svg` in the built MCP App and gets `null`.
- **Evidence:** a probe dump shows the app's tool calls are already flat and correct
  (`cards.get` `{kind,name,version}`, `cards.list` without `space`, `bifrost.query`), and the iframe throws
  `ReferenceError: structuredClone is not defined` from the bundled artifact.
- **Cause:** `packages/ui/src/lib/contract.ts:42` `wyrdProblem` deep-copies the canonical problem with
  `structuredClone`; the `invalid` panel's validation problem runs it inside the jsdom iframe window, which has
  no `structuredClone`, and the throw aborts rendering of the view.
- **Fix site:** not changed here. Either the AppBridge harness (`appBridgeView`, which already supplies the
  jsdom-missing `ResizeObserver`) or `wyrdProblem`. Both are outside this task's write set and the lead
  restricted test-file edits to `space` lines; routed to the lead.

### Four-host harness fix

Lead decision: fix at the harness, leave `wyrdProblem` unchanged, because every real host provides
`structuredClone`. `appBridgeView` now passes the outer realm's `structuredClone` into the iframe window,
next to the existing `ResizeObserver` shim (`src/lib/ui-library.integration.test.ts`).

Independent diagnostician report: it confirmed the cause. `WyrdView` `start()` maps panels through
`validatePanel` → `inputProblem` → `wyrdProblem`, and the `structuredClone` throw inside that map leaves
`panels` unset, so no panel renders. The diagnostician preferred a source fix in `contract.ts:42`, a shallow
copy plus a `details` copy, so that hosts without `structuredClone` also work. That was not taken: Safari 15.4
and Chrome 98 are the support floor. It noted that every local-problem caller in `loaders.ts`,
`WyrdView.svelte`, `view.ts` and `mcp.ts` routes through `wyrdProblem`; server uses run in Node.

- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts'` — PASS (2/2).
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` — PASS. The app-level `svelte-check --tsconfig ./tsconfig.json` also reports 0 errors across 702 files.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` — PASS (40/40).
- `git diff --check` — PASS.
