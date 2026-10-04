---
id: TASK-004-R3
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 13
parent_task: TASK-004
remediates: [FIND-TASK-004-19, FIND-TASK-004-20]
---

# Close the observable step-attempt invariant and remaining import violations

Implementation skill: `$wyrd-implement`.

## Immutable review inputs

- Approved spec: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediations:
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`
- Reviewed base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Reviewed candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Validated diagnosis:
  `changes/active/skald-workflow-runtime/review/TASK-004-r3/findings-validation.md`

## Outcome

Make every externally published `Running` Workflow step represent a begun
attempt and preserve that active-step identity through immediate cancellation,
deadline, or abort. Complete the repository-required module-import cleanup at
the four validated changed sites. Preserve every other Workflow, provider,
query, Oracle, security, tenancy, and lifecycle behavior.

## Diagnoses and required corrections

### Observable step-attempt lifecycle (`FIND-TASK-004-19`)

Revision 13 requires pending and never-started steps to have zero attempts,
running and cancelled active steps to have at least one begun attempt, and
interrupted active steps to retain their timestamps without observable status
regression.

`WorkflowExecutor::drive` currently marks a step `Running` and synchronously
publishes the complete snapshot before spawning or polling its `StepTask`.
`RunLedger::step_started` sets the status and `started_at`, but leaves the
attempt count at zero. `StepTask` increments the shared count only on its first
poll. Cancellation, total deadline, or abort can therefore win after the
`Running` snapshot is externally stored but before that increment. Settlement
leaves the zero-attempt step running, after which `RunLedger::finish` rewrites
it to `Unstarted`, clears `started_at`, and returns a terminal snapshot that
denies the already-published start.

This path is externally observable: the server stores every transition through
`AcceptedRun::observe`, and direct `PreparedWorkflowRun::execute` callers
receive the same callbacks. The existing pre-poll lifecycle test explicitly
expects the contradicted `Running -> Unstarted` result, while the ordinary
transition test does not assert attempts.

Correct the invalid state at the existing scheduling and ledger source. The
transition that publishes `Running` must establish attempt one in both the
published step snapshot and its shared attempt counter before publication.
The existing step task must consume that reserved first attempt rather than
increment it to two; only subsequent retries advance the counter. A pre-poll
abort of a published-running task must then use the existing cancelled-active
settlement path and retain its start/end timestamps.

Keep the existing scheduler, ledger, `JoinSet`, callback, cancellation tree,
retry policy, and terminal owner. Do not add a spawn handshake,
acknowledgement channel, lifecycle service, setting, option, or parallel
downstream guard.

### Module-import and bare-interface rules (`FIND-TASK-004-20`)

The cumulative candidate still has four source-shape violations after R2's
import cleanup:

- `crates/wyrd-spec/src/card/prompt/mod.rs` returns
  `Option<&serde_json::Value>` from the materially changed `raw_body` helper
  even though `Value` is already imported in that module.
- `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs` uses qualified
  `tokio::time::Instant` in the new production interface and its test helper.
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs` retains
  `bytes::Bytes` in the materially changed `fixture_rows_ipc` return interface.
- `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs` places
  `PermissionsExt as _` inside the new, non-generic gateway/external-ownership
  journey, outside the repository's narrow local-trait exception.

Import and use bare `Value`, `Instant`, and `Bytes` in each owning module's
existing top-level dependency block. Move `PermissionsExt as _` to module scope
under `#[cfg(unix)]`. Use an alias only for an actual same-module collision.
This is behavior-neutral; do not add a scanner, lint, allow attribute,
repository check, configuration, setting, or option.

## Preserved behavior and non-goals

- Preserve closure of `FIND-TASK-004-1` through `FIND-TASK-004-18`.
- Preserve exact run IDs, whole-snapshot publication, deterministic plan-order
  errors, retry eligibility and attempt totals, total and step deadlines,
  cancellation precedence, result bounds, and terminal ownership.
- Preserve Cards, gateway, external route, Bifrost query, authorization, audit,
  tenancy, and accepted-authority ownership.
- Keep the deleted Oracle graph-drain polling, supervisor idle refusal,
  reserved-byte poison, and mechanism-specific tests deleted.
- Follower release remains participant grant-stream close. The leader does not
  await a follower release acknowledgement.
- Preserve the approved foreign-tenant harness limitation; do not provision a
  second tenant's gateway credentials to add a model step.
- Do not edit the unchanged `AnalyticalSupervisor::draining_graphs`
  documentation as part of this remediation. Validation rejected it as
  pre-existing debt outside the candidate scope, and no acknowledgement
  mechanism may be inferred or added from it.
- No new lifecycle owner, protocol, dependency, compatibility surface, test
  harness, repository check, setting, or option.

## Acceptance criteria

| Finding | Closure criterion |
|---|---|
| `FIND-TASK-004-19` | Every published `Running` step has `attempts >= 1`; a cancellation/deadline/abort after publication but before first task poll terminalizes that step as `Cancelled` with attempt one and retained start/end timestamps; ordinary success remains attempt one and retry exhaustion remains `max_retries + 1`. |
| `FIND-TASK-004-20` | The four validated sites use module-scope imports and bare interface types, with `PermissionsExt as _` gated at module scope and aliases only for real collisions; behavior is unchanged and no enforcement mechanism is added. |

## Focused and broader proof

Use the existing Skald Workflow tests as the direct behavioral proof:

- extend `prepared_run_keeps_its_id` to assert that every observed `Running`
  step has at least one attempt;
- correct the pre-poll branch in `bounded_attempt_lifecycle` to assert
  `Cancelled`, attempt one, and retained start/end timestamps; and
- retain the existing retry-accounting assertions proving success on attempt
  one and exhaustion at `max_retries + 1`.

Close the import finding through direct source inspection of the four named
modules. It requires no new behavior test or enforcement check.

Run the exact focused Skald test selectors for each named behavioral test using
the repository-prescribed `mise exec -- cargo nextest run --locked` form, then
run the narrow existing Skald family task. Run the existing format and lint
lanes for the Rust source changes. Because the runtime correction changes a
shared Workflow snapshot contract consumed by the server, also run the
existing Wyrd server Workflow journey lane that covers cancellation, deadline,
and terminal snapshots. Use the repository's existing tasks and harnesses only;
do not add a test binary, fixture system, scanner, or check.

## Implementation evidence

Implementation commit: `ca9c7a906`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-004-19`: every published `Running` step has `attempts >= 1` | `RunLedger::step_started` (`crates/skald/skald-workflow/src/run.rs`) records attempt one; `WorkflowExecutor::drive` (`workflow.rs`) stores 1 into the shared counter before `step_started` and publication; `StepTask::run` keeps `store(attempt)` so the first attempt stays one | `prepared_run_keeps_its_id` asserts every observed `Running` step has `attempts == 1` | PASS |
| `FIND-TASK-004-19`: pre-poll cancel/deadline/abort is `Cancelled`, attempt one, timestamps retained | `WorkflowExecutor::settle` drops both `attempts > 0` guards; `RunLedger::finish` rewrites only `Pending` to `Unstarted` | `bounded_attempt_lifecycle` pre-poll branch asserts `(Cancelled, 1)` with `started_at` and `ended_at` set | PASS |
| `FIND-TASK-004-19`: ordinary success remains attempt one; retry exhaustion stays `max_retries + 1` | Retry loop unchanged | Existing retry assertions in `bounded_attempt_lifecycle` (`flaky` = 3, `denied` = 1); `mise run test:skald` | PASS |
| `FIND-TASK-004-20`: module-scope imports and bare interface types | `prompt/mod.rs` `raw_body -> Option<&Value>`; `lifecycle_controls.rs` imports `tokio::time::Instant` in the module and in its test module; `peer_cluster.rs` imports `bytes::Bytes`; `pg_workflow_runs.rs` has a module-scope `#[cfg(unix)] use std::os::unix::fs::PermissionsExt as _;`. No aliases, no checks added | Source inspection; `mise run lints`; `WYRD_TEST_PACKAGES="wyrd-server wyrd-spec" mise run test:wyrd`; `mise run test:bifrost:journey:oracle` | PASS |

Commands (all exit 0):

- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::prepared_run_keeps_its_id) | test(=workflow::tests::bounded_attempt_lifecycle)'`: 2 passed
- `mise run fmt`, `mise run lints`, `git diff --check`
- `mise run test:skald`
- `WYRD_TEST_PACKAGES="wyrd-server wyrd-spec" mise run test:wyrd`: 1591 passed, including every `pg_workflow_runs` journey
- `mise run test:bifrost:journey:oracle`: 43 passed, covering `peer_cluster.rs`

Non-goals still excluded: no new owner, handshake, setting, check, or dependency. Only the two approved test assertions changed. `AnalyticalSupervisor::draining_graphs` is untouched.
