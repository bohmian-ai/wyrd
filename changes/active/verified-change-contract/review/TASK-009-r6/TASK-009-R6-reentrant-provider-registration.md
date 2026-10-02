---
id: TASK-009-R6
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 46
requirements: [REQ-151, AC-032]
depends_on: [TASK-009, TASK-009-R1, TASK-009-R2, TASK-009-R3, TASK-009-R4E, TASK-009-R5]
parent_task: TASK-009
remediates: [FIND-TASK-009-14]
---

# Keep provider registration reentrant and fail-open

Route directly to `$wyrd-implement`.

## Authority and immutable inputs

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 46
  (REQ-151 and AC-032)
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Prior remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, TASK-009-R4E
  (superseding TASK-009-R4), and TASK-009-R5
- Review base: `7d96c30066425e0cde2290842d5801307843283d`
- Reviewed candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Validated ledger: `changes/active/verified-change-contract/review/TASK-009-r6/findings-validation.md`

## Issue diagnosis

### `FIND-TASK-009-14` — provider callback re-entry deadlocks registration

Revision-46 REQ-151 requires optional OpenTelemetry provider integration to be
thread-safe, idempotent, attempted at most once per provider, and fail-open so
it cannot block Run entry or application execution.

`install_run_correlation` currently holds the non-reentrant
`_outcomes_lock` while calling the duck-typed provider's
`add_span_processor`. The outcome owner has already published that provider's
provisional `False` result before the call. If the provider callback re-enters
the supported public `install_run_correlation(self)` hook on the same thread,
the nested call blocks acquiring the same lock before it can read that
provisional result. The outer callback waits for the nested call, so direct
private-provider installation never returns. The same path blocks
`Run.__enter__` when the provider is returned by the global-provider lookup.

The concurrency reviewer and focused follow-up reproduced the path in a
bounded fresh process: the callback began, but the process timed out with exit
status 124. Existing healthy, equal-provider, accept-then-raise, and focused
surface tests do not exercise callback re-entry.

## Intended correction outcome

A provider may re-enter installation for its own identity during
`add_span_processor` without deadlocking or receiving a second processor. The
nested call observes the already-published provisional outcome, the outer
healthy registration completes normally, and later calls observe the final
successful outcome. Existing cross-thread serialization, weak identity,
failed-provider terminal behavior, and processor activation boundaries remain
unchanged.

## Decision-complete recommendation

Keep the correction in the existing `wyrd.otel` provider-outcome owner. Make
only `_outcomes_lock` reentrant with the standard-library reentrant lock while
leaving `OtelObserver`'s unrelated ordinary lock unchanged. Preserve the
existing critical section: publish provisional `False`, make one foreign
provider call, activate that attempt's processor only after normal return, and
then publish final `True`.

This is the smallest safe source correction. Re-entry can read the provisional
outcome without a second provider call, while other threads remain serialized.
Moving the foreign callback outside the critical section would require a new
in-progress coordination mechanism and is outside this remediation.

## Constraints and preserved behavior

- Preserve weak outcome tracking by live provider identity, never equality.
- Preserve one registration attempt and one processor handoff per provider.
- Preserve terminal `False` for unsupported, non-weak-referenceable, and
  failed providers, including accept-then-raise.
- Preserve per-attempt inactive processors and activation only after normal
  provider return.
- Preserve optional dependency behavior and containment of provider errors.
- Preserve the import-time context key, token-free tuple stack, active-span
  stamping, nested/async isolation, and top-matching exit.
- Preserve strict Card lookup, explicit observation errors, authenticated
  ingest, and server-derived identity.
- Preserve caller ownership of provider, exporter, Bifrost, flush, shutdown,
  and publication lifecycles.

## Non-goals

- No provider wrapper, retry, timeout in production, callback interception,
  pipeline introspection, second registry, condition variable, dependency, or
  provider lifecycle ownership.
- No active-span semantic change; the rejected `INV-R6-001` proposal remains
  outside this remediation.
- No server Run, Gate, Scribe, Bifrost, shared Rust, TypeScript, generated
  declaration, persisted schema, or public API change.
- No second integration journey or broad telemetry refactor.

## Acceptance criteria

1. `FIND-TASK-009-14`: a weak-referenceable provider whose
   `add_span_processor` re-enters `install_run_correlation(self)` completes
   without deadlock.
2. The nested call returns the already-published provisional `False`; the
   provider receives exactly one processor; the outer call and every later
   call return `True` without another provider call.
3. The accepted processor becomes active after the outer registration returns
   and applies the exact in-scope CardRef and Run ID.
4. Existing distinct-equal-provider, accept-then-raise, retained-inert-
   processor, healthy global/private provider, explicit-observation fail-open,
   nested, async, and persisted-journey behavior remains green.
5. The correction introduces no new dependency, registry, public surface, or
   provider lifecycle owner.

## Focused proof and broader verification

Add one focused regression beside the existing provider-registration cases.
Run it in a bounded subprocess so a future non-reentrant-lock regression fails
the test instead of hanging the suite. The proof must assert every outcome in
acceptance criteria 1 through 3, not merely that the subprocess exits.

Record and run the exact selected Python test node, then the complete focused
surface:

```bash
mise run py:setup
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py::<exact_reentrant_provider_test>)
(cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q \
  tests/unit/state/test_observe_surface.py)
```

Then run the existing persisted journey and touched-surface gates:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/state/test_observe_journey.py::test_scoped_run_emits_drift_eval_and_generic_rows'
mise run py:test:unit
mise run py:test:integration
mise run py:typecheck
mise run codegen:check
mise run check:pyo3-scope
mise run py:format
mise run py:lints
git diff --check
```

On any failure, rerun with `WYRD_LOG=info` before diagnosing. A later task
review must reassess the complete original base-to-remediated-candidate range,
not only this correction.

## Stop conditions

Stop and report rather than improvising if reentrant safety cannot preserve
one provider call, provisional/final outcome semantics, weak identity, failed
processor inertness, or cross-thread serialization; if the correction needs a
new provider lifecycle or public API; or if any required outcome conflicts
with specification revision 46.
