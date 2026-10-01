# TASK-005 invariant review

Subject: `05d7d741304af3b0b4e667e7e18f93dec16b897b..885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2` (excludes `1f1cbcf5f`). Original task: `/home/thorrester/Documents/GitHub/wyrd-pr-95/changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md`, revision 14. The candidate repository's task describes revision 24 and appends an implementation report; that report is evidence, not a replacement for the user-supplied task. The current approved spec's REQ-012, INV-009, and AC-014 remain compatible with the original task's telemetry obligations. This review read the full diff, relevant producer/consumer modules, rules, and supplied verification record. No source was changed.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Scribe insertion means new process-local rows; ACK means attempts, including replay | `scribe/shards.rs`, `scribe/telemetry.rs`, `gate/mod.rs` move insertion count to shard owner and remove receipt-row counters | Focused Scribe journey and replay assertions reported passing | PASS |
| Staging count, bytes, age, and claims reflect ready plus claimed durable members through restore, claim, and settlement | `StagingAssembler::backlog`, `ScribeStagingRuntime::{register_member,take_claim,take_residue,restore,settle}` | Fresh-recorder Postgres restore test and abrupt-restart journey reported passing | FAIL: concurrent snapshots can publish in reverse order (INV-1) |
| `lane_queued` is waiting, `lane_active` is running | `scribe/execution_lanes.rs` moves gauge at worker start | Focused owner tests and journey reported passing | PASS |
| Oracle queue, admitted work, Degraded, and streamed terminal reflect owner state | `oracle/mod.rs`, `oracle/query_stream.rs`, `gate/mod.rs`; the telemetry guard is admitted after admission and Degraded has a separate terminal | Focused capacity, published, and degraded journeys reported passing | PASS |
| Query/peer traces follow streamed lifetime; physical scan work has no telemetry-only pruning walk | `oracle/query_stream.rs` polls under retained spans; `oracle/telemetry.rs` propagates DataFusion task spans; `oracle/pruning.rs` deletes shadow walk | Local/remote query journeys and captured traces reported passing | PASS |
| Storage cache/request facts come from live owner without telemetry ledger; logical cache hits do not become backend reads | `storage/cache.rs`, `storage/mod.rs`, `storage/telemetry.rs` remove shadow totals and retain owner inspection | Focused cache/request tests reported passing | PASS |
| Forge metrics derive from committed settlement; unknown outcomes still use durable read | `forge/worker.rs` carries settled `ForgeTaskResult` and retains read for ambiguous paths | Focused Forge journey and unit tests reported passing | PASS |
| No ACK, WAL, authorization, query terminal, capacity, cancellation, tenant, or Forge settlement behavior changes | Diff and touched call paths preserve durable authority, and focused write/read, queue, query, and Forge tests reportedly pass | Full owner journey lanes and aggregate gate not run | FAIL: required broad regression proof remains absent (INV-2) |
| Standard benchmark remains valid, retains required latency/throughput/resource evidence, and any regression is investigated | `bench_support.rs` and `wyrd-testing/src/load/matrix.rs` rebind Oracle duration | Standard `bench:bifrost:query-capacity` not run; no raw samples or comparison | FAIL (INV-2) |
| Each dashboard question has owner/labels/unit, before/after production samples, independent fact, focused proof, and readable trace | Candidate task appendix has a family/meaning/test table and selected traces | No before/after samples per transition or independent expected facts/results in the table; prechange inventory absent | FAIL (INV-2) |
| No high-cardinality labels, new exporter, compatibility alias, or public SDK contract | Diff adds none found | Static review | PASS |

## Proposed findings

### INV-1 — INCORRECT: staging gauges may regress behind their owner

**Obligation:** The four staging gauges must show the `StagingAssembler`'s current ready and claimed ownership after each transition, including claim settlement and restoration (original task's fixed Scribe chart contract; REQ-012 and INV-009).

**Location:** `crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs:165-180`, `:239-247`, `:309-320`, `:783-791`; `scribe/assembly.rs:1017-1037`.

**Evidence and reachable consequence:** Each transition releases the assembler mutex, then `publish_backlog` reacquires it, calculates a snapshot, releases it, and calls four separate gauge `.set`s. Concurrent `register_member` calls are reachable from the persistence workers; claim taking and settlement also run against this shared runtime. One task can snapshot one member, pause before `.set`, while another task registers another member and publishes two; the first can then overwrite the gauge with one. Likewise a pre-settlement snapshot can overwrite a zero after successful publication. The gauges then remain stale until another transition, so a backlog chart can hide durable unpublished data or show nonexistent backlog. The focused tests are sequential and do not exercise this ordering. The same snapshot can also be torn across its four gauge writes under a concurrent scrape.

**Required testable correction:** Serialize each owner transition's resulting gauge publication with the assembler ownership it reports, or use an existing owner mechanism that makes an older snapshot unable to overwrite a newer one. Do not add a second member ledger. A controlled concurrent register/settle test should leave the last scrape equal to `StagingAssembler::backlog()` and cover the final zero.

### INV-2 — MISSING / VIOLATION: required acceptance evidence is incomplete

**Obligation:** Original TASK-005 Acceptance Criteria 4–5 and Verification and Evidence require the standard capacity benchmark before `mise run gate`, full Scribe/Oracle/Forge journey lanes, production before/after samples by transition, raw benchmark samples/report, and comparison with the prechange run. AC-009/AC-010 and AC-014 require benchmark and trace proof; the test matrix is not a substitute for these measurements.

**Location:** Candidate task appendix `changes/active/bifrost-scribe-live-reads/tasks/TASK-005-simplify-bifrost-telemetry.md` (Implementation Evidence, dashboard table), contrasted with the user-supplied original task's Verification and Evidence section.

**Evidence and consequence:** The candidate appendix and user verification summary explicitly say the benchmark, `mise run gate`, and whole journey lanes were not run. Its dashboard table lists families, meanings, and test names but not before/after production samples, independent facts, or focused results per question. It provides selected traces but no prechange metric inventory or raw benchmark report. Thus the required cross-owner regression and performance acceptance cannot be decided; `bench_support.rs` and metric report changes have no standard benchmark proof. The appendix marks criterion 4 PASS despite saying the benchmark was not run.

**Required testable correction:** Run the named benchmark and gate in the task's order, run the three owner journey lanes, and append actual raw report paths, command exits, before/after production samples and independent expected facts for each dashboard row. Investigate any benchmark target miss or material regression; do not invent a threshold or replace it with a focused unit test.

## Scope and verification limits

No new telemetry-only durable authority was found in the Scribe, Oracle, storage, or Forge paths inspected. Candidate source was held immutable. Exact focused scenario commands and nearby unit suites were reported green, but this review did not rerun them. The original task's explicitly required benchmark, aggregate gate, and owner journey lanes remain unexecuted. Overall result: **FAIL**.
