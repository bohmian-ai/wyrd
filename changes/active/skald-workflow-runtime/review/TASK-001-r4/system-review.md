# System-Resilience Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority: `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`
- Additional reviewed change: `516d0fbcce4e3051348293d7c645cae91f3e1e42` (`crates/wyrd/wyrd-testing/src/server.rs`)

The complete cumulative base-to-candidate range was reviewed. The candidate
resolved to tree `bf9ee6b314482a8fe028c31f4121eab6b8ac9e92` during this pass, and commit
`516d0fbcc` is an ancestor of it.

## Deployed-path evidence

| Path | Runtime owner and failure boundary | Evidence |
|---|---|---|
| Local Workflow | `WorkflowExecutor` owns one bounded `JoinSet`, fixed total deadline, cancellation token, attempt counters, and terminal ledger. Parent drop drops the set; cancellation and total timeout abort then drain it. | `crates/skald/skald-workflow/src/workflow.rs:88-138,227-312` |
| Step and Agent attempt | `StepTask` fixes step and Agent deadlines before polling the attempt. Its biased race is cancellation, total deadline, step deadline, Agent deadline, then attempt. The complete attempt future is unwind-caught while its attempt span is alive. | `crates/skald/skald-workflow/src/workflow.rs:414-519` |
| Native provider/tool work | The existing Agent loop remains inside the owned step future. Dropping the attempt drops in-flight provider or tool futures; no new detached Skald task or Observer callback remains. | `crates/skald/skald-workflow/src/workflow.rs:521-569`; `crates/skald/skald-agent/src/loop_runtime.rs:29-239,260-529` |
| WyrdGateway seam | The attempt-local adapter carries the earliest fixed deadline, run cancellation, fallback, and correlation into the narrow caller. An expired deadline refuses locally; actual public and in-process implementations remain later-task consumers, so TASK-001 proves this neutral seam with a fake caller only. | `crates/skald/skald-workflow/src/route.rs:367-408,505-549`; `crates/skald/skald-workflow/src/workflow.rs:1841-2039` |
| ExtGateway | Route preparation fixes one screened, no-proxy, no-redirect, bounded client. Provider retry stays within the owned attempt. Successful typed responses are checked for sensitive bound-header values after provider retries; the containment refusal is terminal and safe. | `crates/skald/skald-workflow/src/route.rs:278-345,569-606`; `crates/skald/skald-providers/src/clients/external.rs:38-235` |
| Payload-free tracing | `workflow.run` owns one `workflow.step` span per attempt. Panic conversion happens before the span guard settles, so returned failure and span code agree; dropped attempts remain `cancelled`. Agent/model/tool spans contain metadata rather than payloads. | `crates/skald/skald-workflow/src/workflow.rs:200-224,454-501,611-661`; `crates/skald/skald-agent/src/loop_runtime.rs:730-992` |
| Test-only server harness | `WyrdTestServer` composes production Oracle/Scribe/Forge owners over an ephemeral Postgres fixture. Commit `516d0fbcc` moves the fixture to the last field and cancels `AppState::shutdown_token` from `shutdown` and `Drop`. This path is test-only, but its production role implementations use the real aborting Oracle epoch terminator. | `crates/wyrd/wyrd-testing/src/server.rs:183-241,741-775,3609-3638`; `crates/vala/vala-bifrost-redux/src/oracle/reader_pins.rs:315-329` |

TASK-001 remains a local process-owned runtime. A process crash or rolling
replacement drops an in-flight local run without replaying provider or tool
effects. That is the approved V1 boundary. Server Workflow-run durability,
accepted-job shutdown ownership, and replica affinity remain later tasks and
are not credited to this candidate.

## Failure and recovery paths

| Failure or interruption | Observed behavior | Recovery and proof assessment |
|---|---|---|
| Provider or gateway outage | Typed connection, timeout, decode, 408/429/5xx and the three approved Wyrd gateway codes may retry at the Workflow boundary after route-owned retry finishes. Auth, permission, binding, route, tool, journal, cancellation, invariant, and credential-reflection failures terminate. | `bounded_attempt_lifecycle` and `external_gateway_success_reflection` cover classification, exhaustion, no surviving attempt, and terminal containment. |
| Fixed Agent deadline | The new fourth deadline arm drops the complete Agent attempt, including terminal journal settlement, and projects the ordinary retryable Agent timeout. Cancellation, total, and step deadlines precede it. | `workflow::tests::agent_deadline_bounds_settlement` passed and asserts retry exhaustion, failed spans, zero held appends, zero in-flight calls, and equal-bound precedence. Prior `SYS-R3-001` is closed. |
| Attempt panic | Caller-supplied tool panic is caught inside `StepTask` while the attempt span exists, becomes non-retryable `WYRD_WORKFLOW_500_INTERNAL`, and cannot unwind the shared process. | `workflow::tests::attempt_panic_matches_span` passed; explicit cancellation remains a separate cancelled outcome. |
| Explicit cancellation or total deadline | The parent stops scheduling, aborts every active Skald step, drains the `JoinSet`, retains completed results, marks begun work cancelled and never-started work unstarted, then returns one complete snapshot. | Existing `bounded_attempt_lifecycle`, parent-drop, and tracing controls remain applicable. |
| Ordinary step failure | New scheduling stops, but already-running peers drain normally. A non-cooperative peer can therefore delay an uncapped local run; this is the approved ordinary-failure contract rather than a regression. | `WorkflowExecutor::drive` and `bounded_attempt_lifecycle`. |
| ExtGateway reflection or decode/refusal | A retained successful response containing a bound sensitive header value is refused after provider retry, and public errors contain neither response nor match. Decode and non-success diagnostics are fixed safe strings. | `workflow::tests::external_gateway_success_reflection` and `bound_external_gateway_security` passed. |
| Process restart | Local run state is lost and no provider/tool call resumes automatically. | Approved process-local boundary; callers start a new local run. |
| In-process harness teardown | The candidate cancels the shared process token and immediately proceeds to ordinary field destruction. There is no in-process serve handle and no role join. The last fixture field then force-drops the database. | Cancellation is signalled, but role completion before database destruction is not proven; see `SYS-R4-001`. |

## Affected capabilities

- The deadline and panic remediation affects local Rust and Python Workflow
  execution for Native, WyrdGateway, and ExtGateway routes through the shared
  Skald engine. It adds no service-level crash path or detached execution owner.
- ExtGateway containment affects only successful responses retaining a bound
  secret; clean success, transport retry, and existing non-success handling
  remain unchanged.
- The harness finding affects tests using in-process `WyrdTestServer`, notably
  server/card suites that rely on implicit drop or `shutdown()` without a bound
  serve task. It does not change production server teardown, which runs the
  bounded Bifrost shutdown path through `BoundServer::run`.

## Material proposed findings

### SYS-R4-001 — In-process harness teardown still drops Postgres without joining background roles

- Classification: `INCORRECT`
- Violated obligation or regression boundary: commit `516d0fbcc` was added to
  make the required `test:wyrd` lane reliable by stopping Oracle, Scribe, and
  Forge before the fixture database is force-dropped. Repository shutdown
  authority requires role work to drain or abort through its owner before a
  backing dependency disappears; changing a harness to clear a failure must
  fix that diagnosed cause rather than rely on scheduling.
- Exact location: `crates/wyrd/wyrd-testing/src/server.rs:757-773,3609-3638`;
  fixture order at `crates/wyrd/wyrd-testing/src/server.rs:183-241`;
  asynchronous Oracle cancellation race at
  `crates/vala/vala-bifrost-redux/src/oracle/reader_pins.rs:1791-1834`;
  non-joining runtime drops at `crates/wyrd/wyrd-server/src/state.rs:189-205,252-267`.
- Evidence and reachability: `WyrdTestServer::shutdown` and `Drop` call
  `state.shutdown_token.cancel()`, but the in-process mode has no
  `serve_handle`, and neither path awaits `Bifrost::shutdown`, `Bifrost::abort`,
  or any Oracle/Scribe/Forge supervisor. Field order only delays
  `PgFixture` destruction until synchronous owner values have been dropped.
  `ScribeCoordinationRuntime` and `ForgeCompactionRuntime` explicitly use
  `shutdown_background`, which does not join worker threads. Oracle's lease
  supervisor observes cancellation only when it is next polled and races an
  in-flight renewal result. `PgFixture` then executes `DROP DATABASE ... WITH
  (FORCE)` from its destructor. Thus a current-thread test cannot poll the
  cancelled Oracle task during synchronous drop, while a multi-thread test can
  race cancellation against the forced connection failure that invokes the
  production `AbortingEpochTerminator` path diagnosed in the remediation.
- Observable system consequence: the original suite-level SIGABRT remains
  scheduling-dependent rather than structurally excluded. One green
  `test:wyrd` rerun shows the race did not fire in that run; it does not prove
  that background roles stopped before database destruction, and the new
  rustdoc currently claims a stronger guarantee than the code provides.
- Testable correction: reuse the existing Bifrost shutdown/abort owner and
  retain the fixture until its selected role supervisors have completed or
  been joined under a bounded teardown path. Both explicit `shutdown()` and
  implicit drop must make database destruction unreachable while a role can
  still renew, publish, reconcile, or self-fence; do not replace the aborting
  Oracle terminator or add sleeps/retries. Add one harness regression test that
  tears down an in-process server with a live/renewing role and proves role
  completion precedes fixture drop, including the implicit-drop path used by
  existing current-thread tests.

## Prior-finding closure and recovery assessment

- `FIND-TASK-001-20` / prior `SYS-R3-001` is closed: the Workflow owner now
  races the already-fixed Agent deadline over complete attempt settlement.
- `FIND-TASK-001-21` is closed for process containment: a panicking local tool
  becomes the same internal failure in both the attempt span and run snapshot.
- `FIND-TASK-001-22` is closed at the source transport owner: retained decoded
  ExtGateway content cannot reflect a bound credential into Workflow output.
- `FIND-TASK-001-23` does not add a failure/recovery owner; canonical
  provider/model attributes and payload exclusion were independently exercised.
- The remaining documentation, owner-shape, dependency, and permanent-doc
  remediation items do not alter deployed recovery topology.
- No Observer crate, callback task, replacement telemetry runtime, durable run
  queue, or compatibility lifecycle was reintroduced under Revision 11.

## Verification notes

- Independently run and passed:
  - `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::agent_deadline_bounds_settlement) | test(=workflow::tests::attempt_panic_matches_span)'` — 2 passed.
  - `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::external_gateway_success_reflection) | test(=workflow::tests::bound_external_gateway_security)'` — 2 passed.
  - `mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_genai_google_provider_and_model) | test(=agent_run_emits_genai_spans_without_payloads)'` — 2 passed.
  - `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..a704a8890ef20efe65fee1e116f7d02288f8ec5c` — passed.
- Recorded candidate evidence reports `mise run test:skald` (336 passed) and,
  after `516d0fbcc`, `mise run test:wyrd` (2282 passed, 161 skipped), plus format,
  lints, docs, examples, and shared tests green.
- No focused test accompanies `516d0fbcc`; the full-family rerun detects a
  process abort only if the teardown race happens to schedule adversely. It
  does not assert that in-process role supervisors joined before fixture drop.

## Overall result

**FAIL**

The runtime remediation closes the prior Workflow resilience gaps. The added
test-harness fix remains signal-only and does not establish the shutdown-before-
database-drop ordering it claims, leaving one bounded, independently testable
system finding: `SYS-R4-001`.
