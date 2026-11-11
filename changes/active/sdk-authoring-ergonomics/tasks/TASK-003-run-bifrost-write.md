---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-sdk-authoring-ergonomics
spec_revision: 6
requirements: [REQ-012, REQ-013, REQ-014, INV-001, INV-002, INV-003, AC-009, AC-010]
depends_on: []
---

# Write caller-table records through Run-scoped Bifrost

## Outcome and Value

Replace `run.observe.record(table, row)` with `run.bifrost.write(table, row)`
in Python and TypeScript, and `run.bifrost().write(table, &row).await` in Rust.
Users can see the destination and action directly. The Run still supplies the
invocation ID and selected Card for every row.

This is one coordinated breaking rename across the shared client, three SDKs
and their consumers. Preserve the existing record pipeline and remove the old
public method without aliases. Observe continues to own verification.

## What Changes and What Must Stay True

The scoped write accepts the same table and row inputs as today's record
operation. Tables must already be registered under `bifrost.datasets.<name>`.
Returning from write means buffered submission, not durable acknowledgement;
successful state shutdown remains the existing drain barrier. Document that
distinction beside the method and in examples.

Every Card view retains its own exact subject Card UID; views of one invocation
share one Run ID. Reuse the state's existing writer, described-table cache,
queue and lifecycle. Preserve validation order, canonical error codes, schema
lookup, batching, backpressure, authorization, tenancy, flushing and shutdown.
Changing a method name must not make a previously refused write admissible or
move a deferred validation failure to another stage.

Preserve Observe's verification, submission and grouping behavior. The mixed
Observe handle remains: removing record does not mean renaming that handle.
Only public names exclusively representing the replaced record operation are
removed. Unrelated telemetry `record` functions and direct Bifrost methods
retain their contracts, including buffered insert and durably acknowledged
`write_batch`.

No state-level writer, ambient Run selection, scoped query/table management,
new transport, queue, lifecycle, dependency, Cargo feature or wire/persisted
contract is in scope. The cost is updating callers rather than keeping a
compatibility surface. This task does not depend on TASK-002's allocation
optimizations; use the integrated client owners without reopening that work.

## Approach

1. Revalidate the shared record call path, SDK adapters and consumers below.
2. Establish the new scoped public API through focused runtime and SDK journey
   tests, then expose it through the existing shared Rust owner and adapters.
3. Migrate the existing attribution and refusal coverage to the new operation;
   fill the missing Rust and language-boundary proof using current fixtures.
4. Remove the replaced public method, update exports and authored stubs, and
   regenerate native TypeScript declarations from their source.
5. Migrate callers, examples and owning documentation, then run the focused
   regressions and affected format, lint, typing and declaration checks.

## Ordered Implementation Scenarios

### Scenario 1 — Bifrost write replaces Observe record across the SDKs

**Behavior.** Rust, Python and TypeScript users write caller-table rows through
Run-scoped Bifrost using the new spelling. The replaced Observe record method
is absent; verification and submission remain on Observe. The operation uses
the existing record pipeline with unchanged inputs, attribution, buffering,
validation, errors and lifecycle.

**RED.** Add the planned Python runtime-surface test in
[public surface proof](#public-surface-proof). Against the old extension it
must fail because the Run's Bifrost capability is absent. Assert that the old
record method is absent and Observe verification/submission remain available.
An unrelated build or environment failure does not establish RED.

**GREEN.** Expose the new capability through the shared Rust owner and both
foreign-runtime adapters, remove the old method, and update public types and
callers. Migrate existing SDK write/read journeys and boundary tests to the
new spelling. Add Rust caller-table journey coverage where missing. Pass the
surface test, [attributed-write journeys](#attributed-write-journeys) and
[existing regressions](#shared-pipeline-and-retained-capability-regressions).
Attribution, validation and shutdown are preservation checks, not separate
features or TDD scenarios; do not manufacture failures for unchanged behavior
or create a new negative-test family for the rename.

**REFACTOR.** Remove superseded record-only adapters and reuse current Run
ownership, table resolution, enqueue and binding conventions. Keep one writer
and lifecycle, preserve foreign-runtime input validation and async ownership,
and provide the required rustdoc for changed Rust items.

## Acceptance Criteria

| Obligations | Required proof and violating outcome |
|---|---|
| REQ-012, AC-010 | Surface test, Rust compilation and SDK typing show the new scoped write and retained Observe verification. Old public record methods or aliases remaining would violate the rename. |
| REQ-013, AC-009 | Each attributed-write journey writes, shuts down and reads rows with distinct Card UIDs and the shared Run ID. Lost, duplicated or misattributed rows fail. |
| REQ-013, INV-002, AC-009 | Migrated existing refusal regressions retain exact canonical errors and validation stages, including denied writes. Persisting unauthorized/invalid rows, swallowing drain errors or changing lifecycle behavior fails. |
| INV-001/003, REQ-013 | Shared-client regressions and source inspection show reuse of table resolution, queue and state lifecycle. A replacement writer/validator or automatic Run selection fails. |
| REQ-014, AC-010 | Binding registration, exports, stubs and generated declarations agree; current consumers and docs use the new spelling and explain buffering. Existing Observe and direct Bifrost regressions pass. |

## Implementation Reference

Paths guide ownership, not a private implementation allowlist.

- `crates/shared/wyrd-client/src/observe/mod.rs`: `Run::observe`, `Observe::record`,
  `record_json`, `record_value` and `Run::correlation` currently own the complete
  scoped record workflow. The gap is the public capability/method name. Extend
  this Run boundary while retaining shared table resolution and enqueue logic.
- `crates/shared/wyrd-client/src/bifrost/facade.rs`:
  `Bifrost::resolve_writer_table` and `insert_observation` own cached schema
  resolution and buffered admission; `write_batch` has a distinct durable
  acknowledgement contract. `observe/lifecycle.rs` and `state.rs` own startup
  and draining. None needs a parallel owner.
- `crates/shared/wyrd-client/src/observe/tests.rs` already covers dataset
  description caching, concurrent first use, forbidden namespaces, unknown
  tables and immutable Card views. Migrate calls without weakening assertions.
- `sdks/wyrd-sdk-rust/src/lib.rs` and its existing re-exports project the client;
  `tests/integration/main.rs` registers `observe_a_run.rs` in target
  `integration`. That story already loads a Service, starts Bifrost, submits
  scoped evidence, shuts down and reads it. Extend it for caller-table writes.
- `sdks/wyrd-sdk-python/src/observe/mod.rs`: `PyRun::observe`,
  `PyObserveHandle::record` and module registration project Run ownership and
  JSON conversion. Update the corresponding exports in `python/wyrd/observe`
  and its authored `__init__.pyi`; preserve the mixed Observe class. The
  existing unit surface file and integration observation-surface file below
  supply offline boundary, attribution and namespace refusal coverage.
- `sdks/wyrd-sdk-ts/native/src/cards.rs`: `NativeRun` currently projects record,
  verify and submit over the shared Run. `wyrd/src/index.ts` exposes `Run` and
  `Observe`; `tests/unit/observe.test.ts` covers strict JSON/error projection.
  Split only the public capabilities needed by the approved API, retaining
  existing native error/async handling. `mise run ts:build` emits native
  declarations; do not edit them by hand.
- `sdks/wyrd-sdk-ts/wyrd/tests/integration/observation-surface.test.ts` already
  writes two tables from distinct Card views, shuts down and reads by Run ID.
  Its existing refusals cover reserved and unknown tables. Reuse that story.
- Search current callers in SDKs, `examples/`, `docs/`, architecture and the
  active packet's examples. Update current usage and owning SDK documentation;
  retain historical review evidence. Unrelated telemetry `record` functions and
  native direct-Bifrost record plumbing are separate operations, not aliases
  for the removed scoped method. No shared Card fixture change is expected.

## Verification and Evidence

This is a mixed task: the new executable SDK surface uses the ordered
scenario; preserved semantics use migrated regression evidence. Documentation,
exports and generated parity use static proof. REQ-007's registration TDD waiver
does not waive the new naming scenario.

Commands below are repository-root commands. Before RED, confirm each planned
selector after adding it and record its actual exact command. Preserve the
existing Python and TypeScript attribution tests and their names. Confirm nonzero test selection. Rebuild extensions
after binding changes, and run Cargo-backed setup/checks sequentially.

### Build and environment preparation

```bash
mise run build:server
mise run py:setup
mise run ts:build
mise run ts:build:testing
```

These are the build prerequisites used by the SDK journey lanes. Each focused
journey below provisions repository Postgres and migrations through its owning
wrapper; no external credentials or substitute in-process database are used.

### Public surface proof

Planned Python test: new scoped API, removed method, retained verification and
offline access. Run existing wrapper files as regression proof after migration.

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_run_exposes_bifrost_write_and_observe_verification
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/observe.test.ts
```

### Attributed-write journeys

Migrate the existing Python and TypeScript attribution cases in place. Rust's
selector is planned: add equivalent caller-table coverage to its existing
story. Each journey writes from different Card views of one Run, drains the
state and reads back the expected rows with their distinct Card UIDs and
shared Run ID. Preserve existing readiness and context-manager behavior.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=observe_a_run::writes_land_under_the_view_that_wrote_them)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/observe/test_observation_surface.py::test_records_land_under_the_view_that_wrote_them'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/observation-surface.test.ts -t "^records land in their tables under the view that wrote them$"'
```

### Shared pipeline and retained capability regressions

Run the shared scoped-observation module and Bifrost owner modules, plus the
existing bounded Observe stories. These establish preserved cache, validation,
queue, verification/submission and direct Bifrost behavior without running full
SDK journey suites. Retain exact catalog-code assertions for reserved/unknown
tables, malformed rows, denied writes and writes before startup/after shutdown,
at their existing validation stages. Reuse existing coverage at its owning tier;
extend an existing test only where an approved acceptance obligation lacks
proof. Record the exact focused command for any added assertion/test. Do not
introduce duplicate per-language negative-test families.

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(/^observe::tests::/) or test(/^bifrost::/)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(/^observe_a_run::/)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/observe/test_observation_surface.py'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/observation-surface.test.ts tests/integration/observe-a-run.test.ts tests/integration/bifrost-write.test.ts'
```

### Static and consumer closure

```bash
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
mise run ts:pack:check
mise run docs:check
mise exec -- git diff --check
```

Inspect exports, stubs and declarations to establish that the old public method
is absent and the new type is reachable. Review remaining old-spelling search
hits: historical evidence and explicit absence assertions may remain, current
usage must not. If wire generators or dependency edges change despite the
expected scope, resolve the stop condition before choosing their checks.
Full SDK journey suites run once at integrated change review.

## Material Stop Conditions

Stop for specification authority if completion needs state-only writes, ambient
Run selection, different Card/Run correlation, a new durable contract,
permission/tenant changes, altered validation/error stages, a new lifecycle or
durability guarantee, query/table-management additions, a dependency/feature,
or removal of retained Observe/direct-Bifrost behavior. Private scoped-handle
layout, adapter placement and fixture extensions remain implementation-owned.

## Authority Links

[Approved revision 6](../spec.md),
[workflow](../../../spec-driven-development.md),
[execution](../../../implementation-execution.md),
[testing](../../../../TESTING.md),
[client ownership](../../../../architecture/wyrd-design.md#client-model),
[Bifrost](../../../../architecture/bifrost-design.md),
[Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md),
[TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md).

## Implementation Evidence

Commits: `b2c59df3d` (shared client, Python), `e28a0e249` (TypeScript,
examples, Rust journey), `1fb03699c` (docs, MCP guide).

**Reuse.** `Run::bifrost()` returns `RunBifrost<'_>` in
`crates/shared/wyrd-client/src/observe/mod.rs`, borrowing the Run exactly as
`Observe` does. `RunBifrost::{write, write_json}` are the former
`Observe::{record, record_json}` bodies moved unchanged: the same
`DATASETS_PREFIX` check before lifecycle lookup, `started_bifrost`,
`Bifrost::resolve_writer_table`, `insert_observation` and `Run::correlation`.
The gap was only the public capability name; no writer, validator, queue,
lifecycle or Run selection was added. Python projects it as
`wyrd._wyrd.observe.RunBifrost` (`PyRun::bifrost` getter); TypeScript as
`RunBifrost` over the existing `NativeRun`, whose native `record` became
`write` with unchanged `NativeLifecycleResult` projection.

**RED.** `test_run_exposes_bifrost_write_and_observe_verification` against the
pre-change extension: `ImportError: cannot import name 'RunBifrost' from
'wyrd.observe'` (capability absent).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-012, AC-010 | `Run::bifrost`/`RunBifrost` (Rust), `PyRunBifrost` + `wyrd.observe.RunBifrost`, TS `Run.bifrost`/`RunBifrost`; `Observe::record`, `record_json`, `PyObserveHandle::record`, TS `Observe.record` removed, no aliases | Python surface test (1 passed; asserts `record` absent, `verify`/`submit` present); `cargo check --all-targets -p wyrd-ai -p wyrd-rust-examples`; `py:typecheck`; `ts:typecheck` | PASS |
| REQ-013, AC-009 | Rust `observe_a_run::writes_land_under_the_view_that_wrote_them` added; Python/TS attribution tests migrated in place, names kept | Rust 1 passed; Python `test_records_land_under_the_view_that_wrote_them` 1 passed; TS `records land in their tables under the view that wrote them` 1 passed | PASS |
| REQ-013, INV-002, AC-009 | Refusal tests migrated without weakening: reserved/system/malformed tables (`WYRD_SDK_400_INVALID_OBSERVATION`, before describe), unknown table (404 / `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`), non-string key / strict JSON (`WYRD_SPEC_400_VALIDATION`), not started (`WYRD_SDK_400_BIFROST_NOT_STARTED`), stale-bundle Card scope (`WYRD_VALA_403_BIFROST_CARD_SCOPE` at drain) | wyrd-client `observe::tests` + `bifrost::` 87 passed; Rust `observe_a_run::` 7 passed; Python surface unit file 70 passed; Python observation-surface integration 7 passed; TS unit `observe.test.ts` 38 passed | PASS |
| INV-001/003, REQ-013 | Source: moved bodies call the existing `resolve_writer_table`/`insert_observation`/`started_bifrost`; no new type beyond the scoped handle | `write_describes_a_dataset_table_once`, `concurrent_first_writes_describe_each_table_once` pass (one describe, one producer per table) | PASS |
| REQ-014, AC-010 | `__init__.py`/`.pyi` export `RunBifrost` with buffered-submission docs; `index.d.ts`/`.d.cts` regenerated by `ts:build`; examples (support-desk, support-desk-app ×3 languages), docs pages, TS README, MCP guide updated; `llms-full.txt` regenerated | `py:lints` (stub parity), `ts:napi:check`, `ts:pack:check`, `docs:check` all exit 0; TS `observation-surface`, `observe-a-run`, `bifrost-write` 30 passed | PASS |

Commands (repository root, all exit 0): the three build steps; the exact
selectors listed above under Public surface proof, Attributed-write journeys
and Shared pipeline regressions; `fmt`, `lints`, `py:format`, `py:lints`,
`py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`,
`ts:pack:check`, `docs:check`, `git diff --check`.

Remaining old-spelling hits outside `changes/`: `wyrd/bifrost/_client.py`
names the separate telemetry `wyrd.observe.record` function, which is retained.
Other active packets' historical plans are untouched. Non-goals stayed
excluded: no state-level writer, ambient Run selection, scoped query/table
management, dependency, feature, or wire/persisted contract change; direct
`Bifrost::insert`/`write_batch` unchanged. Linker `__eh_frame` warnings during
builds are toolchain messages, not code warnings.

Status: `IMPLEMENTED`; routes to `$wyrd-task-review`.
