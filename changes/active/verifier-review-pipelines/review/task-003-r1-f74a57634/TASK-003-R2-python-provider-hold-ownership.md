---
id: TASK-003-R2
kind: remediation
status: ready
spec: SPEC-verifier-review-pipelines
spec_revision: 22
parent_task: TASK-003
candidate: f74a576348cb9805ec7a2ef0e1c0bde3dd07928d
prior_candidate: 384a0def73ed6eac4041dc1e428dc30e981514fe
base: e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355
remediates: [FIND-003-5]
depends_on: []
---

# Keep Python provider response holds on their fixture

## Outcome and authority

Move the newly introduced Python response-hold registry from module-global
state into the existing local-provider fixture. Keep the same deterministic
speculative journey and its six cases. The receiver serving a request and the
test releasing that response must use the same fixture-owned gate state.

This closes one explicit SDK repository-rule violation introduced by R1.
FIND-003-1 through FIND-003-4 are closed at the reviewed candidate and are not
reopened. Route this task directly to `$wyrd-implement`; no spec decision or
new test harness is required.

Authority and acceptance context:

- [Approved spec baseline and revision-22 amendment](../../spec.md).
- [Original TASK-003](../../tasks/TASK-003-generic-orchestration.md).
- [R1 remediation and execution evidence](../task-003-384a0def7/TASK-003-R1-check-admission-and-proof.md).
- [Current verdict](verdict.md), [validated ledger](findings-validation.md), and
  [independent focused source assessment](followup-review.md).
- [Python package rules](../../../../../sdks/wyrd-sdk-python/AGENTS.md), whose
  Python-code section explicitly prohibits global mutable state.

## Diagnosis: FIND-003-5

`sdks/wyrd-sdk-python/tests/integration/support.py:60–63` creates `HOLDS` at
module import. The speculative test's `hold` and `release` closures
(`test_verifier_orchestration.py:225–236`) mutate that dictionary. The existing
threaded provider follows `serving -> Handler.do_POST -> completion ->
orchestration_turn`; `conftest.py:414–417` snapshots the same imported global,
signals arrival and waits for the release Event. These are the complete writers
and request consumers of the new state. The six-case test reaches that state on
ordinary execution, so the violation is active rather than a dormant declaration.

The SDK-local rule has no test-support exception. Existing global Events
predate this remediation and do not authorize adding a mutable process-global
registry. The current test's sequential flow and `finally` releases are sound;
this finding does **not** allege a demonstrated runtime flake, data leak or
parallel-test failure. The failure is the direct, independently validated
repository ownership constraint.

Merely clearing, locking or renaming the dictionary would leave it global.
Removing response gating would lose the required deterministic proof. No new
class, registry service, context framework or provider harness is necessary:
the existing `Receiver` already owns mutable request records and synchronization,
and `serving()` already closes over that instance.

## Selected correction

Own only the newly added gate state on the existing receiver/local-provider
fixture. Give the test and the provider callback access to that same instance's
state through the current fixture and completion-to-orchestration flow. Delete
the new global registry and its imports. Use the existing fixture owner rather
than another module-level indirection or new state abstraction; private field,
method and parameter details remain implementation-owned.

Preserve Event arrival/release behavior, ordinary unheld replies and the existing
local responder paths. A handler that already captured its release Event must
still wake when the test releases it. Retain per-case `finally` cleanup and all
six current milestone, call, status, manifest, retained-inspection and outcome
assertions. Keep the user-approved decision-first ADMIT wording: comparison is
accepted while extra is held, and extra settles admitted after release.

The change belongs only to the existing three Python test-support/journey
owners. Do not clean up pre-existing Event globals, redesign provider teardown,
add same-process parallel-execution requirements, change timeouts, introduce
server worker/lease/replica controls, or touch the closed engine/doc/Rust/MCP/TS
remediations. No production SDK/native code, public or persisted contract,
dependency, migration, generated artifact or architecture change is needed.
If a genuine broader boundary conflict is discovered, report evidence instead
of silently expanding this task.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| FIND-003-5 | No new module-global mutable hold registry remains. The receiver/local-provider instance owns the new gates, and its handler and the test consult/release that same instance. Existing sibling provider endpoints do not consult a process-global registry. |
| Preserved R1 proof | The six speculative cases still pass using held replies and accepted public invocation milestones; `finally` releases remain, including waking a handler that captured an Event before removal. No closed finding or outcome assertion is weakened. |
| Scope | Changes remain within Python test support and required callers; no production API, new harness, old-global cleanup or other-language changes. |

This is a static ownership-rule correction with regression proof of unchanged
observable behavior. Do not manufacture a runtime failure for a rule the current
sequential execution can violate while passing. Inspect the state lifetime and
all current writers/readers against the Python rule; run the existing exact
journey and Python owner checks. If implementation adds distinct executable
behavior, apply the repository's ordinary TDD discipline to that behavior, not
an invented concurrency feature.

Use the existing repository-managed setup in
[shared verification](../../tasks/verification.md#existing-targets-and-setup).
No native rebuild is required solely by pure-Python fixture edits when the
existing test environment is already prepared.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_verifier_orchestration.py::test_speculative_disposition_controls_consumer_admission'
mise run py:format
mise run py:lints
mise run py:typecheck
mise exec -- git diff --check
```

Run commands sequentially through the repository environment; confirm the journey selects
one test and retains all six cases. Review the final diff and record ownership
proof plus actual command results. No full language, SDK, platform or original
TASK-003 aggregate is required for this bounded correction. The next task review
checks this finding's closure and affected Python fixture paths, carrying the
accepted four prior closures and unaffected task evidence forward.

## Obligations

| ID | Source | Planned assertion |
|---|---|---|
| R2.1 | Acceptance FIND-003-5 | `HOLDS` deleted from `support.py` and every import; `grep` finds no module-global hold registry. |
| R2.2 | Acceptance FIND-003-5 | `Receiver` owns the gates; `serving()`'s handler consults its own `received` and the test calls `receiver.hold`/`receiver.release` on the same session fixture instance. |
| R2.3 | Acceptance FIND-003-5 | Sibling provider endpoints (`gateway_server` upstream, hooks `serving()`) consult no process-global registry: each `Receiver` has its own empty gates. |
| R2.4 | Preserved R1 proof | Exact journey selects one test and passes all six `SPECULATED` cases with unchanged assertions. |
| R2.5 | Preserved R1 proof | `finally` releases every fragment per case; `release` sets the captured Event, so a handler already waiting wakes. |
| R2.6 | Scope | Diff touches only `support.py`, `conftest.py`, `test_verifier_orchestration.py`; no production, other-language or old-Event-global change. |

## Evidence

Commit `8f1bb789e` (range `f74a57634..8f1bb789e`). Diff: `support.py`, `conftest.py`, `test_verifier_orchestration.py` only.

Ownership proof: the new gates are the `Receiver.holds` dataclass field (`support.py:99`, `default_factory=dict`, so each instance gets its own). The only writers are `Receiver.hold`/`release` (`support.py:103,110`), and the only reader is `Receiver.await_release` (`support.py:115`). `serving()` calls them on the instance it creates and yields (`conftest.py:613,623`). The session `receiver` fixture yields that same instance to the test, which calls `receiver.hold`/`receiver.release` (`test_verifier_orchestration.py:264-284`). `gateway_server`'s upstream (`conftest.py:873`) and the hooks `serving()` own separate empty `holds`, and the `Upstream` handler never consults them. `grep -rn HOLDS sdks/wyrd-sdk-python` returns nothing. RED does not apply: this is a static ownership correction with no new executable behaviour, per the task.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| R2.1: no module-global hold registry | `support.py` `HOLDS` and both imports deleted | `grep -rn HOLDS sdks/wyrd-sdk-python` returns nothing | grep; `py:lints` (no unresolved names) | PASS |
| R2.2: receiver owns the gates; the handler and the test use the same instance | `support.py:99-120`, `conftest.py:623` | `test_verifier_orchestration.py:264,270,273,277,280,284` hold and release on `receiver`; the `arrived[reviewing].wait` assertion (`:272`) only passes if `serving`'s handler set the event from the same instance | exact journey: 1 passed | PASS |
| R2.3: sibling endpoints use no global registry | `conftest.py:873`: `Receiver` per instance; `Upstream` unchanged | gates are a per-instance `default_factory` field; no module-level state remains (R2.1) | grep, diff review | PASS |
| R2.4: six cases still pass with assertions unchanged | `SPECULATED` (6 rows) and the assertions after `finally` untouched | `test_verifier_orchestration.py:286+` | `pytest -v` selected 1 test: PASSED | PASS |
| R2.5: `finally` releases, waking a captured event | `release` pops the entry and sets that same `released` event, which a waiting `await_release` already holds from its snapshot. The `finally` loop (`:283-284`) is kept and calls the same `release`. | Wake: `:272` asserts the extra-review handler arrived and is waiting, and `:277` `receiver.release(reviewing)` must wake it, or the extra step can't settle and `watched.wait(90 s)` fails before the handler's 300 s wait ends. Retention: `:283-284`. | exact journey: PASSED (decision cases) | PASS |
| R2.6: scope | 3 Python test-support files | `git diff --name-only f74a57634..HEAD` | diff audit | PASS |

Commands, run sequentially:
- The exact journey (`-q`, then `-v`): `1 passed`.
- `mise run py:format`: ruff removed the now-unused `threading` import from the test.
- `py:lints`, `py:typecheck` and `git diff --check`: pass.
- Docstring-only rewraps in `conftest.py` were made after the last journey run; format and lints were re-run on them.

Non-goals stayed excluded:
- the old `HOLD_ARRIVED`/`HOLD_RELEASED` globals;
- timeouts;
- other languages;
- production code.

Independent checker: R2.1–R2.4 and R2.6 were proven, and no behaviour was lost. It found R2.5 cited only the `finally` loop, which does nothing on a passing run. The row now cites the wake that is exercised (`:272`/`:277`) and keeps `:283-284` for the retention. The `finally` path calls that same `release`, and a failure partway through a case is not exercised separately.
