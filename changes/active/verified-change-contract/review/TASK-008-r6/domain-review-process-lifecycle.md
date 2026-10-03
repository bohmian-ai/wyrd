# TASK-008 round-six process-lifecycle domain review

## Result

**PASS**

No material process-lifecycle finding is proposed. Within the caller-narrowed
closure scope, `FIND-TASK-008-CLOSEOUT-16` is closed and the remediation range
introduces no process ownership, shutdown, async-blocking, ordering, logging,
error-conversion, or cancellation regression.

## Reviewed boundary

- Immutable subject:
  `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
- Approved authority: `changes/active/verified-change-contract/spec.md`,
  revision 57, especially REQ-171.
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior decision and remediation:
  `review/TASK-008-r5/findings-validation.md`, `review/TASK-008-r5/verdict.md`,
  and `review/TASK-008-r5/TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`.
- Governing repository rules: `AGENTS.md` sections 5, 6, 11, and 12;
  `architecture/agent-rules.md`; the maintainer-style owner/method and async
  guidance; and the process-local audit description in
  `architecture/bifrost-design.md`.
- Source boundary:
  `capacity/main.rs` (`Benchmark::run`, `Benchmark::clean_up`, and the focused
  slow-stop proof), `capacity/step.rs` (`Deployment`,
  `Deployment::stop_replicas`), `release_server.rs` (`LocalServer::stop`,
  `LocalServer::terminate`, and `Drop`), and the shutdown consumer in
  `capacity/report.rs`.

Per the user's closure restriction, I did not reopen process code accepted in
earlier rounds. I checked that the two-commit remediation range preserves that
accepted behavior and does not make the benchmark verdict structurally false.
`FIND-TASK-008-CLOSEOUT-13` remains deferred and supplies no capacity
qualification here.

## Authority and source evidence

| Boundary | Evidence | Assessment |
|---|---|---|
| Concrete lifecycle owner | `Benchmark::clean_up` first takes the deployment, takes and shuts down its clients, and then invokes `deployment.stop_replicas(&self.output)` (`capacity/main.rs:581-604`). `Deployment` owns the replica collection (`capacity/step.rs:30-48`), and the collection workflow is now its inherent method (`capacity/step.rs:427-454`). The prior module-level `stop_replicas` workflow was deleted. | PASS. This is the exact existing-owner correction required by FIND-16 and the repository's struct-centered rule; no manager, trait, helper type, or parallel ownership path was introduced. |
| Single-process owner and blocking boundary | `Deployment::stop_replicas` moves each `LocalServer` into `tokio::task::spawn_blocking`, where `LocalServer::stop` retains synchronous TERM, grace wait, kill/reap, and log-copy ownership (`capacity/step.rs:443-450`; `release_server.rs:365-405`). | PASS. The async method directly composes a real blocking process boundary, and no synchronous process wait runs on a Tokio worker. |
| Newest-first stop and ordinal report alignment | Replicas are taken as one collection, traversed with `rev()`, then the collected outcomes are reversed before return (`capacity/step.rs:444-453`). Replica ordinals are assigned from deployment length and appended during scale-out (`capacity/main.rs:523-533`); log names use the replica's own ordinal (`capacity/step.rs:445-446`). `Report` labels returned positions as replica ordinals (`capacity/report.rs:498-504`). | PASS. Execution remains newest-first while results retain the original ordinal order, identical to the accepted free-function behavior. |
| Join and process error conversion | The blocking closure converts `LocalServer::stop` failures to strings; a failed or panicked blocking task is converted from `JoinError` to the same per-replica error slot (`capacity/step.rs:447-450`). `LocalServer::stop` still attempts the log copy after termination and returns the stop duration only for a clean exit (`release_server.rs:365-377`). | PASS. Error shape and report consumption are unchanged by the owner move. |
| Cancellation and partial progress | `mem::take` leaves `Deployment::replicas` empty before stops progress. The active replica is owned by the blocking task; unstarted replicas remain in the future's iterator and fall through `LocalServer::Drop`, which kills, reaps, preserves the working directory, and prints the retained log path (`capacity/step.rs:438-450`; `release_server.rs:487-503`). | PASS. Dropping the future preserves the previously documented partial-progress boundary: the active stop finishes off-worker and later replicas receive the abnormal-stop fallback. |
| Cleanup and report lifecycle | `Benchmark::run` always awaits `clean_up` after the bounded measurement, records any client cleanup failure, and writes the report only after replica stop results are available (`capacity/main.rs:379-424`). | PASS. Client-before-replica order and report contents remain owned by `Benchmark`; moving collection shutdown onto `Deployment` does not alter report-before-wrapper-teardown behavior. |
| Focused proof shape | `a_slow_replica_stop_leaves_the_runtime_free` now constructs a `Deployment`, installs it on `Benchmark`, and calls `Benchmark::clean_up`; it checks no client failure, continued single-worker heartbeat progress, the clean two-second exit, reaping, the ordinal result slot, and `server-0.log` (`capacity/main.rs:907-993`). | PASS structurally. The test now exercises the production owner path instead of retaining a free workflow for test access. Its empty client set is sufficient for this owner/async proof; source establishes the client-before-replica sequence. |

## Regression trace

The process behavior in the remediation range is a relocation without semantic
expansion: the deleted free function and `Deployment::stop_replicas` have the
same loop, reverse traversal, per-ordinal log derivation, `spawn_blocking`
closure, join/process conversion, final result reversal, and cancellation
effects. The only caller changes from passing a taken replica vector to calling
the method on the deployment that already owns it. `LocalServer::stop`,
`terminate`, and `Drop` are outside the range and continue to define the
single-process lifecycle. I found no range-introduced process-lifecycle path
that can change the benchmark's PASS/FAIL result.

## Verification limits

- The remediation record reports the complete capacity binary passing 20/20
  with ignored tests enabled on a compatible host, including
  `a_slow_replica_stop_leaves_the_runtime_free`, and reports the focused
  `release_server` tests passing 2/2.
- A local attempt of the exact slow-stop test compiled the current target but
  failed at `Benchmark::prepare` with sandbox `Operation not permitted` before
  it reached `LocalServer::start` or any shutdown assertion. The test already
  declares its need for a delegating user systemd manager; this is an
  environment limit, not contradictory lifecycle evidence. No further build
  or test command was run in this domain pass.
- The full unmodified default `mise run bench:capacity` remains deferred as
  `FIND-TASK-008-CLOSEOUT-13`; this review makes no empirical AC-040/AC-041
  capacity claim.
- The separately tracked intermittent
  `verification_runtime::two_bindings_share_one_client_observation` failure is
  outside this range and process-lifecycle boundary. It is noted for the
  integrated branch and is not counted against this remediation.

## Findings

None.

## Final domain verdict

**PASS** — `FIND-TASK-008-CLOSEOUT-16` is closed, and
`1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`
introduces no process-lifecycle regression within the user-directed closure
scope.
