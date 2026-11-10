---
id: TASK-001-R6-R1
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 6
parent_task: TASK-001-R6
remediates: [FIND-TASK-001-R6-1, FIND-TASK-001-R6-2]
depends_on: []
---

# Finish binding-method journey proof and changed-item rustdoc

## Subject and authority

Approved [revision 6](../../spec.md), original [R6 task](../../tasks/TASK-001-R6-evidence-and-input-bindings.md).
Reviewed base `433b22b1f2d90cb90823a8c1f32b92f5d874efeb` to candidate
`ecabe947737ca0bfc40f190e53fe5fe1ee5069f3`.
[Independent diagnoses](findings-validation.md) and [verdict](verdict.md).

## Diagnosis and required correction

**FIND-TASK-001-R6-1 — required journey proof is missing.** The task explicitly
requires SDK authoring journeys to construct and execute both Actions and Checks
using bind_input. Rust's real graph uses whole-map inputs for both, at
`sdks/wyrd-sdk-rust/tests/integration/authoring.rs:187–191` and `:245–250`.
Python's Action uses whole-map inputs at
`sdks/wyrd-sdk-python/tests/integration/cards/test_authoring_journey.py:167–169`;
its Check already uses bind_input. Static inventories establish canonical map
shape and the macros delegate correctly, but the real journeys never call those
missing operations. No runtime defect is claimed.

Use bind_input for an actually consumed Action argument in each existing SDK
graph and an actually consumed Rust Check argument. Reuse the existing
multi-component journeys and their positive/negative assertions. Keep some
whole-map construction coverage, existing sources and Artifact mounts. Avoid
duplicating a key between the map and incremental call: duplicate refusal is
intentional. A few call substitutions close the explicit acceptance gap; no
production change, new test, helper or harness is needed.

**FIND-TASK-001-R6-2 — changed Rust items miss mandatory rustdoc sections.** The
expanded canonical conversion test at
`crates/shared/wyrd-client/src/authoring/tests.rs:1295–1299` contains assertions
and expect_err but has no Panics section. The newly named Python Action/Check
bind_input methods at `sdks/wyrd-sdk-python/src/authoring.rs:878–882,927–931`
are emitted by the map-add arm at `:389–400`; it returns PyResult and propagates
JSON conversion failure without an Errors section. Root AGENTS and
`agent_docs/rust.md` require those sections, including for tests. Stubs and green
lint cannot replace Rust item documentation.

Add the test's panic contract and the existing map-add macro's conversion-error
contract. Use that shared documentation owner for both methods. Preserve all
signatures, conversion order, mutation and error propagation. Do not expand this
bounded correction into cleanup of unchanged items or historical sketches.

## Acceptance and proof

| Finding | Required observation |
|---|---|
| FIND-TASK-001-R6-1 | Existing real Rust journey executes consumed Action and Check arguments authored with bind_input; existing Python journey executes an Action argument authored with bind_input and retains its Check case. Their existing success/failure results still pass. |
| FIND-TASK-001-R6-2 | Changed test has its panic contract; both emitted fallible Rust binding methods have the conversion-error contract. Runtime bodies and signatures are unchanged. |

This is verification of already-correct runtime behavior plus documentation;
no manufactured behavioral RED or new production logic is required. Inspect the
changed graph construction as well as the journey results: rerunning unchanged
whole-map graphs does not close the proof gap.

Run setup once and these exact existing selectors from the repository root:

```sh
mise run build:server
mise run py:setup
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::multi_component_service_verifies_each_subject)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/cards/test_authoring_journey.py::test_multi_component_service_verifies_each_subject'
mise run fmt:check
mise run py:lints
```

Record source/doc inspection and command results. Apply any additional narrow
format/lint check required by the actual write set; do not repeat full journey
suites, broad gates or unrelated runtime proof.

## Preserved behavior and non-goals

Preserve accepted omission/null semantics, canonical field names, schema
conversion, capture, duplicate refusal, validation, publication and tenant
separation. Breaking removal is accepted. No migration, compatibility alias,
TypeScript authoring implementation, new public API, dependency or harness.
Route directly to `$wyrd-implement`; the next review checks these two findings
and affected callers, carrying unaffected R6 evidence forward.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Rust journey executes Action arguments authored with `bind_input` | `sdks/wyrd-sdk-rust/tests/integration/authoring.rs` `fact_step` binds each input incrementally | `authoring::multi_component_service_verifies_each_subject` (journey profile, real server + Postgres) | PASS |
| Rust journey executes a consumed Check argument authored with `bind_input`, keeping whole-map coverage without a duplicate key | `report_policy`: `.inputs` for `report`, `recipient`, `purpose`; `bind_input("destination", ...)` then `add_workspace`; judge prompt consumes `{{destination}}` | same journey | PASS |
| Python journey executes Action arguments authored with `bind_input`, keeping its Check `bind_input` case | `test_authoring_journey.py` `fact_step` loop over `action.bind_input` | `test_multi_component_service_verifies_each_subject` (integration, real server + Postgres) | PASS |
| `canonical_objects_match_constructed_values` documents `# Panics` | `crates/shared/wyrd-client/src/authoring/tests.rs` | `mise run fmt:check` | PASS |
| Python map-add arm documents `# Errors` with signature, body, conversion order and mutation unchanged | `sdks/wyrd-sdk-python/src/authoring.rs` doc attributes only | `mise run py:setup`, `mise run py:lints`, Python journey | PASS |

Commands: `mise run build:server`; `mise run py:setup`; both exact journey commands above; `mise run fmt:check`; `mise run py:lints`; `git diff --check`. Non-goals held: no production behavior, new test, helper, or harness; no unchanged item or historical sketch edited; only the four listed files and this evidence changed.
