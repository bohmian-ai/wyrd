---
id: TASK-007
kind: implementation
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
status: proposed
depends_on: [TASK-003, TASK-004, TASK-005, TASK-006]
requirements: [REQ-026, REQ-035, REQ-038, REQ-040, REQ-042, REQ-043, REQ-044, REQ-045]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-008, AC-014, AC-016, AC-017, AC-018, AC-019]
---

# Measure and improve verification evidence retrieval

## Outcome and Value

Retrieve the same authorized verification evidence with less measured overhead.
For example, an exact result lookup should read only needed fields, while an
old trace must retain all authorized visible spans. Record comparable before/after
measurements, including paths that already work efficiently; promise no new
numeric lookup latency target.

## What Changes and What Must Stay True

Cover inline JSON, digest-addressed files/Artifacts, completed results, repeated
trace checks, subject/series windows, grouped inputs/results, and declared Agent
`bifrost.query` SQL over custom tables. Inline/storage inputs are distinct from
warehouse lookups; custom tables gain no automatic InputSlot discovery.

Optimize existing owners only where evidence supports narrower projections,
equivalent predicates, trusted bounds, batching, or supported pruning. Preserve
accepted authority, tenancy/table/column permissions, nonblocking audit,
owner/subject/Verifier distinctions, typed data, storage length/hash/format/citation
validation, deadlines, complete-result limits and refusal semantics. A binding
adds no implicit workload filter or query grant. Missing/disabled Blooms cannot
change exact results. Never invent a result/trace lookback or omit hot/live data.

TASK-006 owns whole-trace selection, attempt-local deduplication and readiness
retries. Measure/regress that implementation here. No broader reuse/snapshot
contract, cache, durable index, alternate query store or changed partition/sort/
Bloom default is approved. Keep compatible retained layouts readable.

Dependencies remain explicit so measurement uses final identity/telemetry,
table/layout/catalog, runtime ownership and trace contracts. TASK-008 owns final
docsite work. Cost is bounded representative fixture execution, without synthetic
host load or a whole capacity qualification.

## Approach

1. After every dependency settles, inventory each path's caller, authority,
   projection, equality/range predicates, cardinality, source view and layout.
   Baseline these **final contracts before retrieval optimizations**, not the
   current pre-change schemas or intermediate dependency states.
2. Reuse Oracle profiles and harness telemetry. Record fixture history/skew,
   host/build/settings and cold/warm conditions; separate source discovery,
   planning/admission, scanning/pruning, decoding and orchestration. Capture
   latency, query counts and files/bytes/groups/pages examined/pruned. Explain
   unavailable metrics; fill material gaps in the existing owner.
3. Rank avoidable work and record baseline-derived acceptance comparisons,
   variance and trade-offs before edits. Apply TDD below only to changed
   executable behavior; preserve already-correct narrow/batched reads.
4. Repeat the same workloads, compare authorized evidence/outcomes and account
   for improvements/regressions. Review all nine system Bloom defaults against
   actual/intended predicates and measured costs without changing approved sets.
   Retain findings and measurement limits as implementation evidence for TASK-008.

## Ordered Implementation Scenarios

These scenarios apply when profiling justifies executable changes. Unchanged
paths receive regression/profile proof without manufactured REDs. Planned test
selectors must be confirmed after addition and before RED.

### Scenario 1 — Exact completed results remain exact

**Behavior.** A completed execution resolves its canonical result ID and returns
its outcome/Run, whether recent or old. Not-yet-visible evidence remains absent;
another result, binding, owner or tenant never substitutes.

**RED.** [Exact-result proof](#exact-result-proof) compares those outcomes and
refusals and fails on the measured avoidable projection/source work.

**GREEN.** Remove that work in the existing result/query owner; rerun the proof
without narrowing evidence or changing missing-result behavior.

**REFACTOR.** Retain status-to-result-ID resolution and the narrow decoder;
remove redundant retrieval rather than adding another lookup store.

### Scenario 2 — Window reads retain their statistical population

**Behavior.** Subject/series windows retain inclusive start, exclusive end,
completeness decisions and aggregates. Other subjects/series and boundary-excluded
rows stay excluded; missing/invalid features retain their existing outcome.

**RED.** [Window proof](#window-proof) compares populations/outcomes and fails on
the measured redundant query, projection or scan work selected for removal.

**GREEN.** Optimize the existing window reader while retaining its combined
completeness/scoring statement and distinct windows/occurrences. Rerun window
proof and the existing single-cut regression.

**REFACTOR.** Keep selection in the window owner and combine scans only within
the existing evidence view.

### Scenario 3 — Group batching preserves source occurrences

**Behavior.** Input-only and multiple-source groups retain workload/group and
subject/Verifier tuple selection, input ordering, latest-visible-result selection,
pinned inputs and missing-source behavior. Independent occurrences stay separate.

**RED.** [Group proof](#group-proof) compares selected values/ordering and fails
on measured redundant round trips or unnecessary payload decoding.

**GREEN.** Extend the existing combined-source query only for that measured gap;
retain its batched `UNION ALL` semantics and rerun group proof.

**REFACTOR.** Consolidate equivalent selection in the poller without a separate
group loader or persisted snapshot model.

### Scenario 4 — Custom SQL retains its declared meaning

**Behavior.** Declared Agent SQL returns the same exact filtered/projected values
across live/hot/published data, absent/disabled Blooms and retained layouts.
Permissions, deadlines and row/byte overflow still refuse complete results;
workload context adds no unrequested predicate.

**RED.** [Custom-query proof](#custom-query-proof) compares answers/refusals and
fails on measured eligible filter/projection work not reaching the scan. A new
correctness assertion protects any changed pushdown from dropping valid rows.

**GREEN.** Fix the evidenced gap through HostedTools, BoundedQuery and Oracle;
rerun the proof with supplied SQL semantics and terminal-safe collection intact.

**REFACTOR.** Reuse ordinary pushdown/source selection rather than creating a
custom-table query or authorization path.

## Acceptance Criteria

| Obligations | Required proof and failure signal |
| --- | --- |
| REQ-043; AC-017 | Comparable final-contract before/after evidence covers every listed path and measurement dimension. Unsupported speed/pruning claims or unexplained measurement gaps fail. |
| REQ-026, 040; AC-008, 014; INV-001–008 | Four focused proofs preserve exact selections, outcomes, authority and refusal cases described above. Broadened credentials, merged populations/occurrences or partial-success overflow fail. |
| REQ-035, 038, 042, 044; AC-016, 018 | Equal answers with absent/disabled Blooms, live/hot/published data and retained layouts; all nine defaults reviewed against query cost. Required Bloom floors, composite-order claims or silent default changes fail. |
| REQ-043, 045; AC-019 | TASK-006 regressions plus profiles show one complete authorized trace collection per trace/attempt without hidden bounds, with completed judgments unchanged. Wider reuse or a second implementation fails. |

## Implementation Reference

Owner paths guide reuse, not private implementation shape. Server paths below
are relative to `crates/wyrd/wyrd-server/src/`.

| Inspected owner/caller/test | Existing behavior and extension boundary |
| --- | --- |
| `components/verification/service.rs::VerificationControl::{get_execution,read_result}` | Status resolves canonical result ID; `SELECT outcome, run_id` and `LIMIT 1` are already narrow, with no authoritative time bound supplied. Optimize only measured remaining work. |
| `verification/hosted/{blocks,evidence,traces,tools}.rs` | `HostedBlocks::engine` shares accepted-caller owners; `TenantEvidence::{read,artifact}` validates pinned bytes and authorized inventory; `CallerTraces::fetch` belongs to TASK-006; `HostedTools::call` delegates SQL to `query/collect.rs::BoundedQuery`. No alternate evidence loader is needed. |
| `verification/drift.rs::ObservationWindow::{rows_in,psi_statement,spc_statement}` / `DriftEngine`; `verification/groups.rs::GroupPoller` / `Sources::read` | Existing frozen subject/series predicates and combined completeness/scoring, plus grouped `UNION ALL` and ordered occurrence selection. The named single-cut regression below pins shared-view behavior. Extend these owners, preserving their batching. |
| `query/scheduled.rs::ScheduledQueryCaller::{authenticated,run_with}`; `query/service.rs::stream_query` | Both lead to `Bifrost::query_sql`; authorization, audit, terminal settlement, source binding and peer forwarding stay there. |
| `crates/vala/vala-bifrost-redux/src/oracle/profile.rs::QueryProfile`; `src/profile.rs::ProfileFamily::ORACLE_QUERY` | Existing cut/materialization/listing/planning/admission/operator metrics emit counts/nanoseconds and DEBUG events. Reuse them; summed operator time can exceed wall time and distributed attribution has limits. |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs::ResultLayoutJourney::scan` and existing result pruning regression | Telemetry checkpoints already expose work evidence. Manifest and `main.rs` confirm the `server` target/module. Extend its real-server Rust-client proofs; internal metrics establish work, public SDK calls establish behavior. Old schema/floor assertions are dependency work, not authority. |

## Verification and Evidence

Profiling, inventory and nine-table index review are non-TDD measurements of
existing behavior. Reuse telemetry checkpoints and retain exact workload/settings.
Inline/storage and TASK-006 trace behavior receive regression-only proof unless
this task makes an evidenced executable change there.

Commands below use the inspected `server` target and its mise lane's Postgres/
migration setup, journey profile and ignored-test policy. Run
`mise run build:server` first for its Monty-worker prerequisite. Confirm planned
selectors before RED; run Cargo commands sequentially.

### Exact-result proof

Planned `verification_runtime::completed_result_lookup_preserves_evidence`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::completed_result_lookup_preserves_evidence)"'
```

### Window proof

Planned `verification_runtime::window_lookup_preserves_population`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::window_lookup_preserves_population)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::drift::tests::one_statement_decides_completeness_and_scores_from_one_cut)'
```

### Group proof

Planned `verification_runtime::group_lookup_preserves_source_occurrences`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::group_lookup_preserves_source_occurrences)"'
```

### Custom-query proof

Planned `verification_runtime::custom_query_preserves_evidence_and_limits`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::custom_query_preserves_evidence_and_limits)"'
```

### Existing profile and scan regressions

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::profile::tests::profile_folds_oracle_steps_and_operator_metrics)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::result_layout_partitions_blooms_and_prunes_by_result)"'
```

Rerun TASK-006's [attempt-read proof](TASK-006-verification-trace-evidence.md#owner-test-commands)
and [whole-trace proof](TASK-006-verification-trace-evidence.md#journey-commands)
using their recorded exact commands, and attach their
profile/query-count evidence rather than duplicating its implementation or
inventing replacement selectors. Use the same focused proofs for before/after
measurement; a whole capacity benchmark or full journey suite is not required.
Finish with `mise run fmt`, `mise run lints` and
`mise run check:tenant-isolation` for changed Rust/query code. No generated or
public contract change is intended; if an unavoidable such change appears,
coordinate its owning task and required contract checks before proceeding.

## Material Stop Conditions

Return evidence for specification authority if an improvement needs a new
index/store, wider reuse or read consistency, changed defaults, or a latency
promise. Reject optimizations that violate the preserved semantics above.
If final contracts or a reproducible baseline are unavailable, report that
measurement blocker instead of substituting intermediate shapes or numbers.

## Authority Links

- [Approved revision 12 specification](../spec.md)
- [Storage supplement: evidence access and lookup performance](../supplements/table-storage-and-correlation.md#verification-evidence-access-and-lookup-performance)
- [Storage supplement: whole-trace decision](../supplements/table-storage-and-correlation.md#whole-trace-verification-decision)
- [Bifrost query owner](../../../../architecture/bifrost-design.md#query-oracle)
- [Security authority](../../../../architecture/wyrd-security-posture.md)
- [Testing](../../../../TESTING.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
