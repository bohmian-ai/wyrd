# Typed SDK composition and Run-scoped Bifrost writes

- Change: `sdk-authoring-ergonomics`
- Specification: `SPEC-sdk-authoring-ergonomics`, approved revision 8
- Completed: 2026-11-11
- Reviewed target: `ea710806b`, approved by the human owner in place of a final change review
- Delivery reference: not supplied

Users author Cards in Python, Rust, and TypeScript from typed definitions
instead of hand-built dictionaries: the original Prompt goes straight into an
Agent, Python model classes are schemas and instances are evidence, and no
definition needs a version. Run-scoped table writes read as Bifrost writes,
and a Service with one declared table writes rows without naming it.

## Shipped behavior

- Definitions live under Python `wyrd.verify`, Rust `wyrd_ai::verify`, and
  TypeScript's `verify` namespace; the earlier authoring import route is gone
  with no aliases. The original `skald_prompt::Prompt` is the only Prompt:
  Python's `wyrd.verify.Prompt` is the `wyrd.prompt.Prompt` class, and
  TypeScript binds the same Rust owner. `Agent(name, prompt=...)` registers
  the inline Prompt as `<agent-name>-prompt` and stores its exact reference.
- Named definitions take a name and optional version at construction. Every
  definition, at every depth and through both sibling reference forms, may
  omit its version; the server selects each one, binds parents to the
  selected exact child refs before hashing, and refuses an ambiguous
  omitted-version sibling. Unchanged replay keeps every ref; a changed child
  versions its parents.
- Python accepts canonical native values directly: `ComparisonOperator`,
  `ExecutionRoute`, `AgentRunConfig`, Pydantic model classes as slot,
  whole-input, and table schemas, model instances as evidence and rows, and
  `ResponseFormat.verdict(data=...)` over the canonical Verdict envelope.
  JSON Schema dictionaries work everywhere without Pydantic, which ships as
  the optional `wyrd-ai[pydantic]` extra.
- `Table(name, schema)` declares a Service table in all three SDKs over the
  shared Rust owner; dictionary entries remain accepted.
- `run.bifrost.write(row)` writes the state's active table and
  `write_to(table, row)` / `writeTo` names a `bifrost.datasets.<name>` table
  per call. Starting Bifrost with no chosen table activates a root Service's
  sole declared table; `use_table_by_name` switches the shared binding. No
  active table refuses with `WYRD_VALA_412_NO_ACTIVE_TABLE`; a destination
  outside `bifrost.datasets` refuses with `WYRD_SDK_400_INVALID_OBSERVATION`.
  `run.observe.record`, the fire-and-forget telemetry `record` entry points,
  and `Bifrost.dropped` are removed.
- The Author in code guide teaches the three-language program, the modular
  Python Service, and readback; Services, Verifier, Cards, and observation
  pages link and match it.

## Invariants and constraints

- Canonical specs, the shared graph, loader, declaration, publication, and
  server version selection remain the only owners; SDKs never pin or select
  versions. Registered refs stay exact; tenancy, permissions, and errors are
  unchanged.
- Each row keeps its selected Card UID, shared Run ID, and destination. Write
  return means buffered submission; successful shutdown is the drain barrier.
  Switching tables retains prior producers. Direct `insert` and durably
  acknowledged `write_batch` are unchanged.
- An inferred sole table is described at startup; failure stops that state's
  Bifrost writer only, never verification or the server.

## Decisions and deviations

- Revision 5 fixed the typed-composition surface; revision 6 named the
  Run-scoped write; revision 7 removed telemetry `record`; revision 8 added
  `Table`, the optional Pydantic extra, and active-table writes, with the
  matching `verified-change-contract` REQ-127/128 amendment.
- The active table is state-wide, not per Run: concurrent writers needing
  different destinations use `write_to`.
- Bifrost dataset names are fixed to the `datasets` namespace; user-defined
  schemas are out of scope.
- The guide's install line pins 0.4.1 although `Table`, the extra, and the new
  write ship in the next release; the maintainer accepted that timing.
- The final change review was waived by the maintainer; TASK-004-R1 was
  implemented without a closing task review.

## Acceptance closure

AC-001 through AC-013 closed through real-server Rust, Python, and TypeScript
journeys: every authoring interface and all twelve kinds register, versionless
and mixed graphs resolve and replay, the typed finance and modular Python
examples verify, the Author in code programs declare, verify, write, drain,
and read back, and active-table startup, refusal, and switching paths are
proven. Pydantic-free schema forms are pinned by a blocked-import unit test.
Format, lint, typecheck, codegen, examples, and docs lanes passed.

## Current owners

- [Wyrd design](../../../architecture/wyrd-design.md) (Cards authoring,
  Observation identity).
- [Shared authoring](../../../crates/shared/wyrd-client/src/authoring/mod.rs),
  [declaration state](../../../crates/shared/wyrd-client/src/state.rs),
  [graph resolution](../../../crates/shared/wyrd-loader/src/resolve.rs),
  [Run writes](../../../crates/shared/wyrd-client/src/observe/mod.rs), and the
  [Bifrost facade](../../../crates/shared/wyrd-client/src/bifrost/facade.rs).
- [Original Prompt](../../../crates/skald/skald-prompt/src/lib.rs),
  [Python verify stubs](../../../sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi),
  [TypeScript definitions](../../../sdks/wyrd-sdk-ts/wyrd/src/definitions.ts).
- [Author in code guide](../../../docs/src/content/docs/build-and-verify/declare/author-in-code.md)
  and [examples](../../../examples/author-in-code/author.py).
- Journeys: [Rust authoring](../../../sdks/wyrd-sdk-rust/tests/integration/authoring.rs),
  [Rust author in code](../../../examples/tests/author_in_code.rs),
  [Python authoring](../../../sdks/wyrd-sdk-python/tests/integration/cards/test_authoring_journey.py),
  [TypeScript authoring](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/authoring.test.ts), and
  [TypeScript observation surface](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/observation-surface.test.ts).
