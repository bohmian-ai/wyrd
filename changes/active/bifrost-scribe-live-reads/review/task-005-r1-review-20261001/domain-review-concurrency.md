# TASK-005 R1 domain review: concurrency and cancellation

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65` (immutable candidate). **Result: PASS.**

## Boundary and authority

Reviewed the original TASK-005, approved spec revision 24, `AGENTS.md` §§5–6 and 11–12, `architecture/agent-rules.md`, `architecture/bifrost-design.md` (Scribe lifecycle, Oracle cancellation and telemetry), the cumulative source diff, and relevant tests. The concurrency boundary is Scribe's shared staging assembler; adjacent boundaries are Oracle's lazy query stream, Gate's client-edge lifecycle, and the shared metadata-cache and governed-request owners. This review addresses domain correctness, not the benchmark acceptance decision or the user-deferred aggregate gate.

## Source and path coverage

| Path | Producer → consumer and failure/recovery check | Result |
|---|---|
| Scribe staged ownership | `ScribeStagingRuntime::{register_member,take_claim,take_residue,settle,restore}` mutate `StagingAssembler`; `StagingBacklog::publish` sets the four pod gauges. Every live transition publishes while `assembly` remains locked. `restore` rebuilds ownership before its final locked publication; startup holds admission closed through restore. Failed restore returns without publishing a healthy zero. `PersistenceRuntime` and CPU-lane callers reach this one staging owner. | PASS |
| Concurrent Scribe transitions | `staging_runtime::tests::concurrent_transitions_publish_the_final_backlog` stages, claims and settles on four threads; final owner backlog and all four gauges are zero. The test depends on real interleaving and is not a deterministic reproduction of the old race, but the lock placement itself closes the stale-snapshot ordering window. | PASS |
| Restart | `staging_runtime::pg_tests::restored_stage_republishes_backlog` starts with a fresh metrics recorder and checks restored values; `wyrd-testing/tests/bifrost/scribe/telemetry.rs::staged_backlog_survives_abrupt_restart` scrapes before kill and after restart, compares the assembler's backlog, and checks settlement to zero. | PASS |
| Oracle stream and Gate lifecycle | `Gate::query_sql` creates the client-edge lifecycle before dispatch and parents dispatch under its span. `OracleQueryStream::new` wraps frame polling under the Oracle span; forwarded streams attach a Gate lifecycle that observes the terminal frame. `QueryStreamLifecycle::finish` uses an atomic once flag and drop records cancellation only if unfinished. `release_and_finish_terminal` distinguishes degraded from success. `OracleQueryStream` retains cancellation and admission through streamed cleanup. | PASS |
| Spawned Oracle work | `oracle::telemetry::install_query_span_propagation` installs DataFusion's `JoinSetTracer`; its spawn-time span instruments async and blocking work. The only repository call to `set_join_set_tracer` is this installer. | PASS |
| Shared storage concurrency | `ParquetMetadataCache::{register,retain,begin_close,finish_close}` publishes in-flight and resident gauges under its state lock; `begin_close` takes in-flight tasks, cancels and joins them before `finish_close`. `BifrostStorage` derives test inspection from owner state and request settlement; it does not use a shadow telemetry ledger. | PASS |

## Findings and verification limits

No material concurrency or cancellation finding. Prior `FIND-TASK-005-1` is closed by the publication under `assembly` lock at every mutating call site. The four-thread test proves the final state for exercised schedules; source-level lock ordering supplies the invariant for all schedules. I did not rerun the journey or benchmark. Supplied results report 147 Bifrost journey passes; the standard benchmark has one failed selective-query target, which is a separate acceptance question and does not establish a concurrency defect.
