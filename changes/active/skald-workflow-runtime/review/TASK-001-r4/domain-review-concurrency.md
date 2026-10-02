# Concurrency, Cancellation, and Runtime-Lifecycle Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority: `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`

The candidate remained at the stated hash before and after source inspection
and focused verification.

## Reviewed boundary

This pass traced the cumulative local Workflow execution path from
`WorkflowExecutor::drive` through its bounded `JoinSet`, `StepTask`, the fixed
step/Agent/total deadlines, retry and backoff, `AttemptOutcome`, `RunLedger`,
and the Agent provider/tool loop. It covered cancellation and equal-instant
precedence, ordinary peer drain, pre-poll and post-poll abort settlement,
panic conversion while the attempt span is alive, terminal journal settlement,
parent-future drop, and final run snapshots.

The pass also reviewed the remediation-only `WyrdTestServer` change in commit
`516d0fbcc` across in-process and bound modes: process-token propagation,
serve-handle ownership, dedicated-runtime drop, field drop order, and the
fixture's blocking `DROP DATABASE ... WITH (FORCE)` cleanup.

## Prior-finding closure

| Prior finding | Current evidence | Result |
|---|---|---|
| `FIND-TASK-001-20` / `CONC-R3-001` — fixed Agent timeout did not bound terminal journal settlement | `crates/skald/skald-workflow/src/workflow.rs:443-490` fixes an absolute Agent deadline and races it after cancellation, total deadline, and step deadline but before the attempt future. Expiry is projected through `AttemptOutcome::from_agent`; `agent_deadline_bounds_settlement` holds terminal journal append pending, proves two retryable Agent timeouts, equal-deadline precedence, closed spans, and no surviving provider or journal work. | **CLOSED** |
| Panic settlement and span consistency | `workflow.rs:478-490,492-501` catches unwind around the complete attempt while `AttemptSpan` remains live, converts it to non-retryable `WYRD_WORKFLOW_500_INTERNAL`, and settles the span through the ordinary failure path. `attempt_panic_matches_span` proves run, step, and span agreement. | **CLOSED** |

## Review findings

### Important

- **CONC-R4-001 — `WyrdTestServer` still drops the fixture after signalling cancellation without proving that all database-using work stopped.**
  - **Classification:** `INCORRECT` test-harness lifecycle fix.
  - **Violated obligation:** The recorded gate-failure diagnosis and commit `516d0fbcc` require Oracle/Scribe and bound server work to stop before the fixture database is force-dropped. Repository lifecycle ownership requires a retained task to be joined or explicitly aborted before releasing the resource it uses.
  - **Exact location:** `crates/wyrd/wyrd-testing/src/server.rs:745-774,3609-3637`; `crates/wyrd/wyrd-server/src/state.rs:159-205,437-478,698-745,1967-2001`; `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`.
  - **Evidence:** The patch adds `self.inner.state.shutdown_token.cancel()`, but cancellation is only a signal. In-process `shutdown()` has no serve handle and immediately moves `self` to `spawn_blocking(drop)`, while `Drop` on an active Tokio runtime also returns immediately after signalling. Neither path awaits the retained Oracle heartbeat/snapshot/continuity tasks or the Scribe heartbeat/snapshot/WAL tasks. The dedicated runtime owners use `shutdown_background`, which explicitly does not join their workers. The bound path is also not closed: `shutdown()` passes the `JoinHandle` by value into a two-second timeout; on timeout the handle is dropped and the serve task is detached, after which the fixture is dropped. The harness already contains a deterministic bound serve task that ignores cancellation (`server.rs:3555-3573`), proving this is a reachable ownership path rather than a theoretical scheduler race. Moving `fixture` to the last inner field only orders field destruction; it cannot order detached Tokio tasks. `PgFixture` then synchronously executes `DROP DATABASE ... WITH (FORCE)`, exactly the operation that caused the Oracle self-fence abort diagnosed by the remediation record.
  - **Observable consequence:** under a slow or stalled bound drain, or when an in-process role has not yet observed cancellation, teardown can still force-close its database connection. The full `test:wyrd` rerun may be green while retaining the same timing-dependent process-abort/flaky-suite failure the patch is meant to remove; the new `Drop` rustdoc overstates that background roles have stopped.
  - **Testable correction:** make the harness's existing lifecycle owners establish quiescence before releasing `fixture`: for bound mode, retain the serve handle across timeout and use the existing abort-and-join pattern rather than detaching it; for in-process mode, drive the existing `Bifrost::shutdown`/`abort` ownership path (including retained role-task settlement) before dropping the dedicated runtimes and fixture. A timeout/failure path must still abort and join owned work, not merely signal it. Add one focused in-process teardown test and one bound stalled-drain test that hold database-using work across cancellation and assert the work is joined/aborted before the fixture-drop observation; do not add sleeps or widen the timeout.

## Authority and source coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| Ready-step concurrency and parent ownership | Revision 11 `REQ-017`–`REQ-020`, `REQ-048`, `INV-016`, `AC-020`; `workflow.rs:231-311`; `bounded_attempt_lifecycle` | **PASS** |
| Ordinary failure drain, cancellation/total-deadline abort-and-drain, pre-poll settlement | `workflow.rs:239-360`; `run.rs:114-179,210-280`; `bounded_attempt_lifecycle` | **PASS** |
| Retry accounting, backoff, no post-terminal retry | Retry/deadline contract and `REQ-016`; `attempt.rs:58-161`; `workflow.rs:430-518` | **PASS** |
| Deadline construction and cancellation > total > step > Agent precedence | Retry/deadline contract, `REQ-016`, `AC-020`; `workflow.rs:443-490`; `agent_deadline_bounds_settlement` | **PASS** |
| Agent terminal journal settlement | Remediation `FIND-TASK-001-20`; `skald-agent/src/loop_runtime.rs:33-249`; outer fixed deadline in `workflow.rs:454-490` | **PASS** for Workflow execution; standalone Agent behavior remains the explicitly documented best-effort boundary |
| Attempt panic/cancel tracing and run settlement | `REQ-053`; `workflow.rs:478-501,611-661`; `attempt_panic_matches_span`, cancellation control in `run_tracing_spans` | **PASS** |
| Test-server process token, background-role ownership, and fixture order | Commit `516d0fbcc`; `wyrd-testing/src/server.rs:745-774,3555-3573,3609-3637`; `wyrd-server/src/state.rs:159-205,437-478,1967-2089`; `wyrd-dev-fixtures/src/pg.rs:426-475` | **FAIL** — `CONC-R4-001` |

## Verification notes and limits

Focused verification run against the immutable candidate:

- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::agent_deadline_bounds_settlement) | test(=workflow::tests::attempt_panic_matches_span)'` — 2 passed.

The remediation record reports `mise run test:skald` (336 passed) and the full
post-harness-fix `mise run test:wyrd` (2282 passed, 161 skipped). This reviewer
did not rerun those aggregates. No candidate test proves that in-process role
tasks or a timed-out/stalled bound serve task have actually terminated before
`PgFixture` force-drops the database; suite success is therefore not closure of
`CONC-R4-001`.

## Overall result

**FAIL** — Workflow deadline, retry, panic, cancellation, and `JoinSet`
ownership satisfy the reviewed Revision 11 obligations, but the included test
harness remediation still has a reachable detach-before-fixture-drop lifecycle
gap.
