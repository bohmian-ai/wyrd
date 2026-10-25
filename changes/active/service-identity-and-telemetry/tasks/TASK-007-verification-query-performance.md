---
id: TASK-007
kind: implementation
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
status: proposed
depends_on: [TASK-003, TASK-004, TASK-005, TASK-006]
requirements: [REQ-049, REQ-050, REQ-051, REQ-052, REQ-053, REQ-026, REQ-035, REQ-038, REQ-040, REQ-042, REQ-043, REQ-044, REQ-045]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-024, AC-008, AC-014, AC-016, AC-017, AC-018, AC-019]
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

## Exploration proof strategy

Measure direct Service-name aggregation across versions, exact version/UID,
environment, subject kind/name across versions, exact subject UID and combined
Service+subject predicates on all nine system tables, reading optional tags
unchanged. Use the declared sort and Bloom defaults; compare full,
partial and empty filters with identical results. Include a shared Agent under
two Services, concurrent releases/environments, repeated same-name Card kinds,
low-volume and skewed Services, time-bounded and explicitly unbounded queries.
Record candidate files/groups/pages, bytes, planning/scan time and writer memory/
CPU/filter bytes, including the ten-column result set. Do not infer pruning from
field presence or UI controls. Existing performance thresholds in the spec still
apply; no new lookup SLO or default tuning is authorized.

Measurement itself is verification-only and needs no manufactured RED. Add the
exploration variants to the existing profile/scan regression proof below; any
executable retrieval change follows the ordered TDD scenarios. Preserve exact
Service/component binding ownership and whole-trace scope.

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
| REQ-026,049–053; AC-008,017,024 | Exploration proof above compares public SQL aggregates/drilldowns with expected cross-signal populations, missing-filter fallback and physical scans. Lost rows, merged kinds/Services or unreported writer regressions fail. |
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

- [Approved revision 16 specification](../spec.md)
- [Storage supplement: evidence access and lookup performance](../supplements/table-storage-and-correlation.md#verification-evidence-access-and-lookup-performance)
- [Storage supplement: whole-trace decision](../supplements/table-storage-and-correlation.md#whole-trace-verification-decision)
- [Bifrost query owner](../../../../architecture/bifrost-design.md#query-oracle)
- [Security authority](../../../../architecture/wyrd-security-posture.md)
- [Testing](../../../../TESTING.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)

## Implementation Evidence

All measurements use the final contracts on a `BifrostClusterSpec::one_mixed()`
cluster: macOS arm64, debug test profile, warm build, default settings, and
`ScanEvidence::of`, which takes cluster telemetry checkpoint deltas of the
`oracle_query_*`, `scribe_write_profile_*` and `bifrost_*upload*` families. Every
figure below was stable across at least two runs unless stated otherwise.

### Inventory

| Path | Caller and authority | Projection and predicates | Source and layout | Measured work |
|---|---|---|---|---|
| Inline JSON and digest-addressed inputs | queued execution; pinned `TenantEvidence` | no warehouse read; storage length, hash and format validation | Postgres and object storage | 0 Oracle statements for 4 inline turns (group proof) |
| Completed result | `VerificationControl::get_execution` → `read_result` as the caller | `outcome, run_id` by canonical `result_id`, `LIMIT 1` | results, daily on `ended_at`, Bloom `result_id` | 1 statement, 1 of 5 result files, about 1.2 KB; pending status: 0 statements |
| Repeated trace checks (TASK-006) | `CallerTraces` as the accepted caller | exact `trace_id`, no time bound | spans, hourly | 1 statement per trace per attempt (2 executions → 2 statements, 2 files) |
| Subject/series window | `DriftEngine` as tenant SYSTEM serving the owner | exact subject, series, `[start, end)`, one combined statement | observations, daily on `created_at` | 3 windows → 3 statements, 3 files, 1250 B |
| Grouped inputs/results | `GroupPoller::poll` as SYSTEM serving the owner | one `UNION ALL` by `group_id` and eligible tuples; payload read only for slots that read it | inputs and results, live tail | 2 groups → 2 statements, 0 files (rows served live) |
| Declared Agent SQL | `HostedTools` and MCP → `BoundedQuery` as the caller | the supplied SQL only; no workload predicate | custom table: iceberg + hot + live | 1 statement; 2 of 4 published row groups, 174 B |

### Exploration (`verification_runtime::exploration_filters_keep_exact_populations`)

Fixture: Prompt plus same-named Agent and Model `lookup-component`; billing 1.0.0
prod ×4 rounds, billing 2.0.0 staging ×1 (skew), checkout 1.0.0 prod ×1, all
sharing both components; spans, logs, points, observations, inputs, results and
features. Every filter equals the unfiltered ground truth filtered in Rust, on
eight attributable tables, at each of three stages. Files and row groups
scanned (there is one row group per file):

| Filter | Published, default Blooms | Live, Blooms cleared | Published, Blooms cleared |
|---|---|---|---|
| Service family (across versions) | 5 | 5 | 5 |
| Exact release (UID/version) | 1 | 1 | 2 |
| Environment | 5 | 5 | 6 |
| Subject kind + name (across versions) | 6 | 6 | 7 |
| Exact subject UID | 6 | 6 | 7 |
| Service + subject | 1 | 1 | 2 |
| Empty result | 0 | 0 | 0 |
| Time-bounded / unbounded | 6 / 6 | 6 / 6 | 7 / 7 |

Results' ten Bloom columns: `result_id`, `execution_id`, `binding_id`, and
`service_name`/`workload_card_uid` for a single Service each scan 1 file;
`verifier_card_uid`, `subject_name` and `subject_card_uid` scan 6, because every
file holds the shared subjects; `run_id`/`group_id` (absent) scan 0. The audit
principal lookup scans 6. Physical planning takes about 5–7 ms per statement.

Stored Blooms with defaults: 48 files, 14,946 filter bytes (about 311 B per
file), 1,085,169 file bytes. After one Bloom-cleared round: 49 filtered, 7 bare,
15,359 filter bytes. Encoded persistence bytes for one checkout round: 164,660
with Blooms and 173,668 without.

### Changes made

- **Defect fixed:** `LiveUnionBoundary` wrapped each published sibling of a
  live leaf in `CoalescePartitionsExec`, which dropped the sibling's sort. The
  `SortPreservingMergeExec` above then merged unsorted input, so
  `ORDER BY id` over published plus live data returned `4, 1, 2, 6`. Oracle
  plans every query through this session on a node with an analytical handle,
  so interactive, scheduled and MCP reads were all exposed. A sorted sibling is
  now merged on its own ordering by `SortPreservingMergeExec`, which the
  distributed planner treats as the same network-coalesce boundary
  (`datafusion-distributed` `network_coalesce.rs:110-111`).
- **Class closed (reviewer decision):** `LiveUnionBoundary::install` appends
  the rule and then an unconditional `SanityCheckPlan`, so an ordering or
  distribution requirement broken by Wyrd's rule fails planning through the
  existing planning-error path. The check runs before distributed rewriting
  (the distributed query planner) and has no bypass or option. Scoping the
  boundary to distributing plans was rejected because distribution is decided
  later in planning. The stale workspace `Cargo.toml` comment ("Wyrd ships no
  distributed query path") now describes the shipped dependency.
- No other avoidable work was measured: every path already issues one
  statement per logical read, with narrow projections and exact predicates.
  `read_result` keeps no time bound, because Postgres and server clocks can
  skew and no lookback is authorized.

### Nine-table Bloom default review (for TASK-008)

The defaults are unchanged.
- Attributable tables: `service_name`, `subject_name`, `workload_card_uid`,
  `subject_card_uid`, `run_id`, plus each table's own lookup keys:
  - spans and logs: `trace_id`;
  - points: `metric_name`;
  - calls: `call_id`;
  - observations: `series`;
  - features: `result_id`;
  - inputs: `execution_id`, `verifier_card_uid`, `binding_id`, `group_id`;
  - results: `result_id`, `execution_id`, `verifier_card_uid`, `binding_id`,
    `group_id`.
- Audit: `sub_principal_id`, `act_principal_id`, `wyrd_request_id`, `resource`.

Every exploration predicate matched the unfiltered population with defaults
present, absent and cleared. At fixture scale (one row group per file),
pruning comes from file statistics on the per-writer sort (E, S, V, time).
Bloom row-group pruning was 0 everywhere and cannot be observed until files
hold several row groups. Clearing Blooms cost one extra file on the
exact-release, environment, subject and combined filters, and none on the
Service family. High-cardinality ID columns (`result_id`, `execution_id`,
`trace_id`) are the predicates the verification read paths actually issue.
Shared-subject columns are recorded as low-selectivity under shared components.

### Diagnoses

- **Symptom:** custom-query proof: MCP `bifrost.query` returned
  `[[4],[1],[2],[6]]` for `ORDER BY id`. **Evidence:** the operator profile
  shows `CoalescePartitionsExec` under `SortPreservingMergeExec`. The
  diagnostician's per-rule plan log shows a valid plan after
  `SanityCheckPlan`, then `bifrost_live_union_boundary`
  (`oracle/live.rs::LiveUnionBoundary::optimize`, appended after the sanity
  check in `analytical.rs:6679`) rewriting the sorted published branch.
  **Cause:** coalescing interleaves the sorted partitions `[1,2]` and `[4]`
  in arrival order, which is timing-dependent; the scheduled path passed by
  arrival luck. **Fix site:** `LiveUnionBoundary::optimize`, the one shared
  rule. Other callers checked: the typed HTTP query route, `ScheduledQueryCaller`,
  SDK `Bifrost`, and order-dependent operators (`ORDER BY … LIMIT`, windows,
  sort-merge joins). Regression:
  `oracle::live::tests::boundary_keeps_a_sorted_sibling_sorted` (RED observed:
  ordering `None`).
- **Symptom:** a trace fixture judged both executions `failed`. **Evidence:**
  `actual: "retrieve"` against `expected: ["retrieve"]`. **Cause:** a
  one-span selector yields a scalar. **Fix site:** the fixture's expected
  value.
- **Symptom:** the window proof saw no executions. **Cause:** scheduled
  executions are created by the runtime's scheduler, which was not yet
  running. **Fix site:** `settle_all` starts the runtime and waits for a
  count.
- **Symptom:** the first native exploration write was refused with
  `CardScopeDenied`. **Evidence:** `execution_lanes.rs:1160`. **Cause:** the
  subject Card was not among the Service's components. **Fix site:** the
  fixture declares components via the now-public `VerificationFixture::release_with`.
- **Symptom:** planning error `Timestamp(ns) >= Int64`. **Cause:** the
  exploration bounded nanosecond time columns with integers. **Fix site:** the
  test's `timestamp()` literal.

### Limits

- `bifrost.gateway.calls` has no rows: no cluster write path exists
  (`CallCapture` is crate-private, and `GatewayCapture` runs a separate
  server).
- Writer memory and CPU are not exposed to the harness (the process window is
  crate-private). The supplement's ~2 MiB-per-column bitset remains a
  source-derived estimate.
- The encoded-bytes comparison is not a clean A/B, because the cleared round
  also carries audit traffic.
- `oracle_query_files_scanned_total` subtracts open-time statistics skips, so
  the custom lookup reports 2 or 3 files across runs while row groups (2),
  bytes (174) and rows (4) stay constant. The proof asserts on row groups.
- Hosted Agent SQL was measured through MCP `bifrost.query`. Both use one
  `BoundedQuery`, and the hosted path adds only `tool:invoke` authorization,
  which `pg_hosted_artifacts` covers.
- The window proof takes about 34 s because it waits for the scheduler's due
  pass; there are no sleeps beyond 50 ms readiness polls.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-026,049–053; AC-008,017,024: exact cross-signal populations for every filter shape, fallback without Blooms, physical scans | `verification_runtime.rs::{Explored, Filter, StoredBlooms}`; `VerificationFixture::release_with` | `verification_runtime::exploration_filters_keep_exact_populations` | PASS |
| REQ-043; AC-017: comparable final-contract evidence for every path and dimension | inventory and tables above; `ScanEvidence::of` | the four proofs plus exploration, with work lines recorded | PASS |
| REQ-026,040; AC-008,014: exact result, no substitution, pending absent, tenant/unknown not found | unchanged `read_result` | `verification_runtime::completed_result_lookup_preserves_evidence` | PASS |
| Window population, boundaries, other subject/series, empty window unscored | unchanged `ObservationWindow` | `verification_runtime::window_lookup_preserves_population`; `verification::drift::tests::one_statement_decides_completeness_and_scores_from_one_cut` | PASS |
| Group occurrences separate, ordered, one statement per group | unchanged `GroupPoller` | `verification_runtime::group_lookup_preserves_source_occurrences` | PASS |
| Declared SQL meaning across live/hot/published and absent/added Blooms; row, byte, deadline and permission refusals; no workload predicate | `oracle/live.rs::LiveUnionBoundary::{optimize, install}` | `verification_runtime::custom_query_preserves_evidence_and_limits` (`ORDER BY id` and `ORDER BY id LIMIT 3` over two pruned published files plus live); `oracle::live::tests::boundary_keeps_a_sorted_sibling_sorted`; `oracle::live::tests::installed_chain_rejects_a_lost_ordering` (RED observed without the appended check); `oracle::live::` and `oracle::analytical::` (26); `mise run test:bifrost:journey:oracle` (whole lane) | PASS |
| REQ-035,038,042,044; AC-016,018: equal answers with absent/disabled Blooms; nine defaults reviewed unchanged | review above | exploration; `verification_runtime::result_layout_partitions_blooms_and_prunes_by_result` | PASS |
| REQ-043,045; AC-019: one trace collection per trace per attempt; TASK-006 unchanged | unchanged | U1 `blocks::engine::tests::trace_checks_share_one_attempt_read`; I1–I3 `pg_verification_runtime::{missing_trace_retries_same_execution, missing_trace_exhaustion_keeps_decision_rule, trace_unavailability_preserves_execution_mode}`; J1 `visible_trace_spans_preserve_binding_ownership`; J2 `late_trace_spans_require_new_verification`; completed-result proof's 2 statements for 2 executions | PASS |
| Profile and storage regressions | unchanged | `oracle::profile::tests::profile_folds_oracle_steps_and_operator_metrics`; `pg_hosted_artifacts::{artifact_evidence_and_code_reach_every_consumer, artifact_reads_refuse_before_use}` | PASS |
| Format, lints, boundaries | — | `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, `git diff --check` | PASS |

Non-goals stayed excluded: there is no cache, index, store, lookback, latency SLO,
or change to partition, sort or Bloom defaults, and no public or generated
contract change. Custom tables gain no InputSlot discovery. Unrelated files were
not changed.
