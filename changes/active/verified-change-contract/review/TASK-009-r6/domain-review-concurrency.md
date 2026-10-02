# Concurrency and execution-local isolation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 46, especially REQ-123, REQ-151, and AC-032
- Task chain: TASK-009, R1, R2, R3, R4E (superseding R4), and R5

The candidate remained at the stated commit during this review.

## Review Findings

### Critical

None.

### Important

- **`CONC-R6-001` — INCORRECT** — [`sdks/wyrd-sdk-python/python/wyrd/otel.py:205,275-286`](../../../../../sdks/wyrd-sdk-python/python/wyrd/otel.py) holds the non-reentrant `_outcomes_lock` while invoking the foreign, duck-typed `provider.add_span_processor`. A provider that re-enters the public `install_run_correlation(self)` hook from that callback blocks forever attempting to acquire the same lock. This path is reachable because private providers are explicitly supported by duck typing; it hangs direct installation and also hangs `Run.__enter__` when such a provider is global, violating REQ-151's requirement that optional provider failures must not fail Run entry or application execution. A focused subprocess using a weak-referenceable provider whose `add_span_processor` calls `install_run_correlation(self)` timed out after three seconds. Use the standard-library reentrant lock for this existing critical section, preserving the pre-marked `False` outcome, single foreign call, later `True` update, identity-keyed weak bookkeeping, and cross-thread serialization. Add one focused test proving re-entry returns the pre-marked outcome without a second provider call, the outer healthy registration completes, later calls return `True`, and its processor enriches normally.

### Suggestions

None.

## Open Questions

None. The correction stays within the approved registration semantics and needs no specification decision.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Immutable Run identity and views | Revision-46 REQ-123; `architecture/wyrd-design.md` observation identity; `crates/shared/wyrd-client/src/state.rs:492-511`; `crates/shared/wyrd-client/src/observe/mod.rs:42-115`; shared Run tests | PASS — each run mints one `RunId`, sibling views retain it, and their subjects remain immutable. |
| PyO3 entry/exit handoff | REQ-151; PyO3 boundary rules; `sdks/wyrd-sdk-python/src/observe/mod.rs:195-249` | PASS — no scope state is stored on `PyRun`; both entry and exit pass the exact view pair and exit never suppresses an exception. |
| Execution-local tuple stack | Revision-46 REQ-151 and R4E; `changes/active/verified-change-contract/architecture/logic/run_api.md:130-230`; `sdks/wyrd-sdk-python/python/wyrd/otel.py:192-198,293-334` | PASS — one import-time key owns the tuple stack; entry and matching exit use attach-only replacement; nested scopes, task copies, awaits, cancellations through normal `with` unwinding, and identical-Run task use remain context-local. |
| Provider identity, lifetime, activation, and registration races | REQ-151; R4E and R5; `otel.py:200-290`; provider-focused unit tests | **FAIL — `CONC-R6-001`.** Identity comparison, dead-entry pruning, weak lifetime, pre-marking, accept-then-raise inertness, and ordinary cross-thread idempotency are sound, but reentrant provider registration deadlocks under the non-reentrant lock. |
| Persisted healthy path | Telemetry observation authority; `tests/integration/state/test_observe_journey.py:365-430,478-562` | PASS for the exercised stock private provider path; it does not exercise provider callback re-entry. |

## Execution and failure-path assessment

- The same immutable Python `Run` can be entered concurrently by separate asyncio tasks because the current stack is stored only in each task's propagated OpenTelemetry context. The deterministic same-Run test proves one task's exit does not remove the other's correlation.
- Normal Python exception and cancellation unwinding calls `Run.__exit__`; its matching top-pair pop is local to the current execution context and returns `False`, so the original exception continues.
- Mismatched exit and exit-attach failure deliberately leave the current context unchanged, matching revision 46. The documented attach-without-detach residual is approved authority, not a finding.
- Provider identities are compared with `is` through weak references, so equal distinct providers do not alias and dead referents cannot collide with a later object that reuses an address.
- Per-attempt processors remain inactive until normal provider return; an accept-then-raise provider's retained processor cannot enrich. The remaining failure is specifically lock re-entry around the foreign callback.

## Verification Notes

Reviewed the cumulative base-to-candidate diff, the complete current correlation owner and its callers, the Rust Run owner, PyO3 boundary, focused unit tests, persisted Python journey, original task, all listed remediation tasks, repository rules, and applicable Python/PyO3/testing/telemetry authorities.

Executed successfully:

```text
test_concurrent_tasks_entering_the_same_run_exit_independently
test_equal_providers_register_independently_by_identity
test_a_processor_retained_by_a_failed_registration_never_enriches
3 passed in 0.69s
```

Focused negative proof:

```text
timeout 3s ... provider.add_span_processor -> install_run_correlation(self)
exit 124 (deadlock reproduced)
```

The broader verification commands recorded in TASK-009/R5 were inspected but not rerun in this domain pass. No broad lane can close the reproduced reentrancy gap without the focused regression above.

## Overall result

**FAIL** — one bounded concurrency defect remains: reentrant provider registration can deadlock the optional telemetry path.
