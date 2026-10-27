---
id: TASK-007-R1
kind: remediation
status: proposed
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
parent_task: TASK-007
remediates: [FIND-TASK-007-1, FIND-TASK-007-2]
---

# Complete comparable verification-query measurements and populated policy exploration

The ordering correction and existing retrieval behavior are accepted. Complete the missing measurement and representative exploration evidence through the existing test/telemetry owners. This remediation requires no new retrieval optimization.

## Authority and immutable review context

- Approved specification: `changes/active/service-identity-and-telemetry/spec.md`, revision 16.
- Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-007-verification-query-performance.md`; original obligations are present at the base, candidate appends implementation evidence.
- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-7`.
- Original base: `76e095738d2cf89e10523a92bb9b4f585fa1dd3d`.
- Reviewed candidate: `dabe0e7dde58c6e26d584120ae0a48cfede79436`.
- Prior verdict and validated ledger: `verdict.md` and `findings-validation.md` beside this task. Preserve these and the original candidate for the remediation review.
- Governing references: root AGENTS; `agent_docs/{rust,maintainer-style}.md`; `architecture/bifrost-design.md` Query: Oracle and measurement meanings; storage supplement “Verification evidence access and lookup performance”; spec AC-008/016/017; TESTING.md.

Route execution to `$wyrd-implement`. This review does not authorize an implementation, commit, merge or push.

## Diagnosis and selected correction

### FIND-TASK-007-1: the principal measurement deliverable is incomplete

The original task and AC-017 require comparable final-contract measurements: latency, source/planning/scan work, repeated query counts, representative writer costs, repeated observations and explained limits. The candidate records candidate-only work counts, a general planning range and scheduler wait, then marks the all-dimensions comparison PASS. No matched per-path base/candidate measurement record exists. See the original task's Implementation Evidence and limits, and `verification_runtime.rs:813–889`.

The producer already takes a telemetry checkpoint/delta around actual read futures. `ScanEvidence::of` retains a few counters and selected profile families, dropping the elapsed/process evidence; its selected families omit the existing query-duration histogram. The telemetry owner at `crates/wyrd/wyrd-testing/src/bifrost/telemetry.rs` already captures intervals and process CPU/RSS, and its query report consumes duration histograms. Oracle QueryProfile already owns relevant stage timing. Thus private visibility is not evidence that no useful observation can be obtained. The Bloom write-byte comparison is also confounded by extra audit traffic.

This is material because count-only correctness proofs do not establish before/after cost or the effect of the additional sanity traversal. It does not establish an actual performance regression.

Reuse these existing telemetry/profile owners and ordinary elapsed timing to retain a bounded comparable record. Expose only necessary read-only measurement evidence where visibility prevents use; do not add another sampler or profiling framework. Repeat the same representative inputs against original-base production and candidate production with equivalent fixture geometry, host/build/settings and stated warm/cold conditions. If the added proof harness must be used at the base, keep that adaptation test-only and identify it explicitly; do not backport the production ordering correction or replace the baseline with an intermediate schema contract. Use a separate temporary checkout if needed; never write diffs/source archives into the review directory.

Record per-path elapsed/query/stage/work observations and spread, including already-efficient result, trace, window, group and custom SQL reads, plus distinct inline/storage regression observations with warehouse cells marked N/A. Separate scheduler/orchestration wait from evidence access. Unchanged paths may report no attributable retrieval change; no speedup is required. A baseline ordered query that returns the wrong answer must retain its failure and cannot serve as a successful equivalent-answer speed comparison. If a reproducible baseline is truly unavailable, report the measurement blocker instead of claiming acceptance.

For representative equivalent policy write windows, retain existing process CPU/RSS and stored filter bytes. Label resources as whole-process/co-tenant observations, document endpoint or sampled-peak granularity, and isolate or account for audit/background writes. Do not claim writer-specific attribution from process counters or a causal Bloom cost from unequal workloads. Document genuinely unavailable page/decode/distributed detail and summed-operator-time limitations. No new numeric budget or statistically powered capacity experiment is required.

### FIND-TASK-007-2: the all-table policy exploration has unexercised positive populations

The original exploration requires all nine system tables and full/partial/empty Bloom declarations. The existing fixture lists gateway calls but produces no calls, so every gateway filter succeeds against an empty ground truth. It separately queries audit under one principal, and changes system declarations directly from defaults to empty without a proper-subset stage. See `verification_runtime.rs:6201–6235,6345–6367,6470–6546,6586–6709,6761–6837` and the candidate task's explicit gateway limit. AC-008 includes gateway; AC-016 distinguishes complete/partial/empty policies. SQL predicates returning no rows do not establish a partial Bloom declaration.

The consequence is a false-positive all-table proof: lost positive gateway rows or correlation fields would still pass, and required partial-policy and audit cost evidence is absent. No production correlation defect is asserted.

Reuse `GatewayCapture::{start,capture_call}`, the existing typed-payload journey and ordinary authenticated gateway route to produce bounded representative accepted Service/release/environment/component-subject call populations. The existing separate-server topology may be measured separately. Do not expose private CallCapture or fabricate native gateway ingestion. Exercise direct names/versions and exact UIDs, environment, subject kind/name/version, combined Service+subject, empty answers, bounded/unbounded reads and the already-required representative sharing/skew, using known positive expectations rather than relying exclusively on empty ground truth.

Reuse existing table alteration, exact-row comparison and physical filter inspection to add one proper-subset system policy between full and empty states. One representative subset suffices; no exhaustive subset matrix is required. Capture physical work even where pruning is zero; no positive-pruning quota or multi-row-group capacity fixture is introduced.

Include the ninth table through real permission-decision audit records. Audit targets are not observation subjects. Preserve truthful nullable shared descriptors; exercise applicable shared predicates/null behavior and existing principal/request/resource keys without manufacturing Service or subject attribution. Apply the policy/cost investigation to audit's own defaults. These extensions share the measurement work from FIND-TASK-007-1; do not build duplicate experiments.

## Preserved behavior and non-goals

Keep `LiveUnionBoundary`'s native sorted merge and appended sanity checker, the single distributed planner, query-local errors, existing source/tenant/table/column authority, complete-result limits, deadlines and nonblocking audit. Preserve canonical result identity and absence, statistical windows, separate group occurrences, whole authorized trace scope and per-attempt reuse, storage validation, live/hot/published visibility and retained-layout reads.

No cache, durable index/store, guessed lookback, automatic custom InputSlot, new latency SLO, production default/partition/sort/Bloom change, writer-specific profiler, new public contract, dependency, capture API or alternative query owner. Keep code free of task/review citations; correct nearby documentation only when touching it. Do not broaden this remediation into an optimizer or lifecycle redesign.

## Ordered execution and acceptance

This is measurement and already-correct-behavior regression work; do not manufacture RED by changing production retrieval. If extending a nontrivial evidence renderer/accessor, first prove its missing output in one narrow owner test, then implement only that output and preserve existing validation.

1. Establish matching base/candidate inputs and measurement boundaries; retain repeated observations and truthful answer/failure status. Reuse the already-passing focused proofs.
2. Complete the populated gateway, truthful audit and proper-subset policy exploration through existing owners, retaining exact populations and physical work.
3. Record the comparable observations, variance, process attribution and unavailable cells in the task evidence. Replace unsupported PASS/cost claims with supported conclusions.

| Finding | Closure acceptance |
|---|---|
| FIND-TASK-007-1 | Repeated comparable final-contract per-path base/candidate elapsed/stage/work evidence exists; unchanged/no-warehouse paths and invalid baseline answers are labeled correctly; equivalent writer-window observations and filter bytes have honest attribution; remaining metric limits are explicit. |
| FIND-TASK-007-2 | Gateway has explicit positive authorized populations and exact negatives; audit uses truthful accountability/null semantics and applicable keys; one proper-subset policy is distinct from full/empty, with equal authorized answers and retained physical work under each. |

## Focused proof and checks

Confirm exact selectors after any additions. Reuse these existing selectors; select only affected proofs for changed behavior and the required bounded comparison, not whole journey suites. Run Cargo commands sequentially and build the Monty prerequisite once:

```bash
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::completed_result_lookup_preserves_evidence) | test(=verification_runtime::window_lookup_preserves_population) | test(=verification_runtime::group_lookup_preserves_source_occurrences) | test(=verification_runtime::custom_query_preserves_evidence_and_limits) | test(=verification_runtime::exploration_filters_keep_exact_populations)"'
```

If the existing gateway proof is extended separately, its exact command is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::typed_builtin_payloads_are_queryable)"'
```

For inline/storage observations and the existing trace owner, reuse their existing focused proofs where measurements require them; do not rerun all trace retry/language journeys for unchanged trace behavior:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_hosted_artifacts::artifact_evidence_and_code_reach_every_consumer) | test(=pg_hosted_artifacts::artifact_reads_refuse_before_use)"'
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::trace_checks_share_one_attempt_read)'
```

The complete-result proof already exercises actual trace collection; reuse its bounded work observation rather than creating another trace loader or runtime. Run exact narrow owner tests for any telemetry evidence output changed, plus format/lints and the boundary checks applicable to the actual Rust write set. Do not add a full gate, complete Oracle suite, all SDK journeys or capacity benchmark to this remediation. The existing production ordering regressions remain accepted unless executable query code changes; if it does, justify that scope and rerun its exact focused tests.

A later fresh task review compares the remediation diff to dabe0e7dd, validates closure of these IDs and affected paths, and carries forward the accepted production behavior and existing proof. It must retain the original task/base as cumulative acceptance context.

## Implementation Evidence

Candidate commits: `bf94447d6` (measured reads keep elapsed time, the
`oracle_query_duration_seconds` histogram and whole-process resources through
the new read-only `BifrostTelemetryDelta::resources`), `276192423` (nine-table
exploration with populated gateway calls, audit keys and a partial Bloom stage),
`2654e3299` (task citations removed from journey rustdoc). No production code
changed.

### Measurement method

- Same host (macOS arm64), debug test profile, warm build, default settings,
  one `BifrostClusterSpec::one_mixed()` cluster per journey, one journey at a
  time (`--test-threads=1`), three repetitions per side.
- Candidate: `dabe0e7dd` production. Base: `76e095738` production. The only
  production difference is `oracle/live.rs` and `oracle/analytical.rs`; for the
  base side those two files were checked out from `76e095738` into the
  candidate tree and restored afterwards. **Test-only adaptation:** both sides
  ran the candidate harness (the R1 journeys and `ScanEvidence`).
- Each cell is the median (min–max) of three repetitions. A row is the sum over
  every statement of that path within one repetition. "Caller elapsed" is the
  awaited future as its caller saw it; "query duration" is the
  `oracle_query_duration_seconds` sum; planning, listing (Scribe discovery),
  cut (read + materialization) and scan (`DataSourceExec.time_elapsed_processing`)
  are `QueryProfile` stage nanoseconds.
- **Limits of the figures:**
  - Operator times are summed across partitions and can exceed wall time.
  - Process CPU, Tokio busy time and RSS cover the whole test process: every
    server role plus the journey. Their granularity is 10 ms of CPU, and RSS is
    taken at the window's endpoints (no sampler). RSS grows throughout a
    journey, so it is reported, not attributed.
  - Per-page counts and decode-only time are not separated by the existing
    profile.
  - The single-node cluster has no follower, so no distributed fan-out was
    measured.

### Read paths, base vs candidate

| Path (proof) | Statements | Files / row groups / bytes | Caller elapsed ms, base → candidate | Planning ms, base → candidate | Conclusion |
|---|---|---|---|---|---|
| Pending status (result) | 0 | 0 | — | — | no Oracle read either side |
| Completed result, passed / failed (result) | 1 / 1 | 1 / 1 / ~1.2 KB, equal | 29.4 (25.4–32.8) → 27.4 (27.2–30.1); 19.4 (17.9–22.2) → 18.0 (17.5–22.7) | 4.6 → 4.8; 4.3 → 4.3 | no attributable change |
| Trace collection, two executions (result) | 2 | 2 / 2 / 3460, equal | 115.4 (59.2–190.8) → 168.4 (116.9–172.5) | 28.7 (17.7–83.5) → 55.2 (17.8–76.9) | equal work; overlapping spread dominated by first-read cut materialization; no attributable change |
| Trace attempt sharing (`trace_checks_share_one_attempt_read`) | unit | N/A | test 0.035 s → 0.035 s | N/A | unchanged |
| Three drift windows (window) | 3 | 3 / 3 / ~1250, equal | 31 277 → 31 235 | 33.7 → 30.9 | elapsed is scheduler wait for the due pass; query duration 13.2 → 11.5 ms; no attributable change |
| Four turns, two groups (group) | 2 | 0 (live) | 5079 → 5091 | 46.8 → 50.7 | elapsed is runtime orchestration; query duration 35.9 → 38.0 ms; no attributable change |
| Custom SQL `ORDER BY id` (custom query) | 1 | 2–4 / 2 / 174, equal | 53.1 → 58.0 | 6.9 → 6.9 | **base returned `[[4],[1],[2],[6]]` in reps 1–2** (journey failed) and the right order by arrival luck in rep 3; not a successful-answer comparison; equal work, planning cost of the appended sanity check not separable from noise |
| Results' ten Bloom columns (exploration) | 10 | 23 / 23 / ~16.2 KB, equal | 147.4 → 146.6 | 36.1 → 36.4 | no attributable change |
| Exploration, 9 tables × 12 filters + audit keys + aggregate, per stage | 112 | full published 381 / 381 / ~249 KB, equal; partial 449; empty 517 | full 2164 → 2161; partial 2260 → 2227; empty 2291 → 2284 | full 676 → 689; partial 706 → 694; empty 692 → 712 (≈6.0–6.4 ms per statement) | no attributable change |
| Inline JSON / digest inputs (group) | 0 | N/A (no warehouse read) | within group row | N/A | unchanged |
| Storage artifacts (`pg_hosted_artifacts`) | N/A | N/A (no warehouse read) | test 6.90 / 6.89 / 6.77 s → 6.80 / 6.76 / 6.99 s; refusals 1.60 / 1.54 / 1.54 → 1.58 / 1.53 / 1.53 s | N/A | unchanged |

The live and published exploration stages, and the live trace, window and group
reads, give the same statement, file, row-group and byte counts on both sides
(files vary by ±1–3 between runs through open-time statistics skips; row
groups do not). The appended `SanityCheckPlan` traversal is not visible above
run-to-run spread in planning time on any path.

### Writer windows per Bloom declaration (exploration)

One equal checkout round per stage: 2 subjects × 4 native frames, 2 gateway
calls, 2 inputs, and 2 results. Medians of three candidate repetitions; base
within spread.

| Declaration | Ingest elapsed ms | Publish elapsed ms | Publish process CPU s | Upload bytes (all roles) | Stored filter bytes, eight writer tables | Stored filter bytes, audit log |
|---|---|---|---|---|---|---|
| Defaults (full) | 195.9 | 851.0 | 1.45 | 233 212 | 2 585 (8 filtered files) | 413 (1 file) |
| Leading half of each default set | 213.8 | 760.8 | 1.06 | 268 517 | 1 269 (8 filtered files) | 507 (2 files) |
| None | 210.3 | 656.2 | 0.57 | 265 244 | 0 (8 bare files) | 94 (1 filtered, 1 bare) |

The writer tables' stored filter bytes follow the declaration exactly. Each
publish window also writes the audit decisions that the preceding reads made:
more audit files land in the later stages, so upload bytes are not a Bloom
comparison. Publish CPU and elapsed fall with fewer declared filters, but the
CPU is whole-process and also includes audit encoding. It is an association
under equal writer workloads, not a measured per-writer Bloom cost. The
audit log's bytes come from decisions retained under the previous declaration.

### Exploration population

- **Gateway:** each release's runtime account calls the public
  `/v1/chat/completions` with a workload-rooted token, `wyrd-card-uid`, and
  `x-wyrd-deployment-environment`, once per subject per round. Captured calls
  are awaited before any read or publish.
- **Answers:** all nine tables answer every filter exactly against their
  unfiltered ground truth, under each of the three declarations, both live and
  published.
- **Positive populations:** each of the eight writer tables (gateway included)
  answers all ten positive filters with rows. Exact release UID equals
  name + version, and exact subject UID equals kind + name + version.
- **Audit:** read up to a settled instant after `await_audit_retained`. It
  answers the shared filters, including `service_name IS NULL`, against its
  own truthful values, and its
  `sub_principal_id`, `act_principal_id`, `wyrd_request_id` and `resource`
  keys.
- **Partial declaration:** keeps the leading half of each table's defaults
  (for example spans `service_name, subject_name, workload_card_uid`; audit
  `sub_principal_id, act_principal_id`). It is asserted nonempty and is a
  strict subset.
- **Fixture fix:** query JSON omits null columns. The exploration therefore
  reads attribution and audit keys by column name (`Attributed::of`) instead
  of by position, which had misread all-null audit rows as `{}`.

### Verification

| Check | Result |
|---|---|
| `bifrost::telemetry::tests::resources_render_elapsed_and_process_scope` (RED: method missing) | PASS |
| `verification_runtime::{completed_result_lookup_preserves_evidence, window_lookup_preserves_population, group_lookup_preserves_source_occurrences, custom_query_preserves_evidence_and_limits, exploration_filters_keep_exact_populations}` ×3, candidate | PASS |
| Same ×3 on base production | custom query FAIL ×2 on the ordering defect, others PASS |
| `pg_hosted_artifacts::{artifact_evidence_and_code_reach_every_consumer, artifact_reads_refuse_before_use}` ×3 each side | PASS |
| `blocks::engine::tests::trace_checks_share_one_attempt_read` ×3 each side | PASS |
| `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`, `git diff --check` | PASS |

`typed_builtin_payloads_are_queryable` was not changed and was not rerun.

| Finding | Closure evidence | Result |
|---|---|---|
| FIND-TASK-007-1 | Three-repetition base/candidate record per path above, with elapsed, stage, work and spread. Scheduler and orchestration wait is separated from query duration. Inline and storage cells are N/A. The wrong base answer is labelled. Writer windows carry their whole-process and audit attribution. Remaining limits are listed. | PASS |
| FIND-TASK-007-2 | Populated gateway positives and exact negatives. Audit has truthful null and key coverage. One proper-subset declaration sits between full and empty, with equal answers and retained work and filter bytes under each. | PASS |

Non-goals stayed excluded: no cache, index, store, lookback, SLO, default,
public contract, dependency, capture API, or profiler. Production code is
unchanged in R1.

## CI Failure Diagnoses

PR CI (`gate:rust:bifrost`, `gate:rust:families`) failed after approval. Each
failure was reproduced locally with `WYRD_LOG` tracing, except the gateway
ordering, which passed 8 of 8 local runs; independent read-only diagnosticians
confirmed the causes before the test changes.

- **Server-stamped dimensions in fixtures.**
  - *Symptom:* `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite` refused with "service_name is stamped from the accepted request context; send it null". The four `pg_bifrost_e2e::trace_timeline` tests refused with "canonical field service_name differs from the value its source fields derive".
  - *Cause:* `flat_builtin_batch` fills every column, the six shared dimensions included. The timeline span fixture sent `service_name` with no resource `service.name` to derive it from.
  - *Fix site:* the two fixtures now send those columns null. The production validators (`refuse_supplied_dimensions`, `refuse_forged`) are unchanged.
- **Scribe retries under a new principal.**
  - *Symptom:* resends of an acknowledged batch failed. `staged_backlog_survives_abrupt_restart` and `scribe_write_flush_read_user_journey` got "scribe batch commit identity mismatch". `accepted_batch_revision_survives_alteration_and_restart` and `mixed_runtime_specifications_preserve_rows_and_pins` got `TableVersionConflict`.
  - *Evidence:* the per-column `logical batch identity column contribution` digests differ only in `sub_principal_id` and `act_principal_id`.
  - *Cause:* each resend went through a newly minted service principal. The account pair is a correlation column bound into batch identity, and `bifrost-design.md` requires a retry to preserve origin. Production was correct; the tests were not.
  - *Fix site:*
    - The scribe test `support::tenant_key` mints one key per case, and every client in a case is built from it.
    - `ScribeWorkloadTenantBinding` carries one key per tenant for every append, replay and read.
    - `acknowledged_rows_survive_stage_pressure_and_restart` and `composite_partitions_survive_lifecycle_and_peers` resent from the same new-principal pattern without checking the result. They now use the one key and expect the resend to be acknowledged.
- **Gateway capture frame order.**
  - *Symptom:* `gateway_capture_follows_policy_and_never_affects_the_call` asserted calls before spans and saw spans first.
  - *Cause:* `ScribeSink::submit` delivers frames of one phase concurrently: up to `FRAMES_IN_FLIGHT`, each encoded on its own blocking task. Its rustdoc says acknowledgement order is not row order.
  - *Fix site:* the test now compares the frame tables without order, as the sibling outbox test does. The outbox is unchanged.
- **Open drift:** a same-version batch-identity mismatch surfaces as a 500 internal invariant (`scribe_batch_commits.rs`), not the stable batch-identity conflict that `bifrost-design.md` names. This is not changed here.
- **Compaction racing the promoted read.**
  - *Symptom:* under the local `test:bifrost:gate` load, `mixed_specifications_preserve_partition_and_sort_truth` found 4 first-spec rows under the revised spec and sort order after promotion. It passed when run alone.
  - *Cause:* `enable_compaction` sets a snapshot-count trigger of one, so the first promotion commit makes compaction due. When the worker's 5 s pull lands between `drain_forge_backlog` and the promoted read, a rewrite commits one partition under the current output definition. That is the authority's behavior, not a promotion fault.
  - *Fix site:* this test, `compaction_target_registers_describes_and_steers_forge_rewrites` and `failed_memory_attempt_retries_without_partial_publication` now keep the default hourly interval. Each advances the Forge clock one hour only after taking its promoted baseline, as `property_less_public_table_is_compacted_by_default` does. Callers that go straight from drain to awaiting rewrites keep `enable_compaction`.
