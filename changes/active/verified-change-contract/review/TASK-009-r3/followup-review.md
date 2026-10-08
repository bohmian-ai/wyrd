# Focused follow-up: identical-`Run` concurrent entry

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Conflict investigated: whether AC-032's concurrent-task obligation is proved
  by the current sibling-view async test, or requires concurrent entry of the
  identical Python `Run` object.
- CodeGraph: no `.codegraph/` directory exists, so repository search and direct
  source inspection were used.

## Source path inspected

- Approved specification revision 45: `REQ-151` at `spec.md:315-336` and
  `AC-032` at `spec.md:1807-1833`.
- Original TASK-009 Scenario 2 and acceptance criteria at
  `tasks/TASK-009-run-context-and-python-otel-correlation.md:100-123,182-196`.
- Approved Run contract at `architecture/logic/run_api.md:38-67,130-206`,
  especially its execution-local token rule at lines 177-184.
- Complete Run-correlation implementation in
  `sdks/wyrd-sdk-python/python/wyrd/otel.py:186-326`.
- Complete `PyRun` wrapper and every context-manager caller in
  `sdks/wyrd-sdk-python/src/observe/mod.rs:156-249` and the Python unit and
  integration test trees.
- The full async case at
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:313-351`, its
  sibling nested/same-object/provider/failure tests at lines 269-310 and
  354-562, and the real journey entry at
  `tests/integration/state/test_observe_journey.py:384-402`.
- The complete cumulative base-to-candidate diff for these paths and the prior
  TASK-009 review/remediation artifacts.

## Evidence resolving the conflict

Identical-object concurrent entry is a distinct required proof. REQ-151 says
normal async propagation must work "without storing one shared attach token on
the immutable Run" (`spec.md:321-324`). AC-032 then separately requires async
evidence for "concurrent tasks using the same immutable Run"
(`spec.md:1824-1825`). Scenario 2 repeats the same wording
(`TASK-009...md:102-110`). The Run authority explains why: execution-local
tokens, rather than a token stored on `Run`, allow the same immutable run to be
used concurrently without one task detaching another (`run_api.md:177-184`).
Reading "same" as merely the same invocation ID would not test the expressly
prohibited per-object storage design.

The current async test does not exercise that path. It creates two separate
Python objects with `run.for_card("model")` and `run.for_card("backup")` at
`test_observe_surface.py:319-320`; each concurrent coroutine enters its own
object at lines 322-327. `PyRun::for_card` constructs a new `PyRun` at
`src/observe/mod.rs:184-193`, so shared native `run_id` does not make these the
identical Python context-manager object. The surrounding root `with run` does
not close the gap: neither child task concurrently enters that root object.

A reachable per-object-token regression therefore passes the current proof:

1. Task A enters sibling object A and stores token A on A.
2. Task B enters sibling object B and stores token B on B.
3. Each object retrieves its own token on exit, so every span asserted by the
   existing test remains correctly correlated.

The same implementation fails for the required identical object. Task A enters
`run` and stores token A; task B enters that same `run` in its separate copied
async context and overwrites the slot with token B. If A exits first, it tries
to detach B's context-bound token. OpenTelemetry rejects or ignores that reset;
because telemetry errors are deliberately swallowed, A can retain stale Run
correlation after its scope exits. If B exits first, it can clear or overwrite
the slot before A retrieves token A. Either interleaving permits one task to
detach the other task's scope or leak correlation after exit. This is the exact
failure REQ-151's storage prohibition names.

No sibling test closes the proof gap. `with run, run` at
`test_observe_surface.py:360` is nested in one execution context and asserts
only processor registration. The repeated direct `__enter__`/`__exit__` calls
at lines 400-405 are sequential and assert only the public signature/result.
The detach-failure test uses distinct root/model objects in one context. The
threaded first-entry test uses distinct sibling objects and proves one lazy OTel
key, not identical-object token isolation. The integration journey enters one
object once. Repository-wide caller search found no other concurrent entry of
one Python `Run` instance.

The production implementation itself is coherent: `_scope_tokens` is one
execution-local `ContextVar` stack (`otel.py:189-195`), `_enter_run` appends the
current entry's exact token/prior pair (`:279-297`), and `_exit_run` pops only
that execution context's innermost entry (`:300-326`). The uncertainty is an
explicit acceptance-proof gap, not a demonstrated runtime defect.

The existing focused async test was independently run with:

```text
mise exec -- uv run --project sdks/wyrd-sdk-python python -m pytest -q \
  sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py::test_scope_survives_await_and_isolates_concurrent_tasks
```

Result: `1 passed`. That confirms the present sibling-view case, not the
identical-object case above.

## Proposed findings

Retain `TEL-R3-001` for independent validation. AC-032's required
same-immutable-`Run` concurrent-task evidence is missing. The smallest closure
is one coordinated async case in the existing focused test file where two tasks
enter the identical `run` object and an interleaved exit cannot detach or leak
the other task's scope. Keep the existing sibling-Card isolation assertions;
no production change, dependency, fixture, or new harness is indicated.

No additional finding was discovered.

## Resolution

**RESOLVED** — the reports conflict because they treat shared invocation
identity as equivalent to identical context-manager identity. The approved
authority distinguishes them to prohibit per-`Run` token storage. Current
source implements the rule, but current tests do not provide the explicitly
required identical-object concurrent proof.
