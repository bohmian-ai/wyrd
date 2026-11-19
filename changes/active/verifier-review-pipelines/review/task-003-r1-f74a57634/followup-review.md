# Focused follow-up: Python response-hold ownership

Result: **RESOLVED — CONFIRMED**, limited to an explicit repository-rule violation. No independently supported runtime race or flaky-test regression is claimed.

## Subject and scope

Prior candidate `384a0def73ed6eac4041dc1e428dc30e981514fe`; candidate `f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`. Reviewed only the new `HOLDS` producer, consumers, lifetime, applicable authority and smallest correction boundary. CodeGraph exploration preceded source navigation; the precise remediation diff and current candidate source supplied the complete relevant functions. HEAD matches the candidate, relevant source has no working-tree changes, and every hash in `authority-sha256.txt` passed at conclusion. No tests, builds or source edits were performed.

## Authority and applicability

`sdks/wyrd-sdk-python/AGENTS.md:127` states **“No global mutable state.”** This is under “Python code,” not “Public API,” and the document's opening scope is “Rules for the Python package and its PyO3 boundary.” The opening paragraph explicitly includes tests among the package's permitted work. There is no production-only qualifier, test-support exception, or narrower `tests/AGENTS.md` or `tests/integration/AGENTS.md`. The following sentence about typed dataclasses/Pydantic shapes does not qualify the global-state prohibition.

The R1 remediation instruction permits the smallest appropriate local synchronization (`TASK-003-R1-check-admission-and-proof.md:145-151`); it does not require globals or waive the package rule. Existing `HOLD_ARRIVED`/`HOLD_RELEASED` Events at `support.py:54-58` are prior implementation, outside this correction, and supply neither a waiver nor authority for adding another mutable global. Literal immutable case data is not the issue: `HOLDS` is actively mutated during execution.

The task-review skill preserves explicit mandatory repository gates and permits a finding for an explicit mandatory-gate violation. Therefore a newly introduced direct “No …” rule violation does not depend on proving that the currently sequential test flakes. This is a **VIOLATION**, not a preference for another conventional fixture design.

## Complete producer-to-consumer and lifetime trace

| Boundary | Candidate source and behavior |
|---|---|
| Allocation | `support.py:60-63` allocates a module-global dictionary once at import. Its values contain mutable arrival/release Events. |
| Writers | `test_verifier_orchestration.py:225-236`: `hold(fragment)` creates Events and inserts the pair; `release(fragment)` removes the pair and sets its release Event. These are the only dictionary writers. |
| Supported caller | The single test `test_speculative_disposition_controls_consumer_admission`, lines 260-300, loops the six existing cases sequentially, creates holds before SDK `submit`, observes public milestones, and releases the appropriate request. It uses full role-plus-question fragments. |
| Request path | Session `receiver()` (`conftest.py:658-662`) calls existing `serving()` (`610-647`), which creates a `Receiver` and a `ThreadingHTTPServer`. `Handler.do_POST` calls `completion(body)` at line 629. `completion` (`512` onward) routes orchestration markers to `orchestration_turn(text, body["messages"])`. |
| Consumer | `orchestration_turn` (`396-486`), specifically `414-417`, snapshots `list(HOLDS.items())`, finds matching text, sets arrival and waits up to 300 seconds for release before constructing the normal provider reply. Once captured, the Event reference remains valid after dictionary removal. There is no dictionary iterator held over the wait. |
| Normal/failure reset | The test's `finally` (`298-300`) releases all three possible fragments for that case, including assertion/SDK failures. Releasing an absent fragment is harmless. Subsequent requests for a removed hold proceed immediately; a handler already waiting is woken by its captured Event. There is no fixture-owned reset or teardown of `HOLDS`; allocation/lifetime remains the imported module/process. |
| Sibling provider owners | `serving()` also supplies independent local endpoints, including `watching()` at `744-755`, and can take a delay. Each owns a separate `Receiver`, but all its completion callbacks currently consult the same imported dictionary. The new state is not attached to any receiver. |

The changed path is reachable on every existing speculative test case: the public submit produces provider requests that consult the dictionary the test populated. That establishes execution of the forbidden state, rather than a dormant declaration.

## Materiality and rejected extensions

The current six-case loop and `finally` cleanup are credible for its ordinary supported sequential execution. I found no source-backed reason to claim a leaked hold in that flow. Threading alone does not prove a dictionary race, and process-isolated pytest workers would not share this dictionary. Same-process parallel test execution, duplicate concurrent executions with identical markers, arbitrary stalled-handler shutdown guarantees and forced process interruption are not established acceptance requirements here. They must not be bundled into remediation.

The retained consequence is the explicit mandatory ownership constraint: newly mutable response gates are process-global rather than owned by the local provider fixture. This is directly observable in source and violates the scoped prohibition, even though the existing execution is cleaned up. No production correctness, data exposure, server availability or all-language failure is alleged.

## Smallest correction boundary and proof

Retain the response-hold behavior and public milestone observations. Delete the new module-global registry and its imports. Own those new gates in the existing local-provider/`Receiver` fixture instance (`support.py:98-124`, `conftest.py:610-647`) and give the test and that provider callback the same instance-owned state. Thread that state through the existing completion-to-orchestration path rather than introducing another callback-global registry. Preserve per-case release in `finally`, wake already captured Events, and leave ordinary unheld provider responses and existing delay behavior unchanged.

This is a bounded adjustment across the same three changed Python files. The existing Receiver already owns mutable request records and a synchronization Condition, and `serving()` already closes over that receiver; no new class, provider harness, dependency, configuration surface, production hook or authority amendment is needed. Merely clearing/locking/renaming the global cannot satisfy “No global mutable state”; deleting gating entirely would lose the approved deterministic proof. A receiver-owned implementation is the smallest sufficient existing owner, with private implementation details left to the implementer.

Closure proof: inspect that the newly added hold state is instance/fixture-owned and that the handler consults its own provider's state; retain the existing six-case speculative selector and failure cleanup semantics, and run only the narrow Python checks for those changed files plus that selector. No parallel-execution matrix, new test harness, all-language rerun, or cleanup of pre-existing global Events is required by this finding.

No other finding was investigated or proposed. No runtime tests were run; the conclusion rests on immutable source and the explicit applicable rule.
