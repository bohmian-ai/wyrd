---
id: TASK-009-R3
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 45
requirements: [REQ-151, AC-032]
depends_on: [TASK-009, TASK-009-R1, TASK-009-R2]
parent_task: TASK-009
remediates: [FIND-TASK-009-7]
---

# Prove same-Run asyncio scope isolation

## Authority and immutable inputs

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 45.
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`.
- Prior remediations:
  `changes/active/verified-change-contract/review/TASK-009-r1/TASK-009-R1-restore-otel-correlation.md`
  and
  `changes/active/verified-change-contract/review/TASK-009-r2/TASK-009-R2-close-proof-and-boundary-parity.md`.
- Base: `7d96c30066425e0cde2290842d5801307843283d`.
- Reviewed candidate: `d01307b8c37b47488115743c94b624a29d66e4be`.
- Independent ledger:
  `changes/active/verified-change-contract/review/TASK-009-r3/findings-validation.md`.

## Issue diagnosis

### `FIND-TASK-009-7` — identical-Run concurrent-task proof is missing

REQ-151 prohibits storing one shared OpenTelemetry attach token on the
immutable Python Run. AC-032 and TASK-009 Scenario 2 require focused async
evidence for concurrent tasks using that same immutable Run.

The current case
`tests/unit/state/test_observe_surface.py::test_scope_survives_await_and_isolates_concurrent_tasks`
creates `run.for_card("model")` and `run.for_card("backup")`, then gives one
distinct `PyRun` object to each task. It proves execution-local propagation and
sibling-Card isolation, but it stays green if tokens are incorrectly stored on
each Run object: each task owns a different object and therefore a different
slot. Nested same-object uses are sequential in one execution context, the
threaded case proves lazy key publication with distinct views, and the real
journey enters one object once. No existing case closes the explicit
same-object concurrency obligation.

A per-object-token regression is observable when two task contexts enter the
identical `PyRun`: one entry overwrites the other's token, and an interleaved
exit can detach the other task's scope or leave stale correlation after exit.
Because optional telemetry errors are swallowed, current tests could remain
green while a span becomes uncorrelated or misattributed and no longer joins
the Run's custom or Eval evidence.

The production owner already appears correct. `wyrd.otel` keeps exact
token/prior entries in an execution-local `ContextVar` stack and does not store
them on `PyRun`. The shortfall is the required regression proof, not diagnosed
production behavior.

## Intended correction outcome

The focused Python surface proves that two coordinated asyncio tasks may enter
the identical immutable Run object, exit in an interleaved order, and retain
independent execution-local correlation. Exiting one task removes correlation
from that task only; the other task remains correlated until its own exit; no
correlation remains after both scopes end.

## Decision-complete recommendation

Keep the correction in the existing Python focused test home. Reuse its stock
OpenTelemetry provider/exporter, `_correlation` assertions, immutable Run, and
native asyncio coordination. Add or extend one async case so both tasks receive
and enter the exact same `run` object. Coordinate the tasks by events or another
deterministic asyncio primitive so one exits while the other remains inside its
scope.

From each task, create a span after that interleaving point. The exited task's
span must have no Wyrd pair; the still-entered task's span must retain the
Run's exact `(card_ref, run_id)` pair. After both tasks exit, create a final
span and assert it has no Wyrd correlation. Keep the existing sibling-Card
isolation and task-created-inside-scope coverage; they prove separate required
behaviors and do not substitute for this case.

Use deterministic coordination, not sleeps, timeouts, retries, or scheduler
luck. Do not change `wyrd.otel` production code unless the new proof first
demonstrates an actual defect in the current implementation. Do not add a
helper abstraction, test harness, dependency, provider wrapper, or second
integration journey. The existing owner and test mechanisms are sufficient.

## Constraints and preserved behavior

- Preserve shared Rust ownership of Run identity and hydrated Card selection.
- Preserve one UUIDv7 Run ID across immutable Card views and strict local alias
  failure.
- Preserve optional, fail-open Python OpenTelemetry integration and strict
  explicit observation, validation, and authorization failures.
- Preserve active-span stamping, child-span enrichment, nesting, `await`, task
  context copying, sibling-Card isolation, concurrent first key creation,
  provider idempotency, and failed-detach restoration.
- Preserve caller-owned provider/exporter lifecycle and the separate explicit
  tracer flush, state shutdown, and server publication barriers.
- Preserve the public Rust, Python, and TypeScript APIs and generated
  declarations.

## Non-goals

- No server Run, Bifrost/Gate/Scribe contract change, new correlation field,
  wrapper span, second pipeline, queue, exporter, or provider lifecycle owner.
- No production OTel refactor when the current execution-local owner passes the
  new proof.
- No raw-thread context-propagation guarantee.
- No new dependency, fixture, test harness, warning, retry, or timeout-based
  synchronization.
- No replacement of the existing sibling-view, copied-task, nested, failure,
  or authenticated persisted-join coverage.

## Acceptance criteria

1. `FIND-TASK-009-7`: two coordinated asyncio tasks enter the exact same
   Python `Run` object concurrently.
2. After one task exits while the other remains entered, a new span in the
   exited task has no Wyrd correlation and a new span in the active task has
   the Run's exact CardRef and Run ID.
3. After both tasks exit, a new span has no Wyrd correlation.
4. The proof is deterministic and would fail if one attach token were stored
   on the shared `PyRun` object.
5. Existing sibling-Card isolation, nested restoration, `await`, copied-task,
   first-use concurrency, failure containment, provider idempotency, and
   persisted authenticated join behavior remain green.

## Focused proof and broader verification

Name the exact new or adapted test in implementation evidence and run it from
the Python SDK with the repository toolchain:

```bash
mise run py:setup
cd sdks/wyrd-sdk-python
mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::<exact_same_run_async_test>
mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py
```

Then run the unchanged authenticated journey and the touched-surface gates from
the repository root:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run py:format
mise run py:lints
git diff --check
```

Route this remediation directly to `$wyrd-implement`. A later task review must
reassess the complete original base-to-remediated-candidate range, not only this
test correction.
