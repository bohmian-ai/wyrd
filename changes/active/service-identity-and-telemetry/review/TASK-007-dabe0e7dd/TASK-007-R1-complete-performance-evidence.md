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
