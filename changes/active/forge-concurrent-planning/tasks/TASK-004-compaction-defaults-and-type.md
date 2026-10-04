---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-forge-concurrent-planning
requirements: [REQ-011, REQ-012]
depends_on: [TASK-003]
---

# Compaction on by default; tables choose their compaction type

## Scenario 1 — Every table compacts by default (REQ-011)

Flip `ForgeTableSettings::default().compaction_enabled` to `true`
(`crates/vala/vala-bifrost-redux/src/forge/settings.rs`). A table with no
Forge properties gets a compaction track on its first commit and is
dispatched by the 1-hour interval. Update the settings unit test, the
docs property table (`docs/src/content/docs/bifrost/forge.svx`) and every
test that relied on the old default by stating the property explicitly
where it needs compaction off. RED: a redux test that a property-less table
is dispatched after the interval. Journey: a table registered through the
public client with no options is compacted.

## Scenario 2 — Registration chooses the compaction type (REQ-012)

Follow every file on the `compaction_target_file_size_bytes` path
(`rg -l compaction_target_file_size_bytes`): wyrd-spec request/description
and generated schemas, `wyrd-client` table builder, server
`bifrost/service.rs` validation and re-register conflict with a new
`WyrdError` code, `BifrostCatalog` create-transaction property write, Python
SDK (PyO3, exports, stubs via codegen), TypeScript SDK (native, declarations,
src). Journeys in Rust, Python and TypeScript register a table with
`small-files`, read it back from the description, re-register with the same
value (accepted) and a different value (conflict), and show Forge dispatching
that type.

## Verification

Focused exact tests for each scenario, `mise run codegen:check`,
`mise run py:test:unit`, `mise run py:typecheck`, the TypeScript unit and
integration tasks, `mise run test:bifrost:journey:forge`, fmt, lints and
`git diff --check`.
