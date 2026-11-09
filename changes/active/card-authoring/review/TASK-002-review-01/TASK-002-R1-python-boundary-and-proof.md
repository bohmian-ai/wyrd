---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 6
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4]
depends_on: []
---

# Correct Python callback borrowing, schema typing and original task proof

## Subject and authority

Approved [revision 6](../../spec.md), original [TASK-002](../../tasks/TASK-002-python-authoring.md).
Reviewed cumulative base `d6fbbd03d23922931e1c26f5480d8b3ffb6bd260` to
candidate `b599e801130de7bbce3cd892956b51df9f2aaa97`.
[Validated diagnoses](findings-validation.md), [verdict](verdict.md).
Both earlier FIND-TASK-001-R6-1/2 are closed; do not reopen or rename them.

## Diagnosis and selected corrections

### FIND-TASK-002-1 — Schema callback can raise a Rust borrow panic

`sdks/wyrd-sdk-python/src/authoring.rs:337–338` passes a temporary read borrow
of the receiver into `Self::set`. Conversion can call the supported
`model_json_schema()` callback before that borrow drops. If the callback edits
the receiver, its nested setter reaches a mutable borrow and raises
`pyo3_runtime.PanicException: Already borrowed: PyBorrowMutError`. This was
independently reproduced on the freshly built candidate. It bypasses ordinary
Python Exception handling and violates the accepted schema/edit boundary.

End the receiver's read-borrow scope before invoking Python conversion in the
existing shared edit macro. Keep its owned native snapshot and existing
replacement/validation semantics. Existing incremental methods already convert
before borrowing mutably; reuse that boundary principle. This single correction
covers all full-field setters without per-class guards, try-borrow error policy,
new callback scheduling or reentrant merge semantics.

Add one Python-owned offline regression using a supported schema callback that
edits the same receiver. Assert same-receiver return, exact captured schema and
successful subsequent ordinary editing. Do not require nested callback edits to
survive the outer snapshot replacement. Establish RED against this candidate,
then GREEN through the shared correction; keep ordinary capture/error tests green.

### FIND-TASK-002-2 — Two original Python journey cases are unproved

At `tests/integration/cards/test_authoring_journey.py:225,479`, the release
graph has no output-consuming dependent summary, and local Artifact sources are
only individual PDF files. The offline staging test has no consumed upstream
outputs; packet sketches only compile syntax. Native directory support and
Rust's summary journey give credible shared-owner evidence, but do not satisfy
TASK-002's independent Python REQ-007 variation and file/directory publication
proof. No runtime defect is claimed here.

Extend the existing component/parity journeys with their current fixtures and
provider. Preserve the two-step baseline; execute a dependency variation using
existing `release_checks.py::summarize_findings` with recipient and destination
step outputs. Assert actual use of both and a result that changes when a
prerequisite changes. Exercise a directory-backed Artifact through both Cards
and WyrdState declaration, asserting exact registered version and selected content
through the graph/hydration path. Keep file coverage, existing refusal/version/
retained-publication assertions, and public SDK calls. No new harness, uploader,
fixture framework or production machinery. These are verification-only additions
over existing native behavior; do not manufacture production changes for RED.

### FIND-TASK-002-3 — Valid boolean schemas are rejected by public typing

`python/wyrd/authoring/__init__.pyi:80,2551` excludes bool from SlotInput and
InputSlot.json's schema argument. Native validation accepts True/False JSON
Schemas, but independent ty reproduction rejects all four public paths: Verifier
constructor inputs, inputs replacement, add_input and explicit InputSlot.json.
Both unrestricted and always-refusing schemas are therefore unusable in a
correctly typed caller, contrary to REQ-003/009.

Include bool in those two existing declaration owners and accurately document
it as JSON Schema. Keep all other typing precision and native semantics. Add
direct typed calls covering the four paths and both values to the existing
offline file, asserting exact schema lowering. Code inside the Pydantic-free
subprocess string alone is not checked by ty. Use the current type mismatch as
RED, then the narrow annotation correction as GREEN; no Any widening, schema
library, new public alias or runtime validator.

### FIND-TASK-002-4 — Original fallible macro methods miss Errors rustdoc

`src/authoring.rs:345–380` emits fallible single/list/map setters and list-add
methods without Errors sections. Their converters fail and propagate errors.
Root AGENTS and the Rust guide explicitly require those sections on new items.
The prior remediation fixed map-add only; these original TASK-002 arms are newly
reviewed scope, not failed closure of that finding.

Document conversion/error propagation at the four existing macro arms. Preserve
bodies, signatures, ordering and mutation owners. Avoid promising that arbitrary
user callbacks cannot change state when those callbacks themselves fail. Inspect
the emitted representative docs after rebuild. This is static proof, batched
with the functional/proof corrections rather than a separate style round.

## Acceptance and focused verification

| Finding | Required observation |
|---|---|
| FIND-TASK-002-1 | Supported callback edits no longer produce PanicException; same receiver/schema/subsequent-edit assertions pass. Ordinary snapshot and conversion-error behavior remains. |
| FIND-TASK-002-2 | Existing Python component journey executes both prerequisite outputs; parity journey publishes/selects directory-backed Artifacts through both entry points, retaining its file cases. |
| FIND-TASK-002-3 | All four directly typed schema paths accept both boolean schemas; runtime lowering preserves the booleans and type/lint checks pass. |
| FIND-TASK-002-4 | All four fallible macro arms emit their Errors contracts; no runtime behavior is added for documentation. |

Keep new offline assertions in the existing authoring test file; confirm each
new test's actual selector before recording RED. Run these focused commands
from the repository root, rebuilding the changed extension once:

```sh
mise run py:setup
mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q sdks/wyrd-sdk-python/tests/unit/cards/test_authoring.py
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_authoring_journey.py::test_multi_component_service_verifies_each_subject tests/integration/cards/test_authoring_journey.py::test_local_artifacts_and_declaration_parity'
mise run py:lints
mise run py:typecheck
mise run py:format:check
mise run fmt:check
mise run lint:wyrd-sdk-python
mise exec -- git diff --check
```

The server is unchanged and was freshly built in this review. Rebuild it only
if the actual correction changes server/shared runtime code. Record callback
and type RED/GREEN, directory/dependency execution assertions, and final checks.
Do not substitute registration/serialization success for consumed-value proof.

## Preserved boundaries and non-goals

Preserve shared Body/Graph/loader ownership, duplicate refusal, by-value capture,
deferred validation/files, declaration/publication/hydration, error families,
client/tenant propagation, omitted-evidence behavior and exact version lineage.
No migration, compatibility alias, new API/dependency, TypeScript authoring,
callback merge policy, broad refactor, extra harness or full-suite/gate rerun.
Route directly to `$wyrd-implement`; next review checks these four findings and
their affected paths while carrying other accepted TASK-002 evidence forward.

## Implementation evidence

Commits `cc772b379` (callback borrow, boolean schema typing, macro `# Errors`)
and the journey-proof commit that follows it, over candidate `b599e8011`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-002-1: callback edits raise no PanicException; same receiver, exact schema and later edit hold | `src/authoring.rs` `authoring_class!::edit` ends the read borrow (`let current = …`) before `Self::set` converts | RED: `tests/unit/cards/test_authoring.py::test_schema_callback_may_edit_its_receiver` raised `pyo3_runtime.PanicException: Already borrowed: PyBorrowMutError`; GREEN after `mise run py:setup`; whole offline file 7 passed | PASS |
| FIND-TASK-002-2: component journey consumes both prerequisite outputs | `summarized_access` adds `summary` (`summarize_findings` over `steps.recipient.output`/`steps.destination.output`, `depends_on` both); `test_multi_component_service_verifies_each_subject` declares 1.1.0, asserts registered `depends_on`, summary runs last, exact output with both consumed values, and releasable/verdict flipping with a public destination | exact journey selector PASS (with-test-postgres) | PASS |
| FIND-TASK-002-2: directory-backed Artifacts through Cards and WyrdState | `test_local_artifacts_and_declaration_parity` publishes a directory (PDF + `appendix/retention.md`) as 6.0.0 via `Cards.declare` and 7.0.0 via `WyrdState.declare`; asserts registered files, mounted version, 7.0.0 verifier version, the judge citing a mounted policy line, and release; file cases retained | exact journey selector PASS | PASS |
| FIND-TASK-002-3: boolean schemas type and lower on all four paths | `__init__.pyi` `SlotInput` and `InputSlot.json` include `bool`, documented as JSON Schema | RED: `py:typecheck` 6 `invalid-argument-type` on `test_boolean_schemas_are_json_schemas`; GREEN: `py:typecheck` all passed; runtime test asserts exact `True`/`False` lowering for constructor, `inputs`, `add_input`, `InputSlot.json` | PASS |
| FIND-TASK-002-4: four fallible macro arms document errors | `# Errors` doc attributes on the single, list, list-add and map setters | Emitted docs inspected via rebuilt `Agent.prompt`/`Verifier.steps`/`add_step`/`inputs.__doc__`; no body/signature change | PASS |

Final checks: `py:lints`, `py:typecheck`, `py:format:check`, `fmt:check`,
`lint:wyrd-sdk-python`, `git diff --check` OK. Diagnosed one first-run journey
failure: the `summary` assertion assumed sibling step order, but `recipient`
and `destination` share one parallel stage; the assertion now checks the set
and that `summary` runs last. The server and shared runtime were unchanged
and not rebuilt. Non-goals held: no merge policy, try-borrow policy, new API,
dependency, harness, fixture, or TypeScript change; only the three SDK files
above and this record changed.
