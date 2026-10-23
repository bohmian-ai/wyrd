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
