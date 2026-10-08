# Concurrency and execution-context domain review

## Immutable subject

- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Candidate remained `HEAD` before and after review.
- Scope: Python Run/OpenTelemetry execution context, provider/key publication,
  nested and concurrent scope restoration, and the R3 same-Run asyncio proof.

## Authority and source coverage

| Boundary | Authority | Source and consumer paths inspected | Result |
|---|---|---|---|
| Execution-local Run scope | Approved spec REQ-151; TASK-009 Scenario 2; `architecture/wyrd-design.md` observation identity | `sdks/wyrd-sdk-python/python/wyrd/otel.py:186-326`; `sdks/wyrd-sdk-python/src/observe/mod.rs:195-239` | PASS |
| Async propagation and same immutable Run | AC-032; TASK-009-R3 acceptance criteria 1-4 | `test_observe_surface.py:313-389`, including `test_concurrent_tasks_entering_the_same_run_exit_independently` | PASS |
| Nested restoration and detach failure | REQ-151; TASK-009-R1 FIND-TASK-009-3 correction | `_enter_run`/`_exit_run`; `test_nested_card_scopes_share_the_run_and_restore_the_outer_card`; `test_detach_failure_restores_the_prior_correlation` | PASS |
| Concurrent lazy key publication | REQ-151 execution isolation; TASK-009-R1 FIND-TASK-009-4 correction | `_key`; `test_concurrent_first_entries_share_one_scope_key` | PASS |
| Provider registration race/idempotency | REQ-151 and AC-032 | `install_run_correlation`; `_RunCorrelationProcessor`; global/private, unsupported, and raising-provider tests | PASS |
| Optional failure containment | REQ-151 and AC-032; TASK-009-R1/R2 | PyO3 entry/exit delegation; registration, attach, processor, detach, user-exception, and explicit-observation tests | PASS |
| Async/runtime repository rules | `AGENTS.md` sections 6-8 and `architecture/agent-rules.md` | Python owns the foreign-runtime context; Rust exposes only synchronous context-manager hooks and stores no Python/OTel token | PASS |

The complete cumulative base-to-candidate diff was inspected for the Python
OpenTelemetry implementation, PyO3 Run boundary, dependency declaration, unit
proof, and persisted journey. OpenTelemetry 1.42.1's installed primary source
was also inspected: its runtime context is backed by `ContextVar`, `attach`
returns the exact `ContextVar` token, and `detach` resets that token.

## Concurrency assessment

`_scope_tokens` is a `ContextVar` containing an immutable tuple. Every entry
appends its own `(token, prior)` value and every exit removes only the current
execution context's innermost value. Asyncio copies the tuple value into a new
task context; later tuple replacement in either task does not mutate the
sibling task's stack. No token or mutable scope state exists on `PyRun`.

The R3 proof is deterministic. Two tasks close over the exact same `run`
object and use three `asyncio.Event`s to force this order: task one enters,
task two enters, task one exits, then task two observes while still entered.
There are no sleeps, timeouts, retries, or scheduler-order assertions in that
case. The post-exit span in task one must be uncorrelated while task two's span
must retain the exact `(card_ref, run_id)` pair, and both task-local and caller
contexts are checked after exit. A token slot shared by the Run object would
be overwritten by the second entry; task one's interleaved exit would then
either detach the other task's token in the wrong context or leave task one's
scope stale, failing the `first-exited is None` assertion. The test therefore
falsifies the prohibited storage shape rather than merely confirming two
different Run views.

Lazy `_scope_key` creation is double-checked under the same module lock used
for provider registration, so all concurrent first entries publish one key.
Provider membership and processor addition are serialized, failed additions
are removed for a later clean attempt, and the WeakSet does not extend caller-
owned provider lifetime. Nested normal exits detach their exact token. When
OpenTelemetry's public detach raises or logs and swallows a reset failure, the
recorded prior correlation is reattached in the same execution context; the
focused test covers both behaviors and proves explicit observation remains
independent.

Threads do not receive an ambient propagation guarantee, consistent with the
R3 non-goal. The thread test is correctly limited to concurrent first-entry
key publication, with each thread entering its own scope.

## Verification

Executed against the candidate:

```text
mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q \
  sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py
35 passed in 0.87s
```

An earlier focused run of the same-Run, distinct-view async, nested, detach-
failure, first-key, and provider-idempotency cases passed `6/6`. `git diff
--check` passed.

## Verification limits

The authenticated Postgres/OTLP journey and repository-wide Python lanes were
not rerun in this domain pass. Their candidate-recorded results remain useful
broader evidence, while the concurrency obligations are directly exercised by
the complete focused unit file above. No version-sensitive concurrency claim
was accepted solely from the task record.

## Findings

No material concurrency or execution-context findings.

## Overall result

**PASS**
