---
id: TASK-008
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 22
parent_task: TASK-007
depends_on: [TASK-007]
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-006, REQ-007, REQ-008, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-019, REQ-020, REQ-021, REQ-023, REQ-024, REQ-025, REQ-026, REQ-029, REQ-030, REQ-031, REQ-032, REQ-033, REQ-034]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-009, INV-010]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-008, AC-009, AC-010, AC-013, AC-014]
---

# TASK-008 — close out L1 benchmarks and measured optimizations

## Outcome and Value

Finish the packet's L1 work across Verification, Bifrost and Gateway with one
standard measurement pattern, working hot shredding, measured optimization
decisions, and complete results. For example, a `COUNT(*)` query filtering one
learned attribute must avoid reconstructing the whole attributes value merely
to recheck its predicate on the leader. A scalar-output test or a VALID run
cannot establish that outcome.

Start from rollback `07a157cb9` (runtime restored to `471f69ff9`), not the failed
attempt's code. Preserve its records as historical evidence, not current proof.
The maintainer directed this L1-only scope and clarified completion: **target
misses can be accepted only after controlled evidence establishes the attempted
change's effect and the maintainer explicitly approves the remaining miss and
keep/revert disposition**. VALID alone never grants that approval.

## What Changes and What Must Stay True

Cover Bifrost writes-parquet, hot and parquet, Verification, and Gateway. Reuse
the existing experiment lifecycle for instrumentation, measurement, validity,
recording and reporting. Close the packet's existing optimization work,
including verification persistence, gateway capture, Bifrost point/read/write
costs, and hot shredding. Do not expand into unrelated optimization.

Shredding must work through real SQL on memtable and staged data for scalar
outputs and filter-only aggregates, locally and remotely. Staged and published
Parquet share the existing planning and cached-reader policy on the node holding
the data; their stores and capture/lease ownership legitimately differ. Measure
shredding and the shared reader independently, including ingestion costs. Retain
changes on measured merit and revert ineffective attempts. A target waiver does
not waive functional requirements: abandoning the shared-reader or shredding
outcome needs an explicit specification decision with the maintainer.

Preserve exact results, tenant/authentication proof, audit, durability,
publication cuts, exactly-once generation selection, full schema/assignment
authority, deadlines, fences, cancellation and query-bound leases. Discovery,
listing and routing do not change: the leader never plans peer files and no
footer crosses the network. Invalid private projections fail before source IO;
no silent whole-column fallback or weakened validation.

Non-goals: **L2 runs, L2 calibration and L2 investigations**; new benchmark modes,
public APIs, configuration knobs, plan caches, decoded caches, retained staged
Arrow, a replacement harness, weaker workloads/targets, or platform-specific
resource controls. Preserve default L1 declarations and `prepare_hot`'s learned
layout check. Same-key diagnostic comparisons do not restore the withdrawn
four-cohort default benchmark. L1 closeout does not claim deferred L2 or final
change review is complete. Planning authorizes no implementation or Git actions.

## Approach

Execute the five stages below in order. Each stage owns an observable checkpoint;
the scenarios define behavior and the verification sections define its proof.
They are not competing sequences. Keep one change and measurement at a time.

1. [Stage 1 — baseline and diagnosis](#stage-1--restored-baseline-and-diagnosis-only).
2. [Stage 2 — real-SQL shredding](#stage-2--real-sql-hot-shredding).
3. [Stage 3 — shared reading and staged-gap explanation](#stage-3--shared-stagedpublished-reading-and-staged-gap-explanation).
4. [Stage 4 — remaining individual corrections](#stage-4--remaining-corrections-one-measured-change-at-a-time).
5. [Stage 5 — integrated L1 results and disposition](#stage-5--integrated-l1-results-and-maintainer-disposition).

## Execution Stages

An instruction such as **"Execute Stage 1 of TASK-008 only"** authorizes only
that stage. Read the task's invariants and applicable spec first, use the linked
proof, record the result here, and stop at the stage boundary. Do not continue
to another stage without direction. A completed stage is not an IMPLEMENTED
verdict for the whole task. Commits/pushes still require explicit authorization.

Before resuming a later stage, verify the preceding stage's evidence and exact
candidate. Reuse valid completed proof; do not restart or reapply reverted
changes. If a prerequisite is missing, report the specific gap rather than
silently expanding the requested stage. Missing target approval remains pending;
never treat silence, a VALID run, or stage completion as maintainer approval.

### Stage 1 — restored baseline and diagnosis only

**Scope.** Inventory outstanding packet obligations and historical compatibility;
verify that the engines use the shared experiment owner. Run all five restored
L1 baselines. Inspect the actual hot SQL plans and profile equivalent staged and
memory queries using current support before changing production code. Identify
measured costs, hypotheses and evidence gaps separately, including what is needed
for a same-key canonical/learned comparison. This stage changes no production or
test code; benchmark artifacts and execution notes are its outputs.

**Proof.** [V-STANDARD](#v-standard--shared-pattern-regression-proof), existing
[V-OTHER](#v-other--existing-persistence-and-capture-proof) regression proof,
[V-DIAG](#v-diag--causal-comparisons-and-optimization-dispositions), baseline-only
[V-L1](#v-l1--complete-release-process-evidence), and baseline compatibility from
[V-REPORT](#v-report--every-step-and-explicit-approval). No new RED is required
for this evidence-only stage. Existing failures remain visible for later repair;
a broken baseline measurement must not be reported as valid.

**Required evidence and exit.** Record paths and candidate/host/build/controls
for all five experiments; per-step baseline results and any invalidity; the
shared-owner audit; hot plans/profiles; supported cost findings; and the proposed
first implementation change. If existing support cannot isolate a cost, state
the missing observation and its owner, not a causal conclusion. Stop with this
report. Do not implement shredding, reader changes, instrumentation or repairs.

### Stage 2 — real-SQL hot shredding

**Scope.** Execute [Scenario 1](#scenario-1--real-sql-benefits-from-learned-hot-leaves)
only. Prove and fix scalar and predicate-only aggregate behavior on learned
memtable and staged data, with local/remote authority and correctness. Add the
same-key diagnostic proof using existing test/lifecycle support, without changing
the default benchmark declaration. Do not introduce the shared-reader change.

**Proof.** [V-SQL](#v-sql--real-sql-and-physical-work) RED/GREEN/REFACTOR,
[V-DIAG](#v-diag--causal-comparisons-and-optimization-dispositions) for isolated
read and ingestion costs, and affected [V-CHECKS](#v-checks--affected-correctness-and-static-proof).
Use Stage 1's baseline and hold other production behavior fixed. Do not depend
on Stage 4's new scan counters for the initial physical-work proof.

**Required evidence and exit.** Record the diff, expected RED and passing
correctness/authority proof, physical leaf/reconstruction observations,
controlled read and ingest CPU/memory measurements, protected-workload effects,
and a keep/revert recommendation. State every miss and unresolved question.
Stop before the reader change; any proposed functional waiver requires the
maintainer's specification decision.

### Stage 3 — shared staged/published reading and staged-gap explanation

**Scope.** Execute [Scenario 2](#scenario-2--staged-parquet-shares-published-planning-and-reading)
only, preserving the accepted Stage 2 behavior. Share planning and reader policy
through the existing owner while retaining local storage/capture/lease differences.
Revisit the staged/memory profiles to establish what this change removes and
what cost remains; do not promise parity or silently attribute unrelated gains.

**Proof.** [V-READER](#v-reader--shared-planning-and-reader), rerun affected
[V-SQL](#v-sql--real-sql-and-physical-work),
[V-DIAG](#v-diag--causal-comparisons-and-optimization-dispositions) with immediate
pre-reader versus post-reader candidates, and affected
[V-CHECKS](#v-checks--affected-correctness-and-static-proof). Retain the restored
baseline as context, not as a substitute for isolating this change.

**Required evidence and exit.** Record RED/GREEN, reader/pruning/cache/lease
proof, independent reader measurements including staged write pressure, the
supported staged-gap explanation and unexplained remainder, and a keep/revert
recommendation. Stop before time, accounting or other engine optimizations.
Do not waive required shared reading merely because a target is missed.

### Stage 4 — remaining corrections, one measured change at a time

**Scope.** Execute [Scenario 3](#scenario-3--hot-time-pruning-keeps-unknown-bounds-without-warnings),
then [Scenario 4](#scenario-4--local-and-remote-memory-scan-facts-are-accurate).
Then close the remaining in-scope Verification/Gateway/Bifrost optimization
ledger. Reuse already-effective fixes and their compatible evidence. Each
identified cost is a separate correction and measurement checkpoint; do not
combine fixes and claim their aggregate result proves each one helped.

**Proof.** [V-TIME](#v-time--actual-hot-statistics),
[V-FACTS](#v-facts--scan-accounting),
[V-OTHER](#v-other--existing-persistence-and-capture-proof),
[V-DIAG](#v-diag--causal-comparisons-and-optimization-dispositions), regression
proof for earlier stages, and affected [V-CHECKS](#v-checks--affected-correctness-and-static-proof).
Record a focused behavior RED before each new code correction. For already-correct
behavior, record verification-only proof instead of changing production code.

**Required evidence and exit.** Record the diff, correctness results, independent
instrumentation-overhead measurements and per-correction before/after results.
Every ledger item must have evidence and a proposed disposition or a precise
unresolved issue; unknown effects are not proven ineffective changes. Stop before
integrated qualification. Report remaining misses for maintainer approval, and
route material decisions without starting unrelated work or L2.

### Stage 5 — integrated L1 results and maintainer disposition

**Scope.** Qualify the accepted integrated candidate after earlier stages have
completed required correctness repairs. Run all five full L1 experiments and
publish complete per-step results. This stage makes no production/test-code
changes. If a correctness check or diagnosis requires another code change,
identify the earlier stage to reopen and stop qualification; return correctness
repairs not otherwise assigned to Stage 4. Do not bundle repairs or optimizations
into final measurements.

**Proof.** [V-CHECKS](#v-checks--affected-correctness-and-static-proof), final
[V-L1](#v-l1--complete-release-process-evidence), and
[V-REPORT](#v-report--every-step-and-explicit-approval). Reuse unaffected focused
proof with its candidate/coverage recorded; final performance records must all
measure the accepted integrated candidate.

**Required evidence and exit.** Required correctness checks pass; all five final
experiments have valid full records within budget; every default step and all
requested metrics are reported against compatible baselines. Present each target
miss, regression or unsettled result with its evidence and proposed disposition.
Stop awaiting any required maintainer approval; do not declare IMPLEMENTED while
approval, required behavior or required proof remains outstanding. After explicit
disposition, report the task result for independent review. Deferred L2 and final
change-review obligations remain outside this stage.

## Ordered Implementation Scenarios

Use RED/GREEN/REFACTOR for changed behavior. Already-correct behavior needs
recorded regression/no-op proof, not artificial churn. Planned selectors must
be confirmed after adding tests and before RED; setup/compile failure is not RED.

### Scenario 1 — real SQL benefits from learned hot leaves

**Behavior.** SDK queries through the real server return exact scalar and
predicate-only aggregate results across memtable, staging and publication.
Local and remote hot reads avoid unrelated leaves and unused whole-Variant
reconstruction, including `COUNT(*)` with an attribute predicate. Whole-value
requests, residual keys, missing/null values, mixed types, canonical data and
mixed learned layouts retain correct semantics and tenant isolation.

**RED.** [V-SQL](#v-sql--real-sql-and-physical-work) exercises the actual SQL shape
on a learned layout, paired with owner-level physical dependency/reconstruction
assertions. Show unnecessary whole-column work, not just correct scalar output
from a manually constructed fragment. Reject missing/malformed/tampered or
mismatched private projections before source IO. Different-key bytes alone
cannot prove leaf use or its benefit.

**GREEN.** Extend the existing live planning, signed assignment, codec and
physical expression owners together. Preserve revision 22's ordered projection
and alias authority, authenticated input closure/schema, hidden tenant inputs
and local/remote parity. The approved private contract uses ordered serialized
`PhysicalExprNode` expressions and aliases through the Oracle codec, opaque
bytes/names in wyrd-spec, and a fresh proto tag after reserved tags 7/8. Bind
ordered bytes/aliases into the new assignment-digest domain and fragment
fingerprint. Expression-less or schema/function/index/alias-mismatched assignments
fail closed; aggregate planning must still preserve tenant proof. Rerun V-SQL
and its authority regressions.

**REFACTOR.** Remove redundant reconstruction and whole-column assumptions at
the existing owners. Add no expression engine, planner or compatibility path.

### Scenario 2 — staged Parquet shares published planning and reading

**Behavior.** Local staged reads retain real per-run sizes, row counts and time
bounds, and share pruning, footer sizing, partitioning and metadata/range-cache
policy with published reads. Excluded runs are never opened; repeat reads reuse
footers. Unknown bounds stay conservative; stale/missing objects and tenant
mismatches still fail. Early stream drop releases its lease.

**RED.** [V-READER](#v-reader--shared-planning-and-reader) distinguishes scan-open
and footer-read counts across cold/warm reads, known-disjoint/unknown bounds,
object metadata changes and early drop. Current staged handoff loses file facts,
stats paths and diverges from published planning/cached reading.

**GREEN.** Extend the published planning/reader owner to staged local storage,
retaining capture ownership. Confirm metadata validation before using path keys.
Rerun V-READER and V-SQL; measure against the immediate pre-reader candidate
with accepted shredding unchanged. Keep the restored baseline as context rather
than attributing an aggregate task result to the reader.

**REFACTOR.** Remove duplicate staged planning and uncached construction. Update
the owning architecture's staged-uncached claim to the proven implemented policy.

### Scenario 3 — hot time pruning keeps unknown bounds without warnings

**Behavior.** The actual nanosecond-time hot path excludes known-disjoint
containers, keeps unknown containers and emits no conversion warning. Nulls,
inexact bounds, precision and timezone-bearing fields preserve correct answers.

**RED.** [V-TIME](#v-time--actual-hot-statistics) reproduces the real mixed
known/unknown statistics and warning; assert typed bounds, keep mask and warning
absence. If it cannot reproduce, collect real inputs rather than guess a fix.

**GREEN.** Fix the demonstrated statistics owner, in Wyrd or its pinned
DataFusion fork. If the fork is responsible, reuse/revalidate its focused fix
and update existing pins/lockfile coherently, without adding a dependency or
suppressing warnings. Rerun V-TIME and earlier scenarios.

**REFACTOR.** Keep one schema-correct unknown-bound rule. Unknown statistics
must never prune potentially matching rows.

### Scenario 4 — local and remote memory scan facts are accurate

**Behavior.** Count physical input rows after pruning and before row filtering,
and physical Arrow buffers accessed by predicate/output dependencies: necessary
leaves, metadata and residue, excluding unused siblings. Deduplicate shared
buffers within a scanned batch; actual rescans count again. Excluded batches
contribute zero. Empty instrumented scans report zero; unavailable facts remain
unavailable. Preserve facts even if filtering returns no rows. Aggregate each
validated native/remote completion once; cancelled/incomplete attempts cannot
claim successful complete facts.

**RED.** [V-FACTS](#v-facts--scan-accounting) covers pruned/empty/mixed sources,
shared buffers, scalar/whole-value dependencies, no-result filters, repeated
finalization and rejected/missing terminal evidence. Restored memory bytes and
remote row facts are absent; generic uninstrumented memory must stay absent.

**GREEN.** Extend existing capture/completion evidence and private transport
coherently, preserving each metric's availability. Run V-FACTS and earlier
scenarios. Measure instrumentation overhead separately; do not presume it
caused the previous CPU regressions.

**REFACTOR.** Remove duplicate counting and stale absence assumptions. Reuse
physical-read facts; add no endpoint or second accounting lifecycle.

## Acceptance Criteria

One mapping covers this revised task. Prior proof can be reused only with its
candidate, controls and coverage stated; unfinished earlier obligations remain
visible in the closeout ledger.

| Obligations | Required observation and proof |
|---|---|
| REQ-001–004, REQ-007–008, REQ-010–016; AC-001–004, AC-008–009 | V-STANDARD and full release-process records establish one shared declaration/execution/measurement/validity/record/report pattern across all engines. Sound target misses retain VALID; missing or corrupt evidence does not pass. |
| REQ-029–033; AC-001, AC-010 | V-SQL proves scalar and aggregate SQL through learned memory/staged reads and publication, local/remote execution, mixed/residual cases, physical dependencies and tenant/projection refusal. V-DIAG measures equivalent data/key/predicate/results; region-versus-long-tail is workload coverage, not shredding A/B proof. |
| REQ-034; INV-001–004, INV-009 | V-READER proves shared policy, real facts, pruning, validation and lease release. V-DIAG measures the shared reader separately and explains staged/memory costs. A ratio cannot improve by slowing memory. |
| REQ-006, REQ-029–030; INV-001, INV-007 | V-TIME proves warning-free correct pruning; V-FACTS proves available, exactly-once local/remote facts. Memory-buffer and compressed-file bytes are labeled distinctly. |
| REQ-019, REQ-021, REQ-023–026; AC-014 | V-OTHER and V-DIAG close the existing optimization ledger: verification persistence, gateway capture, Bifrost point/Parquet tails, OTLP/write CPU, single-shard costs and approved shard comparisons. Each attempt has causal diagnostics, comparable before/after evidence, correctness proof and a measured keep/revert disposition. |
| REQ-011–014, REQ-019–020; AC-002, AC-005 (L1 only), AC-013 | V-L1 supplies five current full records, budgets, default steps, repetitions and denominators. V-REPORT explains historical compatibility; reuse or collect required same-build export-cost evidence. L2 is expressly deferred. |
| INV-001–007, INV-009–010; AC-010 | V-CHECKS and affected focused journeys preserve correctness, tenancy, durability, audit, privacy, cleanup and bounded evidence. Required red checks block completion regardless of their origin. |
| REQ-021; AC-014; maintainer direction | V-REPORT lists every miss/regression/unsettled result with diagnosis, attempted changes, keep/revert recommendation and explicit maintainer approval. No outstanding approval or required behavior may be labeled IMPLEMENTED. |

## Implementation Reference

Paths identify existing owners and inspected consumers, not a private-code
allowlist. Extend the whole workflow rather than duplicate orchestration.

- **Experiment standardization:** `capacity/main.rs` calls `levels::execute`;
  `levels.rs` assembles declarations and regenerates reports.
  `crates/wyrd/wyrd-testing/src/experiment.rs::{Declaration, Workload}` owns
  measurement, validity and history. Verification `verification/mod.rs::plan`
  and `run.rs::Bench::closed`, Gateway `gateway/mod.rs::plan` and
  `run.rs::Bench::step`, and Bifrost `query/mod.rs::plan` already participate.
  Their V-STANDARD tests are regression proof; close gaps in these owners.
  Here `capacity/` means `crates/wyrd/wyrd-testing/src/bin/capacity/`.
- **Actual query:** `capacity/query/isolated.rs::prepare_hot` learns a layout and
  places both sources; `query/telemetry.rs` builds literal-key COUNT filters.
  Preserve this default fixture. Trace `oracle/live.rs::LiveScribeExec`
  projection/leader filtering to `scribe/memtable.rs::ReadableSource::project`;
  the prior scalar-fragment test missed aggregate whole-column dependencies.
  These read owners are under `crates/vala/vala-bifrost-redux/src/`.
- **Staged/published reading:** `oracle/follower.rs::ScribeTailResolver::live_leaf`
  currently stats paths and supplies unknown facts; published
  `oracle/exec.rs::OracleTableProvider::persisted_input` uses `PlannedFiles`
  and `TenantWarehouseReads::reader_factory`. Both reach `hot_parquet_scan`,
  but only published uses cached warehouse reading. Extend that workflow with
  existing `RunFooters` facts from `LiveCapture::read` / `CapturedRead`; sharing
  one leaf call is not shared planning and reading.
- **Authority and completion consumers:** existing Oracle expression codec,
  dispatcher/follower, `wyrd-spec/src/vala/assignment_authority.rs`, `wyrd-tonic`
  private proto/conversion and completion accumulation change together. Keep
  revision 22's approved private projection representation/authority and opaque
  transport in wyrd-spec, without importing execution dependencies there.
- **Tests:** Oracle `published.rs` and `peer_network/security.rs` cover source
  transitions and remote predicates; Scribe `telemetry.rs` covers layout
  lifetime; follower/exec/live inline tests cover pruning, leases and facts.
  V-SQL closes real SQL/physical-work coverage in these existing targets.
  `scan_sizing.rs` and pinned DataFusion pruning own the time regression.
- **Verification persistence:** `verification/run.rs::Bench::closed` drives the
  SDK's execute route, then `VerificationControl::{execute, run_direct,
  stage_direct}` in the server. Existing `verification/results.rs::ResultPayloadBuilder`
  already writes inputs once, details plus payload-free summary, and encodes
  typed Variants directly. `scribe_outbox.rs::{Frame::frames, Part::dense}` and
  producer execution lanes already batch/sequence these writes. Revalidate
  these owners with V-OTHER and phase measurements; do not reimplement them.
- **Gateway capture:** benchmark `gateway/run.rs::Bench::step` reaches server
  `components/gateway/invocation.rs::GatewayInvocation`, settlement and
  `components/gateway/capture.rs::CallCapture`. Existing outbox projection
  batches calls. `redacted_canonical_json` feeds `payload_column` through JSON
  text, a candidate cost requiring profiling, not a proven bottleneck. Preserve
  redaction, binary extraction, size limits, validation and capture accounting.
  Settlement follows the answer: request latency alone cannot prove lower total
  capture CPU. V-OTHER covers these existing owners before any new correction.
- **Write optimization evidence:** reuse TASK-007 proof and retained phase
  timings for Scribe costs. REQ-025 locates `otlp_decode.rs::WireFields::read_varint`
  and `tables/signal.rs::finish_variant`; REQ-026 bounds shard/group-cycle work.
  Revalidate consumers before changes. No replacement queue, runtime or storage
  format is justified by this task.
- **Contract/docs closure:** update affected private constructors, conversions,
  authored consumers and generated outputs together; correct
  `architecture/bifrost-design.md` at the reader owner and `benchmarks/README.md`
  at metric definitions. Update TASK-007's outstanding evidence without erasing
  history. No public SDK/HTTP capability is introduced.

## Verification and Evidence

All commands run sequentially through mise. Diagnose with
`WYRD_LOG=info,vala_bifrost_redux=debug` and read traces. Reconfirm actual selectors
for planned tests before RED. Oracle/Scribe targets, repository Postgres setup
and journey profile are confirmed in Cargo/mise; no separate test harness.

### V-STANDARD — shared-pattern regression proof

The standardization audit and already-correct behavior need no manufactured RED.
If an identified gap needs executable changes, add its focused failing assertion
at the existing owner and record the exact selector/expected RED first.

```sh
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^experiment::tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::tests::level_one_declares_direct_sweep_and_strict_objective)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::tests::overhead_declares_every_case_at_every_client_count)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=gateway::run::tests::overhead_repetition_subtracts_waits_per_call_and_counts_capture)'
```

### V-OTHER — existing persistence and capture proof

These confirmed existing owner tests support revalidation, not a demand for new
production changes. Add a focused RED only after diagnostics identify a missing
behavior; preserve applicable existing result-returning and tenant-negative
journeys for any changed production workflow.

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::outcome_writes_output_then_summary)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::dense_projection_preserves_records_and_attribution)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::verification_lanes_keep_phase_order_across_retry)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::payload_capture_redacts_secrets_and_references_binary_content)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::oversized_payload_drops_capture)'
```

### V-SQL — real SQL and physical work

First three selectors are **planned** in confirmed existing targets: two SDK
journeys and an owner-level physical dependency test. Other selectors exist.
Add malformed/tampered projection refusal to these focused authority proofs.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::learned_attribute_sql_preserves_results_across_sources)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=peer_network::security::remote_learned_attribute_sql_preserves_results_and_authority)"'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::live::tests::attribute_count_sql_avoids_unused_variant_reconstruction)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::assignment_authority::tests::every_assignment_field_changes_the_digest)'
mise exec -- cargo nextest run --locked -p wyrd-tonic --lib -E 'test(=private_conversion::tests::follower_assignment_v2_contract)'
mise exec -- cargo nextest run --locked -p wyrd-tonic --lib -E 'test(=private_conversion::tests::follower_assignment_v1_shaped_peer_is_rejected)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::source_transitions_preserve_exact_rows)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=telemetry::variant_layouts_survive_memory_stage_publication_and_restart)"'
```

### V-READER — shared planning and reader

First selector is **planned**; the others exist. Prove separate scan-open/footer
counts, conservative bounds, object validation and lease release.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::staged_and_published_share_planning_pushdown_and_footer_reuse)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::scribe_staged_scan_prunes_non_matching_row_groups)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::a_dropped_staged_scan_releases_its_lease_immediately)'
```

### V-TIME — actual hot statistics

Wyrd selector is **planned** after rollback. If the pinned fork is the demonstrated
owner, revalidate already-pushed `8d6d6bf58` rather than invent another patch.
The fork selector below is **planned pending confirmation** in that checkout;
run its commands there only, with the same default-feature library target.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scan_sizing::tests::nanosecond_pruning_preserves_unknown_container_bounds)'
mise exec -- cargo nextest run --locked -p datafusion-common --lib -E 'test(=pruning::tests::mixed_timestamp_bounds_preserve_unknown_containers)'
mise exec -- cargo +nightly fmt
```

### V-FACTS — scan accounting

First selector is **planned**; other selectors exist.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::follower::tests::live_memory_scan_facts_survive_native_and_remote_completion)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::scan_stats_preserve_non_file_absence_and_scan_aggregation)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::live::tests::live_staged_read_reports_scanned_bytes_to_the_query)'
```

### V-DIAG — causal comparisons and optimization dispositions

Profiles, evidence audits and measurements are proof work, not new executable
behavior; no manufactured RED. Each resulting code fix first needs a focused
failing behavior test at its owner, its exact command and expected RED recorded,
then GREEN and REFACTOR. A performance target miss is not a correctness RED.

Before code changes, declare the one intended comparison variable and hold
logical rows, SQL, key, predicate, exact results, concurrency, build/host and
cache conditions fixed. For shredding, compare canonical and learned physical
representations of equivalent data.
For staged versus memory, query the same rows before and after staging. Capture
actual plans/profiles and file/batch/row-group/partition counts, leaf dependencies,
reconstruction, result rows, CPU/query and latency. Distinguish memory-buffer
bytes from compressed-file bytes. Attribute planning, metadata, decode,
reconstruction, filtering, transport and partitioning costs only where evidence
supports them; quantify remaining or unexplained cost rather than invent parity.

Isolate shredding's ingestion cost too: ingest the same stream through the same
production path with canonical versus learned physical retention, holding client
count, acknowledgement boundary, workload size and other settings fixed. Use
existing lifecycle/placement support; do not add a public layout selector or
change the default writes fixture. Compare CPU/row, acknowledgement latency,
throughput and peak/retained memory, including originals, typed leaves, residue
and construction scratch; verify exact accounting through stage/release. Link
compatible existing proof or collect this diagnostic pair and its controls.
Higher construction CPU/memory or slower writes is an explicit trade-off against
read savings requiring disposition, not something an aggregate writes record
can attribute to shredding. Measure the shared-reader change's ingestion/backlog
effect separately with equivalent staged write pressure.

Existing diagnostic command (not final qualification):

```sh
mise run bench:l1:bifrost:hot -- --profile --steps span-shredded-memtable-c1,span-shredded-staged-c1
```

Keep one ledger here for prior and new attempts: cost/source owner, diagnostic
artifact, baseline/candidate and controls, correctness proof, before/after metrics
and spread, protected workload effects, keep/revert recommendation and maintainer
disposition. Audit REQ-025's individual CPU changes and REQ-026's shard comparisons;
one combined write result is not their individual proof. Preserve WAL/fence/replay,
backpressure and accounting even if an older faster run omitted their cost.
Measure scan-accounting overhead independently. Separate profiled diagnosis from
normal unprofiled effect measurements. Repeat independent runs when noise makes
the decision unsettled; within-process repetitions do not establish between-run
noise, and no tolerance is invented after seeing results.

Reuse compatible Verification/Gateway evidence; otherwise locate the current
cost and collect its controlled before/after comparison. Reuse or collect the
required same-build ordinary-export versus disabled-export diagnostic comparison,
never subtracting it from final server costs. Do not change already-correct
production behavior to manufacture activity. Unknown causes and incompatible
baselines do not establish that a change was ineffective: finish available
diagnosis and make any remaining gap explicit in the requested disposition.

### V-CHECKS — affected correctness and static proof

Run focused tests, then affected owner checks. Required format/lint commands are
`mise run fmt` and `mise run lints`. Use `mise run codegen:check` for changed
contracts/generated consumers, `mise run check:deps` for tier/fork changes and
`mise run check:tenant-isolation` for affected query/tenant paths. Repair the
outstanding Bifrost failures and run `mise run verify:bifrost` once on the final
candidate. For Verification/Gateway production changes, inspect the actual write
set and record/run existing owning checks and each new test's focused command.
Unit suites cannot replace required journeys. Broad whole-change aggregates and
unaffected language suites belong to final change review, not this task.
Benchmarks remain manual and outside test gates.

### V-L1 — complete release-process evidence

Run for the restored baseline before optimization and for the accepted integrated
candidate, serially and exclusively on a quiet host. Build through existing bench
tasks: the measured server builds separately with only `bench`, preventing driver
test-support feature leakage. Final records must cover all five experiments on
the same candidate; later changes require replacement of affected records.

```sh
mise run bench:l1:bifrost:writes
mise run bench:l1:bifrost:hot
mise run bench:l1:bifrost:parquet
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:results
```

Require full defaults, clean unprofiled source, three repetitions, exact/integrity
checks and normal configuration. Budgets: 480 s writes, 300 s for each other
experiment. Preserve eligible records under either verdict in the existing
`benchmark-records` workflow; do not rewrite immutable results or commit/push
without authorization. Dirty, selected, smoke and profiled runs are diagnostic
only. An INVALID final run needs diagnosis and a valid replacement.

### V-REPORT — every step and explicit approval

Show all default steps, not selected winners: all recorded server/client latency
quantiles, CPU/work, work/CPU-second, throughput/core and vCPU, row/byte metrics
with their source definitions, relevant phases/backlog/storage, verification
tails and gateway capture. Retain each repetition, median/minimum/maximum/spread,
record paths, host/compiler, controls, denominators, budget and omissions. No
aggregate replaces step results. Regenerate reports and reconcile their values.

Separate restored-baseline and whole-packet comparisons; state compatibility:

| Reference | Permitted interpretation |
|---|---|
| Fresh restored `07a157cb9` baseline | Primary controlled before/after with fixed controls. |
| Hot `20261029T180639Z-471f69ff9`; Parquet `20261029T134546Z-459617bae` | Immediate pre-attempt context when declarations/controls match. |
| Early standardized Parquet `434aaf5c1`; writes `eaa8ad209` | Packet context for matching controls, not an invented pre-packet baseline. |
| Older hot `434aaf5c1` / `12fc33f08` | Different fixture, latter INVALID: recovery targets only, no like-for-like claim. |
| Reverted attempt `9a6e15750` | Historical failed-attempt observations, not current performance. |
| Missing Verification/Gateway historical records | Mark unavailable. Only obtain historical comparisons if the same contract runs without rewriting old production behavior. Fresh final records alone prove no packet-wide improvement. |

Preserve earlier goals alongside unchanged declaration targets: hot shredded
p99 and CPU/query improve for both sources; staged/memory ratios shrink without
slower memory; point memtable c1 recovers at least 861 queries/CPU-second and c64
server p99 at most 4.31 ms; matching Parquet steps do not regress latency, CPU or
scan bytes against `459617bae`. Historical point goals are not causal evidence.
Include the existing Verification, Gateway and OTLP targets unchanged.

Every miss/regression/unsettled result needs diagnosis, controlled attempts and
effectiveness, recommendation, remaining cost and explicit maintainer approval.
Targets remain met/missed/unsettled, independent of VALID/INVALID. **No completion
until the maintainer approves the remaining target misses and optimization
dispositions.** This does not silently waive correctness or required function.
Update TASK-007's open evidence and current task results without erasing history.
Independent task/change review still apply; deferred L2 work remains deferred.

## Maintainer revision — gateway capture backpressure (2026-10-29)

Directed by the maintainer after Stage 1, and executed ahead of Stages 2–4.

**Finding.** Gateway capture is the only Scribe outbox producer whose loss is
caused by load. Verification reserves a slot and refuses its caller
(`WYRD_VERIFICATION_429_STAGING_FULL`); Bifrost and the outbox writer retry
Scribe backpressure. Gateway capture calls `Outbox::stage` after the caller has
its answer (`invocation.rs:1057`). Since `2348d34d8` that sheds once the
unreserved share is full. `f7b962d9e` had already deleted the
retry-until-deadline capture writer that `wyrd-design.md` still describes. At
`chat-capture.c64`, 88% of handed-over outbox items were shed. The gateway
counted every capture as `staged` (`capture.rs:589`) because `stage` never
reports a shed item.

**Decision.**

1. A call whose capture policy is not `Disabled` reserves one Scribe outbox
   slot during admission, before the ledger charges it.
2. While the outbox refuses the reservation, admission backs off and retries
   until the call's deadline. The caller is slowed: this is backpressure.
3. If no slot is admitted by the deadline, the call is refused with the
   retryable `WYRD_GATEWAY_429_CAPTURE_STAGING_FULL` with
   `details.retry_after_seconds`, as gateway limit refusals already do.
   Nothing is charged and no provider is called.
4. The capture is staged on that slot, so it is never shed. Its remaining
   drops are capture's own counted reasons (payload, storage, deadline).
5. Audit staging is unchanged: audits never block.

`wyrd-design.md` changes with the code.

**Target.** The direct Verification benchmark sustains about 75,000 requests/s
through the same Scribe outbox without loss. About 10,000 requests/s with
shedding is not acceptable for Gateway, and the disk is not an accepted
explanation for capture loss. Gateway capture must lose nothing at every
declared step. Its throughput gap to Verification is a Stage 4 ledger item that
needs causal profiles, not a waiver.

Gateway-induced server latency targets the same bound as Verification:
overhead p99 < 500 µs at 64 clients for `chat`, `chat-stream` and
`chat-capture`. This is the existing declared `overhead-p99` criterion.
Baseline: 0.524, 0.239 and 4.04 ms. The capture-slot wait is part of
admission, so it counts toward overhead. Backpressure makes overload visible
as latency instead of hiding it as loss, but it does not meet the target:
the capture path itself must become cheap enough.

**Gateway profiles and labeling (Stage 4).**

- Record per-phase capture timing for request-content resolution, object
  persist (get, put, read-back), the admission wait and outbox staging-to-write,
  reusing the experiment's HDR timing owner.
- Record capture objects and bytes written per call.
- Label outbox loss and write counts by write kind (`audit`, `capture`,
  `result`, `inputs`, `spans`). This lets a record name which items were lost
  instead of one mixed `shed` total.
- Count admission refusals under the new code.
- Profile `chat-capture.c64` with `--profile` and attribute server CPU to those
  phases before proposing any capture optimization.

**Proof.**

- Focused RED, then GREEN: a full outbox makes a capturing call wait, then
  refuse with `WYRD_GATEWAY_429_CAPTURE_STAGING_FULL`. A slot freed before the
  deadline admits the call, and its capture is written, not shed.
- The existing V-OTHER gateway capture tests.
- `codegen:check` for the new error code.
- A clean `bench:l1:gateway` record compared with
  `gateway/overhead/20261030T010016Z-07a157cb9.json`.

### Gateway capture backpressure — implementation evidence

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| A capturing call reserves its slot at admission, waits until its deadline, then refuses with `WYRD_GATEWAY_429_CAPTURE_STAGING_FULL`, uncharged and undispatched | `invocation.rs` `GatewayInvocation::capture_slot`, `AdmittedCall::capture_slot`; `error.rs` `GatewayCaptureStagingFull` | RED: the call was admitted and dispatched (`GatewayDeadlineExceeded`). GREEN: `components::gateway::pg_invocation_tests::capturing_call_waits_for_a_capture_slot_and_refuses_at_its_deadline` | PASS |
| The capture is staged on its reserved slot, never shed | `GatewayInvocation::capture` | same test (a freed slot admits the call and its row is written); L1 `capture_shed` 0 | PASS |
| Existing gateway, outbox and HTTP error behavior holds | — | `components::gateway::`, `scribe_outbox::`, `http::error::` under test Postgres: 90/90; `wyrd-spec` error tests 49/49 | PASS |
| Contracts and docs | TS `error-codes.ts` regenerated; `api/errors.md` row; `wyrd-design.md` capture paragraph | `mise run codegen:check`, `mise run fmt`, `mise run lints`, `git diff --check` | PASS |

Commands:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=components::gateway::pg_invocation_tests::capturing_call_waits_for_a_capture_slot_and_refuses_at_its_deadline)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(/^components::gateway::/) | test(/^scribe_outbox::/) | test(/^http::error::/)'
```

Diagnostic L1: `bench:l1:gateway -- --allow-dirty` →
`target/benchmarks/gateway/overhead/20261030T015633Z-07a157cb9.json` (VALID,
dirty tree, so diagnostic only), against the clean baseline
`20261030T010016Z-07a157cb9`:

| Step | overhead p99 ms | req/s | shed | CPU µs/req |
|---|---|---|---|---|
| `chat.c1` | 0.202 → 0.200 | 5,371 → 5,437 | 0 → 0 | 234 → 231 |
| `chat.c64` | 0.524 → 0.513 | 50,134 → 50,503 | 0 → 0 | 386 → 389 |
| `chat-stream.c1` | 0.429 → 0.380 | 24 → 24 | 0 → 0 | 5,372 → 5,372 |
| `chat-stream.c64` | 0.239 → 0.242 | 1,550 → 1,547 | 0 → 0 | 971 → 981 |
| `chat-capture.c1` | 0.758 → 0.755 | 1,680 → 1,608 | 0 → 0 | 1,165 → 1,227 |
| `chat-capture.c64` | 4.04 → 429 | 10,166 → 3,428 | 90,998 → 0 | 2,088 → 2,999 |

What the diagnostic run shows:

- **Shedding is gone.** The loss became admission wait: payload
  `chat-capture.c64` sustains about 3.4k calls/s, and its overhead p99 is the
  backpressure wait.
- **Throughput is not the outbox.** Metadata capture (`chat.c64`) sustains
  about 50k calls/s through the same outbox with nothing shed.
- **Payload capture is expensive.** It now pays for every capture it writes:
  about 3.0 ms of server CPU per call, against 0.39 ms for metadata. That cost
  is the Stage 4 profiling target.
- **No 429 refusals occurred.** Public calls have a 5-minute deadline.

Not covered: no HTTP journey drives the 429. The public 5-minute deadline
makes it impractical without a test-only deadline control.

## Maintainer revision — gateway capture cost (2026-10-29)

Approved by the maintainer after the capture diagnosis below.

**Cases.** Each L1 case id is named for its answer delivery, message size,
and the tenant's `GatewayCaptureMode`. The id is also the benchmark case label
the server records. A new case isolates message size from payload capture:

| Case | Request and answer | Delivery | Stored per call | Isolates |
|---|---|---|---|---|
| `buffered-1kib-capture-metadata` | 1 KiB | one response | call record | baseline |
| `streamed-1kib-capture-metadata` | 1 KiB | 32 events | call record | streaming |
| `buffered-64kib-capture-metadata` (new) | 64 KiB | one response | call record | message size |
| `buffered-64kib-capture-payload` | 64 KiB | one response | call record, request and response text | payload capture |

The former ids were `chat`, `chat-stream` and `chat-capture`.

**Diagnosis.** Profiled run `20261030T024210Z` (VALID, dirty), at 64 clients:

| Case | req/s | CPU µs/call | overhead p99 | disk write |
|---|---|---|---|---|
| `buffered-1kib-capture-metadata` | ~51,000 | 384 | 0.50–0.53 ms | ~16 MB/s |
| `buffered-64kib-capture-metadata` | ~15,500 | 1,250 | 2.2 ms | ~8 MB/s |
| `buffered-64kib-capture-payload` | ~3,400 | ~3,400 | 428 ms | ~5 MB/s |

- **Request path.** At 1 KiB, the tracing subscriber stack takes about 95 µs per
  call, across about 12 spans, and every span reaches the OpenTelemetry layer
  through the tenant-catalog filter. The HTTP metrics middleware takes about
  30 µs. Upstream TCP connects in steady state take about 10 µs. Capture is
  about 26 µs.
- **Write pattern.** TASK-006-R1 kept the bulk capture projection.
  Verification and audit stage owned scalars and encoded Variant cells, built
  on the producer's task. Captures stage JSON text, and `CallCapture::project`
  decodes and Variant-encodes it inside the serial lane write
  (`ScribeSink::frames`). That is about 11% of server CPU in the payload case.
- **Scribe.** At 16 writers and lanes instead of 4, payload throughput stayed
  at about 3.5k/s while CPU per call rose to about 5,800 µs. Writes in flight
  went from 2.1 to 11.5, and each write went from 11 to 67 ms, mostly
  `shard_wait`, `wal_append` and `visibility`. The shard's commit stages use
  about 0.01 cores. Their time is the WAL thread waiting on write and
  `fdatasync`, the disk-sync limit TASK-006-R2 found. Verification was
  unchanged (68–72k/s). The change was reverted.
- **Workload.** The 64 KiB filler is `"x"` repeated, which compresses to almost
  nothing, so every disk and WAL figure for it is optimistic.

**Decision.**

1. The 64 KiB request and stub answer carry deterministic text that
   compresses like natural-language chat, not one repeated byte.
2. Request-path CPU is reduced at its owners. The upstream client reuses
   connections, and per-call span and metric cost is cut. Changing which spans
   the tenant span copy contains follows the owning `wyrd-design.md` section
   first.
3. This reverses TASK-006-R1's capture decision. A capture stages its row
   pre-encoded, as Verification does: owned scalars and encoded Variant cells
   built on the capture task, appended at drain without re-encoding. Redaction,
   canonicalization and the capture privacy policy are unchanged.
4. Not in this revision: storing large bodies as objects referenced from the
   row needs `$wyrd-spec`. It is considered only if steps 1–3 miss the target.
   Scribe, its shard count and its writer and lane bounds are unchanged.

**Target.**

- Overhead p99 < 500 µs at 64 clients for every case, including
  `buffered-64kib-capture-payload`.
- Every case loses nothing.
- Payload-capture throughput is reported as captured content bytes per second
  on the realistic workload. 75,000 calls/s at 128 KiB is about 9.6 GB/s, so
  Verification's call rate is not a target for payload capture.

### Gateway capture cost — implementation evidence

Run `20261030T034414Z` (gateway) and `20261030T034734Z`
(`direct-verify-c64`), both `--allow-dirty` under `target/benchmarks/`. Both
are INVALID only for `driver.saturated`: the driver used more than 6.4 of its
8 cores in `buffered-64kib-capture-metadata.c64` and `direct-verify-c64`. Every
correctness, capture-loss and shed criterion was met.

| Step (c64) | Before (`20261030T024210Z`) | After | Overhead p99 |
|---|---|---|---|
| `buffered-1kib-capture-metadata` | ~51k/s, 384 µs CPU/call, 0.50–0.53 ms | 55.2k/s, 358 µs | 0.492 ms, met |
| `streamed-1kib-capture-metadata` | — | 1.5k/s | 0.259 ms (end 0.252), met |
| `buffered-64kib-capture-metadata` | ~15.5k/s, 1,250 µs, 2.2 ms | 17.1k/s, 1,106 µs | 1.95 ms, missed |
| `buffered-64kib-capture-payload` | ~3.4k/s, ~3,400 µs, 428 ms | 3.9k/s, 2,824 µs | 427 ms, missed |
| `direct-verify-c64` | 69.3k/s, 0.428 ms | 72.8k/s | 0.352 ms, met |

Payload capture stores about 3,875 × 128 KiB ≈ 508 MB/s of captured content.

| Step | Implementation | Verification |
|---|---|---|
| 1. Realistic text | `GatewayCapture::prose` in `wyrd-testing/src/gateway_capture.rs`, used by the workload and the stub | capacity bin tests 50/50 |
| 2. Connection reuse | `POOL_MAX_IDLE_PER_HOST` in `skald-providers/src/endpoint.rs` | `wyrd-skald-providers` lib 48/48 |
| 2. Span cost | OTel layer without busy/idle, thread or location fields, in `wyrd-telemetry/src/lib.rs` | `wyrd-telemetry` lib 33/33 |
| 3. Pre-encoded capture | `CallCapture::{request,response}` encoded in `redacted_payload` on the capture task; `calls_batch` appends them and no longer serializes payload text | `components::gateway::` and `scribe_outbox::` 81/81 with Postgres; journeys `typed_builtin_payloads_are_queryable` and `variant_sql_registry_covers_every_session` pass; `mise run lints` clean |

Result: the 1 KiB and streamed cases meet the target, and Verification did not
regress. Steps 1–3 cut per-call CPU by 7–17%, but both 64 KiB c64 cases still
miss. The payload p99 is unchanged at 427 ms, so payload encoding on the lane
was not what limits it. Per item 4, the next step needs `$wyrd-spec` and
maintainer direction.

## Material Stop Conditions

Stop for required authority to change security, durability, logical results,
assignment/schema proof or lease semantics; public/persisted contracts,
dependencies/features or owners; default workloads/targets; prohibited machinery;
or approved functional outcomes. Revision 22's approved private projection
change is in scope with coordinated consumers and fail-closed proof. Missing
local services, red gates and recoverable failures require diagnosis and repair,
not a handoff. Rejecting an ineffective implementation does not waive the shared
reader/shredding outcome; bring measured evidence for any proposed spec decision.
Pending performance approval leaves acceptance pending. Do not begin L2 work.

## Authority Links

- [Approved revision 22](../spec.md), including L1-only delivery and the hot
  read-path supplement; current maintainer direction is recorded above.
- [Repository rules](../../../../AGENTS.md), [testing](../../../../TESTING.md),
  [spec-driven development](../../../spec-driven-development.md), and
  [implementation execution](../../../implementation-execution.md).
- [Bifrost authority](../../../../architecture/bifrost-design.md).

## Stage 1 evidence — restored baseline and diagnosis

Executed 2026-10-30 on `07a157cb9`, whose runtime is byte-identical to
`471f69ff9`: `git diff 471f69ff9 07a157cb9` touches only `architecture/`,
`spec.md` and the two task files, and the release server is 1,221,643,760 bytes
in both builds. No production or test code changed. The only uncommitted
tracked change, this task file, was moved aside for the five records so that
`git diff HEAD` was empty (each record says `dirty: false`), then restored
byte for byte (SHA-256 `4ff80f1e…9331`). This stage is not a task verdict.

### Controls

| Item | Value |
|---|---|
| Host | AMD Ryzen 9 9950X, 32 CPU / 91 GiB, Pop!_OS 24.04 x86_64; other-process load 0.87–0.96 cores before each run (limit 3.2) |
| Build | `mise run bench:build`: release, rustc 1.99.0 (b940084d7), `-C force-frame-pointers=yes`, line-tables-only; server built alone with `bench` |
| Order | serial and exclusive: writes → hot → parquet → verification → gateway, then two diagnostic hot runs; nothing else built or tested meanwhile |
| Declarations | every experiment ran full defaults (no `--steps`, `--profile`, `--smoke` or `--allow-dirty`); hot, parquet and writes controls, criteria and variables equal their reference records' |

### Obligation inventory and historical compatibility

| Obligation | State at the restored baseline | Owning stage |
|---|---|---|
| REQ-029–033 real-SQL shredding | Rolled back. Learned layouts are read, but both sources rebuild the whole `attributes` Variant (profiles below) | 2 |
| REQ-034 shared staged/published reader | Rolled back. Staged runs still use the uncached `HotObjects::Staged` reader | 3 |
| REQ-006 time pruning without warnings | RED reproduces: 19,104 `datafusion_common::pruning: Failed to convert min values to array for column start_time_unix_nano` WARNs in one hot diagnostic run; fork pin back at `19796a13e`, fix `8d6d6bf58` still pushed | 4 |
| REQ-029–030 memory scan facts | Absent: memtable steps report no `scanned_bytes_per_query` or rows | 4 |
| REQ-019/024 Gateway capture | Correctness criterion fails: `chat-capture.c64` sheds about 89% of captures; Gateway has no VALID L1 record | 4 |
| Verification L1 validity | Harness drift: `experiment.rs:2290-2298` still judges `driver.saturated` in full runs (added `aaa152dd4`), although revision 17 makes driver CPU context only (spec rule 6; REQ text "Driver CPU is never judged"). It is the only reason the baseline is INVALID | owner: `wyrd-testing` `experiment.rs` validity; not repaired in this stage |
| REQ-025/026 write CPU and shard comparisons | TASK-007's per-change proof (Task 9 table) is still the only per-change evidence; nothing new in this stage | 4 |
| `verify:bifrost` | Seven failures recorded on `3b7f9d5af` and `471f69ff9`; not rerun here (no code change) | 4/5 |
| TASK-007 revision 20 closeout | Its README and `bifrost-design.md` rows for the staged reader, live projection and memory facts no longer apply (already noted in TASK-007) | 3/4 |
| L2 | Deferred, untouched | — |

Historical comparisons follow V-REPORT. The hot reference
`20261029T180639Z-471f69ff9` runs the same binary as this baseline, so hot is
an **A/A pair**. Parquet's `20261029T134546Z-459617bae` and writes'
`20261029T120649Z-e4b65f4d0` have equal controls but 8 and 17 intervening code
commits respectively, so they are context, not attribution. Writes
`eaa8ad209` has different criteria, so it is packet context only. Verification
and Gateway have no record in `benchmarks/history`; their historical columns
are unavailable. `9a6e15750` is failed-attempt history only.

### Shared-owner audit

Every L1 engine runs one path: `main.rs::level` → `levels::base` (host,
source and build) → the engine `plan` (`query/mod.rs:462`,
`verification/mod.rs:207`, `gateway/mod.rs:178`) → `levels::select`/`smoke` →
`levels::execute` → `experiment::run` → `Record::publish`. The workloads
(`query/isolated.rs::Isolated`, `verification/run.rs::Bench`,
`gateway/run.rs::Overhead`) implement `experiment::Workload` and judge through
the shared `Reading::percentile` and `Population::percentiles`. No engine has
its own percentile, validity or record path. No gap was found and no change
was needed.

### Regression proof

| Command | Result |
|---|---|
| `cargo nextest run --locked -p wyrd-testing --lib -E 'test(/^experiment::tests::/)'` | 14/14 PASS |
| `-p wyrd-testing --bin capacity` (the three V-STANDARD selectors) | 3/3 PASS |
| `-p wyrd-server --lib` (the five V-OTHER selectors) | 5/5 PASS |

### L1 baseline records

All five are under `benchmarks/history/` (the uncommitted `benchmark-records` worktree).

| Experiment | Record | Verdict | Elapsed / budget | Reason |
|---|---|---|---|---|
| writes-parquet | `bifrost/writes-parquet/20261030T004200Z-07a157cb9.json` | VALID | 440.3 s / 480 s | — |
| hot | `bifrost/hot/20261030T004925Z-07a157cb9.json` | VALID | 221.2 s / 300 s | — |
| parquet | `bifrost/parquet/20261030T005310Z-07a157cb9.json` | VALID | 285.8 s / 300 s | — |
| verification | `verification/direct-verify/20261030T005903Z-07a157cb9.json` | **INVALID** | 69.2 s / 300 s | only reason: `driver.saturated` at `direct-verify-c64` (7.20, 7.21, 7.28 cores > 6.40). The approved spec never judges driver CPU, so this INVALID is harness drift; no condition the spec judges is broken |
| gateway | `gateway/overhead/20261030T010016Z-07a157cb9.json` | **INVALID** | 151.3 s / 300 s | `criterion.invalid` `chat-capture.c64/capture-shed`: 94,708, 90,998 and 89,788 items shed at the outbox limit (88–89% of outbox items) |

Both INVALID causes are the same ones TASK-006-R1 recorded, and both repeat
with tight spread in every repetition, so they are not host noise. They are
broken baseline measurements, kept visible for repair, and neither is treated as
valid. Performance values from those two records are shown below but
judge nothing.

Cells read `median (min–max)` over three repetitions; `ref` is the reference
median and its change. Scaling and peak rows are derived from the steps shown
and are omitted.

#### Hot, against the A/A reference `471f69ff9`

| Step | p99_ms | cpu_ms_per_query | queries_per_cpu_s | scanned_bytes_per_query |
|---|---|---|---|---|
| `point-memtable-c1` | 1.69 (1.67–1.73) · ref 1.54 +9.4% | 1.33 (1.32–1.34) · ref 1.26 +5.4% | 753.4 (745.8–758.5) · ref 793.9 -5.1% | — |
| `point-memtable-c64` | 5.33 (5.22–5.38) · ref 4.9 +8.8% | 1.77 (1.77–1.77) · ref 1.74 +1.4% | 565.3 (565.0–566.6) · ref 573.1 -1.4% | — |
| `point-staged-c1` | 3.33 (3.32–3.41) · ref 3.6 -7.5% | 2.89 (2.88–2.9) · ref 2.87 +0.8% | 346.1 (345.1–347.2) · ref 348.7 -0.8% | 222,512 (222,279–222,674) · ref 213,323 +4.3% |
| `small-aggregate-memtable-c1` | 3.13 (3.13–3.21) · ref 2.9 +7.9% | 2.6 (2.55–2.63) · ref 2.43 +6.6% | 385.3 (380.8–392.9) · ref 410.9 -6.2% | — |
| `small-aggregate-staged-c1` | 6.07 (5.99–6.16) · ref 5.62 +8.2% | 5.54 (5.52–5.54) · ref 5.22 +6.2% | 180.4 (180.4–181.1) · ref 191.6 -5.8% | 451,903 (451,841–451,935) · ref 439,677 +2.8% |
| `span-shredded-memtable-c1` | 36.2 (35.9–36.3) · ref 36.6 -1.3% | 186.2 (185.4–186.6) · ref 185.9 +0.2% | 5.37 (5.36–5.39) · ref 5.38 -0.2% | — |
| `span-shredded-staged-c1` | 93.1 (93.0–94.9) · ref 98.6 -5.5% | 262.5 (258.4–263.4) · ref 258.8 +1.4% | 3.81 (3.8–3.87) · ref 3.86 -1.4% | 19,709,951 (19,709,951–19,709,951) · ref 19,709,951 +0.0% |
| `span-long-tail-memtable-c1` | 23.2 (23.2–23.2) · ref 23.7 -2.3% | 98.8 (98.6–98.9) · ref 99.0 -0.3% | 10.1 (10.1–10.1) · ref 10.1 +0.3% | — |
| `span-long-tail-staged-c1` | 61.2 (60.6–62.4) · ref 59.7 +2.5% | 165.2 (165.2–165.2) · ref 164.0 +0.7% | 6.05 (6.05–6.05) · ref 6.1 -0.7% | 19,709,951 (19,709,951–19,709,951) · ref 19,709,951 +0.0% |

The same binary moved point and small-aggregate steps by 5–9% in p99 and up to
6.6% in CPU/query between these two runs. Staged scanned bytes moved by 2.8–4.3%:
the fixture's staged files are not byte-identical between runs. Span steps
agree within 2.5% (CPU within 1.4%). Two consequences: this host's between-run
noise on small queries exceeds most of the deltas the failed attempt reported
(for example point memtable CPU +3.1%), and no point or small-aggregate change
below about 9% can be attributed from one run per side. That is an
observation, not an invented tolerance. Effect measurements need repeated
independent runs on each side.

#### Parquet, against `459617bae` (context, 8 code commits apart)

| Step | p99_ms | p95_ms | cpu_ms_per_query | queries_per_cpu_s | scanned_bytes_per_query | scanned_rows_per_query |
|---|---|---|---|---|---|---|
| `point-c1` | 3.21 (3.19–3.41) · ref 2.89 +11.1% | 2.82 (2.78–2.96) · ref 2.54 +11.0% | 2.35 (2.35–2.43) · ref 2.16 +9.0% | 424.6 (411.7–426.0) · ref 462.9 -8.3% | 197,552 (197,457–197,717) · ref 181,370 +8.9% | 1 (1–1) · ref 1 +0.0% |
| `point-c64` | 12.6 (12.4–12.9) · ref 10.8 +17.0% | 10.1 (9.99–10.3) · ref 8.75 +15.8% | 3.78 (3.77–3.84) · ref 3.5 +8.0% | 264.5 (260.3–264.9) · ref 285.6 -7.4% | 197,567 (197,563–197,582) · ref 181,411 +8.9% | 1 (1–1) · ref 1 +0.0% |
| `small-aggregate-c1` | 6.16 (6.09–6.31) · ref 5.78 +6.6% | 5.67 (5.64–5.75) · ref 5.35 +6.0% | 5.38 (5.31–5.39) · ref 5.08 +6.0% | 185.9 (185.7–188.5) · ref 197.0 -5.6% | 516,397 (516,350–516,926) · ref 496,285 +4.1% | 100,031 (100,031–100,031) · ref 100,031 +0.0% |
| `full-scan-c1` | — | 51.7 (51.4–52.5) · ref 52.7 -1.7% | 465.8 (464.0–471.8) · ref 467.4 -0.4% | 2.15 (2.12–2.16) · ref 2.14 +0.4% | 248,512,511 (248,512,511–248,512,511) · ref 248,512,511 +0.0% | 10,002,431 (10,002,431–10,002,431) · ref 10,002,431 +0.0% |
| `span-trace-c1` | 4.62 (4.58–5.24) · ref 4.37 +5.7% | 4.15 (4.11–4.47) · ref 3.98 +4.3% | 3.98 (3.97–4.1) · ref 3.8 +4.9% | 251.1 (243.7–252.2) · ref 263.5 -4.7% | 687,587 (686,641–690,693) · ref 688,315 -0.1% | 8 (8–8) · ref 8 +0.0% |
| `span-trace-c64` | 25.4 (25.3–26.1) · ref 23.6 +7.6% | 21.3 (21.3–21.6) · ref 20.0 +6.8% | 7.08 (7.07–7.12) · ref 6.93 +2.2% | 141.3 (140.5–141.5) · ref 144.4 -2.1% | 689,673 (689,556–689,743) · ref 689,045 +0.1% | 8 (8–8) · ref 8 +0.0% |
| `span-shredded-c1` | 17.3 (17.1–17.5) · ref 16.9 +2.2% | 16.2 (15.9–16.3) · ref 16.3 -0.7% | 75.2 (74.6–75.9) · ref 75.5 -0.4% | 13.3 (13.2–13.4) · ref 13.2 +0.4% | 18,006,015 (18,006,015–18,006,015) · ref 18,006,015 +0.0% | 62,527 (62,527–62,527) · ref 62,527 +0.0% |
| `span-long-tail-c1` | 37.9 (37.8–38.8) · ref 37.5 +1.2% | 36.7 (36.3–37.0) · ref 36.1 +1.6% | 198.4 (197.5–198.6) · ref 194.9 +1.8% | 5.04 (5.04–5.06) · ref 5.13 -1.7% | 29,442,047 (29,442,047–29,442,047) · ref 29,442,047 +0.0% | 977.0 (977.0–977.0) · ref 977.0 +0.0% |

Point lookups scan 8.9% more bytes (197.6 kB vs 181.4 kB) for the same one row.
Without a same-code repeat of `459617bae` this cannot be split into code
change versus fixture layout; the hot A/A pair shows staged bytes moving by
4% on identical code.

#### Writes-parquet, against `e4b65f4d0` (context, 17 code commits apart)

| Step | rows_per_s | cpu_us_per_row | p99_ms | wire_mib_per_cpu_s | peak_mib | memtable_backlog_mib | staged_backlog_mib |
|---|---|---|---|---|---|---|---|
| `otlp-spans-c1` | 42,589 (40,235–49,758) · ref 44,638 -4.6% | 16.1 (12.6–18.5) · ref 15.3 +5.5% | 18.1 (13.5–19.9) · ref 15.2 +19.0% | 20.0 (17.4–25.6) · ref 21.1 -5.2% | 1,940 (1,116–1,959) · ref 1,950 -0.5% | 433.6 (91.6–475.4) · ref 489.7 -11.4% | 42.4 (42.4–84.8) · ref 75.7 -43.9% |
| `otlp-spans-c64` | 279,818 (272,453–389,965) · ref 264,414 +5.8% | 20.8 (20.8–21.9) · ref 20.1 +3.8% | 209.6 (185.2–225.7) · ref 224.0 -6.4% | 15.5 (14.7–15.5) · ref 16.1 -3.6% | 3,156 (2,949–3,205) · ref 2,930 +7.7% | 685.9 (590.7–1,028) · ref 576.5 +19.0% | 493.2 (402.4–584.3) · ref 722.7 -31.8% |
| `otlp-spans-1k-c1` | 21,500 (20,528–21,706) · ref 22,266 -3.4% | 39.6 (39.4–56.2) · ref 40.6 -2.3% | 28.5 (26.5–33.2) · ref 29.4 -3.2% | 25.4 (17.9–25.5) · ref 24.8 +2.3% | 2,483 (2,479–2,519) · ref 2,603 -4.6% | 445.4 (360.2–538.0) · ref 409.8 +8.7% | 45.9 (23.0–81.4) · ref 105.3 -56.4% |
| `otlp-spans-1k-c64` | 133,691 (132,035–135,840) · ref 122,109 +9.5% | 47.3 (46.2–47.5) · ref 45.5 +3.8% | 328.5 (314.0–329.5) · ref 328.5 +0.0% | 21.3 (21.2–21.7) · ref 22.1 -3.7% | 3,626 (3,601–3,679) · ref 3,607 +0.5% | 861.2 (776.3–911.4) · ref 991.7 -13.2% | 260.0 (184.0–316.1) · ref 352.4 -26.2% |
| `otlp-logs-c1` | 41,259 (37,361–43,499) · ref 42,886 -3.8% | 19.8 (19.4–20.3) · ref 18.2 +8.7% | 17.0 (16.9–23.3) · ref 21.4 -20.9% | 60.3 (58.7–61.6) · ref 65.5 -8.0% | 3,294 (3,266–3,297) · ref 3,241 +1.6% | 565.5 (487.7–617.9) · ref 513.1 +10.2% | 181.1 (120.8–241.4) · ref 339.7 -46.7% |
| `otlp-logs-c64` | 275,558 (260,490–366,349) · ref 276,968 -0.5% | 27.6 (27.2–28.9) · ref 27.1 +1.8% | 219.8 (150.5–247.5) · ref 258.1 -14.8% | 43.3 (41.3–43.8) · ref 44.1 -1.8% | 4,867 (4,181–5,011) · ref 4,688 +3.8% | 1,394 (1,372–1,700) · ref 1,394 +0.0% | 1,877 (1,403–2,242) · ref 3,509 -46.5% |
| `native-write-c1` | 853,472 (843,598–869,807) · ref 764,986 +11.6% | 1.56 (1.54–1.64) · ref 1.32 +18.7% | 164.8 (71.4–191.8) · ref 192.0 -14.2% | — | 2,942 (2,847–3,024) · ref 2,433 +20.9% | 702.4 (602.9–923.0) · ref 591.8 +18.7% | 124.2 (82.9–124.3) · ref 159.5 -22.2% |
| `native-write-c64` | 2,522,490 (2,159,054–2,743,677) · ref 1,533,163 +64.5% | 2.41 (2.31–2.65) · ref 1.86 +30.0% | 1,211 (1,183–1,623) · ref 1,709 -29.1% | — | 5,968 (5,873–7,283) · ref 4,781 +24.8% | 2,646 (2,458–3,331) · ref 1,574 +68.1% | 543.3 (315.3–692.2) · ref 914.3 -40.6% |

#### Verification (no historical record)

| Step | overhead_p99_ms | p99_ms | cpu_us_per_verification | verifications_per_cpu_s | phase_stage_p99_ms | lost_writes |
|---|---|---|---|---|---|---|
| `direct-verify-c1` | 0.153 (0.15–0.16) | 0.154 (0.152–0.162) | 130.4 (126.9–145.7) | 7,668 (6,864–7,880) | 0.0415 (0.0402–0.0421) | 0 (0–0) |
| `direct-verify-c64` | 0.445 (0.428–0.455) | 0.45 (0.43–0.458) | 253.1 (244.5–258.5) | 3,951 (3,868–4,089) | 0.0768 (0.0743–0.0774) | 0 (0–0) |

#### Gateway (no historical record)

| Step | overhead_p99_ms | p99_ms | cpu_us_per_request | requests_per_cpu_s | capture_staged | capture_shed |
|---|---|---|---|---|---|---|
| `chat.c1` | 0.202 (0.201–0.203) | 0.29 (0.289–0.291) | 233.8 (233.7–241.0) | 4,277 (4,150–4,280) | 26,862 (26,143–26,921) | 0 (0–0) |
| `chat.c64` | 0.524 (0.522–0.528) | 1.63 (1.61–1.65) | 385.7 (384.1–392.4) | 2,593 (2,549–2,604) | 251,010 (249,259–253,468) | 0 (0–0) |
| `chat-stream.c1` | 0.429 (0.392–0.716) | 0.751 (0.717–0.948) | 5,372 (5,372–16,198) | 186.2 (61.7–186.2) | 122.0 (122.0–122.0) | 0 (0–0) |
| `chat-stream.c64` | 0.239 (0.236–0.362) | 0.483 (0.472–0.605) | 970.7 (965.6–981.6) | 1,030 (1,019–1,036) | 7,817 (7,800–7,842) | 0 (0–0) |
| `chat-capture.c1` | 0.758 (0.729–0.796) | 0.991 (0.973–1.07) | 1,165 (1,151–1,553) | 858.1 (643.9–868.8) | 8,402 (8,230–8,505) | 0 (0–0) |
| `chat-capture.c64` | 4.04 (3.93–4.2) | 8.18 (7.79–8.5) | 2,088 (1,957–2,095) | 478.9 (477.2–511.1) | 50,950 (50,631–53,686) | 90,998 (89,788–94,708) |

#### Target status at the baseline

Targets are reported independently of VALID/INVALID; a target in an INVALID
record judges nothing.

| Target | Baseline | Status |
|---|---|---|
| Hot `point-memtable-c64` p99 < 5 ms (declared) | 5.33 ms | missed |
| Point memtable c64 p99 ≤ 4.31 ms; c1 ≥ 861 q/CPU-s (recovery goals) | 5.33 ms; 753.4 | missed; missed |
| Parquet `point-c64` p99 < 5 ms (declared) | 12.6 ms (ref 10.8 ms, also missed) | missed |
| Writes OTLP floor ≥ 30 wire MiB/CPU-s at `otlp-spans-peak` | 15.5 | missed; native floor (≥ 200k rows/s) met at 2.52M |
| Verification overhead p99 < 0.5 ms at c64 | 0.445 ms | met; the record's only invalidity is the drifted driver check, so the target settles once that check is removed and the run repeated |
| Gateway added overhead p99 < 0.5 ms at c64 | chat 0.524, chat-stream 0.239, chat-capture 4.04 ms | missed (chat, chat-capture); record INVALID |
| Staged/memtable ratios, shredded / long-tail (to shrink later) | p99 2.57 / 2.64; CPU/q 1.41 / 1.67 | baseline |

### Hot plans and profiles

**Plan rendering is a missing observation.** Oracle accepts exactly one
`SELECT` (`oracle/mod.rs:3438`), so there is no `EXPLAIN`. The only plan render,
`wyrd::oracle::analytical` at debug (`analytical.rs:7241`), covers analytical
multi-stage graphs. A diagnostic hot run with
`WYRD_LOG=info,wyrd::oracle::analytical=debug` over the six span and
small-aggregate steps (`target/benchmarks/bifrost/hot/20261030T010503Z-07a157cb9.json`,
VALID, selected) logged no plan. Rendering an interactive query's executed
plan needs a debug render at Oracle's interactive execution owner
(`oracle/mod.rs`). That is instrumentation, so it is not added in this stage.
The plan below is the executed operator chain read from the profile's call
stacks, checked against the source.

`SELECT COUNT(*) FROM spans WHERE start_time_unix_nano >= … AND < … AND attributes ->> 'region' = 'eu-west'`
executes on both sources as:

```text
AggregateExec COUNT (leader)
  FilterExec  time bounds AND variant_get(attributes, 'region') = 'eu-west'   (leader keeps it: live.rs:362-368, 570-572)
    LiveScribeExec  required_columns = closure incl. whole `attributes`; predicate shipped to the Scribe
      ScribeFragmentExecutor → ScribeTailResolver::resolve                       (in-process peer; IPC 0.1-0.3%)
        memtable: FetchLiveTailService::open_capture → LiveCapture::read → ReadableSource::project
        staged:   FileStream → PushDecoderStream (hot Parquet, uncached reader)
```

Profile: `mise run bench:l1:bifrost:hot -- --profile --steps span-shredded-memtable-c1,span-shredded-staged-c1`
→ `target/benchmarks/bifrost/hot/20261030T010341Z-07a157cb9.json` (VALID,
diagnostic), capture `…/20261030T010341Z/profiles/profile-978803.json.gz`,
symbolicated against `target/profiling/release/wyrd-server` with `nm`. The
measured window of each step (warmup excluded) is weighted by on-CPU samples:
87,061 samples (≈87 s) for memtable, matching the record's 29 s per 5 s
repetition, and 44,011 for staged. Shares are inclusive unless marked self.
The profiled run measured 190 / 267 ms CPU per query (unprofiled baseline:
186.2 / 262.5).

| `span-shredded-memtable-c1` cost | Share |
|---|---|
| Scribe capture read `LiveCapture::read` | 70.9% |
| └ whole-`attributes` reconstruction, `ReadableSource::project` → `schema::logical_column` → `unshred_variants` | 48.7% |
| └ capture pre-filter `batch_filter`: `variant_get` on typed leaves 3.8%, filtering every physical column of the learned struct (`filter_record_batch`) 14.6% | 20.3% |
| Leader `FilterExec` re-evaluating `variant_get` on the rebuilt Variants (outside the capture) | 17.5% |
| Leader aggregate and the rest of the stream (outside capture and filters) | 13.1% |

| `span-shredded-staged-c1` cost | Share |
|---|---|
| Hot Parquet decode `ParquetRecordBatchReader` (whole `attributes` struct reader 47.5%; ZSTD 20.9%) | 51.9% |
| Whole-Variant reconstruction in the decoder projection (`DecoderProjection` → `CastExpr` → `cast_to_variant_field` → `unshred_variant`) | 31.8% |
| Row-group reader build, including row-filter evaluation `with_predicate_options` 12.2% | 14.0% |
| Footer and metadata reading | 1.7% |
| Leader `FilterExec` outside the scan | 5.0% |
| Cached reader frames (`CachedParquet…`, `FileMetadataCache`, `RangeCache`) | 0.0% |

### Supported findings, hypotheses and evidence gaps

Supported by the evidence above:

1. **Both sources rebuild the whole `attributes` Variant to answer a
   one-leaf `COUNT(*)`.** Memtable: 48.7% of server CPU, plus 17.5% re-filtering
   the rebuilt values on the leader. Staged: 31.8%, on top of decoding the
   whole shredded struct. This is the cost the rollback note predicted, now
   measured on the real query shape.
2. **The staged/memtable p99 gap is CPU times parallelism.** Unprofiled cores
   used: shredded memtable 5.78 against staged 2.94; long-tail 4.79 against 2.87.
   CPU/query ÷ cores reproduces each p50: 186.2/5.78 = 32.2 ms against 31.5;
   262.5/2.94 = 89.3 ms against 86.8. So the shredded p99 ratio of 2.57 is the
   1.41 CPU ratio times about 1.97 lower parallelism.
3. **Same-code noise.** The hot A/A pair moves small-query p99 by up to 9% and
   CPU by up to 6.6%, and staged bytes by up to 4.3%. Effect claims below that
   size need repeated independent runs.
4. **The time-pruning warning is live on the hot path** (19,104 WARNs in one
   diagnostic run).
5. **Gateway's baseline is a broken measurement**, not noise; Verification's
   INVALID is harness drift (driver CPU is judged against revision 17). Gateway capture
   durably stages only 1.3–1.7k items/s (82–105 MiB/s) against about 10k
   offered/s at `chat-capture.c64`.

Hypotheses (not causal conclusions):

- H1: projecting the scalar `attributes ->> 'region'` (and, for `COUNT(*)`,
  no Variant column) into the live fragment removes most of finding 1's
  reconstruction and leader re-filter. The profile shares bound the memtable
  saving at about 66% and staged's at about 32% plus the unread leaves' share of
  the 47.5% struct decode. Stage 2 must measure the saving; this bound is not
  a promise.
- H2: staged's lower parallelism comes from the hot-Parquet scan's file and
  partition grouping (`oracle/exec.rs::hot_parquet_scan`, shared planning in
  Stage 3), not from the Scribe.
- H3: the shared cached reader's direct saving on this step is small: footer
  and metadata cost 1.7%, and the row-group builder 14% is mostly predicate
  evaluation. Its value is in planning and pruning parity, so Stage 3 must
  measure it separately and must not be credited with Stage 2's savings.
- H4: the parquet point-lookup byte increase (+8.9%) and writes' native CPU/row
  increase (+18.7% c1, +30% c64, with +64.5% c64 throughput) are
  code-or-fixture effects of the intervening commits. They are unattributed.

Missing observations and their owners:

| Missing observation | Needed for | Owner |
|---|---|---|
| Executed physical plan of an interactive query | Plan proof without reading stacks | Oracle interactive execution, `oracle/mod.rs` |
| Same-key canonical vs learned comparison: the same spans, key `region`, predicate, bounds and expected count, in canonical and in learned layout | Isolating shredding's read benefit (V-DIAG); different-key bytes cannot prove it | Existing lifecycle: `capacity/query/isolated.rs::prepare_hot` places spans only after the layout is learned. Loading an equivalent span range before the sample publish keeps it canonical. Stage 2 must add that diagnostic without changing the default declaration, or use the Scribe journey `telemetry::variant_layouts_survive_memory_stage_publication_and_restart` for physical assertions |
| Shredding's ingestion cost (CPU/row, ack latency, retained memory for canonical vs learned retention of one stream) | V-DIAG ingest trade-off | Scribe memtable retention plus the same diagnostic placement; no current record separates it |
| Staged per-query file, row-group and partition counts | Confirming H2 | `oracle/exec.rs::hot_parquet_scan` scan metrics |
| Memtable scanned rows and bytes | REQ-029–030 facts | Stage 4, `scribe/memtable.rs` and completion facts |
| Same-code repeat for parquet and writes references | Attributing H4 | Repeat runs at `459617bae` / `e4b65f4d0` if the comparison matters; otherwise report it as unattributed |
| Profiles of the long-tail and small-aggregate steps | Extending findings 1 and 2 beyond the shredded pair | Same `--profile --steps` command; not taken in this stage |

### Proposed first implementation change (Stage 2)

Scenario 1 at the existing owners. Start with a V-SQL RED on the benchmark
query shape, `COUNT(*)` with a time range and an `attributes ->> 'region'`
predicate, over learned memtable and staged data. Pair it with the planned
`oracle::live::tests::attribute_count_sql_avoids_unused_variant_reconstruction`
owner assertion that today's plan requests whole `attributes` and rebuilds it.
Then carry revision 22's ordered projection expressions through
`LiveScribeExec` → assignment authority → follower, so that each source
evaluates `variant_get` on the typed leaf and returns only the scalar (or no
column for a predicate-only `COUNT`). Measure against this baseline with the
same-key canonical/learned diagnostic above, plus repeated independent runs
sized to the A/A noise. Do not change the staged reader in that stage.

### Stage 1 exit

| Required evidence | Location | Result |
|---|---|---|
| Paths, candidate, host, build, controls for all five experiments | Controls; L1 baseline records | PASS |
| Per-step baseline results and invalidity | Per-experiment tables; INVALID reasons | PASS (two experiments INVALID, kept visible) |
| Shared-owner audit | Shared-owner audit | PASS, no gap |
| Hot plans and profiles | Hot plans and profiles | PASS for the shredded pair; the interactive plan render is a missing observation |
| Supported costs, hypotheses and gaps kept separate | Findings | PASS |
| Proposed first implementation change | Proposed first change | PASS |
| No production or test code changed | `git status`: only this task file is modified | PASS |

Stopped at the Stage 1 boundary. Nothing was committed or pushed, and no
maintainer approval is implied. The new records sit uncommitted in the
`benchmark-records` worktree.

## Historical Attempt — superseded execution evidence

The following records the rolled-back attempt. Old implementation claims,
commands and acceptance wording are historical, not current instructions.
They do not override the revised evidence-and-approval process above.

## Evidence

### Diagnoses

| Failure | Symptom | Evidence | Cause | Fix site |
|---|---|---|---|---|
| `ingest_bounds::native_ingest_bounds_expanded_bytes_not_rows` | `SELECT count(*)` failed with query execution failed | the live fragment carried an empty projection; the projection wire rejects an empty list | `LiveUnionProjection`, now on every Oracle session, let the live leaf absorb the zero-column `count(*)` projection | `LiveScribeExec::try_swapping_with_projection` refuses empty projections, so they stay on the leader; checked against every swap caller in `oracle/live.rs` |
| `distributed::remote_staged_footer_refusal_fails_closed` | `QUERY_EXECUTION_FAILED` where `QueryTenantInvariant` was expected | `peer_service` WARN `ParquetError(... "Corrupt footer")`; `HotFileSource::staged` sizes from `run.bytes` (`oracle/exec.rs:1380`) | the fixture re-encoded the run at a new length; step 2 plans with the recorded size, so the footer read fell inside the file before the tenant proof ran. Independent read-only diagnosis agreed: the fixture is at fault, and production matches step 2 | `rewrite_footer_tenant` now overwrites the owner id with an equal-length foreign id inside the footer, so only the tenant proof can fail. No other fixture rewrites a staged run |
| `published::source_transitions_preserve_exact_rows` | 2 cut reads where 1 was expected; 2 of 6 runs failed | trace: Forge's case-1 promotion commit at 39.602; warm query served a stale cached cut at 39.604; NOTIFY eviction; held query read twice | a latent test race, not caused by this diff. The warm-up exited on a cache hit while Forge's promotion NOTIFY was still in flight (independent read-only diagnosis) | the warm-up first waits until `file_tier_counts` reports no hot file left to promote; 8 of 8 runs pass |

### Step 5: memory scan facts

Reuse map. The capture already owns the memory read (`ReadableSource::project`). Facts ride the existing chain `CapturedRead` → `LiveTailBatches` → `MemtableSource` → `OracleQueryScanStats::visit` → `FollowerScanEvidence::finalize` → `WorkerScanStats` → `RemoteScanMetrics`, so no new channel was added. Leaf selection reuses the fork's public `ScalarFunctionExpr::required_input_fields`, the same declaration the Parquet projection mask uses.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Memory rows are counted after bounds pruning and before filtering; bytes are distinct buffers of the declared leaves | `MemoryScan`, `BufferTally`, `ReadableSource::project` in `scribe/memtable.rs` | `oracle::follower::tests::live_memory_scan_facts_survive_native_and_remote_completion`: 3 rows; scalar bytes < whole `attrs` bytes. A mutation that disables leaf selection fails it (206 of 206) | PASS |
| A filter-emptied capture keeps its facts; generic memory stays absent; an instrumented empty scan reports 0 | `live_leaf` builds the leaf when `scanned.rows > 0`; `LiveTailBatches::from_snapshot` is `None` | `oracle::exec::tests::scan_stats_preserve_non_file_absence_and_scan_aggregation` | PASS |
| Native and remote completions carry identical facts; a refused completion records nothing; repeated finalization does not double count | `WorkerScanStats.rows_scanned`, proto tag 8, `private_conversion.rs` | follower test above; `oracle::live::tests::live_staged_read_reports_scanned_bytes_to_the_query`; wyrd-tonic footer round-trip | PASS |
| Mixed memory/staged/remote sums once; a participant without rows leaves rows unavailable | `OracleQueryScanStats::finalize`, `RemoteScanMetrics::terminal_values` | exec aggregation test (mixed: bytes 42, rows `None`) | PASS |
| The metric documentation states that memory bytes are scan work | `WorkerScanStats` rustdoc, proto comment, `architecture/bifrost-design.md` Traces | `mise run lints`, `mise run codegen:check` | PASS |

Lanes:
- `cargo nextest run -p vala-bifrost-redux -p wyrd-tonic -p wyrd-spec --lib` under test Postgres: 1697 of 1698 pass. The one failure is the step-4 V-TIME test, which waits on the fork pin.
- Five journeys pass: remote shredded projection, the two shredded and layout telemetry journeys, staged footer refusal, and native ingest bounds.

### Step 4: V-TIME

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Unknown and inexact containers keep min/max pruning of nanosecond UTC bounds, with no "Failed to convert min values" warning | Fork `datafusion/common/src/pruning.rs`, commit `8d6d6bf58`: `PrunableStatistics::get_exact_column_statistics` fills unknown containers with a null typed by the schema, replacing the untyped `ScalarValue::Null` that `iter_to_array` rejected | fork `pruning::tests::mixed_timestamp_bounds_preserve_unknown_containers` fails before the fix and passes after it; `scan_sizing::tests::nanosecond_pruning_preserves_unknown_container_bounds` fails on `19796a13e` and passes on `8d6d6bf58` | PASS |
| One conversion rule at the demonstrated owner | The fix is at the fork owner. Wyrd's statistics types were already consistent, so there was no workaround to remove. The 11 pins, `Cargo.lock` and `ORACLE_DATAFUSION_REVISION` now point to `8d6d6bf58` | `oracle::codec::tests::plan_fingerprint_binds_the_pinned_datafusion_revision`; lib suite 1698/1698 under test Postgres; `mise run lints` | PASS |

Cache validation (step 1) is confirmed:
- `CachedFileMetadataEntry::is_valid_for` compares `ObjectMeta` size and `last_modified`.
- Staged run paths are unique per assembly key, shard and generation, and a lane's generations only move forward.
- The caches are process-local.

Path keys therefore never alias a different run. The limit is that a run rewritten in place at its recorded size would be served from a stale cache entry; that is outside the staging contract.

### Closeout gate

`mise run verify:bifrost` on `3b7f9d5af` is **red with seven failures. All seven predate this task.** Each one was rerun alone on HEAD and on the pre-task base `471f69ff9`, with the same result on both:

| Test | HEAD | Base | Read |
|---|---|---|---|
| `wyrd-testing` lib `bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot` | FAIL 322122546 vs 161061273 | FAIL, same | Stale assertion. It expects only the footer-cache charge; `storage/mod.rs:163` also charges the byte-range budget |
| `pg_verification_runtime::attempts_record_queue_wait_phases_terminal_latency_and_one_trace` | FAIL (2 queue waits vs 1) | FAIL | Deferred attempt at `verification/runner.rs:573` |
| `pg_verification_runtime::two_replicas_claim_each_queued_run_exactly_once` | fails only in the full lane; passes alone | same | Ordering within the lane |
| `verifier_building_blocks::custom_verdict_data_survives_readback` | FAIL "No field named outcome" | FAIL | Stale query; `bifrost.verification.results` has no `outcome` column |
| `verify_in_real_time::verification_staging_refuses_and_recovers` | FAIL `[6]` vs `[5]` | FAIL | One extra `execution_inputs` row |
| `server` `query::generated_grpc_and_scheduled_queries_share_audit_terminal_and_cleanup` | FAIL 0 vs 1 (`query.rs:877`) | FAIL | Predates this task |
| `server` `owner_inspection::failed_wal_replay_is_role_local` | FAIL (`owner_inspection.rs:365`) | FAIL | Predates this task |

Every Bifrost read lane this task touches is green: oracle 62/62, scribe 42/42, forge 24/24. The seven failures are in the verification runtime, WAL replay, the audit journey and stale SDK/cluster tests, all outside TASK-008's write set, and are left for their owners.

### Step 6: exclusive benchmarks

Both runs were exclusive: nothing else built, tested or loaded the host, and the tree was clean at each start. The records are committed on `benchmark-records` (`3412ffce9`, not pushed).

| Run | Record | Verdict | Measured elapsed / budget |
|---|---|---|---|
| `mise run bench:l1:bifrost:hot` | `hot/20261029T223134Z-9a6e15750.json` | VALID, clean | 219.2 s / 300 s |
| `mise run bench:l1:bifrost:parquet` | `parquet/20261029T223613Z-9a6e15750.json` | VALID, clean | 284.8 s / 300 s |

Host and build match every reference record: AMD Ryzen 9 9950X, 32 CPU / 91 GiB, Linux, rustc 1.99.0 (b940084d7), release, `-C force-frame-pointers=yes`, line-tables-only. Other-process load before and after each run was 0.52–0.65 cores, under the 3.2-core limit. The server binary differs only by this task's code and the fork repin (1,226,533,368 bytes vs 1,221,643,760 in 471f69ff9). The hot controls are identical to 471f69ff9's, and the parquet controls are identical to 459617bae's.

Spread is the range across the three repetitions. A "—" for historical memory bytes or rows means the reference record never had them; no estimate is substituted. Memory bytes are distinct Arrow buffer bytes of the declared leaves (step 5), so they measure scan work, not storage, and are not comparable with staged Parquet bytes.

#### Hot, against `hot/20261029T180639Z-471f69ff9.json`

| Step | Metric | Repetitions | Median | Spread | Reference median | Δ |
|---|---|---|---|---|---|---|
| `point-memtable-c1` | p99 ms | 1.608 / 1.582 / 1.640 | 1.608 | 0.058 | 1.542 | +4.2% |
| `point-memtable-c1` | CPU ms/q | 1.298 / 1.299 / 1.298 | 1.298 | 0.001 | 1.260 | +3.1% |
| `point-memtable-c1` | bytes/q | 393,244 / 393,244 / 393,320 | 393,244 | 76 | — | — |
| `point-memtable-c1` | rows/q | 16,390 / 16,390 / 16,393 | 16,390 | 3 | — | — |
| `point-memtable-c1` | q/CPU-s | 770.5 / 770.0 / 770.1 | 770.1 | 0.5 | 793.9 | -3.0% |
| `point-memtable-c64` | p99 ms | 5.022 / 4.964 / 5.214 | 5.022 | 0.250 | 4.903 | +2.4% |
| `point-memtable-c64` | CPU ms/q | 1.783 / 1.783 / 1.792 | 1.783 | 0.009 | 1.745 | +2.2% |
| `point-memtable-c64` | bytes/q | 393,296 / 393,288 / 393,265 | 393,288 | 31 | — | — |
| `point-memtable-c64` | rows/q | 16,392 / 16,391 / 16,390 | 16,391 | 1 | — | — |
| `point-memtable-c64` | q/CPU-s | 560.9 / 560.9 / 558.2 | 560.9 | 2.7 | 573.1 | -2.1% |
| `point-staged-c1` | p99 ms | 3.557 / 3.416 / 3.398 | 3.416 | 0.160 | 3.598 | -5.1% |
| `point-staged-c1` | CPU ms/q | 2.872 / 2.788 / 2.801 | 2.801 | 0.085 | 2.868 | -2.3% |
| `point-staged-c1` | bytes/q | 222,766 / 222,211 / 222,778 | 222,766 | 566 | 213,323 | +4.4% |
| `point-staged-c1` | rows/q | 1 / 1 / 1 | 1 | 0 | — | — |
| `point-staged-c1` | q/CPU-s | 348.2 / 358.7 / 357.0 | 357.0 | 10.6 | 348.7 | +2.4% |
| `small-aggregate-memtable-c1` | p99 ms | 3.308 / 3.408 / 3.469 | 3.408 | 0.162 | 2.902 | +17.4% |
| `small-aggregate-memtable-c1` | CPU ms/q | 2.897 / 2.933 / 2.919 | 2.919 | 0.036 | 2.434 | +19.9% |
| `small-aggregate-memtable-c1` | bytes/q | 2,264,040 / 2,263,670 / 2,263,945 | 2,263,945 | 370 | — | — |
| `small-aggregate-memtable-c1` | rows/q | 94,324 / 94,308 / 94,320 | 94,320 | 15 | — | — |
| `small-aggregate-memtable-c1` | q/CPU-s | 345.2 / 340.9 / 342.6 | 342.6 | 4.3 | 410.9 | -16.6% |
| `small-aggregate-staged-c1` | p99 ms | 5.648 / 6.033 / 5.825 | 5.825 | 0.385 | 5.616 | +3.7% |
| `small-aggregate-staged-c1` | CPU ms/q | 5.235 / 5.338 / 5.303 | 5.303 | 0.104 | 5.220 | +1.6% |
| `small-aggregate-staged-c1` | bytes/q | 456,427 / 455,657 / 456,927 | 456,427 | 1,270 | 439,677 | +3.8% |
| `small-aggregate-staged-c1` | rows/q | 78,143 / 78,143 / 78,143 | 78,143 | 0 | — | — |
| `small-aggregate-staged-c1` | q/CPU-s | 191.0 / 187.3 / 188.6 | 188.6 | 3.7 | 191.6 | -1.6% |
| `span-shredded-memtable-c1` | p99 ms | 38.011 / 37.454 / 38.273 | 38.011 | 0.819 | 36.635 | +3.8% |
| `span-shredded-memtable-c1` | CPU ms/q | 164.740 / 164.516 / 164.870 | 164.740 | 0.354 | 185.897 | -11.4% |
| `span-shredded-memtable-c1` | bytes/q | 278,134,783 / 278,134,783 / 278,134,783 | 278,134,783 | 0 | — | — |
| `span-shredded-memtable-c1` | rows/q | 500,223 / 500,223 / 500,223 | 500,223 | 0 | — | — |
| `span-shredded-memtable-c1` | q/CPU-s | 6.1 / 6.1 / 6.1 | 6.1 | 0.0 | 5.4 | +12.8% |
| `span-shredded-staged-c1` | p99 ms | 90.571 / 91.095 / 89.981 | 90.571 | 1.114 | 98.566 | -8.1% |
| `span-shredded-staged-c1` | CPU ms/q | 255.172 / 254.138 / 254.310 | 254.310 | 1.034 | 258.772 | -1.7% |
| `span-shredded-staged-c1` | bytes/q | 19,709,951 / 19,709,951 / 19,709,951 | 19,709,951 | 0 | 19,709,951 | +0.0% |
| `span-shredded-staged-c1` | rows/q | 31,487 / 31,487 / 31,487 | 31,487 | 0 | — | — |
| `span-shredded-staged-c1` | q/CPU-s | 3.9 / 3.9 / 3.9 | 3.9 | 0.0 | 3.9 | +1.8% |
| `span-long-tail-memtable-c1` | p99 ms | 25.641 / 24.822 / 24.723 | 24.822 | 0.918 | 23.691 | +4.8% |
| `span-long-tail-memtable-c1` | CPU ms/q | 108.000 / 107.699 / 108.222 | 108.000 | 0.523 | 99.046 | +9.0% |
| `span-long-tail-memtable-c1` | bytes/q | 278,134,783 / 278,134,783 / 278,134,783 | 278,134,783 | 0 | — | — |
| `span-long-tail-memtable-c1` | rows/q | 500,223 / 500,223 / 500,223 | 500,223 | 0 | — | — |
| `span-long-tail-memtable-c1` | q/CPU-s | 9.3 / 9.3 / 9.2 | 9.3 | 0.0 | 10.1 | -8.3% |
| `span-long-tail-staged-c1` | p99 ms | 60.228 / 63.308 / 56.492 | 60.228 | 6.816 | 59.736 | +0.8% |
| `span-long-tail-staged-c1` | CPU ms/q | 159.889 / 161.798 / 158.352 | 159.889 | 3.446 | 163.977 | -2.5% |
| `span-long-tail-staged-c1` | bytes/q | 19,709,951 / 19,709,951 / 19,709,951 | 19,709,951 | 0 | 19,709,951 | +0.0% |
| `span-long-tail-staged-c1` | rows/q | 505 / 505 / 505 | 505 | 0 | — | — |
| `span-long-tail-staged-c1` | q/CPU-s | 6.3 / 6.2 / 6.3 | 6.3 | 0.1 | 6.1 | +2.6% |

Point steps against the earlier records, by explicit mapping only (`point-c1` → `point-memtable-c1`, `point-c64` → `point-memtable-c64`). Those records predate the mixed-source fixture controls, so they are absolute recovery targets, not a like-for-like comparison:

| Current step | Metric | 9a6e15750 | 471f69ff9 | 434aaf5c1 (`point-*`) | 12fc33f08 (`point-*`) |
|---|---|---|---|---|---|
| `point-memtable-c1` | q/CPU-s | 770.1 | 793.9 | 860.8 | 885.8 |
| `point-memtable-c1` | p99 ms | 1.608 | 1.542 | 1.620 | 1.514 |
| `point-memtable-c64` | p99 ms | 5.022 | 4.903 | 4.219 | 4.305 |
| `point-memtable-c64` | q/CPU-s | 560.9 | 573.1 | 578.0 | 584.0 |

Staged/memtable ratios, each within its own record:

| Pair | Ratio | 9a6e15750 | 471f69ff9 | Memtable side |
|---|---|---|---|---|
| shredded | p99 | 2.383 | 2.690 | slower: 36.635 → 38.011 ms |
| shredded | CPU/q | 1.544 | 1.392 | faster: 185.897 → 164.740 ms |
| long-tail | p99 | 2.426 | 2.521 | slower: 23.691 → 24.822 ms |
| long-tail | CPU/q | 1.480 | 1.656 | slower: 99.046 → 108.000 ms |

#### Parquet, against `parquet/20261029T134546Z-459617bae.json`

`full-scan-c1` reports p95, as the experiment declares. The warehouse path is unchanged: scanned bytes and rows on full-scan, shredded and long-tail are identical.

| Step | Metric | Repetitions | Median | Spread | Reference median | Δ |
|---|---|---|---|---|---|---|
| `point-c1` | p99 ms | 2.871 / 2.906 / 3.176 | 2.906 | 0.305 | 2.886 | +0.7% |
| `point-c1` | CPU ms/q | 2.215 / 2.232 / 2.229 | 2.229 | 0.017 | 2.160 | +3.2% |
| `point-c1` | bytes/q | 185,436 / 185,150 / 185,179 | 185,179 | 286 | 181,370 | +2.1% |
| `point-c1` | rows/q | 1 / 1 / 1 | 1 | 0 | 1 | +0.0% |
| `point-c1` | q/CPU-s | 451.4 / 448.0 / 448.6 | 448.6 | 3.4 | 462.9 | -3.1% |
| `point-c64` | p99 ms | 11.338 / 11.297 / 11.182 | 11.297 | 0.156 | 10.789 | +4.7% |
| `point-c64` | CPU ms/q | 3.604 / 3.626 / 3.605 | 3.605 | 0.022 | 3.501 | +3.0% |
| `point-c64` | bytes/q | 185,297 / 185,287 / 185,316 | 185,297 | 29 | 181,411 | +2.1% |
| `point-c64` | rows/q | 1 / 1 / 1 | 1 | 0 | 1 | +0.0% |
| `point-c64` | q/CPU-s | 277.5 / 275.7 / 277.4 | 277.4 | 1.7 | 285.6 | -2.9% |
| `small-aggregate-c1` | p99 ms | 5.738 / 6.033 / 5.722 | 5.738 | 0.311 | 5.784 | -0.8% |
| `small-aggregate-c1` | CPU ms/q | 5.016 / 5.412 / 5.021 | 5.021 | 0.395 | 5.076 | -1.1% |
| `small-aggregate-c1` | bytes/q | 503,765 / 503,759 / 503,651 | 503,759 | 114 | 496,285 | +1.5% |
| `small-aggregate-c1` | rows/q | 100,031 / 100,031 / 100,031 | 100,031 | 0 | 100,031 | +0.0% |
| `small-aggregate-c1` | q/CPU-s | 199.3 / 184.8 / 199.2 | 199.2 | 14.6 | 197.0 | +1.1% |
| `full-scan-c1` | p95 ms | 53.674 / 53.543 / 52.527 | 53.543 | 1.147 | 52.658 | +1.7% |
| `full-scan-c1` | CPU ms/q | 468.119 / 467.228 / 459.608 | 467.228 | 8.511 | 467.426 | -0.0% |
| `full-scan-c1` | bytes/q | 248,512,511 / 248,512,511 / 248,512,511 | 248,512,511 | 0 | 248,512,511 | +0.0% |
| `full-scan-c1` | rows/q | 10,002,431 / 10,002,431 / 10,002,431 | 10,002,431 | 0 | 10,002,431 | +0.0% |
| `full-scan-c1` | q/CPU-s | 2.1 / 2.1 / 2.2 | 2.1 | 0.0 | 2.1 | +0.0% |
| `span-trace-c1` | p99 ms | 4.469 / 4.604 / 4.456 | 4.469 | 0.147 | 4.370 | +2.2% |
| `span-trace-c1` | CPU ms/q | 3.755 / 3.927 / 3.853 | 3.853 | 0.172 | 3.795 | +1.5% |
| `span-trace-c1` | bytes/q | 688,024 / 688,335 / 689,583 | 688,335 | 1,559 | 688,315 | +0.0% |
| `span-trace-c1` | rows/q | 8 / 8 / 8 | 8 | 0 | 8 | +0.0% |
| `span-trace-c1` | q/CPU-s | 266.3 / 254.6 / 259.5 | 259.5 | 11.7 | 263.5 | -1.5% |
| `span-trace-c64` | p99 ms | 26.411 / 24.609 / 24.347 | 24.609 | 2.064 | 23.642 | +4.1% |
| `span-trace-c64` | CPU ms/q | 7.095 / 7.021 / 7.040 | 7.040 | 0.074 | 6.925 | +1.7% |
| `span-trace-c64` | bytes/q | 688,907 / 689,311 / 688,956 | 688,956 | 405 | 689,045 | -0.0% |
| `span-trace-c64` | rows/q | 8 / 8 / 8 | 8 | 0 | 8 | +0.0% |
| `span-trace-c64` | q/CPU-s | 140.9 / 142.4 / 142.0 | 142.0 | 1.5 | 144.4 | -1.6% |
| `span-shredded-c1` | p99 ms | 17.203 / 16.957 / 17.416 | 17.203 | 0.459 | 16.925 | +1.6% |
| `span-shredded-c1` | CPU ms/q | 74.788 / 74.247 / 76.116 | 74.788 | 1.869 | 75.518 | -1.0% |
| `span-shredded-c1` | bytes/q | 18,006,015 / 18,006,015 / 18,006,015 | 18,006,015 | 0 | 18,006,015 | +0.0% |
| `span-shredded-c1` | rows/q | 62,527 / 62,527 / 62,527 | 62,527 | 0 | 62,527 | +0.0% |
| `span-shredded-c1` | q/CPU-s | 13.4 / 13.5 / 13.1 | 13.4 | 0.3 | 13.2 | +1.0% |
| `span-long-tail-c1` | p99 ms | 39.059 / 36.700 / 36.176 | 36.700 | 2.884 | 37.487 | -2.1% |
| `span-long-tail-c1` | CPU ms/q | 196.549 / 201.958 / 195.139 | 196.549 | 6.819 | 194.931 | +0.8% |
| `span-long-tail-c1` | bytes/q | 29,442,047 / 29,442,047 / 29,442,047 | 29,442,047 | 0 | 29,442,047 | +0.0% |
| `span-long-tail-c1` | rows/q | 977 / 977 / 977 | 977 | 0 | 977 | +0.0% |
| `span-long-tail-c1` | q/CPU-s | 5.1 / 5.0 / 5.1 | 5.1 | 0.2 | 5.1 | -0.8% |

#### Acceptance by record

| Target | Result | Status |
|---|---|---|
| Full hot VALID within 300 s | VALID, 219.2 s | PASS |
| `span-shredded-staged-c1` improves p99 and CPU/q against 471f69ff9 | 98.566 → 90.571 ms; 258.772 → 254.310 ms/q | PASS |
| `span-shredded-memtable-c1` improves p99 and CPU/q against 471f69ff9 | CPU 185.897 → 164.740 ms/q; p99 36.635 → 38.011 ms (+3.8%) | **MISS** (p99) |
| Staged/memtable p99 and CPU ratios shrink, with memory not slower | shredded p99 ratio shrinks, but memtable p99 is slower; shredded CPU ratio grows 1.392 → 1.544; long-tail ratios shrink, but the memtable is slower on both | **MISS** |
| Point memtable c1 ≥ 861 q/CPU-s | 770.1 | **MISS** |
| Point memtable c64 server p99 ≤ 4.31 ms | 5.022 ms | **MISS** |
| Parquet: no latency, CPU or scanned-byte regression against 459617bae (no tolerance) | Higher on point c1 (p99 +0.7%, CPU +3.2%, bytes +2.1%), point c64 (p99 +4.7%, CPU +3.0%, bytes +2.1%), small-aggregate bytes (+1.5%), span-trace c1 (p99 +2.2%, CPU +1.5%), span-trace c64 (p99 +4.1%, CPU +1.7%), shredded p99 (+1.6%), long-tail CPU (+0.8%), full-scan p95 (+1.7%). Lower or equal on small-aggregate p99/CPU, full-scan CPU, shredded CPU, long-tail p99 | **MISS** |

The other hot memtable steps also cost more server CPU per query than in 471f69ff9: point c1 +3.1%, point c64 +2.2%, small-aggregate +19.9%, long-tail +9.0%. Their staged counterparts held or improved on CPU, except small-aggregate staged (+1.6%). Staged scanned bytes rose on point (+4.4%) and small-aggregate (+3.8%). These measurements do not establish a cause, and none is claimed. Every **MISS** above is unfinished performance acceptance; it does not change either VALID verdict.

## Rollback

**Status: NOT DONE. The implementation is rolled back to `471f69ff9`.** The task, its evidence and both benchmark records are kept.

Why:
- Five performance acceptance targets missed, and `verify:bifrost` was red. That is unfinished acceptance, so the task should never have been declared IMPLEMENTED.
- The tests checked scalar leaf projection on hand-built fixtures. The benchmark's real query shape was never tested: `COUNT(*)` with an `attributes ->>` predicate. The live path keeps the leader's filter (`oracle/live.rs`), so a scan can filter on a shredded leaf and still rebuild the whole `attributes` column afterward.
- Identical scanned bytes for the shredded and long-tail steps do not prove the leaf went unused. The two predicates use different keys and selectivities, so comparing them does not measure what shredding is worth.

Also removed by the rollback:
- The V-TIME fix: the fork pin goes back to `19796a13e`; fork commit `8d6d6bf58` stays pushed.
- Memory scan rows and bytes.
- The `count(*)` and journey-fixture repairs.

A next attempt starts with a failing end-to-end test of the benchmark query on memtable and staged data in the learned layout. It needs a same-key, same-selectivity comparison of shredded against unshredded, and staged must be profiled against memtable before any change.
