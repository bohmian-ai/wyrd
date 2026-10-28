---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 19
parent_task: TASK-006
depends_on: [TASK-006]
---

# TASK-007 — shared WAL and consistent shredding; measured query work

## Decision and scope

Use one physical write-ahead log (WAL) per Scribe process, and preserve native
Variant shredding in every queryable storage tier. A WAL is the recovery log;
shredding stores selected JSON fields as typed columns alongside the remaining
values. Improve Parquet page sizing through a bounded experiment. Remove a
duplicate SQL parse within each query; do not introduce a cross-query plan cache.

This task extends TASK-006 and its revisions for remaining items 9, 10, 11 and 12.
It is an implementation plan, not a new specification or specification revision. Its
four executable task sections below replace those items' remaining instructions.
Items 1–8, prior implementation, independent reviews and historical measurements
remain intact. This document does not claim implementation or measured gains.

The maintainer explicitly approved the directions for items 9 and 10 in the
conversation requesting this plan and requested this task extension. The detailed
recovery/concurrency contracts below and recommendations for 11/12 are proposed
implementation instructions; this does not retroactively approve new decisions.
The base revision-19 specification remains unchanged. No separate task documents
are required: the four work sections here are the handoff units of TASK-007.

Current user instructions take precedence over the older architecture. The
implementation must update the owning Bifrost sections along with the code:
per-shard physical logs become one process log; queryable recovery staging may
be shredded; item 10 no longer waits for item 9. Public Variant wire format,
tenant boundaries and durability guarantees do not change.

No new database, dependency family, Cargo feature, public data API, decoded-page
cache, query-plan cache or alternative query planner is authorized.

The maintainer approved a limited benchmark extension for item 10 only: a
learned-layout setup in the existing hot workflow and its missing c64 shredded
filter case. Reuse selected existing L1 cases everywhere else, with unchanged
windows, controls, targets, validity rules, record schema and reporting. No
duration flag, long shard sweep, staged timing framework, extra timing fields
or new benchmark runner is authorized. Each selected L1 command, including the
learned-layout setup, must stay within the existing five-minute budget.

## Why these changes, and what the evidence actually proves

| Item | Established evidence | Decision and limit |
|---|---|---|
| 9 | Sustained one/16-shard counters show 6.29/50.79 physical WAL syncs/s, 0.84/4.00 active WAL workers, 0/11.11 queued jobs and 2.160/2.027M inserted rows/s. Other matched repetitions reverse the throughput difference. | Consolidate durability work across shards in one file stream. No reliable shard scaling is established; disk bandwidth saturation is not established. |
| 10 | Memory and staged files are queryable; staged and hot files use the same Parquet scan. Memory/staging are currently unshredded. Claim assembly already samples all staged claim inputs. | Reuse claim-selected columns for future memory generations and preserve them into staging. Still resample complete values when producing hot files. |
| 11 | Recorded point/span c64 client p99 is 16.1/26.1 ms; server p99 is 13.1/24.2 ms. CPU was about 29/32 cores. Decompression, filtering and skipped-record decoding are large overlapping categories. | Reprofile with the exact binary, then compare smaller pages. The old profile used offset-based symbolication against a rebuilt binary; percentages are not precise current attribution. |
| 12 | Existing session registries and per-query file planning are already reused. SQL is parsed twice. Existing logical/physical plans contain query-specific permissions, parameter values, files, live routes and execution state. | Reuse the validated statement within its request. Keep cross-query caching parked: the claimed 10–13% planning share is an estimate and does not justify a rebinding/cache subsystem. |

Item-9 historical records used dirty snapshots; compare matching diff hashes,
not just their common base commit. Their effective 120-second windows differ
from default benchmark controls. They are diagnostic history, not a clean
TASK-007 baseline. Item-11 raw profile location is unavailable in this checkout;
the recorded analysis is not an independently reproduced profile calculation.

## Requirements, boundaries and completion proof

All base specification invariants continue, especially exact results,
authentication/authorization, ack-after-durability, bounded ownership, portable
execution, attributable records and advisory performance targets. A performance
miss is not invalid evidence. Do not manufacture host load or build during an
experiment window. Do not weaken tests or change benchmark validity rules.

| Obligation | Owner | Observable proof |
|---|---|---|
| T007-9: group writes across shards in one process log | Task 9 | Two shards in one group require one sync of its active segment; all touched segments sync on rollover; existing c64 case at 1/16 shards with physical counters. |
| T007-9D: ordered durability, upgrade and retirement | Task 9 | v8 recovery, v9 interleaved replay, crash boundaries, no early acknowledgement or shared-segment deletion. |
| T007-10: one exact representation across readable tiers | Task 10 | Same values/types through active/frozen memory, staging, hot publication and compaction; local and peer reads. |
| T007-10L: stable layouts and fresh whole-claim discovery | Task 10 | Old/new layouts coexist; keys exclusively in residual data are discoverable; no current generation changes layout on refresh. |
| T007-10M: complete bounded memory ownership | Task 10 | Conversion and old readers are charged; pressure refuses/defer work safely before new WAL writes; no leaked reservations. |
| T007-11: profile-led page improvement | Task 11 | Matching-binary attribution and isolated fresh-fixture comparisons with explicit retain/revert rules. |
| T007-12: parse once without stale bindings | Task 12 | One parse within the executing Oracle's validation-to-planning path; fresh parameters, permissions, providers, source cut and physical execution root. Independent forwarding validation remains. |
| T007-I: integrated behavior and attributable costs | Integrated closeout | Real client/server journeys, combined crash/read transitions, then selected existing unprofiled writes/hot/Parquet cases. |

## Parallel work and the shared boundary

Tasks 9 and 10 may be implemented simultaneously in isolated branches. Neither
needs the other's implementation to begin. Agree this existing prepared-slice
contract before branching, and preserve it in both branches:

- Canonical IPC bytes, logical schema fingerprint and logical batch digest are
  computed by preprocessing and are the only inputs to WAL identity/retry checks.
- Retained Arrow rows may have a different physical layout, but have exactly
  the same values, order and row count as those canonical bytes.
- The shard owns generation/layout selection and memory admission. It finishes
  optional shredding before submitting that work to the WAL owner. The WAL owner
  never interprets a learned Variant layout.
- Only successful WAL durability plus the existing tenant SQL fence authorizes
  insertion/visibility/acknowledgement. Conversion failure before submission
  cannot create durable-but-unreported work.

These tasks overlap in shards, admission and recovery tests. Resolve those seams
on integration; do not have two agents mutate the same shared checkout files.
Task 10 owns layout binding and retained rows; task 9 owns WAL scheduling,
record routing, durability callbacks and segment lifetime. No private type or
file allowlist is imposed.

Task 11 analysis and Task 12 development may also run independently. Integrate
Task 11's shared writer recipe after Task 10's writer changes. Run Cargo-backed
commands sequentially per TESTING.md. Performance runs are always exclusive.
Use a clean common baseline and only the existing selected cases relevant to
the changed behavior. Measure a coherent task candidate, not every private edit.
Reuse compatible baseline results and avoid rerunning unrelated cases. A combined
gain cannot establish which individual change caused it.

## Task 9 — one process WAL, shared durability groups

Task ID: T007-9. Dependencies: items 1–8 already completed; shared boundary above.
Maps: T007-9, T007-9D, T007-I and base durability/resource invariants.

### Outcome, decisions and approach

Today separate shard files multiply flushes. Extend the existing WalWriter and
execution lanes into the sole process-wide append/sync owner. Do not add a
second log service. Shard actors still own their queues, retry decisions,
memtables and tenant fences.

1. Keep existing hierarchical tenant/table fairness and the existing maximum
   64-request shard group. The process owner drains already-admitted groups in
   round-robin shard order into a durability cycle, with at most one ready group
   per configured shard (at most 64 times the configured shard count requests),
   additionally bounded by existing byte admission, and no additional linger.
   Do not cap the whole process cycle at one shard's 64-request allowance: that
   would prevent cross-shard batching under load. Preserve order within a shard;
   duplicate identities use existing retry handling rather than a second append.
2. Write one ordered SLICE/COMMIT stream. Sync each distinct segment touched by
   the cycle; a cycle fitting one segment takes one file sync regardless of how
   many shards participated. Rollover legitimately requires more than one.
3. After sync, return completion to each owning shard. Keep tenant fences scoped
   by tenant and using existing SQL transactions. Make each shard's rows visible
   and acknowledge only after its fence succeeds, in shard submission order.
4. Overlap the next append/sync cycle with the prior cycle's SQL fences and
   insertion. Initially allow one sync cycle plus one durable cycle completing
   fences, and at most one unresolved cycle per shard. A slow shard cannot admit
   a second unresolved cycle. Other shards may use the next cycle while global
   cycle credits remain. Do not append to a segment while its sync holds its
   mutex; append-during-sync is not required by this task.
5. Existing byte admission bounds every queued group and all retained work.
   One additional cycle credit is reserved before overlap; release it only at
   settled completion or transfer to recovery ownership. No unbounded central
   queue or future list; do not increase the WAL pool as part of this change.

With one unresolved cycle per shard, a one-shard run cannot overlap its next
cycle with its own prior fence. This task deliberately bounds that ambiguity;
it does not promise a single-shard pipeline speedup. Multi-shard runs can share
syncs and overlap work for shards whose preceding fence has settled.

Normative completion order:

    admit and finish retained-row conversion
    enqueue canonical WAL records with shard identity
    append all cycle SLICE and COMMIT records
    sync every touched segment
    complete each owning shard's durable fence
    insert and publish that shard's rows; acknowledge

A sync error is a shared Scribe-write failure: stop further append/admission,
retain all uncertain work and files, and use existing readiness/recovery paths.
A fence failure follows existing conservative shared poison/retention behavior;
do not invent partial-success retries. Oracle queries over already authoritative
data and unrelated server capabilities need not crash. Shutdown stops admission,
settles bounded work when possible and leaves unresolved work replayable.

### Persisted format, upgrade and retirement

Use WAL version 9 for new writes. Every v9 SLICE and COMMIT record carries its
own checked shard identity, covered by the record checksum. The v9 segment header
identifies a process stream, not one shard. Store v9 segments using the existing
shard-0 directory and segment naming/discovery rules so the v8 reader discovers
and rejects the unsupported version rather than silently overlooking live data.
This physical location does not assign its records to logical shard 0.
Preserve existing node/epoch identity,
stream-wide LSN ordering, batch identity and record payload meanings. The byte
encoding must be documented next to the existing format and pinned by fixtures;
never repurpose v8 bytes without a version change.

Upgrade policy: read existing v8 shard segments with the existing v8 decoder,
recover them through their existing manifests/fences, and create only v9 segments
after replay. Retain v8 files until their recovered owners permit retirement.
Never rewrite/delete them merely to upgrade. There is no mixed-version writer
on one process data directory. Downgrade after a v9 write is unsupported: the old
binary must refuse it without deleting files. Public SDK compatibility is
unchanged. This narrow durable-reader migration is not a public alias/shim.

Per-shard durable manifest suppression remains per shard. A shared segment can
be removed/recycled only when it is closed, no shard has an active append into
it, and every generation/replay owner referencing it has durably retired. Reuse
the existing writer-global path-reference and deferred-delete machinery; rebuild
all references before replay permits deletion. One shard's watermark never
suppresses another shard's record. Corruption checks use record shard identity;
when corruption prevents identifying the shard, conservatively refuse recovery
unless the existing durable evidence proves the entire affected suffix unneeded.

### Ordered implementation scenarios

**Scenario 9A — shared group durability.**

**Behavior.** Two admitted shards share a segment/sync cycle; rows remain hidden
until sync and their fences complete. Rollover syncs both touched segments.
Fair scheduling prevents one tenant/table/shard monopolizing admission.

**RED.** Add planned lib test S9A below; current shard-local streams cannot
satisfy its one-segment/one-sync assertion. Inject sync failure and assert no
ack or memory visibility, with all reservations retained/released correctly.

**GREEN.** Extend the existing writer/lanes and shard completion path as above;
preserve fair queues, per-shard ordering and bounded cycle credits.

**REFACTOR.** Remove superseded per-shard physical writer state; retain one
format/recovery owner and rerun earlier durability tests.

**Scenario 9B — mixed-version recovery and shared retirement.**

**Behavior.** Start with durable v8 work, restart into v9, append interleaved
shards, crash at append/sync/fence/visibility boundaries, and recover each
acknowledged batch exactly once. Retiring one shard cannot remove another's log.
The preserved v8-only reader discovers v9 paths and refuses the unsupported
version without removing files; verify this against a disposable copy with the
saved pre-upgrade binary as well as version/discovery unit coverage.

**RED.** Add planned S9B; current decoder/routing cannot read v9 record-level
shards. Exercise incomplete tails, corruption, duplicate retry and deferred
deletion, including a restart before all shared references are rebuilt.

**GREEN.** Implement v9 decoding and routing plus explicit v8 recovery. Extend
global segment ownership/closure and existing manifest suppression.

**REFACTOR.** Keep version-specific encoding inside the WAL owner, not in shards
or SDKs. Run S9A and existing closeout/restart coverage.

### Acceptance, reuse and affected boundaries

Use existing native-write-c64 at 1 shard and 16 shards, with its current windows
and repetitions. Compare baseline/candidate rows/s, Arrow MiB/s, client/server
tails, CPU and existing physical sync/queue/fence/backlog counters. Fewer physical
syncs per accepted request/byte must explain the intended improvement; a throughput
change alone does not establish its cause. Default shard count remains 1.

The two selected endpoint runs form a quick comparison with a combined
five-minute budget, including setup. If they cannot complete within that budget,
record the setup/runtime limitation and stop; do not extend windows, add a sweep
or change benchmark controls. No 2/4/8-shard or 120-second runs are required.
This explicitly replaces revision 19's exhaustive scaling proof for this task.
Report whether 16 shards improve over 1; do not claim a complete scaling curve,
maximum sustained capacity or a hardware limit from these short endpoint runs.
Do not remove unrelated staging/publication syncs.

Reuse evidence: scribe/shards.rs process_group (3211), existing fair scheduler
(622), scribe/execution_lanes.rs (2130), scribe/wal.rs SegmentHeader (135),
WalRecord (395), sync_segments (2509), path-reference retirement (2697), replay
(3603), and scribe/manifest.rs (149). They already own the required lifecycle;
their missing behavior is cross-shard physical ownership and record-level routing.
Inspect their closeout, crash, restart and retirement tests before editing.
Likely closure includes bench controls and Bifrost WAL/recovery authority;
no SDK contract or generated schema changes are expected.

Material stop: a proposed alternative drops durability, introduces another DB,
permits deleting live v8 files, changes public acknowledgement meaning, or
requires more unresolved cycles than the explicitly bounded pipeline above.

## Task 10 — shredded memory and staging, fresh final-file selection

Task ID: T007-10. Dependencies: items 1–8; shared boundary above, not Task 9.
Maps: T007-10, T007-10L, T007-10M and T007-I.

### Outcome and the complete flow

Before a layout is learned: canonical memory → canonical staging → whole-claim
sample → shredded hot files. After learning: shredded memory → same shredded
staging → whole-claim sample of complete values → newly shredded hot files.
Existing chunks/files keep their layout; different layouts coexist safely.

The sample never counts memory plus its staged copy twice. Existing coherent
capture selects one readable copy. Sampling itself reads staged claim inputs,
not published hot files and not an additional sample of current memory.

### Fixed decisions and implementation sequence

1. Preserve canonical preprocessing/WAL IPC, fingerprint and digest. At the
   shard, pin the current cached layout when opening a generation for a SealKey.
   Keep that layout until the generation closes. All rows routed into it use
   that pinned layout, even if preprocessing completed before a cache refresh.
   The shard passes the chosen layout to existing admitted CPU execution work,
   waits for conversion before WAL submission, and holds no memtable/capture
   lock while conversion runs. Do not independently choose layouts in preprocess.
2. Use the existing native Variant typed-plus-residual representation, not a
   permanent duplicate batch or hidden parallel columns. Reserve output/scratch
   and any original/native overlap before conversion. Release original rows once
   conversion completes and canonical IPC owns WAL input. If reservation cannot
   fit, apply existing admission/backpressure before WAL submission; do not mix
   an ad-hoc canonical fallback into an already pinned physical generation.
   A generation with no learned layout remains canonical throughout its life.
3. Separate logical table schema from physical retained schema. The logical
   fingerprint identifies assembly/retry/table compatibility; physical changes
   must not split one claim or change digest. Preserve typed fields through both
   memory projections until Variant-aware predicates run. Separate physical
   sources when layouts differ; reconstruct canonical projected results before
   their union or authenticated remote output. No custom SQL Variant semantics.
4. Extend the existing rolling Parquet writer with native-input preservation.
   If input already matches the selected physical file schema, write it directly
   with ArrowWriter; never call layout.shred on it. Staging copies the frozen
   generation's layout. Keep logical footer identity, checksums, preflight,
   ready-record durability and leases. One shared writer supports canonical and
   native input; do not create separate staging pipelines.
5. Keep existing whole-claim principal counts, sample probability and selection
   rules. Reconstruct complete Variant values from typed leaves plus residual
   data before offering sampled input. Initially reconstruct bounded projected
   batches with existing recursive unshred_variants; no JSON text roundtrip and
   no new direct reshredding kernel. Accept physical-layout differences only
   after checking the same logical schema and tenant. Merge bounded canonical
   batches, then shred them into the newly sampled final layout. Do not merely
   union prior selected keys: previously residual keys remain discoverable.
6. Return the learned layout from successful claim assembly to a process-local,
   governed advisory cache keyed by exact tenant/table/version. Reuse it across
   partitions of that definition; claim sampling remains principal-stratified.
   Refresh on successful assembly completion, in completion order. Sampling or
   publication retries may install equivalent hints; no correctness depends on
   publication order. Only later generations use a refresh. No cross-process
   coordination, catalog recipe or bootstrap footer scan is introduced. Each
   process learns from claims it assembles; a cold/restarted process stays
   canonical until then. Eviction releases unused cache ownership; active
   generations keep their charged pinned layout. Bound cache bytes within the
   existing Scribe governed memory budget, not an unbounded per-table map.
7. Keep final Parquet/compaction files self-describing. Compaction continues
   deriving layouts from source footers, without discovering new residual keys.
   Preserve its existing exact nested-only Variant fallback: the current guard
   emits canonical data for tables having no top-level Variant. Test equal
   values, but do not claim typed-layout preservation for that case. No external
   managed-compaction fork/pin change is required or authorized by this task.

Exact sample policy remains: 99% confidence, ±2 percentage points, finite
population correction, expected about 4,147 samples per large principal stratum;
principals below 30 rows pool; existing effective eligibility about 8%; 1,000
tracked/300 emitted children per object node; depth 50. These are selection
heuristics, not a global memory ceiling or an exact sample-row count. Charge
principal maps, counters and reconstruction scratch to existing assembly memory;
on exhaustion use existing retry/backpressure and retain recovery evidence.
Do not silently substitute a smaller population or a truncated biased sample.

Mixed types, missing/null and unselected keys stay exact through residuals.
DataFusion's typed-text fast path can fall back for the entire batch when any
residual value remains. Measure that limitation; do not promise universal direct
column access or add a new kernel/pruning subsystem in this task.

### Ordered implementation scenarios

**Scenario 10A — immutable generation layout and memory ownership.**

**Behavior.** A generation opened before learning is canonical. New generations
use the learned layout; refresh does not alter existing generations or late
preprocessed appends. Captures are coherent and all conversion/reader ownership
remains charged until released. Pressure before WAL admission is safe.

**RED.** Add S10A below: interleave preprocessing, generation creation, refresh,
capture and conversion failure. Assert exact values, stable physical layout,
unchanged WAL bytes/digest, no partial visibility and bounded ownership.

**GREEN.** Implement shard-bound layout selection, governed cache/conversion,
logical/physical schemas and Variant-aware memory filtering/output.

**REFACTOR.** Reuse existing retained ownership and unshared_arrow_bytes rather
than duplicate accounting. Run earlier tests after every projection change.

**Scenario 10B — mixed staged layouts and fresh discovery.**

**Behavior.** One claim includes canonical staging and different shredded layouts.
Select a frequent key present only in residual values, retain a minority
principal's eligible key, preserve missing/null/numeric/nested/list distinctions,
and write final files with the newly selected layout. Foreign tenant/schema
files remain rejected. No second shred is attempted on native staging input.

**RED.** Add S10B below. Current merge rejects physical inequality; current
sampler/writer does not support complete native input. Include full-column and
literal-path query checks rather than checking schemas alone.

**GREEN.** Preserve native staging, reconstruct complete bounded sample/merge
input, retain logical validation and populate the advisory cache.

**REFACTOR.** Share existing rolling writer and recursive Variant reconstruction;
keep one sampler policy. Replace the old unshredded-only staging assertion with
stronger exact round-trip/recovery assertions, not a weakened test.

**Scenario 10C — real lifecycle and peers.**

**Behavior.** Real SDK writes are read from active/frozen memory, local/peer
staging and published/compacted files with identical values and tenant-negative
results. Hold readers across transitions; restart from shredded staging and
canonical WAL. Read every row once. A cold cache never prevents recovery.

**RED.** Add S10C journey below and retain nested-only compaction exactness
coverage using existing fixtures. Current staging/memory behavior cannot satisfy
native physical checks; the existing nested-only fallback itself is regression
proof and needs no manufactured RED.

**GREEN.** Close follower projection, staging recovery, cache-loss, compaction
and ownership seams without changing authenticated result schemas.

**REFACTOR.** Keep native layouts internal. Rerun existing capture/lease and
compaction recipe tests before measuring performance.

### Acceptance, reuse and affected boundaries

Prove cold and learned paths separately. Establish exact result/recovery parity
before performance experiments. Compare writes throughput/CPU, conversion peak
memory and backlog using relevant existing writes cases. Measure memory filters
with the approved learned-layout hot setup and file filters with selected existing
Parquet cases. Use matched data and verify the intended typed/residual path.
Correctness journeys prove native staged layout, exact reads and safe transitions;
there is no separate staged timing benchmark or independently measured staged
latency claim. Staged and hot files share the file reader, but that fact alone
does not prove equal latency. Targets remain advisory; a correctness pass is not
a measured speedup.

The ordinary hot benchmark is cold-only: it starts a fresh process and its
residence guard forbids every staged/claimed/published row, so it cannot learn
from a completed claim. Do not weaken that guard or report its results as learned
shredding. Add one explicit --learned-layout diagnostic to the existing Bifrost
L1 hot workflow, requiring --steps and rejecting --smoke/other workflows:

- Declare 16 GiB memory/disk rotation thresholds and 90-second retention for
  this diagnostic only, rather than the ordinary hot fixture's 24-hour retention.
- In the same server process and same tenant/table/version, load a warmup cohort
  in day 1. Wait normal production rotation and successful claim assembly, verify
  a nonempty learned layout, and wait for warmup publication to settle. Do not
  change the table definition/version, restart, or mistake client Bifrost.flush
  for server sealing.
- Load identical-shaped measured rows in day 2. Restrict every measured filter
  and expected count to day 2. Run existing span-shredded-c1 and add the missing
  span-shredded-c64 diagnostic case. Use three 5-second windows per case; finish
  all measured work within 90 seconds of the first day-2 row. Warmup, rotation,
  loading and measurement must also fit the existing five-minute L1 budget;
  fail the diagnostic setup honestly if they cannot. No budget extension.
- Scope residence evidence to the measured partition/row cohort. Permit only
  already-settled warmup files; require exact measured results and zero measured
  file scans. Target staging/publication or expiry of the 90-second deadline
  invalidates this diagnostic rather than silently measuring a different tier.
- This selected diagnostic is history-ineligible. Use the existing record,
  server/client histogram and CPU reader; keep normal hot controls unchanged.
  It has no automatic retry that changes retention or data size mid-experiment.

Reuse S10C's ordinary correctness journey for staged coverage: learn through a
real claim using WyrdTestServer and its existing flush_bifrost test helper; read
canonical and learned-layout generations through memory, staging and publication.
Assert exact values, native typed columns and source selection. Hold an existing
capture/lease across publication and release it normally. No repeated timing
loops, extra performance environment variable or new staged benchmarking barrier.

Approved learned-layout invocation after implementation:

    mise run bench:l1:bifrost:hot -- --learned-layout --steps span-shredded-c1,span-shredded-c64

Add a focused declaration/guard test proving warmup files do not fail measured
residence checks, while a measured-tier leak or deadline overrun does:

    mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::learned_layout_diagnostic_scopes_residence_and_deadline)'

Reuse evidence: PreparedSlice/prepare_slice in preprocess.rs (432,789), Memtable
freeze/ReadableSource in memtable.rs (607,2110), SealKey in seal_key.rs (105),
member_stager.rs (181), shared parquet_writer.rs (368,693,755), claim_sample.rs
(60), claim_merge.rs (221), claim_assembly.rs (253,292), follower.rs (442,465)
and memory.rs unshared_arrow_bytes. They own these transitions already. Extend
them and existing source-capture/lease tests instead of adding another lifecycle.
Pinned Iceberg/Arrow supply recursive reconstruction; pinned managed compaction
supplies footer combination, with the explicit nested-only canonical fallback
above. Do not edit Cargo caches, vendor dependencies or change dependency pins.

Material stop: changing wire/digest semantics; sharing hints across tenants or
table versions; unaccounted native/original overlap; sampling only residuals;
using ordinary casts as reconstruction; changing statistical selection policy;
or requiring mandatory remote layout coordination.

## Task 11 — smaller Parquet pages, chosen by a bounded comparison

Task ID: T007-11. Analysis independent; integrate recipe after Task 10.
Maps: T007-11, T007-I and base REQ-028. Proposed scope, not previously approved.

### Analysis, recommendation and implementation

Current shared recipe is ZSTD level 3, 128 MiB row groups, 256 KiB dictionary
pages, default data pages (1 MiB/20,000 rows), and 1,024-row write batches. Page
indexes/filter pushdown/Bloom reads are enabled. The existing range cache stores
compressed bytes, so cache hits still pay decompression. Another footer cache
would duplicate existing metadata priming.

First capture a fresh exact-binary point/span profile and matching unprofiled
baseline. Retain binary hash/build ID and matching symbols before rebuilding.
Attribute CPU/query, scanned rows/bytes, page/decompression work and p99; do not
sum inclusive profile percentages or infer waiting from CPU samples.

Compare exactly these existing writer settings, rebuilding fixtures each time:

| Recipe | Data-page bytes | Rows/page | Everything else |
|---|---:|---:|---|
| Baseline | 1 MiB | 20,000 | Existing recipe |
| First candidate | 256 KiB | 4,096 | Unchanged |
| Second candidate, only if first fails retain rules | 64 KiB | 1,024 | Unchanged |

Both limits are best-effort, checked between write batches. Configure them in the
shared writer recipe used by Scribe and Forge; bump its existing recipe version
and metadata together. No eager rewrite of existing files. Newly staged/hot and
normal compaction output use the chosen settings; old files remain readable.

Predeclare the decision using three paired unprofiled repetitions and medians:
retain the first candidate only when point-c64 and span-trace-c64 CPU/query each
fall at least 10%, neither server p99 worsens by more than 5%, and at least one
server p99 improves by at least 5%. Also require no greater than 5% regression
in full-scan/aggregate server p99 or writes-parquet rows/s, and no more
than 10% increase in persisted bytes for the same fixture. These are proposed
experiment selection rules, not new product SLOs or benchmark validity rules.

If the first fails, evaluate only the second against the same baseline/rules.
Retain it only if all rules pass. If neither passes, restore baseline recipe,
record the negative result, and stop page tuning. If the fresh profile contradicts
the decode-work premise, skip recipe changes and record no supported fix under
this task. Codec changes, decoded-page caching and more query threads are not
fallback implementations authorized by this plan. Never mark the 5 ms point-c64
server goal achieved using client timing or a different span target.

### Ordered implementation scenarios

**Scenario 11A — finer pages preserve exact pruning.**

**Behavior.** The production recipe writes multiple smaller pages; page-index
selection reads fewer candidate rows for a selective predicate and returns the
same complete result, including residual Variant matches. Scribe and compaction
use the same versioned recipe. Page limits are not asserted as hard maxima.

**RED.** Add S11A below using the existing real Parquet writer/scan fixture;
inspect actual page boundaries and selected rows, not merely setter constants.

**GREEN.** Apply only the selected candidate's existing byte/row setters and
recipe metadata. Existing read/filter logic stays upstream-owned.

**REFACTOR.** Keep one shared recipe. Rerun pruning, foreign-footer and
compaction coverage. If no candidate passes, revert production tuning and retain
the experiment evidence; no manufactured GREEN or performance claim is required.

### Acceptance, reuse and material stops

Preserve exact results, tenant footer checks, current source cuts, scanned-byte
accounting and memory budgets. Report before/after server and client p99
separately, CPU/query, page structure, scanned rows/bytes, write throughput and
storage ratio. Targets remain advisory even when no candidate is retained.

Reuse evidence: parquet/writer_properties.rs writer_properties (150), Scribe
parquet_writer.rs (765), Forge managed/policy.rs (368), oracle/exec.rs
hot_parquet_scan (1142), metadata priming (1557), resources.rs (4118) and
storage/range_cache.rs (19). No query engine/cache replacement is necessary.
Stop for inconclusive attribution or proposed codec/cache/planner expansion;
report that evidence rather than inventing another optimization.

## Task 12 — parse once inside Oracle planning; keep cross-query plans parked

Task ID: T007-12. No implementation dependency on 9/10/11; final profile after
their integration. Maps: T007-12, T007-I and base REQ-021/REQ-028.

### Analysis, recommendation and implementation

Do not cache current logical or physical plans across requests. Logical plans
retain tenant/permission-bound providers, files and live routes; physical roots
also retain cancellation, deadlines, dispatch and resource bindings. Safe reuse
would require rebuilding those bindings and a new bounded cache. Parsing is only
about 1.5% of recorded CPU, and total planning's 10–13% share is not exact.

Inside the executing Oracle, carry the validated parsed statement into fresh
per-request planning using an internal validation result. Preserve the public
validate_query return contract used by server forwarding. Use statement_to_plan
rather than feeding the SQL string to a second parser. Table extraction and
SELECT validation inspect that same statement. Continue fresh provider/catalog
construction, parameter binding, optimization, optimized payload permission
checks, source-cut selection, sizing/admission and physical planning on every
request and on any required repin attempt. A retry may reuse its unbound AST,
but never an optimized plan containing earlier parameter/provider bindings.

The server ReadyOracleForwarder independently validates SQL before local or
remote dispatch. Preserve that ingress/trust-boundary validation, including its
local route. Its parse is outside the executing Oracle's one-parse promise.
Do not send ASTs over the forwarding wire, remove receiver validation, or change
public validation errors to meet a misleading end-to-end parse count. Include
both local and remote forwarding routes in the unchanged-schema journey proof.

Keep QueryProfile, benchmark records/readers and reports unchanged. Use existing
profiler output for attribution, existing CPU/query and client/server latency
measurements for comparison, and the focused test for the one-parse assertion.
Do not add parse/logical/physical timing fields. No cross-query cache, SQL
normalization, parameter-value cache, persisted plan or result cache is introduced.

The observed available_parallelism calls occur inside pinned Iceberg constructors;
setting a concurrency option afterward does not remove them. Do not add an
upstream CPU-topology cache to this task. Record that separate opportunity with
the fresh profile; it is not a prerequisite for the bounded parse correction.

### Ordered implementation scenarios

**Scenario 12A — one parse, fresh query bindings.**

**Behavior.** The executing Oracle parses once across validation and planning;
forwarding retains its independent ingress validation. Same SQL in requests with
different parameters/tenants/payload permissions/schema/source cuts gets fresh
bindings and correct results or existing errors. Volatile expressions are not
frozen. Repinning does not retain stale providers or execution locks.

**RED.** Add S12A below using existing planner validation/permission fixtures;
assert one parser invocation inside that execution boundary and fresh binding
behavior, including SELECT/CTE validation and payload-denied negative cases.
Prove local/remote forwarding still validates without carrying a parsed AST.

**GREEN.** Carry the validated statement to statement_to_plan inside existing
per-query session/planner owners; keep all authorization and admission gates.

**REFACTOR.** Remove the duplicate SQL-string parse path without introducing
another parser wrapper/cache or retaining previous request state.

### Acceptance, reuse and material stops

Semantic correctness and one parse per executing Oracle validation/planning
path are mandatory. Reuse selected existing point/span cases before/after for
total CPU/query and client/server p99; use existing profiler samples only when
attribution is needed. No benchmark instrumentation changes are required. Retain the correction only with no greater than 5% median
regression in total CPU/query or server p99 across three matched repetitions;
otherwise investigate/revert the change without expanding to a cache. No fixed
end-to-end speedup or 5 ms promise follows from removing a small CPU component.

Reuse evidence: oracle/planner.rs validate_query (47), oracle/mod.rs
plan_logical (3172), parse_select_tables (3405), per-query planning (2875,2942),
oracle/session.rs OracleSessionTemplate (20), oracle/profile.rs (50),
oracle/exec.rs OracleTableProvider (1430) and planned_files (1627), and
oracle/bindings.rs (78). Existing integration/oracle archived bodies are not
executable tests; wire any new integration module into its live target.
Server consumer: crates/wyrd/wyrd-server/src/oracle/forwarding.rs forward (223)
and execute_local (330); keep its public validation call and wire body unchanged.
Stop if parsing cannot be carried without changing validation/error semantics,
or a proposal retains bound query state across requests. Cross-query reuse stays
parked unless a later separately approved design establishes need and rebinding.

## Verification and evidence commands

Run every command through mise. Confirm planned test selectors after adding the
test and before RED; a zero-test run proves nothing. These test names are planned,
not claims that tests already exist. Each scenario's RED/GREEN evidence includes
its setup, expected failure, critical assertions and final selector.

Focused new lib tests (package/target confirmed from current owner):

    S9A: mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::shared_wal_cycle_preserves_fair_durable_completion)'
    S9B: mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::wal::tests::v8_recovery_and_v9_shared_retirement_are_exact)'
    S10A: mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::generation_layout_binding_keeps_canonical_wal_and_bounded_memory)'
    S10B: mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::mixed_staged_layouts_preserve_values_and_fresh_discovery)'
    S11A: mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::shared_recipe_smaller_pages_preserve_exact_pruning)'
    S12A: mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::validated_statement_is_parsed_once_with_fresh_bindings)'

S10C is a planned real Rust SDK → WyrdTestServer/Postgres → SDK journey in the
existing scribe target; extend companion Python/TypeScript/MCP journey owners to
check unchanged returned values where their current Bifrost surfaces ship.

    mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::variant_layouts_survive_memory_stage_publication_and_restart)'"

Required existing focused regressions (each command selects exactly one test):

    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::wal_closeout::sync_failure_has_no_ack_or_memtable_visibility)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::wal_closeout::failure_after_fsync_before_ack_reuses_stable_batch_once)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::wal::tests::segment_retirement_waits_for_every_generation_reference)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::wal::tests::wal_reader_orders_records_by_stream_wide_lsn_across_shards)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::pg_scribe_restart::replay_memory_is_bounded_by_owner_backpressure)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::claim_sample::tests::writer_with_its_own_stratum_keeps_its_key)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::claim_merge::tests::a_run_under_another_schema_is_refused)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tail_rpc::tests::a_generation_staged_before_the_shard_settles_is_read_once)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::page_index_selects_only_the_matching_pages)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=forge::compact::tests::compacted_output_uses_shared_writer_properties)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority)'
    mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::exec::tests::planned_files_are_reused_for_the_same_pushed_filters)'

Task-local lanes: test:bifrost:integration:redux and the affected
test:bifrost:journey:scribe/oracle/forge lanes. Run fmt/lints for changed Rust;
check:deps and check:tenant-isolation when affected; codegen:check only if a
generated contract input changes. Do not run broad gate for this change.

Experiments reuse existing L1 controls and windows. Build first; run no concurrent
experiments. Select only cases relevant to a coherent task candidate. The existing
experiment supplies its three repetitions; do not add an outer repetition loop.

Item 9 endpoint comparison (combined five-minute budget; no duration override):

    WYRD_SCRIBE_SHARD_COUNT=1 mise run bench:l1:bifrost:writes -- --steps native-write-c64
    WYRD_SCRIBE_SHARD_COUNT=16 mise run bench:l1:bifrost:writes -- --steps native-write-c64

Items 11/12 point/span confirmation, selecting c64 first:

    mise run bench:l1:bifrost:parquet -- --steps point-c64,span-trace-c64

Use the existing --profile option once with matching binary/symbols when item 11
needs fresh causal attribution; use unprofiled results for performance comparison.
Do not profile every candidate or add new profile/record fields. If the page
candidate improves the selected queries, confirm its read/write trade-offs using
only the existing full-scan/aggregate and writes cases needed by Task 11's retain
rules. Resolve those existing IDs from the declaration, without adding cases.
No full L1 matrix is mandated for every change or at integrated closeout.

Item 10 uses the approved learned-layout invocation in its task section. Reuse
selected existing writes cases for ingestion cost and existing Parquet filter
cases for file-read checks; the ordinary hot setup remains the cold control.
Capacity CLI owners for the sole extension are
crates/wyrd/wyrd-testing/src/bin/capacity/levels.rs and capacity/query/mod.rs,
plus the existing telemetry fixture/expected-count owner. Extend the accepted
flag set only for --learned-layout in Bifrost L1 hot. No duration flag, new timing
framework or record/report changes are permitted.

Profiled, partial, selected-step and dirty-source runs remain diagnostic/history
ineligible under existing rules. Retained full records must be clean, complete
and unprofiled. Report per-change evidence even when a performance target misses.

## Integrated closeout and authority updates

1. Integrate both ingestion tasks; test shared-WAL recovery with mixed native
   generations/staging, cache loss and memory pressure. Exercise failure after
   shared sync and before one tenant's fence; no duplicated or missing rows.
2. Run focused proof, then once on the integrated candidate run
   mise run verify:bifrost, including its real multi-language journeys. This
   integrated capability gate does not replace task-specific named commands.
3. Confirm the integrated candidate using only the selected existing L1 cases
   affected by integration; reuse still-compatible task evidence. Keep the
   five-minute L1 budgets, ordinary windows and record/report contracts unchanged.
   No long sweep or full-matrix rerun is required. Publish no speedup unsupported
   by matched measurements or learned-memory claim based on the cold hot setup.
4. Update architecture/bifrost-design.md in the owning task: process WAL format,
   v8→v9 upgrade/recovery and shared retention; native memory/staging layouts,
   complete-value sampling and advisory layout learning; page recipe if retained;
   planning ownership if clarified. Update operations recovery/upgrade guidance
   for the v9 downgrade restriction. Keep public wire/SDK schema unchanged.
5. Record scenario RED/GREEN results, exact commands/selectors, per-change
   measurements, retain/revert decisions and unresolved advisory target misses.
   Existing benchmark record validity and history ownership remain unchanged.

## Source references

- Base authority: spec.md revision 19; tasks/TASK-006-R2-revision-19-supplement.md;
  changes/spec-driven-development.md; changes/implementation-execution.md;
  TESTING.md; AGENTS.md and the Rust/maintainer/agent-surface guides.
- Independent evidence: codex-reviews/item-9-diagnosis.md and
  codex-reviews/item-10-recommendation.md with their prompts, committed db29ee915.
- Architecture: architecture/bifrost-design.md storage/Variant, WAL, resource,
  live read, cache and compaction sections; architecture/wyrd-security-posture.md
  tenant/data isolation; architecture/operations/ upgrade/recovery owners.
- [Apache Parquet native Variant shredding](https://parquet.apache.org/docs/file-format/types/variantshredding/)
  defines exact typed-plus-residual storage.
- [Pinned Arrow shredding](https://github.com/bohmian-ai/arrow-rs/blob/ca5feff3b1c523640a03acaa437096b8934c3749/parquet-variant-compute/src/shred_variant.rs)
  rejects already-shredded input; use existing recursive reconstruction at rewrite boundaries.
- [Pinned DataFusion Variant evaluation](https://github.com/bohmian-ai/datafusion/blob/19796a13ed46877cf998013886947b28a6f774b3/datafusion/functions-variant/src/get.rs)
  supports native leaves but has whole-array residual fallback for text.
- [Pinned Parquet page properties](https://github.com/bohmian-ai/arrow-rs/blob/ca5feff3b1c523640a03acaa437096b8934c3749/parquet/src/file/properties.rs)
  provide existing byte/row page controls and best-effort semantics.
- [Logfire selection](https://pydantic.dev/articles/dynamic-shredding-2026-01-26)
  ranks 128 keys by total bytes; it does not publish Wyrd's statistical policy.
  [DuckDB](https://duckdb.org/docs/2.0/sql/data_types/variant#writing-variant-to-parquet)
  and [ClickHouse](https://clickhouse.com/blog/a-new-powerful-json-data-type-for-clickhouse)
  use different learning windows; no industry-wide sampling rule is assumed.

No material decision is delegated to measurement by an open-ended tuning loop:
the experiments above have fixed candidates, retain/revert rules and an honest
no-change outcome. Private helper shapes, byte-layout mechanics preserving the
v9 contract and ordinary fixture organization remain implementation-owned.
