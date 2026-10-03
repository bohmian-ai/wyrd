# System-resilience review

## Immutable subject

- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `852894689388124960993014a46934e73c0ed2a8`
- Range: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd..852894689388124960993014a46934e73c0ed2a8`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 57, specifically REQ-171, AC-040, AC-041, and the revision 57 history entry
- Original task record: `changes/active/verified-change-contract/tasks/task-008-closeout.md`, section “bench:capacity implementation (revision 57)”

## Deployed-path evidence

The candidate has no production runtime or deployment-semantics change. The only non-test crate change is the `QueueConfig::default` rustdoc reference from the retired ingest benchmark to `bench:capacity` (`crates/shared/wyrd-queue/src/config.rs:100-105`). The runtime-facing changes are confined to the opt-in `wyrd-testing` release harness and tests.

The benchmark deployment path is:

1. `mise.toml:510-526` starts RustFS, provisions its bucket, builds release `wyrd-server --features cloud`, and runs the `capacity` binary under the repository-managed Postgres wrapper.
2. `capacity/main.rs:121-176` creates one local TLS judge, per-replica peer certificates, one shared signing key, shared Postgres/RustFS configuration, and four tenants.
3. `release_server.rs:140-239` migrates once, starts replica 0, provisions the tenants, then starts a joined replica against the same Postgres and object store but with its own temporary working directory, Scribe local state, listeners, and 8-CPU/16-GiB cgroup.
4. `capacity/main.rs:200-245` runs warmup and the one-replica ramp/sustained path, starts replica 1 only after the one-replica sustained step, reconnects every tenant to both replicas, then runs two-replica sustained and scale-out steps.
5. `capacity/load.rs:249-307` schedules open-loop arrivals, uses one bounded driver semaphore, rotates each lane across replicas, waits for issued operations, and flushes the client ingest queues after arrivals stop.
6. `capacity/step.rs:128-215` samples both replicas, waits for server-owned work, computes per-operation outcomes, and captures per-replica cgroup CPU/memory. `main.rs:247-279` then shuts down client states and terminates replicas in reverse order.

This affects only benchmark evidence for direct and queued verification, Scribe ingest, Oracle query, audit publication, Forge demand, and scale-out. It does not alter the availability, durability, retry, cancellation, or recovery behavior of those production capabilities.

## Failure and recovery paths

| Failure or interruption | Observed boundary and consequence | Recovery / proof assessment |
|---|---|---|
| One request is refused, times out, or returns a wrong judgment | `load.rs:278-300` records the result; the step's zero-error or traffic SLO fails. Public-client request deadlines remain the production owners. | Correctly fails the affected benchmark step rather than crashing a shared server. There is no benchmark retry that could duplicate non-idempotent verification work. |
| Driver saturation | `load.rs:268-274` counts an unsent arrival as missed; `report.rs` folds it into achieved traffic. | Correctly exposes load-generator saturation as failed traffic rather than silently reducing offered load. |
| Postgres, RustFS, metrics, or evidence query becomes unavailable | A client result is counted when it reaches the public client; a scrape/owner-query failure propagates out of `Deployment::run` (`step.rs:128-215`, `233-261`). | Evidence loss aborts the benchmark, which is safer than producing a passing report from incomplete evidence. Abnormal `LocalServer` drop kills and reaps the process and preserves its working directory/log (`release_server.rs:469-483`). No dependency-failure injection was supplied. |
| Replica 1 cannot start or become ready | `release_server.rs:201-239,393-424` waits up to 120 seconds and returns the last readiness body; the already-running deployment is dropped on propagation. | Failure stays within the benchmark deployment. The candidate's joined gRPC/peer ports below the Linux ephemeral range (`release_server.rs:43-51,595-611`) avoid the observed mid-run bind race without changing production configuration. |
| Process shutdown | Client queues are shut down before replicas, then replicas receive `SIGTERM` in reverse order and get 45 seconds to drain (`main.rs:247-256`, `release_server.rs:330-381`). | Clean shutdown is reported. A timeout kills and reaps the process. This is lifecycle evidence, not restart/WAL-replay evidence, and the approved revision does not require a recovery fault injection in the capacity benchmark. |
| Accepted Scribe work crosses from immutable memory into durable staging but has not published | The owning Scribe telemetry exposes durable unpublished members and outstanding claims (`scribe/assembly.rs:710-741`; `architecture/bifrost-design.md:910-921`). | The benchmark currently loses this state from its drain decision; see SYS-001. |
| A saturated or impaired request outlives its arrival window | Every issued task is awaited after the window (`load.rs:286-301`); direct execution may legitimately wait 70 seconds and other public-client calls have their own deadlines. | There is no run-level deadline enforcing REQ-171's 30-minute completion boundary; see SYS-002. |

## Material findings

### SYS-001 — INCORRECT — Scribe can be reported drained while durable unpublished work remains

- **Violated obligation:** REQ-171 requires every Scribe backlog to drain within 60 seconds before the saturation cell passes (`spec.md:1736-1746`).
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:89-99`, consumed as the complete Scribe backlog by `capacity/step.rs:244-258`.
- **Evidence:** `scribe_backlog` sums only `bifrost_scribe_persistence_queue_depth` and `bifrost_scribe_immutable_generation_count`. The Scribe owner separately exposes `bifrost_scribe_staging_live_members` for durable members not yet published and `bifrost_scribe_staging_outstanding_claims` for unsettled claims (`crates/vala/vala-bifrost-redux/src/scribe/assembly.rs:710-741`). The Bifrost authority explicitly says these restored members “appear in backlog” (`architecture/bifrost-design.md:915-919`). Once an immutable generation becomes a staged member, both gauges used by the benchmark may be zero while the staged work remains unpublished.
- **Observable system consequence:** A step can mark `backlogs drained` PASS and proceed or terminate while Scribe still owns publication work. A dependency slowdown or interrupted publisher is therefore hidden precisely at the durable staging/recovery boundary the SLO is intended to measure.
- **Testable correction:** Derive the benchmark's Scribe drain state from all existing Scribe backlog owners needed to prove no waiting, active, staged, or claimed publication work remains, including the existing staging live-member/claim telemetry, and add a focused evidence/report test where persistence and immutable gauges are zero but a staged member or outstanding claim keeps the step failed until it clears. Reuse the production metric catalog; do not add a second work ledger or production behavior.

### SYS-002 — MISSING — the default benchmark has no 30-minute completion boundary

- **Violated obligation:** REQ-171 requires the default run, including setup, to complete within 30 minutes (`spec.md:1706-1713`).
- **Location:** `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:121-279` and `capacity/load.rs:249-307`.
- **Evidence:** The benchmark records `started` only for reporting and executes setup, every step, client shutdown, and sequential replica shutdown without an overall deadline. Each lane then waits for every issued request after its arrival window (`load.rs:286-301`). The nominal maximum windows plus allowed 60-second backlog drains consume about 21.5 minutes before setup and shutdown; delayed public calls can add up to their independent request deadlines at multiple steps, and no owner stops the run at 30 minutes.
- **Observable system consequence:** Under a slow dependency, saturated replica, or cancellation failure, the command can outlive the required bound rather than returning a bounded failed verdict. That also delays cleanup of both release replicas and makes the advertised operational duration unreliable.
- **Testable correction:** Enforce one benchmark-lifetime deadline covering setup through client and replica cleanup, preserve bounded cleanup and diagnostic logs when it expires, and prove with a reduced-duration test that a deliberately non-completing step exits within the configured total budget with failure rather than waiting for every request deadline. This boundary belongs to the benchmark orchestrator; production request deadlines and recovery semantics must remain unchanged.

## Verification and recovery assessment

Recorded candidate evidence includes clean format/lints, six `capacity` binary tests, two real-Postgres two-runtime claim/fairness tests, Rust SDK judgment and sustained-ingest journeys, and one reduced-duration smoke run. The smoke exercised every topology transition and reported clean shutdown of both replicas, but it intentionally exited 1 and is not the default REQ-171 run (`task-008-closeout.md:1535-1554`). No passing full-duration capacity report, dependency-outage run, process-restart/WAL-replay run, or cancellation fault injection is available. The latter recovery scenarios are outside the revision 57 benchmark's assigned correctness-test split, but the missing full run means actual scale-out capacity remains unproven by the supplied evidence.

The two source defects above are independently reachable without changing production behavior: SYS-001 can produce a false PASS at a durable recovery boundary, and SYS-002 can violate the required benchmark-lifetime bound during dependency or request delay. Both corrections are bounded to `wyrd-testing` and require no product, protocol, concurrency, persistence, or deployment decision.

## Overall result

**FAIL**
