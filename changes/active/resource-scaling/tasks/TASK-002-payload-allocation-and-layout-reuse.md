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

## Execution Evidence

Candidate commits on `change/resource-scaling` after allocation baseline
`1a0699e42` (fixed revision-3 geometry): `b75caaf98` (integrate
`variant-file-reads` at `5f92c71f3`; textual merge clean, no file touched by both
sides, excluded dirty path `changes/active/gateway-latency-capture/`),
`3b19e120f`, `07a9b08c1`, `3f077efa2`, `8c3e75fca`, `1f8ef0983`, `90058fcbb`,
`603d6050c`, `f8dda554d`, `05e2e4264`. Fork pins: `bohmian-ai/iceberg-rust`
`wyrd-production` `2902554d1` ("Sample shredded Variant input by reconstructing
only the chosen values"; fork RED `shredded_input_samples_like_its_logical_values`
panics on composite `typed_value` without it) and
`bohmian-ai/iceberg-compaction` `wyrd-production` `ab3da17ae` (resolves iceberg at
that revision). AC-003 baseline main `5234961d5`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-007/INV-007/AC-006 imported reads | `b75caaf98` integration; source-local leaf reads unchanged | `mise run test:bifrost:integration:redux` 1002/1002 with Postgres; imported journeys compiled in `wyrd-testing`; L1 `hot`/`parquet` below | PASS (correctness); performance see below |
| REQ-008/INV-005/AC-007 sample | `claim_sample.rs` reconstructs only chosen observations through fork `VariantSampler` values; seed, strata, thresholds unchanged | `scribe::parquet_writer::tests::claim_sampling_reconstructs_only_selected_observations`; RED with whole-batch reconstruction restored: "reconstruct a staged run for Variant sampling … 1-byte buffer" | PASS |
| REQ-009/INV-006/AC-007 layout reuse | `claim_merge.rs` `RunCursor.converts`/`physical_batch` passes matching runs through, converts only differing runs after `verify_layout_footer`; `parquet_writer.rs` `VariantPlan::Final { layout, logical }` | `matching_layouts_publish_without_variant_roundtrip` (RED: forcing every cursor to convert fails "matching runs are not converted"); `mixed_staged_layouts_preserve_values_and_fresh_discovery`, `rerun_claim_writes_byte_identical_objects`, `recovery_runs_are_unshredded_and_the_claim_sample_shreds` | PASS |
| REQ-011/INV-006/AC-009 byte bounds | `resources.rs` `ScribeMemoryLease::resize_maintenance`; `claim_merge.rs` `shaped_batch_rows`, `MERGE_BATCH_BYTES`, per-cursor lease; `claim_sample.rs` leased scan; `RollingArtifactWriter.buffered` lease | `scribe::parquet_writer::tests::claim_conversion_overlap_stays_within_reserved_bytes` (40 MiB root fits 2×320×64 KiB claim, 36 MiB held fails `IngestBusy` and releases, two concurrent claims on 80 MiB succeed and return to baseline); RED: row-count geometry fails "a byte-shaped claim fits its reservation: IngestBusy" | PASS |
| REQ-010/INV-008/AC-008 request | `adapter/mod.rs` `NativeBody<'a>`, `prepare<'a>`, `Prepared<'a>`; typed requests deserialize from `&Value` | `adapter::tests::request_preparation_avoids_duplicate_body_materialization` (RED: clone in `native()` fails "preparation borrows the caller's body"); `cargo nextest run -p wyrd-gateway --lib` 67/67 | PASS |
| REQ-010/INV-008/AC-008 capture | `wyrd-queue` `EncodedVariant::into_column`; `capture.rs` `ArrayRef` payloads, `NULL_PAYLOAD`, `concat`; `without_secret` in place, `encodes_secret` length guard | `capture::tests::capture_columns_avoid_intermediate_payload_copies` (RED: `VariantColumnBuilder` copy fails the shared-buffer assertion); `adapter::tests::capture_scrubbing_rebuilds_only_secret_holding_content` (RED: always-replace fails "secret-free content keeps its allocation"); capture lane 11/11 incl. `payload_capture_redacts_secrets_and_references_binary_content` | PASS |
| REQ-010/REQ-011 lifecycle journey | `tests/bifrost/scribe/write_read.rs` | `write_read::learned_layout_evolution_preserves_published_evidence` (learned publish, learned staging, restart, cold changed distribution from a second writer, mixed claim, restart; exact ordered readback at every stage, `a` shredded in every object, selective `v->>'a'` reads, per-writer `sub_principal_id`, second tenant reads nothing); gateway `compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage` | PASS |
| Bifrost owner | `architecture/bifrost-design.md` Scribe sampling/merge paragraph | `mise run docs:check` | PASS |

Commands:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::learned_layout_evolution_preserves_published_evidence)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
mise run test:bifrost:integration:redux
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^components::gateway::capture::tests::/)'
```

Checks: `mise run fmt:check`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation`, `git diff --check` pass.

Diagnoses:

- **Symptom** the byte-bounds test failed `IngestBusy` with shaping on.
  **Evidence** footer `uncompressed_size` gave 211 B/row for repeated 64 KiB
  payloads. **Cause** dictionary encoding shrinks encoded pages; the decoded
  width is the unencoded byte-array payload. **Fix site** `shaped_batch_rows`
  uses `unencoded_byte_array_data_bytes` plus fixed leaf widths, rows clamped to
  the file's row count.
- **Symptom** `scribe::staging_runtime::pg_tests::a_claim_stranded_in_publishing_publishes_after_restart`
  failed "promotion evidence contradicts the published object". **Evidence**
  attempt 1 sealed 972 B, the resumed attempt 1472 B; the resumed object's
  `ARROW:schema` carried the staged run's `wyrd.bifrost.{object_identity,schema_fingerprint,tenant}`;
  bisect: passes at `3b19e120f`, fails at `07a9b08c1`. **Cause** `restore_context`
  rebuilds the claim schema from a run footer, whose key-value metadata the
  Parquet reader folds into the Arrow schema; since final objects seal under the
  claim's logical schema, a resumed claim wrote different bytes. **Fix site**
  `ScribeStagingRuntime::restore_context` drops the run writer's footer identity
  keys (`f8dda554d`); the live context path has none. Redux lane 1002/1002.
- Earlier non-Postgres lib runs failed only Postgres fixtures with
  `WYRD_TEST_DATABASE_ADMIN_URL` unset; the Postgres lane passes them.

Non-goals: no new public setting, wire or persisted format, dependency, feature,
cache, allocator, pool, outbox geometry or batching change, capture admission
redesign or Forge change; `changes/active/gateway-latency-capture/` untouched.
Response media capture still clones the answer bytes (changing it needs a Skald
signature change); translated (Anthropic/Google/Jev) bodies are still built at
both planning and dispatch.

### Performance

Same host (32 vCPU Linux x86_64), sequential quiet-host runs, `--allow-dirty`
records under `benchmarks/history/` (unprofiled) and `target/benchmarks/`
(`--profile`, `--steps`). Candidate `05e2e4264`, allocation baseline
`1a0699e42`, main `5234961d5`. Every run VALID, zero errors/lost/shed/undrained.

**AC-010, unprofiled L1 `buffered-64kib-capture-payload.c64`** (median vs
baseline min–max):

| Metric | Candidate | Baseline `1a0699e42` | Main |
|---|---|---|---|
| peak RSS | 4587 MiB | 7357–9944 MiB | 7471–7783 MiB |
| server CPU per call | 1841 µs | 2218–2575 µs | 2201–2254 µs |
| rate | 5616 req/s | 4174–4857 req/s | 4809–4979 req/s |
| server p99 | 112.6 ms | 143.4–192.2 ms | 156.1–167.8 ms |

The c1 payload step matches: RSS 2510 vs 4315–4714 MiB, CPU 724 vs 832–1090 µs.

**AC-010, paired `--profile` heap `alloc_space`** (step delta divided by the
step's reconciled calls; warmup allocations are in both numerators):

| Step | Candidate | Baseline | Δ |
|---|---|---|---|
| buffered-64kib-capture-payload.c64 | 4361 KiB/call | 6223 KiB/call | −29.9% |
| buffered-64kib-capture-payload.c1 | 4272 KiB/call | 5842 KiB/call | −26.9% |
| buffered-64kib-capture-metadata.c64 | 1814 KiB/call | 2014 KiB/call | −9.9% |
| buffered-1kib-capture-metadata.c64 | 285 KiB/call | 296 KiB/call | −3.5% |

Attribution at payload c64: the request-body `clone` frame (12.6 GiB in the
baseline) is gone; Arrow binary `append_slice` falls 127.6 → 79.8 GiB,
`with_capacity<GenericBinaryType>` 32.7 → 13.2 GiB, Variant `build`
43.2 → 25.3 GiB. Remaining costs are WAL IPC encode, zstd, Parquet/Variant
encode and serde serialization. The streamed 1 KiB c1 and c64 profile deltas
move between adjacent steps (cumulative profile: c1 −64%, c64 +41%, about
+2% combined over equal calls). Limits: the heap profiler samples per step,
not per repetition, so the allocation comparison is one paired aggregate
rather than a median against the baseline minimum repetition. The samply CPU
captures failed on this host (`perf_event_paranoid` > 1), so CPU attribution
is missing.

**AC-003 vs main** (all steps, three repetitions). Out-of-range medians in the
worse direction on the first run: `buffered-1kib-capture-metadata.c1`
(rate, CPU), `streamed-1kib-capture-metadata.c1` (rate),
`buffered-64kib-capture-metadata.c1` (rate, CPU, RSS, p99). After the fresh
back-to-back pair:

- streamed c1 clears.
- buffered 1 KiB c1 falls outside again: rate 5706 vs 5787–5879, CPU 218.3
  vs 209.5–214.2 µs.
- buffered 64 KiB metadata c1 is outside on p99 only: 0.309 vs
  0.288–0.294 ms.

Two further interleaved buffered 1 KiB c1 pairs disagree in direction:

| Pair | Candidate rate | Main rate range |
|---|---|---|
| A | 5776 | 5846–5927 |
| B | 5907 | 5676–5851 |

Commit `3f077efa2` swings the same way (5663 and 5869), so the gap follows
host variation rather than a commit. The 64 KiB metadata c1 p99 gap is already
present at baseline `1a0699e42` (0.332 vs main 0.284–0.308 ms), so it predates
TASK-002. By AC-003's rule both c1 steps are recorded as **FAIL**. Every c64
step is in range or better than main.

**AC-006 imported reads, L1 `hot`/`parquet` vs `1a0699e42`** (p50 median vs
baseline range):

- **Gains on hot covered paths:**
  - span-shredded staged 16.8 vs 85.3–85.5 ms;
  - span-shredded memtable 22.0 vs 30.7–30.9 ms;
  - span-long-tail staged 38.0 vs 55.5–55.8 ms;
  - point memtable c64 1.82 vs 2.02–2.05 ms.
- **Hot regression:** span-long-tail memtable is slower, 26.7 vs 20.2–20.5 ms
  (rate 36.6 vs 47.4–47.8).
- **Parquet tier:** within about ±5%. span-shredded c1 is 15.4 vs
  14.5–14.6 ms, and span-trace c64 is 13.4 vs 12.8–13.0 ms.

The memtable long-tail filter is a residual/fallback read with no speedup
promise. It reads exactly but slower, and it comes from the imported
live-read integration, outside this task's write-path owners.

| Acceptance criterion | Result |
|---|---|
| AC-010 peak RSS and allocation at payload c64 | PASS on RSS (median below baseline minimum); allocation −29.9% paired aggregate; per-repetition allocation and CPU attribution unavailable |
| AC-003 L1 non-regression vs main | FAIL by rule on `buffered-1kib-capture-metadata.c1` (unstable across pairs) and `buffered-64kib-capture-metadata.c1` p99 (predates TASK-002) |
| AC-006 imported performance | PASS for covered memtable/staged paths; memtable long-tail fallback 30% slower, reported |

### AC-003 diagnosis

- **Symptom:** after the rule's re-run pair, the two c1 steps above stay outside
  main's range. That is 1–3% on 1 KiB c1 rate/CPU and 3–5% (about 10 µs) on
  64 KiB metadata c1 server p99. Every c64 step is in range or better.
- **Evidence:**
  - **User-space CPU profiles.** `perf record -e cpu-clock:u -F 1999 --call-graph fp`
    attached to the server for the selected step, main vs candidate,
    41–46k samples each. On 1 KiB c1, no symbol's self time differs by more
    than 0.25 points, and the rows that differ are the same permission
    deserializers under different monomorphized names. The request-body
    `indexmap::Bucket` clone is gone in the candidate. On 64 KiB c1 the
    largest delta is +0.69 points in `Utf8Chunks::next`, reached only from
    unchanged `skald_providers::clients::send_bytes_with_retry`. Total samples
    (41210 vs 41333) and request counts match.
  - **Bench allocator hook.** `6d12ff842`'s `heap_profile::{allocated,freed}`
    costs one relaxed load per call while sampling is off.
  - **Main against itself.** Two main runs pass the rule against each other on
    both steps.
  - **Interleaved repeats disagree in direction:**
    - 1 KiB c1 candidate vs main: 5776 vs 5846–5927, then 5907 vs 5676–5851.
    - 64 KiB c1 p99 at `89162f762` vs main: 0.300 vs 0.277–0.293, then 0.305
      vs 0.295–0.312.
- **Cause:** not established. The 64 KiB c1 p99 shift is already present at
  `1a0699e42`, inside TASK-001's range (`5234961d5..1a0699e42`). The profiles
  show no code path that TASK-002 adds or changes doing more work. The shift is
  the same size as same-host repeat variation, which three repetitions per run
  cannot separate.
- **Fix site:** none identified. No change was made; re-running until a pair
  passes would not be evidence.

AC-003 remains **FAIL** by its rule; the maintainer decides how the c1
evidence is treated.
