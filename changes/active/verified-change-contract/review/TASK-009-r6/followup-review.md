# Focused follow-up: provider re-entry and failed-provider active spans

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Task chain: TASK-009, R1, R2, R3, R4E (superseding R4), and R5
- Claims investigated: `CONC-R6-001` and `INV-R6-001`

The candidate remained at the stated commit throughout this follow-up. No
`.codegraph/` directory exists, so direct source and caller inspection was used.

## Paths inspected

- `changes/active/verified-change-contract/spec.md`, especially REQ-151,
  AC-032, and the revision-46 history
- `changes/active/verified-change-contract/architecture/logic/run_api.md`,
  especially "Python OpenTelemetry run scope"
- `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- `changes/active/verified-change-contract/review/TASK-009-r4/TASK-009-R4E-token-free-run-correlation.md`
- `changes/active/verified-change-contract/review/TASK-009-r5/TASK-009-R5-provider-identity-and-proof-closure.md`
- `changes/active/verified-change-contract/review/TASK-009-r5/findings-validation.md`
- `sdks/wyrd-sdk-python/python/wyrd/otel.py`
- `sdks/wyrd-sdk-python/src/observe/mod.rs`
- `sdks/wyrd-sdk-python/python/wyrd/stubs/observe.pyi`
- `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`
- `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py`
- OpenTelemetry 1.42.1's installed `Tracer.start_span` and SDK `Span`
  implementation, to determine whether the public active-span surface exposes
  its owning provider
- The two r6 discovery reports containing the claims

## Resolution 1 — `CONC-R6-001`

**Confirmed as a reachable, bounded implementation defect.**

`install_run_correlation` acquires the non-reentrant `_outcomes_lock` at
`sdks/wyrd-sdk-python/python/wyrd/otel.py:275`, publishes the provisional
identity-keyed `False` entry, and then invokes the foreign duck-typed
`add_span_processor` while still holding that lock at lines 281-286. If that
callback re-enters the public `install_run_correlation(self)` hook, the nested
call blocks acquiring the same lock before it can observe the already-published
entry. The outer callback is waiting for the nested call, so neither can make
progress.

This is caused by Wyrd's critical section rather than by a provider that merely
takes a long time: a weak-referenceable provider whose callback only calls the
same supported public hook reproduces the deadlock. A four-second subprocess
probe printed entry into the callback and then exited with timeout status 124.
The path affects direct private-provider installation and `Run.__enter__` when
the object is returned by the global-provider lookup. The PyO3 boundary at
`sdks/wyrd-sdk-python/src/observe/mod.rs:206-217` cannot contain a call that
never returns, so optional integration blocks application execution contrary to
REQ-151.

The smallest correction remains inside the existing registration owner: use a
standard-library reentrant lock for `_outcomes_lock`. Re-entry then observes the
provisional `False` entry without a second foreign call, while the outer call
can return normally, activate its processor, and publish the final `True`.
Cross-thread serialization, weak identity bookkeeping, at-most-once handoff,
and accept-then-raise behavior remain unchanged. Moving the foreign call outside
the critical section would require additional in-progress coordination and is
not the smaller correction.

Focused closure proof should use one weak-referenceable provider whose
`add_span_processor` re-enters `install_run_correlation(self)`: the nested call
returns the provisional `False`, only one processor is handed off, the outer
call and later calls return `True`, and the activated processor enriches an
in-scope span. The pre-correction execution must be bounded by a subprocess or
thread timeout so the regression cannot hang the suite.

## Resolution 2 — `INV-R6-001`

**Runtime observation confirmed; proposed finding rejected because the
failed-provider clause does not govern the provider-agnostic active-span
operation.**

The reported execution is real. `_enter_run` ignores the Boolean result from
`install_run_correlation`, attaches the Run pair, and directly stamps an
eligible current recording span at `otel.py:302-308`. A real `TracerProvider`
subclass that retains the processor and raises therefore remains terminal
`False`, while a span it created before entry receives both Wyrd attributes.
The local reproduction returned `False` and showed both attributes on that
active span.

That fact does not establish a violation of revision 46. REQ-151 separately
requires entry to set both attributes on an already-active recording span when
it lacks `wyrd.card_ref`, with no registration-success condition. R4E required
behavior items 2 and 5 preserve that ordering and unconditional best-effort
active-span operation. The owning Run architecture likewise defines three
independent entry operations: push context, stamp the current span, and ensure
the global processor. Its failed-provider explanation narrows "gets no
enrichment" to the registration-owned mechanism: the processor retained by a
failed attempt stays inert. R5's approved correction and closure proof address
exactly that processor path, not direct mutation of a span already current at
entry.

An active OpenTelemetry span is not provider-owned or provider-identifiable
through the public Span API. The current span can also have come from a
successfully installed private provider while global-provider installation
returns `False`. Gating all active-span stamping on the global Boolean would
therefore remove the explicitly required attributes from that supported private
span. Distinguishing the two cases would require provider or pipeline
introspection through SDK-private state; R4E expressly prohibits reading private
SDK internals beyond readable span attributes, and neither the public Span nor
`SpanProcessor.on_start` contract supplies provider identity.

Accordingly, the provider-registration outcome controls only the processor
handed to that provider. The active-span operation remains best-effort and
provider-agnostic. `INV-R6-001` should not enter the validated finding ledger.
If the product instead intends terminal `False` to prohibit direct stamping of
a failed global provider's own active span, that requires a specification and
provider-identification decision; it cannot be prescribed as this review's
bounded correction without violating the current active-span and
private-provider obligations.

## Shared-source assessment

The claims do **not** share a source.

- `CONC-R6-001` is produced by synchronization ownership around a reentrant
  foreign registration callback.
- `INV-R6-001` observes the intentionally separate direct active-span operation
  after registration. It neither causes nor depends on lock re-entry.

The confirmed lock correction does not change active-span semantics, and no
active-span guard is needed to close it.

## New proposed findings

None beyond the confirmed `CONC-R6-001`. No additional reachable defect was
found while tracing the two paths.

## Verification notes

- Reproduced `CONC-R6-001` in a bounded subprocess: callback entry printed,
  process timed out with exit 124.
- Reproduced the factual active-span state behind `INV-R6-001`: installation
  returned `False`; the already-active span held exact `wyrd.card_ref` and
  `wyrd.run_id` values.
- Inspected the current unit coverage for healthy global/private providers,
  accept-then-raise terminal caching, retained-processor inertness, active-span
  stamping, nested non-overwrite, and explicit-observation fail-open behavior.
  No current test exercises registration callback re-entry.

## Result

**RESOLVED** — retain `CONC-R6-001` for independent validation; reject
`INV-R6-001` under the current revision-46 active-span and provider-integration
contract. The claims are independent.
