---
id: TASK-002
kind: implementation
status: implemented
spec: SPEC-card-authoring
spec_revision: 3
spec_approval: approved
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, INV-001, INV-002, INV-003, INV-004, INV-005, AC-002, AC-003, AC-005, AC-007]
depends_on: [TASK-001]
---

# Expose common Card authoring through Python and its authored stubs

## Outcome and Value

A Python developer constructs `Verifier(...)` and `Service(...)`, adds steps
and component bindings, and declares through existing Cards or WyrdState.
Pydantic evidence models can describe inputs; Python does not implement its
own registration graph, validation engine or uploader.
This task adds only ergonomics and native argument/type adapters; it introduces
no registration or upload path and reuses existing machinery through Rust.

The Python journey independently authors the exact logical multi-component
scenario introduced by Task 1, rather than loading a bundle produced by Rust.
Begin after spec approval and integration of Task 1's public common interface.
That dependency includes the entire [native primitive surface](TASK-001-common-rust-authoring.md#complete-native-primitive-surface),
not just Service/Verifier or the types used in the sample journey.

## What Changes and What Must Stay True

Expose the shared Rust authoring types through PyO3 and `wyrd.authoring`, with
thin schema conveniences. Task 1 owns all core definitions, enums, helper
behavior and canonical conversions, including AgentCheck, AssertCheck and all
other check/action/reference/input/Artifact families in its inventory. Expose
that complete surface with native-backed constructors and authored stubs;
do not implement missing core types or their meaning in Python. Python-only
work is argument/schema adaptation, idiomatic names and binding behavior.
Direct construction and edits are synchronous and
network-free. Add methods mutate and return the receiver; added children are
captured by value. No builder/build requirement, JsonInput wrapper, Python
closures, graph compiler, temporary file conversion or duplicated registry loop.

Accept plain supported Python scalar types, canonical JSON Schemas and optional
Pydantic models. Converting an input model to a schema is separate from
serializing evidence at execution. Preserve required/nullable semantics and
the existing server refusal/inconclusive distinction. Plain-schema authoring
must not import or require Pydantic.

Local Artifact descriptors and file selections lower through common Rust.
Both entry points publish local Artifacts first using existing sole-submission
upload calls, then declare the graph with exact registered references. Existing
Artifact CardRefs trigger no preparation or upload. This sequencing is Rust-owned;
Python adds no registration machinery. A later graph failure retains publication.
Cards returns its existing declaration result and WyrdState its existing
hydrated state; preserve path/selected-holder calls. Python code must never own
upload/activation/retry or registration meaning. New PyO3 code lives only in
this SDK; release the GIL around existing blocking IO and use the shared runtime.

Register new native classes and public imports; update all corresponding
authored `.pyi` files and exports/`__all__`. New public methods document inputs,
return/mutation behavior and structured errors. Test Python-owned lifetimes in
Python, keeping production and private testing imports separate.

## Approach

1. Inspect the integrated common interface, then follow existing PyO3 Card and
   state adapters to extend accepted inputs without replacing their lifecycle.
   Map every Task 1 primitive to its native registration, public Python export
   and authored stub before limiting runtime examples to the finance subset.
2. Add offline construction/schema/lifetime tests, then native wrappers and
   small Python schema adapters over shared authoring values.
3. Create the same `card_authoring` story through Python constructors/edits,
   using Task 1's policy/source fixtures, identities and case expectations.
4. Close Artifact reuse, registration parity and error paths through actual
   public Python calls. Complete public/native stubs and export registration.
5. Run focused journeys, typing and stub checks; update Python-facing authoring
   documentation/examples after the real workflow is established.

## Ordered Implementation Scenarios

### Scenario 1 — Define and edit Cards without a server or mandatory Pydantic

**Behavior.** Public constructors and add methods work with no reachable
server. Every Task 1 primitive family has its Python native-backed projection,
including variants absent from the finance journey; representative construction
and lowering reaches the common Rust type rather than a Python implementation.
Methods return the same Python receiver, child insertion captures
values, and retained parents remain usable after original child Python
references leave scope. Reject duplicate IDs/aliases with shared errors.
Retain explicit depends_on and inferred output-reference conversion for the
common native dependency behavior proved in Task 1.
Plain schemas/scalars work when Pydantic cannot be imported. When available,
a Pydantic model supplies the intended schema; nullability and omission remain
independent and no stale mutable input changes a declared snapshot.

**RED.** Add planned unit tests `test_definitions_edit_offline` and
`test_plain_schemas_do_not_require_pydantic` in the proposed public-authoring
unit file. Confirm collection and run [offline proof](#offline-proof). Missing
native exports may need minimal skeleton before assertions establish RED;
an import/setup failure alone is not behavioral evidence.

**GREEN.** Add native projections and the minimum type-to-schema adapter,
forwarding to common Rust validation/conversion. Keep runtime dependencies
unchanged; use optional model methods/adapter imports only when applicable.
No local function execution or network side effects during construction.

**REFACTOR.** Remove redundant Python validation/graph logic and duplicate
typing definitions. Keep ordinary Python references/lifetimes safe without
exposing native internals or adding global mutable state.

### Scenario 2 — Author and verify the same multi-component Service in Python

**Behavior.** Python independently creates Service finance-release, both
distinct components and their two-step Verifiers. Real SDK calls register and
hydrate, then run each component's bindings with exact subject identity. The
shared permitted/identifier/recipient/destination cases produce expected
individual outcomes, citations and release/hold decisions. Inconclusive and
request errors follow the complete REQ-007 matrix, including missing report,
provider uncertainty and raw schema-invalid JSON. Cross-component lookup returns exactly
`WYRD_SDK_404_UNKNOWN_VERIFIER`; unchanged declaration is idempotent.

**RED.** Add planned integration test
`test_multi_component_service_verifies_each_subject` in the new authoring story
file. Run [component journey](#component-journey) using the existing real-server,
Postgres and local-provider fixtures. Assert typed refs/outcomes and the actual
selected policy evidence; do not accept only successful serialization.

**GREEN.** Extend Cards/state accepted inputs to native authoring. Build the
same logical graph using Python objects and Task 1's checked-in sources, not
by parsing Rust-generated output. Exercise Pydantic instances as runtime
evidence as well as the equivalent JSON values. Rerun Scenario 1.

**REFACTOR.** Reuse existing test-fixture domain objects and provider responses
without hiding the user actions. Keep every public call through `wyrd.*` and
leave scheduler, permission decisions and durable behavior in Rust/server.

### Scenario 3 — Reuse Artifact preparation and preserve existing entry points

**Behavior.** Through both Cards and state authored-Service calls, a local
file/directory Artifact is published first as a sole submission; the graph then
references its exact registered version. The existing-CardRef variant performs
no upload. Assert that graph failure after successful publication retains that
Artifact and its original graph error. It is reusable across requests. New policy versions retain old
state attribution. Cards and state routes match the file-authored baseline.
Changed immutable bytes, insufficient authority, source conflicts and invalid
references surface the same exact shared codes as the Rust scenario. Existing
path and Data/Model/Prompt holder registration remain valid.

**RED.** Add planned integration test
`test_local_artifacts_and_declaration_parity`; run [parity journey](#parity-journey).
Assert selected file/digest/ref, receipt/state identities and exact error codes,
including `WYRD_REGISTRY_409_SPEC_DRIFT` and
`WYRD_PERMISSION_403_DENIED_RBAC` in their respective existing failure cases.

**GREEN.** Forward source descriptors, arguments and authored input snapshots
to the common Rust path without Python upload loops or temporary YAML. Preserve
exception mapping and partial-progress semantics. Rerun earlier scenarios.

**REFACTOR.** Collapse duplicate conversion only at its existing owner. Keep
new stub annotations and native signatures synchronized; do not weaken the
Pydantic-free or original holder/path coverage.

## Acceptance Criteria

| Obligations | Required evidence |
|---|---|
| REQ-002–004, REQ-008–009; AC-002, AC-005; INV-001–003 | Scenario 1 proves direct edits, return identity, lifetime/value capture, model/plain schema support and local error behavior without server/Pydantic requirements. |
| REQ-006–009; AC-005; INV-001–005 | Scenario 2 proves actual Python graph creation, both multi-step components, all shared cases, exact binding/subject refs, evidence and unbound refusal. |
| REQ-005–006, REQ-008–009; AC-003, AC-005; INV-002, INV-004 | Scenario 3 proves common local upload reuse/versioning, receipt/state and file parity, exact stable errors and prior declaration inputs. |
| REQ-009; AC-005 | Native registration, public imports, exports/`__all__`, authored public/native stubs and typing examples agree; stub/lint/type checks pass. |
| REQ-001, REQ-009; AC-007; INV-001, INV-003 | Offline native-export/construction coverage maps every Task 1 inventory family to Python with correct stubs; no missing core definition/helper is supplied only by Python. |

## Implementation Reference

- **Existing Python boundary:** `sdks/wyrd-sdk-python/src/state/mod.rs`
  owns `Cards`, `WyrdState.declare`, `holder_kind`, `upload_artifact`,
  `build_declaration_input` and selected-holder declaration adapters. They
  already forward to shared Rust; `holder_kind` currently allows only selected
  concrete holders. Missing: authored-definition input conversion and public
  native types, not registration logic.
- **Shared owner:** Task 1's client authoring reexports and existing
  `Cards::{declare,declare_from_path,upload_artifact}` / state hydration remain
  authoritative. No shared crate gains a Python feature or PyO3 dependency.
- **Schemas/evidence:** existing `model_dump_json` detection in
  `src/observe/mod.rs` serializes model evidence. Prompt/telemetry code already
  accepts model-derived schemas. Add only authoring schema conversion; do not
  reimplement native schema/slot validation or mutate observed values.
- **Registration/exports/stubs:** `src/lib.rs` and native module wiring,
  `python/wyrd/cards/{__init__.py,__init__.pyi}`,
  `python/wyrd/state/{__init__.py,__init__.pyi}`, the new public authoring module
  and corresponding `_wyrd` stubs are the likely closure. Public and native
  nesting may differ, but imports/`__all__` must agree. Pure Python definitions
  are authored once and reexported; `.pyi` files are never generated.
- **Test owner:** `tests/integration/conftest.py` owns real-server/local
  `receiver`, Cards and permission fixtures. `test_verifier_building_blocks.py`
  covers actual uploaded policy reads/citations; `cards/test_cards_crud.py`
  and `test_docsite_quickstart.py` cover holder/path declaration and errors.
  New story file tests Python authoring using the common fixture assets; do not
  move Python lifetime or stub behavior into Rust.

## Verification and Evidence

All newly named tests/paths below are planned; confirm actual selectors before
RED. Run sequentially. Build prerequisites: `mise run build:server` and
`mise run py:setup`. The integration wrapper/migration matches the owning
Python lane; local provider fixtures supply responses without external keys.

### Offline proof

```bash
mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q sdks/wyrd-sdk-python/tests/unit/cards/test_authoring.py::test_definitions_edit_offline sdks/wyrd-sdk-python/tests/unit/cards/test_authoring.py::test_plain_schemas_do_not_require_pydantic
```

### Component journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/cards/test_authoring_journey.py::test_multi_component_service_verifies_each_subject"
```

### Parity journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/cards/test_authoring_journey.py::test_local_artifacts_and_declaration_parity"
```

Run `mise run py:format`, `mise run py:lints` (includes authored-stub checks),
`mise run py:typecheck`, and `mise run check:py-wheel-no-testing`. If Rust
wrapper code changes, also run `mise run fmt`, `mise run lints` and the focused
dependency boundary check `mise run check:deps`. Finish with
`mise exec -- git diff --check`. Native registration/public exports/stub typing
are part of this task, not deferred documentation work. Follow the PyO3
checklist's six wiring layers when implementation changes native classes.
Broader Python journeys run once on the integrated candidate.

## Material Stop Conditions

Stop if the wrapper requires new wire fields, registration/upload behavior,
server validation semantics, security/tenancy behavior, Python-owned durable
state, mandatory Pydantic or another dependency, or exposing private/testing
types in production. Correct private binding/stub plumbing locally; do not
leave exports or authored stubs knowingly incomplete.
If a required native primitive/helper is missing, route the gap back to Task 1
under the same specification. Do not invent a Python implementation to bypass
that dependency or call the finance-only subset a complete projection.

## Authority Links

- [Approved spec revision 3](../spec.md), [common Rust dependency](TASK-001-common-rust-authoring.md)
- [Python examples](../examples/python/README.md), [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md)
- [Repository rules](../../../../AGENTS.md), [contracts](../../../../architecture/wyrd-design.md)
- [Workflow](../../../spec-driven-development.md), [execution](../../../implementation-execution.md), [testing](../../../../TESTING.md)

## Evidence

Implemented against spec revision 5 (constructor-only fixed arguments, field
setters replace, `add_*` appends, `Check.assert_`, `Artifact.from_path(...).extract(True).file(...)`),
superseding this task's revision-3 wording.

### Reuse map

| Need | Existing owner reused | Gap closed | Why sufficient |
|---|---|---|---|
| Card values, validation, plan, lowering | `wyrd-client` `authoring` (Task 1) | `From<Value>` canonical forms for `Component`, `Binding`, `Step`, `Requirement`, `Action`, `Check`, `InputSlot` via `Body::authored` (`authoring/{mod,kinds,verifier}.rs`) so mapping inputs reach the shared value, not a Python copy | One canonical conversion, shared by every SDK. |
| Declaration, local Artifact publication | `Cards::declare`, `WyrdState::declare_with` over `Declarable` | `src/state/mod.rs` forwards authored Cards and paths; holders keep `PythonCardRegistry` | No Python upload, retry, or registration loop. |
| Evidence serialization | `observe::json_value` (`model_dump_json`) | none | Pydantic instances verify as JSON. |
| Native surface | PyO3 module wiring in `src/lib.rs` | `src/authoring.rs`: 23 classes, schema adapters (scalar types, `model_json_schema()`, mappings) | Thin argument adaptation only; Pydantic never imported. |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002–004, REQ-008–009; AC-002, AC-005; INV-001–003: offline edits, return identity, value capture, lifetimes, schemas without Pydantic | `src/authoring.rs`, `python/wyrd/authoring/` | `tests/unit/cards/test_authoring.py::{test_definitions_edit_offline,test_plain_schemas_do_not_require_pydantic,test_pydantic_models_supply_input_schemas}` | PASS |
| REQ-006–009; AC-005; INV-001–005: Python graph, both two-step components, REQ-007 matrix, citations, unbound refusal, idempotent rerun | `tests/integration/cards/test_authoring_journey.py`; `conftest.sharing_verdict` judge | `test_authoring_journey.py::test_multi_component_service_verifies_each_subject` | PASS |
| REQ-005–006, REQ-008–009; AC-003, AC-005; INV-002, INV-004: publish first, CardRef reuse, versions, retained Artifact, file parity, `WYRD_REGISTRY_409_SPEC_DRIFT`, `WYRD_PERMISSION_403_DENIED_RBAC`, `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`, `WYRD_LOADER_400_INVALID_ENVELOPE`, path and holder declaration | `src/state/mod.rs` `Cards.declare`, `WyrdState.declare` | `test_authoring_journey.py::test_local_artifacts_and_declaration_parity` | PASS |
| Approach 5: Python-facing example after the real workflow | `examples/finance-release/finance_release.py`; `examples/README.md` row | `test_authoring_journey.py::test_finance_release_example` (runs the program from an empty directory against a fresh server; output matches the Rust program's) | PASS |
| REQ-009; AC-005: registration, imports, `__all__`, authored stubs agree | `src/lib.rs`, `python/wyrd/{__init__,authoring/__init__}.py[i]`, `cards`/`state` stubs | `tests/unit/test_public_exports.py`; `py:lints` (stub check); `py:typecheck` | PASS |
| REQ-001, REQ-009; AC-007; INV-001, INV-003: every Task 1 family projected natively | `src/authoring.rs` (all 12 kinds, `CardDefinition`, children, `CardRef`, `ArtifactFile`, `CodeSource`) | `test_authoring.py::test_every_native_family_constructs_canonical_specs`; `wyrd-client` `authoring::tests` (7, incl. `canonical_objects_match_constructed_values`) | PASS |

Commands: `mise run build:server`; `mise run py:setup`; offline proof; the three journeys via the task's Postgres wrapper (together, 3 passed; with `cards/test_cards_crud.py`, 12 passed); `mise run py:format`; `mise run py:lints`; `mise run py:typecheck`; `mise run check:py-wheel-no-testing`; `mise run fmt`; `mise run lints`; `mise run check:deps`; `git diff --check`. All passed.

Decision: authored inputs are typed with a covariant `JsonInput` (`Mapping`/`Sequence`) alias so ordinary literal dicts type-check; returned specs and plans are `dict[str, Any]` as `HydratedCard.spec` is.

Non-goals held: no new wire field, registration or upload path, dependency, feature, or Python-owned validation; Pydantic remains optional. Commits `e50d0fbe2`, `1e3fac923`, and the finance example commit.
