# TASK-004 follow-up review: peer-target shutdown ordering

Role: follow-up reviewer (discovery pass on one open question; this is not a vote).
Subject: base `a56ab7569` .. candidate `990803fc0`. All source was read with `git show 990803fc0:<path>`.
Comparison sources: base `a56ab7569` and approved TASK-006 source `0e9c6e98c`.
This was a static review. Nothing was edited, built, or run.

## Obligation under test

TASK-004 replay map, boot/peer row (`tasks/TASK-004-...md` ~line 262): "Peer mode publishes ready
membership only after role activation and the private listener is serving; shutdown withdraws
readiness and role advertisements before draining accepted work." Related: verified-change-contract
REQ-077 (`changes/active/verified-change-contract/spec.md:752-761`): "If enqueue fails or the process
stops before it completes ... the server emits a structured tracing error."

## Source inspected (candidate 990803fc0)

- `crates/wyrd/wyrd-server/src/app/shutdown.rs:15-22` `signal_watcher`
- `crates/wyrd/wyrd-server/src/app/supervise.rs:125-201` `classify_first_exit_with_shutdown`, `drain_with_shutdown_hooks`, plus tests `:299-330`
- `crates/wyrd/wyrd-server/src/app/server.rs:502-888` `BoundServer::run` (peer task `:682-725`, signal `:739-742`, hooks `:748-779`, post-drain `:791-884`), test `:972-1020`
- `crates/wyrd/wyrd-server/src/state.rs:640-695` (Oracle `start_draining`/`activate`/`begin_shutdown`), `:1045-1092` (Scribe equivalents), `:1728-1736` `activate_peer_roles`, `:1927-1938` `Bifrost::begin_shutdown`, `:1963-2038` `shutdown`/`drain_selected_owners`
- `crates/wyrd/wyrd-server/src/boot/mod.rs:1031,1256,1294` (`role_shutdown` is the server shutdown token), `:1104-1106` (ObservationEnqueue wiring)
- `crates/vala/vala-bifrost-redux/src/cluster/mod.rs:26,496-600` (heartbeat, `deactivate`, `ROLE_LIVENESS_CUTOFF`)
- `crates/wyrd/wyrd-server/src/grpc/mod.rs:401-470` `build_peer_grpc`
- `crates/wyrd/wyrd-tonic/src/server/mod.rs:253-380` `serve_peer_grpc_with_listener`, `StoppingIo`
- `crates/wyrd/wyrd-server/src/oracle/peer_service.rs:44-115,540-580`
- `crates/wyrd/wyrd-server/src/verification/observations.rs:1-132`
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_network/analytical.rs:1238-1340`
- `review/task-008-r4-review/TASK-008-R5-preserve-oracle-only-peer-drain-and-complete-evidence.md:25-40`

## Claim A: SIGTERM cancels transports before readiness withdrawal; durable deactivate comes only after drain

**Confirmed. The claim was too narrow: the durable withdrawal comes after transport drain on every shutdown path, not only on SIGTERM.**

1. On the SIGTERM/SIGINT path, `signal_watcher` calls `shutdown.cancel()` directly (`shutdown.rs:18`).
   Every transport is driven by that same token: HTTP `server.rs:671`, public gRPC `:677-679`, and the
   peer listener `:688,702/704`. `classify_first_exit_with_shutdown` returns through its biased
   `shutdown.cancelled()` arm (`supervise.rs:129-131`). Only then does `drain_with_shutdown_hooks` run
   the "before_cancel" hook (`supervise.rs:179`, `server.rs:754-765`). The second `shutdown.cancel()` at
   `supervise.rs:183` does nothing. So on the production signal path, `Bifrost::begin_shutdown`
   (gate close, local `start_draining`) runs after the listeners have begun stopping. The hook only
   runs before cancellation when a supervised task exits first.
2. On every path, `Bifrost::begin_shutdown` (`state.rs:1927-1938`) changes only process-local state:
   `gate.close()`, `Oracle::start_draining` / `Scribe::start_draining` (`state.rs:640-645`,
   `:1045-1050`), which set the `bifrost_role_ready` gauge and `advertise_ready=false`. It does not
   write membership.
3. Nothing republishes `advertise_ready=false` durably before the drain. The readiness heartbeat
   (`cluster/mod.rs:502-528`) is spawned with `role_shutdown`. `role_shutdown` is the server shutdown
   token itself (`boot/mod.rs:1256` creates it, `:1294` passes it to `compose_bifrost`, `:1031` and the
   Oracle builder set `role_shutdown: shutdown`). The heartbeat therefore returns at the same instant
   the transports are cancelled (`cluster/mod.rs:513`). It never writes `ready=false`.
4. The only durable withdrawal is `cluster.deactivate` (`cluster/mod.rs:587-600`). Its own doc says
   "Shutdown uses this before the transport drain window". It runs from `Oracle::begin_shutdown` /
   `Scribe::begin_shutdown` (`state.rs:692-695`, `:1089-1092`), and those are called only from
   `drain_selected_owners` (`state.rs:1999`, `:2009-2013`). That code is reached through
   `bifrost.shutdown(deadline)` at `server.rs:863`, after all of the following:
   - the supervised transport/worker drain (`server.rs:750-779`)
   - the MCP tracker drain (`:791-794`)
   - the gateway tracker drain (`:798-801`)
   - the gateway capture shutdown (`:805-810`)
5. The result is a gap. From token cancellation until `drain_selected_owners` runs, or until the
   15 s `ROLE_LIVENESS_CUTOFF` expires (`cluster/mod.rs:26`), whichever is first, the role's
   membership row is still `ready=true` with a fresh heartbeat. During that time the peer listener has
   already stopped accepting connections or is severing them. Remote leaders keep choosing this node
   from their snapshot. A connection failure to a stopped listener is a transport loss, not a
   pre-accept capacity refusal. Spec AC-013 says only an explicit pre-accept refusal is retried
   (`spec.md:561-563`), so the remote query fails instead of routing to a surviving node. The
   `cluster::deactivate` doc and the `start_readiness_heartbeat` doc ("Role shutdown flips `ready`
   before durable deactivation, allowing the heartbeat to continue fencing the role during transport
   drain") describe the intended order. The server does not implement it.
6. Provenance:
   - Base: `a56ab7569` has the same hook structure (`server.rs:769-798` at base). It has the same
     sync `Bifrost::begin_shutdown` (base `state.rs:1823-1834`), the same deactivate inside
     `drain_selected_owners` (base `state.rs:1895`), and the same `shutdown.rs`. `shutdown.rs` and
     `supervise.rs` are unchanged in the range (`git diff a56ab7569 990803fc0` is empty for both).
   - TASK-006 source: `0e9c6e98c` has the same order (`server.rs:703,712-725`; `state.rs:1902-1913`;
     deactivate at `state.rs:1974`).
   - Conclusion: TASK-004 did not introduce the ordering; it replayed source and base faithfully. The
     TASK-004 replay row still states the required order as a required outcome ("Integrated owner and
     required ordering"), and the candidate does not deliver it. "Pre-existing" does not discharge an
     explicit task obligation.
7. Tests that claim to prove the ordering do not model the production trigger:
   - `supervise.rs:299-330` `shutdown_hook_runs_before_active_request_drain` spawns
     `worker_task(TaskId::Signal, async {})`, which exits without cancelling the token. The real
     `signal_watcher` cancels first. Run against the real watcher, the hook's
     `!hook_shutdown.is_cancelled()` assertion would fail.
   - `server.rs:972-1020` `bound_server_run_uses_one_deadline_for_each_stalled_phase` triggers shutdown
     with `spawn_worker("shutdown_trigger", async {})`, a worker exit and not a signal, and it checks
     phase deadlines, not membership.
   - No peer journey checks that membership is `ready=false` before the stopping pod's peer listener
     refuses. `peer_network/analytical.rs:1253` covers fragment release only.

## Claim B: `StoppingIo` severs every accepted peer connection on combined pods, including Analytical and lifecycle services

**Factually confirmed, but this is an approved sibling decision, not a new TASK-004 finding.**

- `serve_peer_grpc_with_listener` and `StoppingIo` (`wyrd-tonic/src/server/mod.rs:253-380`) do not
  exist at base `a56ab7569` or at source `0e9c6e98c` (`git grep StoppingIo` finds nothing in either).
  In the range they were introduced by `83ccbc634` ("serve live reads from one Parquet scan with
  per-file tenant proof"), the only commit in `git log -S StoppingIo a56ab7569..990803fc0`. That is
  sibling TASK-007/008 work.
- Selection happens at `server.rs:699-705`: `serves_scribe = bifrost.scribe().is_some()`. On All,
  Server, or Scribe peer pods that also serve query, the same router carries the Analytical stage
  worker service and `OracleLifecycleGrpc` (`grpc/mod.rs:446-470`). Those accepted streams fail with
  `ConnectionAborted` at token cancellation. Before this change they received tonic's graceful drain.
  All-target peer mode is a supported topology: `config.rs:2855-2870` validates it and
  `peer_network/join.rs:71` uses it.
- This is an explicit, reviewed decision. TASK-008-R5 (`task-008-r4-review/TASK-008-R5-...md:38`)
  chose "cancellation-aware accepted IO only for listeners serving Scribe fragments, **including
  mixed-role listeners**" and kept graceful drain only for Oracle-only listeners. Separately,
  `OraclePeerGrpc` already ends open fragment streams on the same token (`peer_service.rs:44,111,572-578`),
  whichever runner is used.
- Relation to TASK-004: this makes Claim A worse on combined pods. Accepted work is cut immediately,
  and membership keeps advertising the pod as ready (item A.5), so leaders keep sending new stages
  into the severed listener. No separate TASK-004 finding is raised. If the owner wants mixed-role
  Analytical drain, that belongs to the TASK-008 R5 decision, not this review.

## Claim C: `ObservationEnqueue`'s `TaskTracker` is never closed or awaited

**Confirmed. One narrow finding.**

- `observations.rs:36,48,111`: a private `TaskTracker` spawns each enqueue. Nothing exposes it, and no
  shutdown path calls `close()` or `wait()`. Boot hands the only instance to Gate
  (`boot/mod.rs:1104-1106`). `BoundServer::run` drains `mcp_tasks` and `gateway_tasks` (`server.rs:791-801`)
  but not this tracker. `Bifrost::shutdown` does not reach it either.
- Source `0e9c6e98c` is identical (`observations.rs:17,36,48,111`, wired at `boot/mod.rs:1133`), so
  this was replayed as-is.
- Consequence: on a graceful stop, up to `PENDING_LIMIT` (256) in-flight post-ACK enqueues are
  dropped when the runtime ends. A dropped task never reaches `record_failure`, so the stop produces
  no structured error and no `verification_observation_enqueue_failures_total` increment.
- Losing the run itself is accepted (verified-change-contract `spec.md:79-82`, REQ-077). Losing it
  silently is not. REQ-077 requires that "If enqueue fails **or the process stops before it
  completes** ... the server emits a structured tracing error". The module doc
  (`observations.rs:7-9`: "a failure is logged and counted") makes the same promise.

## Proposed findings

### FUP-004-01: Peer shutdown does not withdraw durable readiness before transports stop and accepted work drains

- Classification: INCORRECT. The task-stated ordering is not implemented. It is pre-existing at base
  and in the TASK-006 source, so it is not a REGRESSION.
- Violated obligation:
  - TASK-004 replay map, boot/peer row (task file ~line 262): "shutdown withdraws readiness and role
    advertisements before draining accepted work".
  - AGENTS.md §15/§9 tenant-preserving server lifecycle (supporting).
- Location:
  - `crates/wyrd/wyrd-server/src/app/shutdown.rs:18`
  - `crates/wyrd/wyrd-server/src/app/server.rs:748-779,863`
  - `crates/wyrd/wyrd-server/src/state.rs:1927-1938,1999,2009-2013`
  - `crates/wyrd/wyrd-server/src/boot/mod.rs:1031,1294` (heartbeat bound to the server token)
- Evidence: items A.1-A.7 above.
  - The signal watcher cancels the shared token before the "before_cancel" hook.
  - The hook writes no membership.
  - The heartbeat that would carry `advertise_ready=false` stops on that same token.
  - `cluster.deactivate` runs only inside `drain_selected_owners`, after the supervised, MCP, gateway,
    and capture drains.
- Observable consequence: on SIGTERM of a peer-enabled Oracle, Scribe, All, or Server pod, membership
  keeps the node `ready` with a fresh heartbeat for up to 15 s or until the late deactivate. Its
  private listener has already stopped (or, on Scribe-bearing pods, severed connections at once).
  Other leaders keep scheduling Analytical stages, live fragments, and tail reads onto it. Each one
  fails as a transport loss (no AC-013 retry) instead of being routed to a surviving replica. A
  rolling restart therefore fails queries that were admitted on other pods.
- Required correction:
  - Make durable withdrawal precede transport cancellation on every trigger.
  - For example: the signal watcher only signals (it does not cancel the token), and the pre-cancel
    hook awaits each selected role's `cluster.deactivate`, bounded by the process deadline. Or give
    roles a child token so the heartbeat can publish `ready=false` through the drain window.
  - Keep the later `drain_selected_owners` deactivate idempotent.
  - Add a test that drives the real trigger: a test-support signal path, or a peer journey that stops
    a pod through the same API `signal_watcher` uses. It must assert:
    1. The stopping role's membership row is `ready=false` before its peer listener refuses connections.
    2. A leader query planned immediately after the stop selects only surviving peers and succeeds.
  - Fix `supervise.rs:305` so its `Signal` task cancels the token the way production does.

### FUP-004-02: In-flight Eval observation enqueues are lost silently at graceful shutdown

- Classification: MISSING. The required shutdown-loss signal is absent.
- Violated obligation:
  - verified-change-contract REQ-077 (`changes/active/verified-change-contract/spec.md:759-761`),
    carried by the TASK-004 replay row "`ObservationEnqueue` ... tracked task ... backlog or enqueue
    failure logs/counts failure".
  - AGENTS.md §6 (tracked, bounded background work at shutdown).
- Location:
  - `crates/wyrd/wyrd-server/src/verification/observations.rs:36,48,111-119`
  - `crates/wyrd/wyrd-server/src/boot/mod.rs:1104-1106`
  - `crates/wyrd/wyrd-server/src/app/server.rs:791-810` (other trackers are drained here; this one is not)
- Evidence: Claim C above. The tracker is never closed or awaited, and an aborted task never calls
  `record_failure`.
- Observable consequence: an Eval observation ACKed just before SIGTERM can lose its run with no
  `tracing::error!` and no failure-counter increment. Operators cannot tell an accepted best-effort
  loss from a correct run that was never needed.
- Required correction:
  - Expose the tracker, or a `shutdown(deadline)` method, through the state that owns Gate.
  - In `BoundServer::run`, close it and wait for it within the same process deadline as
    `mcp_tasks`/`gateway_tasks`.
  - For each task still pending at the deadline, or dropped, call `record_failure`. A drop guard
    inside the spawned future is enough.
  - Add a unit or integration test that stalls one enqueue (for example on an unavailable tenant
    connection), triggers shutdown, and asserts that the failure counter increments and the
    structured error is emitted.

## Result

- Claim A: **RESOLVED**, confirmed (FUP-004-01). Pre-existing at base and in the TASK-006 source, but
  contrary to the explicit TASK-004 ordering obligation.
- Claim B: **RESOLVED**. Factually confirmed and introduced by sibling commit `83ccbc634`. It is an
  explicitly approved TASK-008-R5 mixed-listener decision, so it is not a TASK-004 finding; it does
  make FUP-004-01 worse on combined pods.
- Claim C: **RESOLVED**, confirmed (FUP-004-02).

Overall: **RESOLVED**. Overall result for this follow-up: **FAIL** (FUP-004-01, FUP-004-02).
