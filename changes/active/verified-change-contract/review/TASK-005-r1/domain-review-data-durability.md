# Data, concurrency, and durability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `9cfe9b69c5990603e07458aa4402f6750131bfbc`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`
- Candidate identity was rechecked immediately before this report was written and still matched the requested commit.

## Reviewed boundary

This review traced the production Drift data boundary from registration through baseline fitting and runtime result settlement:

1. PSI/SPC registration validation, exact `Data` reference resolution, Card persistence, and insertion of the baseline work row in the caller-owned transaction.
2. `drift_baselines` schema, forced RLS, PostgreSQL-clock due/lease/retry transitions, `SKIP LOCKED` claims, token-fenced settlement, restart reclaim, status projection, and integration tests.
3. Exact Verifier/Data UID loading, registered `data/data.parquet` artifact lookup, bounded artifact read, blocking Arrow/Parquet fit, and fitted-profile persistence.
4. Typed Oracle logical plans for exact subject, series, and half-open managed `wyrd_event_time` windows; authenticated provider replacement; aggregate decoding and scoring.
5. Detail-before-summary publication, per-batch Scribe acknowledgement, partial visibility, and Postgres settlement/dispatch ordering.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Registration atomicity and baseline status | `REQ-073`, `REQ-074`, `REQ-078`; task scenarios 1-2; `AGENTS.md` SQL transaction rules | `components/cards/resolve.rs:249-320`; `components/cards/service.rs:1080-1209`; `drift_baselines.rs:34-42,172-197,325-370`; migration lines 12-79 | PASS |
| PostgreSQL coordination clock | `REQ-152`, `AC-033`; `AGENTS.md` section 15 | Migration lines 12-79; `drift_baselines.rs:34-137,199-323`; `pg_drift_baselines.rs` clock-controlled lifecycle tests | PASS |
| Claim concurrency, lease fencing, retry, restart | `REQ-073`, `REQ-115`, `REQ-146`; task scenario 2 and refactor instruction | `drift_baselines.rs:44-109,199-323`; `fitter.rs:63-214`; `verification/mod.rs:344-409`; `verification/permits.rs:1-88`; SQL integration tests | FAIL (`DATA-001`) |
| Exact immutable baseline identity and artifact | `REQ-072`, `REQ-073`, `REQ-110`; Drift architecture baseline rules | `resolve.rs:249-320`; `service.rs:1194-1204`; `fitter.rs:216-323`; artifact metadata owner; Rust/Python journeys | PASS |
| Oracle typed aggregate plans and immutable windows | `REQ-080`, `INV-015`; task scenarios 3-5; `architecture/logic/drift.md`; Bifrost/DataFusion authorities | `verification/drift.rs:93-267,395-669`; `oracle/planner.rs:283-357`; plan unit test and real journey evidence | FAIL (`DATA-002`) |
| Result durability ordering and settlement | `REQ-085`; Drift architecture result/retry semantics; task scenario 6 | `verification/results.rs:90-250`; `publisher.rs:117-163`; `runner.rs:480-579`; `pg_verification_runtime.rs:1521-1626` | PASS |
| Tenant isolation | `INV-010`; `AGENTS.md` and `agent-rules.md` TenantConn/RLS rules | Forced RLS migration; tenant `TenantConn` operations; explicit `OperatorPool` due discovery; cross-tenant SQL and journey cases | PASS |

Applicable references read were `architecture/bifrost-design.md`, `architecture/references/domain/{drift-monitoring,olap-serving,datafusion,analytical-operations-reliability}.md`, `architecture/references/languages/{spec-driven-development,testing-workflows}.md`, `changes/active/verified-change-contract/architecture/logic/{drift,table_schema}.md`, `AGENTS.md`, and `architecture/agent-rules.md`.

## Material findings

### DATA-001 — VIOLATION: the baseline fitter bypasses the shared Verifier/baseline execution ceiling

- **Violated obligation:** `REQ-146` requires one shared Verifier/baseline ceiling of 16 globally and 4 per tenant, with both permits acquired before durable work is claimed. The task also directs the fitter to reuse the generic runtime's permits.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/mod.rs:361-398`; `crates/wyrd/wyrd-server/src/verification/fitter.rs:63-80,143-214`; compare `crates/wyrd/wyrd-server/src/verification/runner.rs:280-307` and `crates/wyrd/wyrd-server/src/verification/permits.rs:25-87`.
- **Evidence:** `VerificationRuntimeBuilder` constructs `VerifierPermits` only inside the `VerifierRunner` constructor. `BaselineFitter` has no permit owner or permit argument. Its pass lists tenants and calls `fit_next`; `fit_next` claims and commits a durable baseline row before any shared global or tenant capacity check. The fitter's one-at-a-time-per-process loop is a separate limit, not the required shared ceiling, and none of the fitter/permit searches or tests joins these owners.
- **Observable consequence:** a process already using all 16 Verifier slots, or all four slots for one tenant, can claim and execute an additional Parquet fit. The configured ceiling and tenant fairness are therefore false at the expensive Arrow/Parquet CPU and memory boundary; overload can also consume a durable fit attempt that should not have been claimed.
- **Required testable correction:** construct one shared permit owner in `VerificationRuntimeBuilder`, pass it to both runner and fitter, and have the fitter acquire the tenant and global permit before claiming a baseline row and retain it through fitting and fenced settlement/release. When capacity is unavailable, it must leave the row unclaimed. Add a runtime/SQL integration test with the ceiling occupied that proves no baseline claim is written, then releases capacity and proves the same row is claimed; also prove a saturated tenant does not prevent another tenant from progressing when global capacity remains.

### DATA-002 — VIOLATION: SPC aggregate output is collected without the required bounded rule window

- **Violated obligation:** `changes/active/verified-change-contract/architecture/logic/drift.md:162-163` requires SPC aggregate rows to stream through a bounded eight-point rule window instead of accumulating an unbounded vector. The OLAP/DataFusion authorities reject unbounded `collect()`, and the task requires bounded server aggregation using the existing SPC semantics.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/drift.rs:342-367,521-532,608-669`; `crates/vala/vala-drift/src/spc/mod.rs:181-190,207-253`.
- **Evidence:** `DriftEngine::aggregate` pushes every decoded Oracle `RecordBatch` into a `Vec`; `spc_chunks` then pushes every subgroup mean into `SpcTargetChunks.means`; `score_spc_chunks` allocates another `Vec<i8>` for every zone before evaluation. Retention is proportional to the full number of subgroups, not the fixed rule lookback. The current tests exercise plan shape and score parity but provide no bounded-retention proof.
- **Observable consequence:** an otherwise valid large retained window can exhaust verifier memory after DataFusion has correctly reduced raw observations to subgroup aggregates. The engine therefore does not provide the specified bounded production path and can retry/OOM on input whose aggregate stream should be safely consumable.
- **Required testable correction:** make the existing `vala-drift` SPC aggregate-input owner consume ordered subgroup aggregates incrementally, retaining only the fixed rule lookback, counters, and report state required by the current scorer. Feed decoded Oracle batches to that owner as frames arrive rather than returning a collected `Vec<RecordBatch>`. Preserve current ordering, trailing-chunk, zone, trend, alert-threshold, and inconclusive semantics. Add parity tests against the existing scorer plus a long multi-batch aggregate test that demonstrates retained subgroup state remains bounded by the rule window.

## Verified behavior without findings

- The baseline row is inserted through the same caller-owned `TenantConn` as the Card and committed only by the registration owner.
- Due times, lease deadlines, retry deadlines, and due/expiry comparisons use `statement_timestamp()`; Rust supplies durations rather than coordination instants.
- Claims use `FOR UPDATE SKIP LOCKED`; every completion, failure, and release is token-fenced, and expired leases are reclaimable through the same row.
- The fitted row pins exact Verifier and Data Card UIDs. The fitter loads both under tenant RLS and reads the registered `data/data.parquet` path for that exact Data UID.
- Oracle plans use an authenticated source cut and exact subject/series/`[start,end)` predicates on managed `wyrd_event_time`; PSI, SPC, and Custom return aggregates rather than raw observations.
- Result payload construction orders a non-empty detail batch before the summary, the publisher awaits each Scribe acknowledgement serially, and Postgres completes the run and inserts dispatches only after publication succeeds.

## Verification limits

- No test command was rerun in this domain sub-review. The task records successful `test:vala`, `test:sql`, `test:wyrd`, `test:bifrost`, journey, storage, codegen, tenant-isolation, format, and lint lanes; this review inspected their relevant source and assertions but did not independently reproduce the command results under the review time budget.
- The baseline missing/invalid artifact runtime branch remains identified by the task itself as code-covered but not journey-covered.
- The review did not re-qualify the complete Oracle distributed planner or object-store integrity stack beyond the typed-plan and exact artifact path used by TASK-005.

## Overall result

**FAIL**

The PostgreSQL durability and fencing design is otherwise coherent, but `DATA-001` violates the explicitly shared execution ceiling at the baseline fit boundary and `DATA-002` violates the explicitly bounded SPC aggregate-consumption boundary. Both are reachable on the production TASK-005 path and require bounded corrections before acceptance.
