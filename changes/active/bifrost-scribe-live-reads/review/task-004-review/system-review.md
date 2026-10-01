# TASK-004 System-Resilience Review

- Reviewer role: system-rev (processes crash, dependencies disappear, messages are delayed, recovery follows interruption)
- Subject: base `a56ab7569` to candidate `990803fc0`. Source was read at the candidate.
- Authorities:
  - `changes/active/bifrost-scribe-live-reads/spec.md` (rev 24)
  - `tasks/TASK-004-integrate-eval-server-and-unify-bifrost-memory.md`
  - `AGENTS.md`
  - `architecture/agent-rules.md`
- Method: static review only. I ran no lanes, builds, or benchmarks. The task's D1-D19 evidence was treated as claims to falsify.

## 1. Deployed Topology

The topology comes from `deploy/kubernetes/kind/wyrd.yaml` plus the infra manifests.

| Unit | Target | Peer | State | Limits | Probes |
|---|---|---|---|---|---|
| `wyrd-core` StatefulSet (1 replica) | `WYRD_TARGET=all` | mTLS, `WYRD_PEER_ADDRESS=$(POD_IP):50052` | 2Gi PVC data root (WAL, Forge spill, Oracle spill) | 2 CPU / 4Gi; Bifrost cap 3Gi | `/readyz` every 2s, failureThreshold 3; no liveness; grace 45s |
| `wyrd-oracle` Deployment (HPA 1-2) | `WYRD_TARGET=oracle` | mTLS | 2Gi emptyDir | 3Gi | `/readyz` |
| Postgres | infra | - | durable registry, membership, verification queue, audit staging | - | - |
| RustFS S3 | infra | - | object storage for Iceberg/Bifrost | - | - |

`BifrostTarget` is `{All, Server, Oracle, Scribe, ForgeWorker}` (`config.rs:183-212`).

- `serves_api()` is true for All, Server, Oracle and Scribe (`config.rs:200`).
- Oracle and Scribe require peer mode.
- The deleted `deploy/kubernetes/bifrost/*` role-separated manifests are replaced by the kind manifests. A Scribe-only target therefore remains a supported `WYRD_TARGET` value, but no checked-in manifest deploys it.

Supervision (`app/server.rs:502-888`, `app/supervise.rs`) works as follows:

1. Every component runs in a single `JoinSet`.
2. The first exit is classified by `classify_first_exit_with_shutdown`, which is biased toward `shutdown.cancelled`.
3. `drain_with_shutdown_hooks` runs the `before_cancel` hook. That hook is `Bifrost::begin_shutdown` (`state.rs:1927`), a synchronous, local-only call: it closes Gate, starts Oracle/Scribe draining, and begins Forge shutdown.
4. The hook then calls `shutdown.cancel()` and drains to the deadline.
5. The MCP and gateway trackers drain.
6. `Bifrost::shutdown` runs `drain_selected_owners` (`state.rs:1963-2045`). This performs the durable `cluster.deactivate`, drains the owners, and closes storage.

`signal_watcher` (`app/shutdown.rs:18`) cancels the same token directly on SIGTERM or SIGINT.

## 2. Failure-Path Evidence

For each path the table records five things: what stops, what remains, whether anything amplifies, what state survives, and how service resumes and whether that is proven.

| Path | Stops | Remains | Amplification | Survives | Resume / proof |
|---|---|---|---|---|---|
| Boot / readiness | `/readyz` is 503 until `ReadinessSnapshot.all_ok`. Peer roles activate only after the listener task is polled (`server.rs:699-725`); a failed activation is terminal. | - | None. The probe reads a cached snapshot that `readiness_loop` computes. | Last snapshot | The pod restarts on terminal exit. Boot journeys cover the combined target. |
| `/readyz` combined vs Scribe-only | A WAL fault is excused when `scribe_fault_is_role_local` (`health/mod.rs:150`, `:195-197`). | Combined target: Oracle queries and verification | See **SR-1**: a Scribe-only target is also excused when verification is composed. | - | The unit test only toggles the flag (`health/mod.rs:683`). No Scribe-target case exists. |
| Peer membership publish/withdraw | Publish happens after activation. Withdraw: `begin_shutdown` is local, and the durable `cluster.deactivate` runs only inside `drain_selected_owners` after the transport drain. | The heartbeat (`ROLE_HEARTBEAT_INTERVAL` 5s) keeps rows fresh. | None | Membership rows age out | See **SR-3**: there is no proof of withdraw-before-drain. |
| mTLS peer transport / remote live fragments | `serve_peer_grpc_with_listener` wraps every accepted connection in `StoppingIo` (`wyrd-tonic/src/server/mod.rs:268-330`). All IO fails as soon as the token is cancelled. | `OraclePeerGrpc` fragment streams already end on the token (`grpc/mod.rs:416-424`). | None | Nothing; the fragment is ephemeral | See **SR-2**: on the All target the same listener carries Oracle analytical stage and lifecycle services. |
| Scribe WAL fault (role-local) | `mark_faulted` (`scribe/wal.rs:1948-1961`) makes appends return `IngressClosed` and defers segment deletion (`~2615`). A rollback failure also faults (`1930-1945`). | `run_scribe_wal_fault_monitor` (`state.rs:833-861`) sets `advertise_ready=false`, begins draining, and runs a one-shot deactivate. The heartbeat rewrites `ready=false` every 5s, so a failed deactivate converges. | None | WAL segments retained on disk (deletion deferred) | Restart then replays the WAL under the root lock (`boot/mod.rs:847-850`). This is recovered, but only on the combined target in practice (SR-1). |
| Governor poison | `BifrostResourceHealth` poison (Accounting, Volume, RuntimeOwner, `resources.rs:600-700`). `wait_for_poison` is a fallible supervised task, so the process exits non-zero. | - | None | Durable state only | The pod restarts. The journey poisons at `wyrd-testing/tests/bifrost/server/owner_inspection.rs:241`. Fail-stop is correct. |
| Verification worker drain | `VerificationRuntime::run` stops on the token. The drain grace is clipped inside `shutdown.drain_ms` (`server.rs:446-453`). | Leases are released and settle before teardown. | None. A single queue is used and no leader is elected. | Postgres queue | Another replica or a restart claims the work. Acceptable. |
| Eval post-ACK enqueue during a Postgres outage | `ObservationEnqueue` uses `try_acquire` over `PENDING_LIMIT` 256. When full, it drops, logs and counts. Each task holds the frame `Bytes` and a tenant connection bounded by the pool `acquire_timeout`. | The ingest ACK is preserved (fail-open is permitted by the task). | Bounded at 256 | Nothing for dropped enqueues | When the outage clears, new observations enqueue. The tracker is not awaited at shutdown (best-effort). Acceptable; see the residual note. |
| Forge spill: full disk and crash | A full disk fails the spill write, and the query fails. Spill shares the volume with the WAL and is uncapped by design (task ~297). | - | None | Spill files | `boot/data_root.rs` `prepare` calls `clear_directory(forge_spill)` under the root lock. Acceptable. |
| Storage permit waits under a backend outage | `acquire_request` waits under owner cancel and the deadline (`storage/mod.rs ~915`). The per-attempt `request_timeout` starts after the permit is taken. Backoff is `50ms*2^n` within the bound. | Readers fail at their deadline | None. Concurrency is capped at `max_concurrent_requests`, so retries never exceed the permit count. | - | Resumes when the backend returns. Acceptable. |
| D6 leader retry of peer capacity | `wait_for_peer_capacity` (`oracle/dispatcher.rs:68`) waits `retry_after_ms` (1000) bounded by the deadline and cancel. `reserve` releases round reservations before waiting (`analytical.rs ~3046-3200`). Ambiguous failures are not retried. | - | None | - | Any livelock is bounded by the query deadline. Acceptable. |
| D15 cgroup detection | Walks the `/proc/self/cgroup` `0::` ancestors and takes the minimum `memory.max` / `cpu.max`, with a v1 fallback (`resources.rs ~4397-4520`). | - | - | - | Correct under k8s cgroupns and systemd. Acceptable. |
| D16 separated loops | `scribe_lifecycle_scanner` runs `check_age` every 1s. `scribe_publisher` runs `publish_due`, raced against shutdown (`server.rs:550-595`). | A claim stays staged on cancel and converges on retry (`scribe/persistence.rs ~950-976`). | None | Staged claim | Acceptable. |
| D19 tenant begin failure / pool poisoning | `TenantConn::acquire` issues `set_config(...,true); BEGIN` in one round trip (`wyrd-sql/src/tenant_conn.rs`). A failed bind means BEGIN never runs. | - | None | - | Correct. A cancellation mid-begin was an existing leak class in the base, and the next `TenantConn` overrides the binding. Not a finding. |
| D13 audit fire-and-forget | `OracleQueryAudit` (`oracle/query_audit.rs`) is a bounded mpsc queue (16,384) feeding one tracked writer, which batches through `append_audit_batch` to `vala.audit_staging`. Queue-full and commit failures are logged and counted. Shutdown runs at `state.rs:724`. | - | None | Staging rows | Conforms to AGENTS.md section 2: the tracked, non-blocking Oracle read path. Acceptable. |

## 3. Affected Capabilities

- **Public ingest availability on a Scribe-only target after a WAL fault.** The load balancer keeps routing writes to a pod that rejects every write (SR-1).
- **Distributed analytical queries across `wyrd-oracle` and `wyrd-core` during a `wyrd-core` rollout or SIGTERM.**
  - In-flight remote stages on the combined pod are severed at once instead of draining (SR-2).
  - Peers observe the withdrawal only through heartbeat and snapshot staleness (SR-3).

## 4. Recovery / Proof Assessment

The following paths fail stop correctly, persist the state needed to resume, and have bounded retries:

- WAL fault: the monitor runs, the heartbeat converges membership, and segments are retained for replay.
- Poison: fail-stop.
- Storage, leader-retry, publish/scan loops, audit and Eval enqueue paths.

The gaps are in readiness role classification and in shutdown ordering on peer nodes:

- The only proof of WAL-fault readiness is a unit test that sets `scribe_fault_is_role_local = true` by hand (`health/mod.rs:683`).
- No test derives the flag from a Scribe target with verification composed.
- No journey exercises SIGTERM on a peer-enabled combined node that has an in-flight, remotely led analytical stage.
- No journey asserts that membership withdrawal precedes the transport drain.

## 5. Findings

### SR-1: A Scribe-only target stays ready after a WAL fault whenever verification is composed

- **Classification:** INCORRECT
- **Violated obligation:** TASK-004 Scribe failure boundary (task lines 318-322): "A combined target keeps Oracle queries and the verification worker available ... A Scribe-only target is unready." It also violates acceptance criterion 1 (readiness per target).
- **Location:** `crates/wyrd/wyrd-server/src/components/health/mod.rs:195-197` (consumer), with `all_ok` at `:150`.
- **Producer:** `crates/wyrd/wyrd-server/src/verification/mod.rs:366-465`.
- **Evidence:**
  - `compute_snapshot` sets `scribe_fault_is_role_local = oracle().is_some() || forge().is_some() || state.verification.is_composed()`.
  - `VerificationRuntime::build` always composes the Scheduler and Fitter when an operator pool exists, and calls `health.require` for each (`verification/mod.rs:455-458`). That makes `is_composed()` true.
  - The runtime is built and spawned for every `serves_api()` target, including Scribe (`app/server.rs:651-659`, `config.rs:200`).
  - Verification is enabled by default (`config.rs ~1638`).
  - Production requires an operator pool for Card recovery (`boot/mod.rs:2151-2167`).
  - So on a production Scribe-only target the fault is classified as role-local, and `/readyz` returns 200 while every append returns `IngressClosed`.
- **Consequence:**
  - The Kubernetes readiness gate and load balancer keep the faulted Scribe in rotation.
  - Clients receive write rejections instead of being routed to a healthy Scribe.
  - The pod never restarts, because readiness never fails and there is no liveness probe.
- **Required correction:**
  - Derive role-locality from the target's selected serving roles: the Scribe fault is local only when the process also serves Oracle or Forge (that is, a combined target). It must not depend on incidental verification composition.
  - Add a unit test that builds the snapshot for a Scribe target with verification composed and asserts unready under `ScribeWalFaulted`.
  - Add a journey assertion that `/readyz` returns 503 on a Scribe-only target after a WAL fault.
- **Fail-closed boundary:** readiness reports unready for any process whose only data-plane role is the faulted Scribe.

### SR-2: The peer listener's `StoppingIo` severs accepted Oracle analytical work on the combined target at shutdown

- **Classification:** REGRESSION, introduced by sibling commit `83ccbc634` (TASK-007) and breaking a TASK-004 obligation.
- **Violated obligation:** TASK-004 replay map row (task line 262): "shutdown withdraws readiness and role advertisements before draining accepted work."
- **Location:** `crates/wyrd/wyrd-server/src/app/server.rs:699-705` selects the listener, and `crates/wyrd/wyrd-tonic/src/server/mod.rs:255-330` implements it.
- **Evidence:**
  - The peer listener uses `serve_peer_grpc_with_listener` whenever `bifrost.scribe().is_some()`, and that includes `WYRD_TARGET=all` (the deployed `wyrd-core`).
  - `StoppingIo` fails every read and write on accepted connections as soon as the shutdown token is cancelled.
  - `build_peer_grpc` (`grpc/mod.rs:401-461`) mounts the analytical stage worker services (`AnalyticalStageAuthLayer` / `GraphWorkerServices`) and `OracleLifecycleGrpc` on the same router as `OraclePeerGrpc` and `ScribeTailGrpc`.
  - The hard stop is therefore applied to accepted Oracle stage work, not only to Scribe fragment streams, which already end on the token through `OraclePeerGrpc`.
- **Consequence:**
  - On SIGTERM, a rollout, or a scale event of `wyrd-core`, analytical stages that a `wyrd-oracle` leader is running on it fail at the instant of cancel.
  - That failure happens within the 45s grace and the configured drain budget, so distributed queries fail during every core rollout instead of completing.
- **Required correction:**
  - Keep graceful drain for accepted peer work until the drain deadline.
  - Apply the immediate IO stop only at that deadline, or only to the Scribe tail and fragment streams that need it.
  - Add a journey test that sends SIGTERM to a peer-enabled combined node while a remotely led analytical stage runs on it, and assert that the stage completes within the drain budget.
- **Fail-closed boundary:** new peer work is refused once draining begins (admission closed). Already-accepted work is bounded by the drain deadline, not cut at cancel.

### SR-3: Signal shutdown cancels transports before readiness and membership are withdrawn, with no proof

- **Classification:** MISSING (ordering obligation unproven and unmet on the signal path). The verdict is PLAUSIBLE: confirmed statically, but timing-dependent.
- **Violated obligation:** TASK-004 replay map row (task line 262): "shutdown withdraws readiness and role advertisements before draining accepted work."
- **Location:** `crates/wyrd/wyrd-server/src/app/shutdown.rs:18` (`signal_watcher` calls `shutdown.cancel()`), `crates/wyrd/wyrd-server/src/app/server.rs:750` (`drain_with_shutdown_hooks`), and `crates/wyrd/wyrd-server/src/state.rs:1963-2045` (the durable `cluster.deactivate` inside `drain_selected_owners`).
- **Evidence:**
  - On SIGTERM the watcher cancels the shared token directly.
  - That stops HTTP, gRPC and the peer listener (see SR-2) before the `before_cancel` hook (`Bifrost::begin_shutdown`, `state.rs:1927`) runs.
  - The hook is local only. The durable membership withdrawal runs after transport and tracker drains, inside `Bifrost::shutdown`.
  - Until then, peers keep selecting this node from membership, up to the 5s heartbeat freshness plus the snapshot poll.
  - The signal path itself predates this task, but TASK-004 owns the composed ordering obligation, and no test proves it.
- **Consequence:** during each rollout, Oracle leaders route live-fragment and stage work to a node whose listener is already stopping. Live sources degrade (the spec allows this) and analytical stages fail (they are not allowed to).
- **Required correction:**
  - On the signal path, run `begin_shutdown` and the durable role deactivate (withdraw membership and advertisements) before cancelling transport tokens.
  - Add a journey that sends a signal to a peer node and asserts that its membership row is not-ready or absent before the listener refuses accepted work.
- **Fail-closed boundary:** membership withdrawal fails closed. If the deactivate fails, drain proceeds and the heartbeat's absence ages the row out. The withdrawal is attempted before transports stop.

## 6. Residual Notes (not findings)

- During a Postgres stall, the Eval enqueue tasks hold up to 256 frame `Bytes` outside governor accounting. The count is bounded and fail-open is permitted.
- The cgroup 90% tripwire (`resources.rs:1380`) reads `memory.current`, which includes page cache. This behavior predates the task.
- Oracle spill is not cleared on boot. This predates the task and is not a TASK-004 obligation.
- `readiness_loop` leaves its last snapshot in place after shutdown. This is harmless because the HTTP listener has stopped.

## Overall Result

FAIL
