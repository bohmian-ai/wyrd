---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-resource-scaling
spec_revision: 4
requirements: [REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, INV-005, INV-006, INV-007, INV-008, AC-006, AC-007, AC-008, AC-009, AC-010]
depends_on: [TASK-001]
---

# Reduce payload allocation while preserving learned layouts

## Outcome and Value

Integrate the learned-layout read work, then remove avoidable payload rebuilding
from Gateway through Scribe publication. This is one end-to-end allocation outcome:
captured evidence stays exact and queryable while repeated payload passes and
overlapping temporary memory fall substantially.

For example, a generation already uses layout A. Every hot-file claim still takes
a fresh sample. If it chooses A again, matching shredded columns are merged and
written without reconstructing and reshredding their values. If it chooses B,
only incompatible inputs/columns are converted. Sampling reconstructs selected
observations, rather than every row it scans.

**Authority:** the maintainer approved spec revision 4 on 2026-11-07, including
its explicit Scribe architecture correction. This task remains proposed for
separate implementation; recording approval performs no merge or implementation.

## What Changes and What Must Stay True

Merge the committed `variant-file-reads` implementation first, preserving its
layout-aware memory/staged query behavior and its outstanding verification.
The inspected source is branch `change/variant-file-reads-task-002`, commit
`5f92c71f33a98e7d6616aab9f0a201762da84ebf`, common base
`5234961d51798581a026834cfffd5efaadc226b4`. Execution must confirm the source tip;
do not silently substitute later commits or import dirty worktree deletions or
untracked review files. Resolve overlapping source edits by preserving both
changes' behavior. Reuse the imported read owners; do not build another reader.

A physical Variant layout consists of typed leaves plus residual content. Every
claim still samples complete logical observations under the same seeded,
stratified policy. Physical equality must cover layout semantics and metadata,
not merely equal Arrow leaf types. Newly eligible residual fields remain learnable.
Different layouts may share one claim; preserve current membership, deterministic
sort/tie ordering, canonical WAL, exact retry identity, logical/physical footer
proof and recoverable local staging. Update the owning Bifrost paragraphs with
the approved selective reconstruction and physical reuse rules.

Keep the cached layout pinned per generation. Staging writes that representation
unchanged. Cache refresh remains advisory after successful assembly; restart or
eviction may leave a generation cold. Publication uploads encoded artifacts;
Forge promotion does not rewrite them. Forge compaction's footer-weighted layout
combination and rewrite behavior remain unchanged.

Remove known avoidable full-body preparation/copies and intermediate capture
materializations at their current owners. Preserve validation, deployment model
substitution, native and translated protocols, retries/fallback, streaming,
credential scrubbing, binary-content references, attribution and accepted errors.
Raw ingress bytes cannot replace redacted persisted capture. Shared buffers must
not extend unnecessary full-body retention or outlive their reservation owner.

Fit simultaneous decoded inputs, conversion outputs, merge buffers and writers
within existing byte reservations, including concurrent claims. A fixed row count
is not proof. Preserve accepted large rows, failure/cancellation cleanup and ACK
durability; do not introduce an oversized-row accounting exception.

Canonical WAL serialization, sort/gather and Parquet encode/decode remain real
costs. No absolute allocation floor or hard RSS ceiling is promised. Exclude new
public settings, wire/persisted formats, dependencies/features, caches, allocators,
pools, outbox concurrency/batching changes, capture admission/ownership-policy
redesign and Forge optimization. Do not absorb the separate draft
`gateway-latency-capture` packet. Existing fixed 4/4/8/32 outbox geometry and shared
startup resources are prerequisites, not tuning variables for this task.

The TASK-001 dependency means its fixed revision-3 resource/cancellation behavior
must be present before this task. It does not claim that TASK-001's historical L1
results passed. Final revision-4 evidence must close AC-003; its old suggestion of
maintainer discretion is not an acceptance waiver. Allocation baseline means the
immutable pre-TASK-002 candidate with fixed geometry, before read integration;
main remains the separate AC-003 baseline.

## Approach

1. Confirm the approved revision and fixed resource geometry, integrate the pinned
   read implementation, and record its exact source and preservation evidence.
2. Capture matched baseline allocation and live-memory evidence through completed
   capture/drain work; distinguish the original candidate from the integration-only
   state where necessary to attribute write-path changes.
3. Preserve fresh sample selection while avoiding reconstruction of unsampled
   observations. Preserve compatible physical columns through merge/output and
   convert incompatible inputs under existing governance.
4. Remove redundant Gateway preparation and capture materializations, completing
   their callers, ownership, retry and redaction closure at existing owners.
5. Prove byte-bounded overlap and failure cleanup, update the Bifrost owner, and
   demonstrate exact evidence through real server journeys.
6. Run the focused checks and matched performance experiments below. Report actual
   reductions and residual costs; do not infer completion from a valid benchmark exit.

## Ordered Implementation Scenarios

### Scenario 1 — Every claim learns without reconstructing unsampled payloads

**Behavior.** Cold and learned runs receive the same deterministic selected
observations under the current policy. Complete selected values include residuals,
null/missing cases, mixed types and nested/list values. A later distribution with
an eligible residual-only field learns it; an unchanged distribution may choose
the old layout. Selection never requires reconstructing every unsampled payload.

**RED.** Add planned `scribe::parquet_writer::tests::claim_sampling_reconstructs_only_selected_observations`.
Use existing real staged-run sampling and compare selected observations/layout
with current complete-value semantics, plus executed reconstruction evidence.
The current warm path reconstructs whole batches before sample selection. Run
[sample proof](#sample-proof), confirming the actual selector before RED.

**GREEN.** Preserve the existing seed, strata, thresholds and nested observation
semantics while limiting reconstruction to selected observations. Rerun this
scenario and the existing mixed-layout/residual regression.

**REFACTOR.** Reuse the existing sampler and Variant traversal/selection support;
remove duplicate work rather than introducing another sampling policy or cache.

### Scenario 2 — Matching layouts survive merge and final writing

**Behavior.** Fresh sampling selects the layout already used by staged inputs.
Matching Variant columns are not reconstructed for merge or reshredded for output.
Output retains exact values, residuals, logical/physical footer identity, stable
ordering and byte-identical retries. A mixed claim preserves matching columns
while converting incompatible inputs; cold and changed-layout inputs remain valid.

**RED.** Add planned `scribe::parquet_writer::tests::matching_layouts_publish_without_variant_roundtrip`.
Exercise the actual claim encoder with matching, changed and heterogeneous layouts;
assert conversion work as well as decoded rows, footer identity and retry bytes.
Current assembly normalizes every cursor before final shredding. Run
[layout proof](#layout-proof) and the named regressions below.

**GREEN.** Complete physical reuse through the merge and final writer, keeping
logical identity separate from the incoming physical schema. Use bounded
conversion for incompatible inputs. Rerun scenarios 1–2 and the named regressions.

**REFACTOR.** Extend the existing merge, layout identity and writer pass-through
paths. Do not partition claims by a new durable layout key or add a second writer.

### Scenario 3 — Large payloads and concurrent merges remain byte-bounded

**Behavior.** Under tight existing budgets, multiple runs/claims with large
payloads fit decoded input/output overlap into reservations. An accepted large row
can progress; insufficient capacity uses existing pre-acceptance pressure without
losing acknowledged rows. Failure and cancellation release each owner once.
Increasing row width cannot bypass accounting through the fixed batch-row count.

**RED.** Add planned `scribe::parquet_writer::tests::claim_conversion_overlap_stays_within_reserved_bytes`.
Drive actual decode/conversion/encoding with large rows, matching and changed
layouts, and simultaneous governed work. Assert peak owned bytes, pressure,
progress and cleanup using existing resource inspection. Expose the current
row-count-only or missing-overlap boundary. Run [workspace proof](#workspace-proof).
If a branch is already correctly bounded, record preservation evidence for that
branch; do not manufacture a failure or weaken its current accounting.

**GREEN.** Fit work geometry to existing reservations, preserving one-row progress
and lifecycle accounting. Rerun scenarios 1–3; do not raise budgets to pass.

**REFACTOR.** Consolidate ownership at existing admission/persistence owners;
remove redundant retained representations without adding a new memory governor.

### Scenario 4 — Gateway prepares requests without redundant body materialization

**Behavior.** Routing and dispatch preserve eligibility checks, provider model
substitution, native/translated payloads, retry/fallback and stream behavior while
removing repeated equivalent full-body preparation and cloning. Different selected
deployments still receive their correct model and protocol; reuse is not shared
across incompatible attempts or tenants.

**RED.** Add planned `adapter::tests::request_preparation_avoids_duplicate_body_materialization`
in `wyrd-gateway`. Exercise preparation through its planning/dispatch callers;
assert preparation/copy work together with upstream content, errors and fallback.
Current deterministic preparation constructs a body during planning and again at
dispatch. Run [request proof](#request-proof).

**GREEN.** Remove demonstrated duplicate work at the existing preparation owner
and update all consumers, preserving cancellation and attempt lifetime. Rerun
this scenario and existing adapter owner tests.

**REFACTOR.** Reuse current prepared request and dispatch ownership. No alternative
transport, raw-body validation bypass or general request cache is authorized.

### Scenario 5 — Redacted captures reach columns without redundant intermediates

**Behavior.** Payload and metadata captures retain the same selected values,
redaction, binary references, null handling, byte limits and attribution. Batch
construction avoids redundant full-payload intermediates and unnecessary buffer
growth; retry/ACK ownership preserves content without extra retained copies.

**RED.** Add planned `components::gateway::capture::tests::capture_columns_avoid_intermediate_payload_copies`.
Drive real capture projection and batching, including nested request/response
payloads and binary references; assert content, bounds and materialization work.
Current per-capture Variant buffers are copied into batched columns. Run
[capture proof](#capture-proof) and the existing redaction regression.

**GREEN.** Remove avoidable intermediates using existing capture/Variant column
owners; preserve current outbox ordering, retries, byte accounting and ACK lifetime.
Rerun this scenario, scenario 4 and their content/security regressions.

**REFACTOR.** Prefer ownership transfer and existing destination encoding support.
Keep mandatory redaction/conversion; do not store raw JSON as a new durable format.

### Scenario 6 — Real clients retain exact evidence through the optimized lifecycle

**Behavior.** A real client writes evolving Variant data and reads exact results
from memory, local staging and published data, including after restart. Learned
selective queries retain their leaf execution benefit. Real Gateway calls retain
responses and attributed redacted captures after drain across buffered/streamed,
retry, cancellation and split-role paths. Another tenant cannot read the evidence.

**RED.** Add planned `write_read::learned_layout_evolution_preserves_published_evidence`
in the existing `wyrd-testing` `scribe` target. Drive successive learned
generations with matching and changing layouts, mixed claims, drain and restart;
assert exact values/order/attribution and tenant refusal. Run [lifecycle proof](#lifecycle-proof).
First establish RED for the added optimized execution assertion or the minimal
new behavior it exposes; unchanged readback is preservation proof, not a forced
RED. Run the existing focused Gateway journey as regression proof; extend its
coverage only where changed capture/dispatch behavior needs it.

**GREEN.** Complete consumer wiring and lifecycle closure while keeping production
startup, readers and durability owners. Rerun the journey and scenarios 1–5.

**REFACTOR.** Reuse existing WyrdTestServer, clients and source-lifecycle controls;
do not emulate the server or add public source/layout selectors.

## Acceptance Criteria

| Spec obligations | Observable proof and failure condition |
|---|---|
| REQ-007; INV-007; AC-006 | Integration preserves imported source-local leaf reads and complete fallback across tiers. Close imported correctness/performance obligations; missing TASK-003 measurements remain unfinished work. |
| REQ-008; INV-005; AC-007 | Scenario 1 selects the same fresh sample and discovers changing residual fields while avoiding unsampled reconstruction. Reusing a cached layout without sampling fails. |
| REQ-009; INV-006; AC-007 | Scenario 2 and existing mixed-layout/retry regressions prove matching-column reuse, incompatible conversion, exact metadata/values/order and deterministic bytes. Dropped residuals or physical footer misidentification fail. |
| REQ-010; INV-008; AC-008 | Scenarios 4–6 demonstrate reduced body materialization with unchanged protocol, redaction, attribution, retries and real readback. Required transformations cannot be skipped. |
| REQ-011; INV-006; AC-009 | Scenarios 3 and 6 prove byte-bounded concurrent overlap, accepted-row progress, cleanup and restart durability. Row-count-only bounds, accounting bypass or lost ACKed evidence fail. |
| REQ-012; INV-008; AC-010 | Matched profile deltas and repeated load/drain observations demonstrate removed passes plus allocation/peak-RSS reductions beyond baseline variation. Outstanding work, a changed workload or valid exit alone do not pass. |
| Preserved REQ-001–006; INV-001–004; AC-001–005 | TASK-001's integrated revision-3 behavior remains fixed 4/4/8/32 with shared resources and cancellation-safe delivery accounting. AC-003's L1 non-regression rule remains in force. |

## Implementation Reference

Inspected existing owners and gaps; paths guide discovery, not a file allowlist.

- **Imported reads:** the pinned branch changes `scribe/memtable.rs` physical
  requirement evaluation and `oracle/follower.rs` staged `hot_parquet_scan` use.
  Its approved revision-2 spec retains learning/publication and requires measured
  memory/staged gains. It does not modify claim sampling/merge/final encoding.
  Merge these owners rather than replace them; preserve their tests and remaining
  proof in `changes/active/variant-file-reads/` after integration.
- **Sampling/assembly:** `ScribeClaimAssembler::encode` in
  `scribe/claim_assembly.rs` selects a layout before opening `StagedRunMerge`.
  `ClaimVariantSample::layout/scan` in `claim_sample.rs` reconstructs decoded
  batches before `offer` chooses rows. Extend that workflow with selected-only
  reconstruction; keep its residual discovery and deterministic selection.
- **Merge/writer:** `StagedRunMerge::next_batch` and its cursor normalization in
  `claim_merge.rs` force logical arrays. `VariantPlan` handling and schema identity
  in `parquet_writer.rs` already support physical pass-through, but final opening
  currently infers logical schema from its first batch. Extend both together so
  physical input cannot corrupt logical footer identity. Existing writer tests
  cover mixed layouts/fresh discovery, canonical recovery and deterministic retries.
- **Learned ownership:** `LearnedLayout`/`LearnedLayoutCache` in `layout_cache.rs`,
  generation pinning in `memtable.rs`, `bind_slice_layout` in `shards.rs`, and
  `ScribeStagingRuntime::assemble` in `staging_runtime.rs` already own layout reuse.
  Local stage writing preserves frozen rows. Reuse exact schemas/layout semantics;
  no durable layout ID or new cache is needed.
- **Memory/durability:** `preprocess.rs::encode_ipc` builds canonical compressed
  WAL bytes; staged sort/gather and merge/writer work remain. Existing admission,
  material plan, resource reservations and stage/publication owners must account
  overlap. Preserve canonical IPC and immutable staged recovery authority.
- **Requests:** `wyrd-gateway/src/adapter/mod.rs::prepare/native` and their
  planning/dispatch consumers build deployment-specific bodies; native currently
  clones the JSON object. Existing `adapter/tests.rs` checks rewriting, translation,
  credentials, streams and fallback. Reuse that owner, not a new routing layer.
- **Capture:** `wyrd-server/src/components/gateway/capture.rs::redacted_payload`
  serializes redacted values to `EncodedVariant`; `CallCapture::calls_batch`
  copies them into precisely sized columns. Existing exact preallocation must not
  be credited as a new optimization. `wyrd-queue/src/variant.rs` already provides
  `EncodedVariant::from_serialize` and `VariantColumnBuilder::encode/append`;
  extend the existing representation/ownership closure rather than add a Variant
  implementation. Follow `invocation.rs`, Scribe outbox and peer transport callers.
- **Journeys/measurement:** existing `wyrd-testing` targets `scribe` and `gateway`
  use real servers; the `capacity` binary owns L1 experiments and heap profiling.
  `benchmarks/README.md` distinguishes cumulative allocation, sampled live heap,
  RSS and measurement deltas. No production, wire, generated or SDK API change is
  planned; if one becomes necessary, stop for authority instead of expanding scope.

## Verification and Evidence

All planned tests below are explicitly **planned**, not existing. Their package,
library/target and module owners were confirmed from source/manifests. After adding
each test, confirm its actual selector before RED and record the exact command.
Run Cargo operations sequentially. Correct an equivalent selector or private
fixture locally without weakening proof.

### Sample proof

`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::claim_sampling_reconstructs_only_selected_observations)'`

### Layout proof

`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::matching_layouts_publish_without_variant_roundtrip)'`

### Workspace proof

`mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::claim_conversion_overlap_stays_within_reserved_bytes)'`

### Request proof

`mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::request_preparation_avoids_duplicate_body_materialization)'`

### Capture proof

`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::capture_columns_avoid_intermediate_payload_copies)'`

### Lifecycle proof

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::learned_layout_evolution_preserves_published_evidence)"'`

### Named regressions

Existing writer regression selectors, confirmed from source:

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::mixed_staged_layouts_preserve_values_and_fresh_discovery)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::rerun_claim_writes_byte_identical_objects)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::recovery_runs_are_unshredded_and_the_claim_sample_shreds)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::payload_capture_redacts_secrets_and_references_binary_content)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
```

These prove residual discovery, deterministic retries, cold recovery, redaction
and answered-call/capture-outage behavior. New claims do not replace these tests.

### Owner and boundary checks

- `mise run test:bifrost:integration:redux` — source-local reads, writer, resource,
  recovery and merge integration closure with repository Postgres setup.
- `mise exec -- cargo nextest run --locked -p wyrd-gateway --lib` — affected adapter
  protocol, validation, retry/fallback and cancellation closure.
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^components::gateway::capture::tests::/)'`
  — capture content, limits and projection closure.
- `mise run fmt:check`, `mise run lints`, `mise run check:deps`,
  `mise run check:tenant-isolation`, `mise exec -- git diff --check` — formatting,
  Rust correctness policy, dependency/tenant boundaries and clean diff.

This task adds internal optimization rather than a new language capability. Use
focused journeys here; integrated change review owns broad cross-language gates.
Imported named tests retain their exact commands from their existing packet;
confirm and run those whose behavior is touched instead of inventing selectors.

The imported performance closure requires the same logical Variant workloads,
keys, row counts, selectivity and query shapes across memory, staged and published
sources. Record placement, layout coverage and cache state. Compare baseline versus
changed execution within each tier and canonical versus learned layouts on matched
data. Both memory and staged covered queries must show CPU and latency reductions
beyond repetition variation, explained by leaf execution and less reconstruction;
whole-root/fallback reads retain exactness without a speedup promise. Report each
tier separately plus write/staging cost. A correctness PASS or unchanged timing
does not close this obligation. The imported approved spec remains authoritative
for its full proof; these criteria explain why the integration is not just a merge.

### Performance and integration proof

Integration itself uses source/diff review and preservation tests, not a fabricated
RED. Record source SHA, resolved conflicts and excluded dirty paths. Complete the
imported read packet's outstanding proof; its task correctness PASS is not a
whole-change or performance PASS.

Run identical commands on the recorded baseline and integrated candidate, one
quiet-host experiment at a time:

- `mise run bench:l1:gateway -- --allow-dirty` — all existing L1 cases and three
  repetitions, applying the spec's non-regression rule.
- `mise run bench:l1:gateway -- --allow-dirty --profile` — matched attribution
  runs including payload c64; distinguish profiler overhead from normal L1 results.
- `mise run bench:l1:bifrost:hot -- --allow-dirty` and
  `mise run bench:l1:bifrost:parquet -- --allow-dirty` — imported learned-layout read
  performance/preservation, including its existing comparable tier proof. If the
  imported packet extends these plans, use its integrated canonical plan unchanged.

The flags record dirty provenance; capture all source diffs and actual geometry.
Use AC-010's baseline repetition rule, not the unverified supplied 3.4/4.8 MiB
totals. Keep equal-load, fully reconciled populations for memory comparisons;
closed-loop throughput differences must not masquerade as lower memory. Reuse the
existing driver/control ownership for any missing repeated load/drain observation,
and record the exact invocation and sampling boundaries. Do not add another
benchmark framework or treat the short L1 window alone as steady-state proof.

Report allocation per completed/reconciled call, profile interval denominator,
total versus incremental capture cost, live heap, RSS peak/time series, post-drain
ownership, throughput/CPU/p99, errors and capture reconciliation. The 64 KiB case
contains two bodies of that size. Explain remaining WAL, codec, sorting and
necessary changed-layout conversion costs. Evidence is incomplete until the
specified passes are removed, reductions exceed baseline variation and retained
work stops accumulating over equivalent cycles.

## Material Stop Conditions

- Revision 4 is not approved, or the required Bifrost owner correction is not
  included in that approval: do not execute the new behavior.
- Correctness needs a changed sample distribution/threshold, stale-cache output
  choice, claim membership, persisted format/footer contract, canonical WAL,
  tenant/security/redaction policy, ACK or publication fence: seek a spec revision.
- Required physical reuse cannot preserve full layout semantics, or bounded
  conversion cannot process an already accepted row within existing resource
  policy: report the conflicting evidence; do not silently change admission limits
  or add an uncounted fallback.
- Integration requires a materially different source tip, new dependency/feature,
  new governor/cache/transport, or adoption of the separate gateway ownership
  redesign: establish the conflict and obtain the missing authority.
- Profiles show an additional material cause outside the specified owners:
  finish in-scope work and report it; do not expand into unrelated optimization.
  Missing proof or gains do not justify weakening acceptance.

Reversible schema plumbing, private helper organization, test evidence mechanisms,
equivalent selectors and fixture geometry remain implementation decisions.

## Authority Links

- [Approved specification, revision 4](../spec.md)
- [Repository rules](../../../../AGENTS.md)
- [Spec-driven development](../../../spec-driven-development.md)
- [Implementation execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md)
- [Bifrost owner](../../../../architecture/bifrost-design.md)
- [Wyrd protocol](../../../../architecture/wyrd-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Benchmark method](../../../../benchmarks/README.md)
