# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `5c3bb79b3598abd88a3a234611fc400096adc975`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..5c3bb79b3598abd88a3a234611fc400096adc975`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57, especially REQ-171, AC-040, AC-041, and the revision-57 history entry
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review hypotheses: `review/TASK-008-r1/` and `review/TASK-008-r2/`

The candidate commit remained unchanged during this review. CodeGraph is not
available in this checkout. The untracked round-2 review directory is not part
of the immutable candidate and was read only as prior-review evidence.

## Deployed-path evidence

The candidate does not change production server availability, storage,
recovery, queue, verification-engine, or SDK behavior. Its runtime-shaped
change is the opt-in release-process capacity harness and supporting journeys;
the `wyrd-queue` production edit changes only a rustdoc benchmark reference.

The benchmark deployment path is:

1. `mise.toml:507-524` starts RustFS, provisions its bucket, enters the
   repository-managed Postgres wrapper, builds release `wyrd-server`, and then
   starts the `capacity` binary.
2. `capacity/main.rs:240-295` creates the output and scratch roots, starts the
   trusted local TLS judge, materializes peer certificates and the shared
   signing key, and constructs the `Benchmark` owner.
3. `capacity/main.rs:359-429` provisions the first replica through
   `LocalServer::start`, provisions the four tenants and their workloads, and
   connects the cross-tenant evidence reader.
4. `release_server.rs:147-189` runs the database migration, starts replica 0
   in its 8-CPU/16-GiB systemd scope, waits for readiness, and runs four
   operator `setup` commands. `release_server.rs:206-259` joins replica 1
   against the same Postgres and object store with its own listeners, local
   state, process, and cgroup.
5. `capacity/main.rs:359-394` executes warmup, the one-replica ramp and
   sustained step, adds replica 1, reconnects every tenant to both replicas,
   and executes the two-replica sustained and scale-out steps.
6. `capacity/load.rs:273-331` schedules open-loop arrivals, rotates each lane
   across replicas, counts driver saturation as missed traffic, waits for
   issued request tasks, and flushes client ingest after arrivals stop.
7. `capacity/step.rs:303-420` measures one common CPU/memory window, finishes
   optional per-replica profiles, and waits for the run, Scribe, audit, and
   Forge backlogs. The Scribe decision includes durable staged members through
   `capacity/evidence.rs:90-104`, matching the Bifrost recovery authority.
8. `capacity/main.rs:509-531` bounds client shutdown, sends `SIGTERM` to the
   newest replica first, waits up to the release-server grace per replica,
   kills a replica that does not drain, and copies logs before writing the
   report.

Affected capabilities are benchmark evidence for direct and queued
verification, buffered Scribe ingest, Oracle queries, audit publication, Forge
demand, resource use, and scale-out. Those production capabilities are under
measurement; their deployed ownership and recovery semantics are not changed.

## Failure and recovery paths

| Failure or interruption | What stops and what remains available | Recovery and proof assessment |
|---|---|---|
| A public request is refused, times out, or returns a wrong judgment | The operation is recorded as an error while other lanes and the other replica continue. | The harness does not retry an arrival, so an uncertain direct execution is not duplicated. The step or report fails from client-visible evidence. |
| Driver permits are exhausted | That arrival becomes missed traffic; requests already holding permits continue. | Achieved/offered traffic exposes generator saturation instead of silently lowering offered load. |
| A request tail or async dependency call outlives the measurement share | `Lifetime::measure` drops the measured future; dropping each lane's `JoinSet` aborts outstanding client tasks, and accepted server effects may remain durable. | `Benchmark::clean_up` still owns client shutdown and replica termination. No partial step `Record` is presented as complete. The paused-time unit test proves this cooperative async path only. |
| Scribe has persisted work into durable staging but not published it | Persistence and immutable gauges may be zero, but staged live members keep the Scribe backlog nonzero. | The prior false-drain defect is closed: restored or claimed staged members block a PASS, and the focused evidence test covers the handoff state. |
| A backlog first appears empty after the 60-second limit | The drain becomes expired, including on its first late empty observation. | The report records no drain time and fails the saturation cell. Focused boundary tests cover below, exactly at, and above the limit. |
| A serving replica exits during load | Requests to that replica fail and a later scrape/read fails; the other replica and durable Postgres/object-store state remain. | The benchmark aborts measurement and enters owned cleanup. It does not restart a replica; restart/WAL-replay qualification remains in the dedicated correctness and recovery suites, not this capacity SLO. |
| Postgres or RustFS becomes unavailable after setup | Public calls, evidence SQL, or scrapes return errors and measurement ends; already durable state remains in its authoritative system. | Ordinary async failures reach cleanup and produce a failed report. The candidate contains no dependency-outage injection, which revision 57 does not require for the capacity benchmark. |
| Migration or tenant setup stalls | `LocalServer::start` blocks inside synchronous `Command::output()` while the migration/setup child remains live; after replica 0 starts, that replica also remains live. | The Tokio deadline cannot poll or cancel a future while its poll is blocked in `Command::output()`. No owner can kill/reap the operator child at expiry, and report/replica cleanup do not run until the blocking call returns. See SYS-R3-001. |
| A replica ignores `SIGTERM` | Other replicas are stopped only after the current synchronous stop returns. | `LocalServer::terminate` polls for 45 seconds, then kills and reaps the child. However the external signal command, final wait, log copy, and report finalization are not bounded by the absolute lifetime; this is part of SYS-R3-001. |
| The benchmark future is cancelled by its caller | `Benchmark`, `Deployment`, `LocalServer`, `Judge`, `JoinSet`, and `Capture` drops stop their owned processes/tasks; durable effects already accepted remain. | This is safe for cooperative cancellation. It does not repair a thread blocked in an operator `Command::output()`, and `main` does not install an outer command-lifetime supervisor. |

## Material proposed finding

### SYS-R3-001 — INCORRECT — the complete benchmark command can outlive its 30-minute boundary

- **Prior stable finding:** continuation of
  `FIND-TASK-008-CLOSEOUT-2`; the round-2 system report identified the same
  source path under an authority mismatch. This review independently
  revalidated it against the caller-corrected revision 57 subject.
- **Violated obligation:** REQ-171 requires the default `mise run
  bench:capacity` run, including setup, to complete within 30 minutes. The r1
  remediation's R2 closure further requires one complete-command boundary
  from setup through bounded cleanup.
- **Exact locations:** `mise.toml:507-524`;
  `crates/wyrd/wyrd-testing/src/bin/capacity/main.rs:240-259,311-335,509-531,623-628`;
  `crates/wyrd/wyrd-testing/src/release_server.rs:147-189,347-388,624-635`.
- **Evidence:** The `Lifetime` begins only after the mise task has started
  RustFS, entered the Postgres wrapper, and built both release binaries.
  `Benchmark::prepare` is awaited before `Benchmark::run` installs
  `timeout_at`. More importantly, migration and every tenant setup call the
  synchronous `run`, which uses `std::process::Command::output()`. That blocks
  the task while waiting for an unowned child and cannot be interrupted by
  `Lifetime::measure`'s Tokio timer. The only deadline test supplies
  `std::future::pending`, so it proves a yielding future is dropped, not that
  the real subprocess path is bounded. Replica stop also performs synchronous
  signal/wait/log-copy work outside an absolute deadline, and
  `total_seconds` is captured before report serialization, file writes, and
  command exit.
- **Observable system consequence:** A wedged migration/setup process during
  a Postgres outage, process interruption, or child deadlock can leave the
  benchmark command and an already-started replica alive beyond 30 minutes
  without a report. Slow pre-binary setup/build or cleanup/report IO can also
  exceed the advertised command limit while the report's recorded total omits
  that elapsed time. The harness can therefore neither enforce nor accurately
  prove the required bound on the failure path.
- **Testable correction:** Put the actual `mise run bench:capacity` lifecycle
  under one enforceable absolute deadline, including required environment
  setup, binary preparation, migration, tenant setup, measurement, client
  shutdown, replica termination, log capture, report finalization, and command
  exit. The existing benchmark/release-process owners must retain handles for
  operator children so deadline expiry kills and reaps them, preserve bounded
  cleanup and diagnostic logs within the remaining budget, and record the
  complete command duration. Add focused proof using a deliberately
  non-terminating migration or setup child and an already-started replica: the
  command must exit failed within a reduced test budget, reap both children,
  and retain failure diagnostics. Preserve production server timeouts and
  behavior; this correction belongs only to the benchmark and release harness.

## Recovery and proof assessment

The candidate closes the prior Scribe staged-backlog, exact drain-edge,
resource-window, and ordinary cooperative-cancellation paths. Recorded
evidence includes 14 passing capacity unit tests, focused real-Postgres
two-replica claim/fairness tests, Rust SDK ingestion and judgment journeys,
clean format/lint checks, and a shortened two-replica smoke in which both
replicas stopped cleanly. Those results are credible for their named paths but
do not exercise a stalled migration/setup child or prove that cleanup and
reporting obey one absolute command deadline.

Per the caller's sequencing instruction, the missing unmodified full default
run (`FIND-TASK-008-CLOSEOUT-13`) is **deferred to integration after the other
workstreams merge** and is not a blocker or an additional finding for this
candidate. That deferral does not supply proof for, or remove, the independent
source-level lifetime defect above.

No crash-restart, dependency-outage, or WAL-replay scenario is newly required
inside this capacity benchmark: revision 57 assigns correctness and isolation
to dedicated tests, and the candidate does not change the production recovery
owners. The material regression is narrower: the benchmark's own claimed
failure boundary cannot stop and recover its subprocess topology.

## Overall result

**FAIL**

`SYS-R3-001` is a reachable, bounded benchmark/release-harness defect under
approved revision 57. It can be corrected without a product, public API,
storage, concurrency-semantics, or deployment-architecture decision.
