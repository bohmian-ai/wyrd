# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a82d72f51901e1dc5`
- Base: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13`
- Candidate: `0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Cumulative range: `f6159606c5c959e8fcc3423574ab0e7e6c86ee13..0973a03e5a389ea0fb3635c6d9175e25db0a6da0`
- Approved authority: `changes/active/verified-change-contract/spec.md`, approved revision 57
- Original task: `changes/active/verified-change-contract/tasks/task-008-closeout.md`
- Prior review hypotheses and evidence: `review/TASK-008-r1/` through `review/TASK-008-r4/`
- Remediation reviewed: `review/TASK-008-r4/TASK-008-CLOSEOUT-R3-process-boundary-and-proof.md`

The candidate remained at the named commit throughout this review. The
checkout has no `.codegraph/` directory, so the cumulative diff and callers
were traced directly from repository source. Prior findings and remediation
claims were treated as hypotheses, not acceptance evidence.

The caller fixes two interpretation boundaries for this review. The complete
default `bench:capacity` execution (`FIND-TASK-008-CLOSEOUT-13`) is deferred to
post-merge integration and is therefore a verification limit, not a candidate
blocker. The report is intentionally written before Postgres teardown; the
rejected part of prior FIND-2 that required `Report::total_seconds` to include
post-report teardown is not an obligation of this candidate.

## Deployed path and affected capabilities

The cumulative candidate does not change production server, queue, Scribe,
Oracle, Forge, verification-engine, SDK, persistence, or deployment recovery
semantics. The `wyrd-queue` change is a documentation reference to the renamed
benchmark; the other non-harness changes are correctness and journey tests.
Runtime effects are confined to the opt-in capacity command and the shared
release-process test harness it uses.

The deployed benchmark path is:

1. `mise.toml:507-556` fixes one wall-clock start/deadline before any setup.
   Its `phase` owner starts each RustFS, Postgres-wrapper, build, and benchmark
   phase under non-foreground GNU `timeout`, which creates the phase process
   group, forwards an external INT/TERM, escalates TERM to KILL, waits for the
   direct owner, and kills any descendant still in that group before returning.
2. `scripts/postgres/with-test-postgres.sh:19-40,114-123` owns one isolated
   Postgres Compose lifecycle, tears it down on normal exit or INT/TERM, and
   runs the nested build/benchmark shell. That shell imports the same absolute
   deadline and `phase` definition rather than starting a second lifetime.
3. `capacity/main.rs:134-265,724-750` converts the exported wall deadline to
   monotonic in-process bounds. `Lifetime` reserves client shutdown, two
   replica-stop windows, report work, wrapper teardown, and process exit; a
   measured future that reaches its share is cancelled at an async yield and
   produces a failed result.
4. `capacity/main.rs:392-424,440-578` and `release_server.rs:157-207,224-275`
   prepare fixtures, migrate, start replica 0, provision four tenants, run the
   one-replica steps, add replica 1 against shared Postgres/RustFS, reconnect
   clients, and run the two-replica steps. `OperatorRun` owns migration/setup
   subprocesses while pending, so cancellation kills and reaps the direct
   child while retaining stderr.
5. `capacity/main.rs:581-677` shuts clients down and stops replicas newest
   first. Each synchronous `LocalServer::stop` runs through
   `tokio::task::spawn_blocking`; the async runtime remains available while
   `release_server.rs:365-405` applies the 45-second TERM grace, forced kill,
   reap, and log copy.
6. `capacity/main.rs:399-424` writes the failed or passing report before the
   binary exits and the Postgres wrapper performs its teardown, which is the
   intentional report boundary supplied by the caller.

Affected user-visible capabilities are evidence collection for direct and
queued verification, buffered Scribe ingest, Oracle queries, audit and Forge
drain, resource saturation, and one-to-two-replica scale-out. These production
capabilities are measured, not modified.

## Failure and recovery paths

| Failure or interruption | What stops and what remains available | Recovery and proof assessment |
|---|---|---|
| A public operation is refused, times out, or returns a wrong judgment | The affected benchmark operation is counted as an error; other lanes and replicas continue until the step or evidence owner returns failure. | The driver does not retry ambiguous direct execution, so the harness does not amplify non-idempotent work. The step cannot pass with the failed operation hidden. |
| Postgres, RustFS, metrics, or an evidence query becomes unavailable after startup | The affected client call, scrape, or SQL probe fails. The harness does not deliberately crash the serving process or reinterpret incomplete evidence as success. | The cooperative path returns through `Benchmark::run`, performs owned client/replica cleanup, and writes a failed report. Production durability and replay remain with their existing owners. |
| Migration or tenant setup stalls inside the binary | The current operator subprocess and any already-started replica are owned; later tenant setup and measurement do not start. | `OperatorRun` is asynchronously polled and its drop kills/reaps the direct child while retaining stderr. The recorded stalled-setup proof exercises this path through failed-report generation. |
| A measured future reaches its in-process share | The future is dropped at its next yield; completed database/store effects remain, and no incomplete step record is promoted. | `Benchmark` retains the deployment for cleanup. The paused-time proof covers the local bound; workflow-specific owners retain the cancellation/retry semantics documented in their modules. |
| Ordinary replica shutdown is slow or a replica ignores TERM | Other async timers/tasks remain schedulable because stop/reap/log-copy work is on Tokio's blocking pool. Replicas are still stopped sequentially, newest first. | `LocalServer::stop` waits for a clean exit, then kills and reaps after `STOP_GRACE`, returning a failed stop record without losing the copied log. The ignored heartbeat proof directly exercises the non-blocking boundary. |
| A RustFS setup or nested benchmark descendant ignores TERM | That phase fails; no later phase starts after setup failure. A nested benchmark failure still allows the Postgres wrapper's normal EXIT teardown while its reserved interval remains. | Non-foreground `timeout --kill-after` owns the whole phase group, and the post-wait `pkill -KILL -g` sweep removes surviving group members. The task-text tests exercise both a pre-wrapper setup descendant and a nested Cargo/run descendant. |
| The entire command receives INT/TERM | The active phase's timeout owner receives TERM and relays termination to its monitored group; the shell waits for the phase result and performs the survivor sweep. | A native probe during this review confirmed a TERM-resistant monitored command is escalated to KILL and the phase returns nonzero. No production server signal policy is changed. |
| Report writing fails or Postgres teardown is slow | Report failure makes the binary fail; teardown remains owned by the wrapper and is bounded by the outer phase reserve/escalation. | Per caller authority, the already-written report does not claim to include post-report teardown in `total_seconds`. Command completion remains bounded even though that tail is intentionally outside the report sample. |
| The outer phase deadline expires while the nested build/run phase is active | Inner build/run phases reserve 30 seconds and escalate by 10 seconds before the absolute deadline; the outer wrapper phase reserves 10 seconds and escalates by 5 seconds. No subsequent phase starts after a nonzero phase result. | The same exported absolute timestamp drives every layer. Nested groups do not depend on an independent relative timeout, so their earlier reserve closes before the wrapper's own forced boundary. |

## Material proposed findings

None.

The round-four process-tree failure is closed: `--foreground` is gone, every
mandatory command phase now owns a process group with finite TERM-to-KILL
escalation, and the focused task-entry proofs cover both pre-wrapper and nested
run descendants. The round-four async-blocking failure is also closed by the
explicit blocking boundary around `LocalServer::stop`. No reachable regression
was found in crash, dependency-outage, timeout, cancellation, TERM-resistance,
or cleanup behavior within the approved private benchmark boundary.

## Recovery and proof assessment

The remediation records these focused results:

- `a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline`: passed;
- `a_hung_benchmark_run_is_killed_with_its_descendants_by_the_deadline`: passed;
- `a_slow_replica_stop_leaves_the_runtime_free`: passed;
- `a_stalled_tenant_setup_stops_the_run_by_its_deadline`: passed;
- complete `capacity` target: 14 passed, four environment-gated tests skipped;
- `release_server` unit selection: two passed;
- formatting, lints, and diff check: passed.

This review reran the exact setup process-group proof:
`mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
--run-ignored=only -E
'test(=tests::a_hung_setup_step_is_killed_with_its_descendants_by_the_deadline)'`;
it passed in 7.046 seconds. A separate native signal probe confirmed the
active phase's external-TERM escalation behavior.

The focused stand-ins prove command ownership and failure cleanup; they do not
prove empirical capacity, SLO attainment, or production dependency recovery.
The unmodified default run needed for those empirical claims remains expressly
deferred as `FIND-TASK-008-CLOSEOUT-13` until the integration sequence. That
gap is recorded without converting it into a blocker or a capacity claim.

## Overall result

**PASS**

Within the caller-approved boundaries, the candidate closes the prior
process-group and async-cleanup defects without changing production recovery
semantics. Its command lifecycle now fails boundedly and cleans up across the
credible interruption paths assigned to this benchmark.
