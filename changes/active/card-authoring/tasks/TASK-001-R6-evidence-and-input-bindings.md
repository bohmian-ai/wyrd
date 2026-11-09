---
id: TASK-001-R6
kind: remediation
status: proposed
spec: SPEC-card-authoring
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-009, REQ-010, AC-001, AC-005, AC-006, AC-007, AC-008, AC-009]
parent_task: TASK-001-R5
remediates: []
depends_on: []
---

# Separate evidence declarations from executable argument bindings

## Outcome and Value

Declare evidence once on a Verifier, then bind supplied values to the arguments
of Actions and Checks. Missing evidence must remain missing, rather than turn
into null and let a check pass or skip. Ordinary JSON evidence needs only a
name and schema, without a mandatory InputSlot.json wrapper.

For example, Python `verifier.add_input("report", Report)` declares evidence
against a Pydantic model. `check.bind_input("document", "inputs.report")`
passes that evidence to the check's document argument. An Action uses the same
binding operation to compute data; a Check uses it to produce a verdict.

## Authority and Execution Boundary

This records the maintainer-requested remediation, including explicit agreement
to keep Verifier.add_input and rename Check/Action.add_input to bind_input.
The maintainer approved specification revision 6 with “revise the spec and
approve”. That revision and the updated architecture supersede revision 5's
optional slots and Action/Check add_input methods. No review finding IDs are
invented for this human-directed request. No specification decision remains pending.

## What Changes and What Must Stay True

- Remove slot-level required, SDK setters and omission-to-null behavior from
  every evidence-slot form that currently supports them. Reject explicit
  slot-level required, true or false, through canonical validation. JSON Schema's
  required keyword inside a slot's schema remains valid and unchanged.
- An omitted slot makes dependent checks inconclusive, including checks or
  conditions testing null. If evaluating a condition attempts to resolve missing
  evidence, the dependent check is inconclusive, not intentionally skipped.
  Preserve existing evaluation order; merely mentioning a reference does not
  introduce eager evaluation. Unrelated checks may continue; omission is not a request
  refusal. Preserve existing overall verdict aggregation.
- Supplied null must satisfy its JSON Schema. Nullable data, optional object
  fields and conditions over supplied evidence remain supported. Missing traces
  cannot establish that an event did not occur.
- Keep Verifier.add_input(name, schema/slot). Rename Action and Check incremental
  methods to bind_input(argument, reference), with idiomatic language casing.
  Keep their canonical inputs field and whole-map inputs setters/options: those
  maps already represent argument bindings. Do not rename wire fields or add
  compatibility aliases. Binding values retain existing supported semantics.
- Revise the shared Rust authoring interface first where necessary, then its
  Rust SDK exports and Python/TypeScript projections. Rust accepts a bare JSON
  Schema value directly; Python accepts a schema dictionary or supported model
  class. Dictionaries are schemas, not example data for schema inference.
  Preserve explicit canonical slot objects and distinct media/trace/samples/
  group-source forms. No new wrapper or replacement authoring grammar.
- Keep existing validation, by-value capture, duplicate handling, declaration,
  Artifact publication and tenant isolation. Update affected callers, authored
  stubs, generated contracts, fixtures, examples and current documentation.

No migration or compatibility work is in scope.

## Approach

1. Revalidate the existing owners against approved revision 6.
2. Remove optional-slot semantics at the existing contract and resolution owners.
3. Align shared Rust schema conversion and declaration/binding methods; project
   the same operations through the three SDKs and update affected consumers.
4. Prove negative evidence behavior and authoring parity; regenerate consumed
   outputs and update documentation without changing unrelated authoring work.

## Ordered Implementation Scenarios

### Scenario 1 — Missing evidence stays inconclusive

**Behavior.** Omit declared evidence used by an assertion, executable binding or
condition. Dependent checks are inconclusive, never passed or deliberately
skipped because omission became null. An independent supplied-input check still
runs. Supplied null passes schema admission only for a nullable schema. Explicit
slot-level required is rejected; nested JSON Schema required remains accepted.

**RED.** Extend the existing engine and canonical validation tests with those
cases. Demonstrate the current optional omission-to-null behavior and acceptance
of the removed field fail the new expectations. Use focused selectors for the
actual added/renamed tests before RED; the existing engine regression commands
below establish the current owner, not a requirement to retain obsolete names.

**GREEN.** Remove the field and optional resolution branch through existing
owners, keeping supplied-value validation and unrelated work intact. Prove the
same outcomes through real-server SDK and MCP/HTTP verification journeys.

**REFACTOR.** Delete now-unused requiredness helpers and optional-only fixtures;
retain independent negative and schema-validation coverage.

### Scenario 2 — Declare evidence and bind arguments without extra wrappers

**Behavior.** Rust and Python declare ordinary JSON evidence by name and schema;
Python also accepts a Pydantic model class. Verifier.add_input declares evidence;
Action/Check.bind_input binds it to an executable argument. Canonical fields and
executed results agree across SDKs and file-authored definitions. Canonical
non-JSON slot construction, capture and duplicate refusal remain intact.

**RED.** Extend the existing Rust public authoring inventory to use a bare schema
and bind_input on both Action and Check. Confirm the missing API/conversion,
then exercise equivalent SDK authoring journeys. Python's already-supported
schema/model shorthand needs regression proof, not an invented RED.

**GREEN.** Align the existing shared Rust conversion and methods, then bindings,
stubs/exports and affected callers. Rerun Scenario 1 and the changed authoring
journeys. No aliases for the removed Action/Check method.

**REFACTOR.** Reuse shared conversion and capture owners. Remove obsolete wrapper
usage and duplicate adapter logic while keeping canonical validation centralized.

## Acceptance Criteria

Revision 6 maps the existing authoring obligations through REQ-001/002/003/004/
009/010, with AC-009 covering the corrected missing-evidence behavior.

| Outcome | Owner and proof |
|---|---|
| REQ-003 / AC-009: missing evidence never turns into null, pass or intentional skip | Vala resolution/engine tests and real-server verification journeys, including independent-work and missing-condition cases. |
| REQ-003 / AC-009: removed field rejected; nullable data unchanged | Canonical parsing and declaration journeys reject slot-level required and accept nested schema required; null-schema cases retain results. |
| REQ-001/002/009/010 / AC-001/005/006/007: declaration and binding names are distinct in Rust and projected SDKs | Public inventory and SDK authoring journeys construct and execute both Actions and Checks with bind_input; public export/stub inspection finds no old incremental alias. |
| REQ-003 / AC-001/005/006/007: bare schemas work without wrappers | Rust public inventory and Rust/Python authored finance journeys agree with canonical definitions; Python schema and model-class paths both work. |
| Existing behavior outside the change survives | Existing authoring capture/duplicate/Artifact regressions and scoped contract checks pass; no new wire binding field. |

## Implementation Reference

- `crates/wyrd-spec/src/card/verifier/mod.rs::InputSlot` and its remote
  TypedInputSlot deserializer own slot fields and reject unknown fields.
  Extend that owner; do not add an SDK-only validator.
- `crates/vala/vala-verify/src/blocks/scope.rs::Scope::resolve` currently maps
  omitted optional slots to null. `blocks/engine.rs` owns supplied-input
  conformity and condition execution; its inline test module already covers
  optional omission, null schemas and skipped work. Remove the exceptional
  resolution, retaining shared evidence handling.
- `crates/shared/wyrd-client/src/authoring/verifier.rs` owns Verifier/Action/Check
  inputs methods and InputSlot::from(Value). The latter currently preserves bare
  schemas as canonical slots rather than wrapping them: extend this existing
  conversion, retaining canonical forms and deferred validation.
- `sdks/wyrd-sdk-python/src/authoring.rs::slot` already supports bare schemas
  and model classes. Align with shared semantics; update runtime methods and
  `python/wyrd/authoring/__init__.pyi` together. SlotInput/JsonInput are typing
  aliases, not new evidence types or a reason to add runtime wrappers.
- `sdks/wyrd-sdk-rust/tests/integration/authoring.rs` public inventory and
  `sdks/wyrd-sdk-python/tests/integration/cards/test_authoring_journey.py`
  exercise real authoring consumers. Extend their existing journeys and the
  corresponding TypeScript and MCP/HTTP owners; do not create a parallel harness.
- Update `examples/finance-release/`, relevant `fixtures/cards/`, current packet
  examples and SDK documentation; preserve historical revision/review evidence.

## Verification and Evidence

Existing focused commands, confirmed from current package/module ownership:

```sh
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::missing_and_optional_evidence_resolve_honestly) | test(=blocks::engine::tests::supplied_null_follows_the_slot_schema_not_requiredness)'
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'
```

Record replacement selectors after renaming obsolete tests. Before each new
RED, confirm its actual selector and selected count. For SDK and MCP/HTTP
journeys, record exact focused selectors and repository-managed server/Postgres
setup from their current owners; no guessed commands or whole-suite substitutes.
Use those journeys to prove observable results, not only canonical serialization.

Run `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:typecheck`, `mise run ts:typecheck` and `mise run codegen:check` for
the implementation's affected surfaces. Generated outputs come from their owners;
Python stubs are authored. Record RED/GREEN and final results. Documentation and
removed-export inspection use static proof. Do not run gate for this task.

## Material Stop Conditions

Implement against approved revision 6, not historical revision 5. Reopen authority if
execution requires a new migration, changed overall verdict aggregation, renamed
wire fields, compatibility aliases or new evidence types.

## Authority Links

- [Approved spec, revision 6](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Repository rules](../../../../AGENTS.md)
- [Execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
