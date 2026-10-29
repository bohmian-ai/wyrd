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
   must not split one claim or change digest. Preserve physical layouts
   through capture-local projection and predicate evaluation; adapt
   expressions per layout; reconstruct canonical projected results before
   capture output, union, or authenticated remote output. Expression
   adaptation is DataFusion's (`DefaultPhysicalExprAdapterFactory`); Wyrd
   classifies no query shape, and any optimization the adapter lacks belongs
   in the DataFusion or Iceberg fork. No custom SQL Variant semantics.
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

## Task 9 implementation evidence

Branch `item9-shared-wal` from 0aa09ae05. Commits: 5f9415dbc (skip covered
segment syncs, refuse syncs after a fault), 69fd5d360 (one v9 process stream),
cd02f330e (S9A), cefd5610d (S9B), d56cff07d (authority docs).

What landed:

- WAL v9 (`scribe/wal.rs`). Record magic `WYRDWAL9`, 80-byte record header;
  byte 72 is the writing shard, bytes 73..80 must be zero, all covered by the
  record CRC. The v9 segment header names no shard (nonzero is refused). The
  decoder dispatches on the segment header version and requires each record's
  version to equal it; v8 records take their shard from the segment header.
  Segments live in `shard-00` under the v8 naming, so a v8 binary discovers
  them. Its header check (`version != WAL_VERSION` → `UnsupportedWalVersion`,
  base wal.rs:361) refuses them at startup without deleting anything. New
  writes are only v9; v8 files are read in place and never rewritten.
- One writer state replaces the 256 per-shard states: one append position,
  plus one `ShardGeneration` per shard (own bytes and records for rotation,
  and the segments it touched). A generation's first append to a segment
  takes one retirement reference. Rotation hands the generation's segments to
  the cohort (`retain_segments`), then drops the generation's references
  (`retire_segments`). Selective members treat active-generation segments as
  referenced. Deferred deletion skips the current segment and any path that
  was retained again.
- Replay keys accumulators by (stream, record shard). Fences use the
  segment-header lane (`ReplayedSealKey.wal_lane`: the shard in v8, 0 in v9).
  Each shard's watermark manifest is chosen by the record's shard. A shared
  segment is retirement-safe only when every shard reading it is.

Deviation, to be decided by review: there is no central round-robin cycle
scheduler (decision 1 / point 4 wording). Shard actors stay serial, which
keeps at most one unresolved cycle per shard and the existing completion
order (append → sync touched segments → fence → insert/publish → ack).
Cross-shard sharing comes from sync coalescing instead: an append marks its
segment unsynced; a sync of a segment with nothing unsynced records no
fdatasync (span outcome `covered`); a faulted writer refuses every sync. So
concurrent shards whose appends one fdatasync already covered do not issue
another. No cycle credits or overlap pipeline were added. Within one segment,
an append waits on the segment mutex while a sync holds it.

Removed: `close_segments_if_unowned` (writer, handle and its shards.rs call
site). With a shared current segment, one shard detaching it would end every
other shard's appends, so the behavior no longer exists. Its test
`low_volume_current_segment_closes_when_last_bucket_detaches` was deleted with
the behavior; the retirement it protected is covered by S9B. Also removed:
`WalWriter::new_with_shard_id`, which had no non-test caller.

Tests (RED was reasoned, not run, for both: on 0aa09ae05 two shards write to
separate `shard-NN` streams, and records carry no shard):

- S9A `scribe::shards::tests::shared_wal_cycle_preserves_fair_durable_completion`.
  Shards 0 and 7 append and then sync, and get exactly one fdatasync per
  touched segment: one, or two after a forced rollover. The records carry
  shards [0, 7] in `shard-00`. Two real owners joined concurrently ack, and
  their visible rows equal the acked rows. After an injected sync failure,
  neither group acks, the WAL is faulted, no rows become visible, and
  Raw/Decode/Prepared/Queued bytes return to 0.
- S9B `scribe::wal::tests::v8_recovery_and_v9_shared_retirement_are_exact`.
  Starts with v8 per-shard fixtures at epoch 1 and an interleaved v9 shared
  segment at epoch 2, where only one shard has a watermark. Restart replay
  (with no rebuilt references) yields exactly
  {(1,a,a),(1,b,b),(2,b,0)}: the watermark suppresses only its own shard.
  The v8 files and the shared segment survive. A flipped shard byte fails
  the CRC. Live retirement keeps the shared segment through: one shard's
  release, the current-segment deferral, and a re-retained deferred path.
  The last release deletes it.
- Commands (all PASS):
  `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p vala-bifrost-redux --lib`
  (896/896), the same with `--features test-support --test integration`
  (51 passed, 23 skipped by the profile), each focused selector above,
  `mise run fmt`,
  `cargo clippy --locked -p vala-bifrost-redux --lib --tests --features test-support,bench-support -- -D warnings`,
  and `cargo check --locked -p wyrd-server --all-targets`.

Not done:

- The native-write-c64 1/16-shard benchmark comparison (no bench runs in this
  task; it belongs to the integration step).
- A crash matrix at each append/sync/fence/visibility boundary beyond the
  existing crash-injection and closeout suites.
- Running the saved pre-upgrade binary against a v9 copy. The downgrade
  refusal is shown from base code, not executed.
- The cycle scheduler and overlap credits described above.

Authority: `architecture/bifrost-design.md` (shared stream, generation
references, per-record-shard manifests, v8 upgrade and unsupported
downgrade), `operations/deployment-and-release.md` (v9 rollback boundary),
and `operations/reliability-and-recovery.md` (restart routing, shared
retirement).

## Task 10 implementation evidence

Status: PARTIAL. The correctness lifecycle (S10A, S10B, S10C) is done. The
Variant-aware memory filter and the `--learned-layout` diagnostic are not.

Implemented:

- `scribe/layout_cache.rs`: a process-local advisory cache.
  - Keyed by exact tenant, table and table version; holds at most 256 entries.
  - Each entry is charged to the governed root (`Metadata`).
  - Installed from `StagingRuntime::assemble` when a claim succeeds, in
    completion order. An unshredded layout removes the hint; a failed charge
    drops it.
- Shard (`ShardOwner::bind_slice_layout`):
  - Each slice takes the open generation's pinned layout. With no generation
    open, it takes the cached layout, and only when that layout's logical
    fingerprint matches the rows.
  - A memo per group keeps every slice of one generation in agreement.
  - Shredding runs on `ScribeIngressCpuPool` before WAL submission, with no
    memtable lock held.
  - Before conversion, the root grows by an upper bound: twice the rewritten
    Variant bytes plus 40 bytes per slot per shredded node, counting list
    elements. After conversion it is resized to the exact retained bytes.
  - WAL IPC bytes, logical fingerprint and digest are untouched.
- Memtable:
  - A bucket pins its layout and keeps the logical schema.
  - Captures reconstruct logical Variant storage
    (`schema::logical_column`) before rows leave `ReadableSource::project`.
- Rolling writer (`VariantPlan::Staged`/`Final`):
  - Staging writes native input as is, with logical footer identity.
  - Final objects shred logical batches.
- Claim sample and merge:
  - Both reconstruct complete values (`unshred_variants`, then a cast to
    canonical storage only inside Variant storage).
  - The merge accepts a physical difference only after the footer's logical
    fingerprint and tenant match.
- Recovery: `restore_context` reconstructs a run's logical schema
  (`schema::logical_schema`) before the key fingerprint check. The S10C
  journey found this gap: a restart over shredded staging failed WAL recovery.
- Compaction: unchanged.
- `architecture/bifrost-design.md` § Variant shredding: updated.

Tests (all passing):

- S10A: `cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::generation_layout_binding_keeps_canonical_wal_and_bounded_memory)'`
- S10B: `... -E 'test(=scribe::parquet_writer::tests::mixed_staged_layouts_preserve_values_and_fresh_discovery)'`
- S10C: `scripts/postgres/with-test-postgres.sh -- ... cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=telemetry::variant_layouts_survive_memory_stage_publication_and_restart)'`
- Regressions: `recovery::variant_staging_restores_and_publishes_once`,
  `telemetry::staged_backlog_survives_abrupt_restart` and
  `source_boundary_recovery::scribe_failure_retry_replay_remain_atomic`.
- Full `vala-bifrost-redux` lib suite under test Postgres: 897/897.
- `mise run fmt` and `mise run lints`: clean.

Not done:

- Variant-aware memory filtering. Memory captures reconstruct canonical rows
  at projection, so predicates run on logical rows. This is correct, but
  learned memory generations do not yet filter on typed leaves. A
  learned-layout memory read pays reconstruction and gains no filter speedup.
- The `--learned-layout` Bifrost L1 hot diagnostic, its `span-shredded-c64`
  case, and the guard test
  `tests::learned_layout_diagnostic_scopes_residence_and_deadline`. Global
  `Residence` gauges cannot scope evidence to the measured partition, and the
  separate server process exposes no learned-layout signal. This needs a
  server metric and harness orchestration that could not be validated without
  running a bench.
- S10A does not interleave a conversion failure or a capture held during
  conversion. S10C holds no capture or lease across publication and makes no
  tenant-negative read. Both journeys check exact values and layout evidence
  only.

Open risks:

- Item 9 integration. `bind_slice_layout` assumes one serial group per shard
  (write → sync → fence → insert). If item 9 overlaps the next group's
  `write_group` with the previous group's insert, a generation's layout could
  disagree with a slice chosen before the insert. That fails after commit
  (`insert_with_layout` refuses the mismatch) and poisons memory.
- The conversion bound is conservative. For large list-heavy batches it can
  exceed the ingress sublimit and refuse appends (backpressure) for as long
  as a learned generation stays open.

## Task 9 outcome: reverted, with measured write-path findings

The shared process WAL was reverted in 1e9680f3c. Scribe again writes one WAL
stream per shard. Kept per segment: the covered-sync skip (5f9415dbc) and
appends that proceed while a sync runs (8461ff6a5, by the maintainer's
decision; this replaces the task's "no append during sync" line). Added:
write steps report `wal_bytes_per_row`, `staged_bytes_per_row`,
`published_bytes_per_row`, `wal_syncs` and `stage_syncs` (87efb8d8f), and boot
runs at least one WAL IO worker per shard (70dad7c6b).

`native-write-c64`, medians of 3 repetitions, all VALID. Logs are in the
session scratchpad, named as in the first column.

| Run | Code | Shards | rows/s (min–max) | requests/s | WAL append p50 | WAL sync p50 |
|---|---|---|---|---|---|---|
| base | db29ee915 | 1 | 1.18M (0.57–1.60M) | 72 | 52 ms | 180 ms |
| base | db29ee915 | 16 | 1.96M (1.95–2.90M) | 120 | 81 ms | 67 ms |
| cand | b8b55fda2, shared WAL | 16 | 1.20M (1.07–1.56M) | 73 | 138 ms | 49 ms |
| opt1 | 8461ff6a5, shared WAL | 16 | 1.39M (0.35–1.94M) | 85 | 98 ms | 93 ms |
| rev | 87efb8d8f, per-shard WAL | 1 | 1.79M (1.13–2.41M) | 109 | 48 ms | 104 ms |
| rev | 87efb8d8f, per-shard WAL | 16 | 2.88M (1.80–3.02M) | 176 | 73 ms | 56 ms |
| io16 | rev with 16 WAL IO workers | 16 | 2.96M (2.32–3.28M) | — | 3 ms | 60 ms |
| ev | 70dad7c6b | 16 | 2.43M (1.93–3.90M) | — | 2.8 ms | 111 ms |

Bytes written per row at 16 shards (`rev`): WAL 38.7, staged 31.4, published
18.1. WAL syncs per window: 40 at 1 shard and 733 at 16 shards, which is 23.6
and 2.1 acknowledged requests per WAL sync.

- **Symptom:** at 16 shards the shared WAL measured below per-shard streams,
  and per-shard throughput stays well below the CPU's capacity: the server
  uses 4–7 of 32 cores.
- **Evidence:** during the `ev` run, a sampler read every server thread's state
  and kernel wait channel every 20 ms, and `/sys/block/nvme1n1/stat`,
  `/proc/vmstat` and `/proc/pressure/io` every 100 ms. Over the 36 loaded
  seconds:
  - WAL IO threads were blocked in the kernel 73% of the time: 32% waiting on
    page writeback, 17% on ext4 journal commits, 11% on block IO, and 10%
    appending behind a journal commit. They were idle 25% and running 1.5%.
  - The device was 96% busy at the median, writing 300 MiB/s, at 58 cache
    flushes per second taking 25 ms each (p90: 254 flushes/s, 81 ms each).
  - IO pressure (`some`) was 85%.
  - Dirty-page throttling took 0.8% of WAL thread time, and dirty pages
    peaked at about 208 MiB, under the 256 MiB limit.
- **Cause:** durable filesystem and device IO. The WAL IO pool's four workers
  also queued one shard's append behind other shards' syncs (append p50
  73 ms → 3 ms with one worker per shard). The shared-WAL experiment
  serialized appends on one file, and it recorded no sync counts, so it did
  not test whether shared syncs reduce syncs per request.
- **Fix site:** WAL IO lane sizing at boot (`ScribeRuntimeConfig::wal_io_workers`).
  No further write-path change is justified by this evidence. The cost of a
  sync is specific to the host's device and filesystem; the portable property
  is syncs per acknowledged request, which grows with shard count. An
  independent read-only diagnostician (Codex) reviewed the same raw data and
  reached the same conclusion.

Run-to-run spread on this host is wide: the `rev` and `ev` 16-shard runs
differ by up to 2×, while device flush time ranges from 25 to 163 ms. Every
10-second window ended with a staged backlog of up to about 1.4 GB, so these
are short-window figures, not steady state. A future shared-sync change needs
an A/B that counts completed syncs per request, holds staging constant, and
runs to steady state.

## Task 12 implementation evidence

91dc80e93 parses a query's SQL once in `OraclePlanner::validate_statement`
and plans the stored `DfStatement` with `statement_to_plan` and fresh
parameter bindings for each attempt. Test:
`oracle::tests::validated_statement_is_parsed_once_with_fresh_bindings`.

`mise run bench:l1:bifrost:parquet -- --steps point-c64,span-trace-c64`, VALID
before (db29ee915) and after (b8b55fda2):

| Step | CPU per query | server p99 | client p99 |
|---|---|---|---|
| point-c64 | 3.633 → 3.733 ms (+2.8%) | 11.30 → 11.01 ms | 15.22 → 15.12 ms |
| span-trace-c64 | 6.937 → 7.001 ms (+0.9%) | 22.18 → 22.88 ms | 25.89 → 26.24 ms |

Within the 5% retain rule; no measurable gain. Kept because it removes a
second parse from every attempt.

## Scribe journey failures: diagnosis and fix

Four `wyrd-testing --test scribe` journeys failed at 0aa09ae05 and later. An
independent read-only diagnostician (Codex) read the traced run
(`WYRD_LOG=info,vala_bifrost_redux=debug`) and the history, and found two
causes. Both are test setup that disagrees with intended production
behavior; no assertion changed.

- **Claim budget** (`lifecycle::scribe_publishes_due_claims_concurrently_within_the_claim_budget`,
  `lifecycle::concurrent_flushes_share_the_claim_budget_and_publish_each_claim_once`).
  - Symptom: "9 claims were in flight at once; the claim budget is 4".
  - Evidence: `boot/mod.rs` passes `persistence_cpu_threads` to
    `ScribePersistenceConfig::new`, and `persistence.rs` builds the
    assembler's claim budget from that worker count.
  - Cause: e8e8342b9 sized staging workers, and so the claim budget, by
    `persistence_cpu_threads` (20 on this host) instead of `wal_io_threads`
    (4). The tests still assumed 4.
  - Fix site: the harness gains
    `WyrdTestServerBuilder::with_scribe_persistence_cpu_threads_for_test`,
    and both journeys pin the budget to `CLAIM_BUDGET`.
- **Unsettled audit write** (`telemetry::scribe_hot_path_telemetry_reconciles`,
  `write_read::acknowledged_rows_survive_stage_pressure_and_restart`).
  - Symptom: "the age tick did not publish the frozen memtable", and "1
    staged Scribe writes did not settle within 60s".
  - Evidence: the trace shows a `bifrost.system.audit_log` batch written
    after the freeze, and an audit outbox write refused as `saturated` once
    the test filled the shared root.
  - Cause: the final read stages an audit row on the outbox, which delivers
    in the background (edc1f2eac added the 50 ms linger). The tests froze
    telemetry or filled memory before that row landed.
  - Fix site: both journeys call `WyrdTestServer::settle_bifrost_outbox`
    after the read.

Verification: `cargo nextest run --locked -p wyrd-testing --test scribe -P
journey --run-ignored=only` under the test Postgres: 41 passed.

## Task 11 outcome: no page recipe retained

Result: NEGATIVE. Neither candidate met the predeclared retention rules, so
the production recipe was restored to parquet-rs's default data pages
(`d9b06b5f5` reverts `5c0991a50` and `71cbf1932`). No GREEN or performance
claim is made.

**Profile premise (held).** Samply on HEAD `0bd122d6d`, point-c64 and
span-trace-c64, symbolized against the profiled `wyrd-server`, CPU-weighted
on the query worker threads (`tokio-rt-worker`): Parquet frames are 29.9%
inclusive, page decode 15.3%, zstd page decompression 12.1%. Opening the
object file on every query (`File::open` plus `statx`) is another ~11%; that
is outside this task and is recorded, not acted on.

**Recipe version.** The task asks to bump the writer recipe's "existing
recipe version". None exists: `scribe/write_recipe.rs` versions a table's
physical recipe, and rewrite selection compares sizes, not page settings. No
version was invented. Every producer (Scribe staging, Forge compaction and
rewrite) builds from `parquet/writer_properties.rs::recipe_builder`, so the
candidates were two setters there.

**Runs.** Each column is one full `bench:l1:bifrost:parquet` plus one full
`bench:l1:bifrost:writes` run (each step: warmup and 3 measured repetitions,
medians), all VALID, run back to back on the same host.

| Metric (median) | Baseline `0bd122d6d` | 256 KiB / 4,096 `5c0991a50` | 64 KiB / 1,024 `71cbf1932` |
|---|---:|---:|---:|
| point-c64 CPU/query | 3.846 ms | 2.529 ms (−34.2%) | 2.595 ms (−32.5%) |
| point-c64 server / client p99 | 11.31 / 15.53 ms | 7.28 / 10.38 ms | 7.54 / 10.81 ms |
| point-c64 scanned bytes/query | 183 KB | 74 KB | 64 KB |
| span-trace-c64 CPU/query | 7.155 ms | 6.795 ms (−5.0%) | 6.592 ms (−7.9%) |
| span-trace-c64 server / client p99 | 22.30 / 25.98 ms | 21.96 / 25.54 ms | 20.86 / 24.42 ms |
| span-trace-c64 scanned bytes/query | 689 KB | 988 KB | 1,037 KB |
| full-scan-c1 server p95 (no p99: too few samples) | 52.2 ms | 59.4 ms (+13.8%) | 45.4 ms (−12.9%) |
| native-write-c64 rate | 128.5 req/s | +14.5% | +1.8% |
| native-write peak rows/s | 2.10M | 2.14M (+1.8%) | 2.41M (+14.5%) |
| otlp-spans-c64 rate | 556.1 req/s | +1.3% | −6.7% |
| storage ratio native / logs / spans | 0.080 / 0.118 / 0.071 | 0.085 / 0.119 / 0.079 | 0.082 / 0.125 / 0.091 |

| Rule | 256 KiB / 4,096 | 64 KiB / 1,024 |
|---|---|---|
| point-c64 and span-trace-c64 CPU/query each ≥10% lower | FAIL (span-trace −5.0%) | FAIL (span-trace −7.9%) |
| neither c64 server p99 >5% worse; one ≥5% better | PASS | PASS |
| full-scan/aggregate server latency ≤5% worse | FAIL (+13.8%) | PASS |
| writes rows/s ≤5% lower | PASS | FAIL (otlp-spans −6.7%) |
| persisted bytes ≤10% higher | FAIL (spans +11%) | FAIL (spans +28%) |

**Why span-trace does not benefit.** Point lookups filter a sorted column,
so one small page holds the match and scanned bytes fall 60–65%. A
`trace_id` lookup's 8 rows are spread across the group, so smaller pages
mean more pages, headers and index entries to touch: scanned bytes rise
43–51% and the decode saving is mostly cancelled. The full-scan swing
(+13.8% then −12.9%) is larger than any plausible recipe effect and is
treated as run-to-run noise. Both candidates fail on span-trace CPU and span
storage, which do not depend on it.

**S11A.** The page-selection test (`oracle::exec::tests::page_index_selects_only_the_matching_pages`)
was extended to assert the pages actually written (largest value page below
the 20,000-row default) and the rows kept (fewer than one default page). It
was RED on the baseline (kept 20,480 rows; baseline page starts 0, 20480,
32768, 53248, 73728, 94208) and GREEN under both candidates with all 898
`vala-bifrost-redux` lib tests and clippy passing. It was reverted with the
recipe, because it asserts the candidate's behavior.

Records: `benchmarks/history/bifrost/parquet/20261029T020424Z-0bd122d6d.json`,
`20261029T022644Z-5c0991a50.json`, `20261029T024612Z-71cbf1932.json`;
`benchmarks/history/bifrost/writes-parquet/20261029T020913Z-0bd122d6d.json`,
`20261029T023134Z-5c0991a50.json`, `20261029T025100Z-71cbf1932.json`.
Profile: `target/benchmarks/bifrost/parquet/20261029T015852Z/profiles/`.

Not tried, because the task forbids it as a fallback: per-column page sizes
(small pages only on sorted point-lookup columns), codec changes, and
decoded-page caching. Per-column page sizing is the evidence-backed
follow-up if point-lookup CPU becomes the goal again.

## Per-query planning costs: two kept, one reverted

Profile of steady point-c64 and span-trace-c64 load (one sample weighs one)
named three costs that every filtered query pays on the one planning path.
Each was changed, measured with the full parquet experiment, and kept only
if it helped every query shape.

| Change | Commit | Outcome |
|---|---|---|
| Probe host parallelism once per Iceberg scan build, and only for an unset limit (iceberg-rust `wyrd-production` 469cf4aff, iceberg-compaction `wyrd-production` 41662c251); Oracle sets its listing limit to the session's target partitions | 45391226d | kept |
| Share one never-reported pruner metric set across scan sizers | 122ff3a0a | kept |
| Prune each planned footer once and hand the plan to the Parquet opener as its initial access plan | 8f8265f8a | reverted (459617bae) |

Same host, full plan, all VALID:

| Step | Baseline (fe6ef51a9) | Kept changes (459617bae) | Change |
|---|---:|---:|---:|
| point-c64 requests/s | 7,469 | 8,226 | +10.1% |
| point-c64 queries/CPU-s | 260.0 | 285.6 | +9.8% |
| point-c64 server p50 / p99 ms | 5.38 / 11.31 | 4.99 / 10.79 | −7% / −4.6% |
| point-c1 requests/s | 477.9 | 497.1 | +4.0% |
| span-trace-c64 requests/s | 4,064 | 4,166 | +2.5% |
| span-trace-c64 queries/CPU-s | 139.8 | 144.4 | +3.3% |
| span-trace-c64 server p99 ms | 22.3 | 23.6 | +6% |
| span-trace-c1, small-aggregate, full-scan, shredded, long-tail | — | — | within ±2% |
| storage ratio native / logs / spans | 0.08 / 0.118 / 0.071 | unchanged | — |

The point-c64 p99 goal (< 5 ms) is still missed.

**Why the access-plan hand-off was reverted.** With it, span-trace lost
5–9% throughput (c64 p99 25.7 ms). Its profile showed row-group pruning
rising from 9.8% to 14.8% of CPU: the planning pass cost 6.3%, sizing 3.7%,
and the opener still 4.8% of its former 5.2%. Pruning cost is per call,
not per row group, because each call rebuilds the pruning predicate
(`build_inverted_predicate` 4.3% → 6.3%), so skipping row groups saved
almost nothing. The opener's metric contract it had to preserve (planned
skips counted as `row_groups_pruned_statistics`) went with it.

**Left alone.** DataFusion's `CachedParquetFileReaderFactory` builds a full
`ParquetFileMetrics` on every file open (2.7% of span-trace CPU) beside the
opener's own; removing it changes a public `ParquetFileReader` API in the
DataFusion fork. Logical optimization (~15% of each lookup) is DataFusion's
one planner doing its normal work.

Verification: `vala-bifrost-redux` lib (898 passed), clippy `-D warnings`,
`wyrd-testing --test oracle -P journey --run-ignored=only` (59 passed with
the hand-off in place; `remote_live_window_blocked_scribe_stops_cleanly`
failed once at 46 s in the first full run and passed in both reruns, not
diagnosed), iceberg-rust `scan::` tests (63 passed, new
`test_table_scan_concurrency_limits`).

Records: `benchmarks/history/bifrost/parquet/20261029T132713Z-122ff3a0a.json`
(all three), `20261029T134546Z-459617bae.json` (kept two). Span-trace
profile with the hand-off: `target/benchmarks/bifrost/parquet/20261029T133235Z/profiles/`.

## Item 10: capture-local Variant-aware memory filtering

Owner: `ReadableSource::project` (`scribe/memtable.rs`). The follower already
decodes the signed filter over the closure and sends it to the capture as
`BatchPruning`. For a generation whose projected columns are in a learned
physical layout, the capture now:
1. projects the stored columns without converting them;
2. rewrites the predicate with DataFusion's `DefaultPhysicalExprAdapterFactory`,
   once per generation;
3. keeps only the rows that match;
4. reconstructs those rows exactly as before.

Rows still leave the capture in the logical schema
(`bifrost-design.md` §Variant shredding). Expression adaptation is the
DataFusion fork's:
- a function that accepts any layout reads typed leaves;
- any other read of a shredded column gets the adapter's by-value Variant
  cast.

Wyrd classifies no query shape. A predicate that does not adapt or evaluate
filters nothing in the capture, because the plan's own filter still runs. A
filtered batch's bounds become inexact. Canonical generations take the path
they took before. Decision 3 was reworded to match (approved).

Rejected first attempt: keeping physical columns past the capture and
choosing filter order by Variant function name in `MemtableSource`. It was a
query-type-specific path, and it contradicted the design's capture boundary
(independent read-only Codex review).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Shredded memory rows filter before reconstruction, by any predicate | `scribe/memtable.rs` `ReadableSource::project`, `physical_projection` | `scribe::shards::tests::generation_layout_binding_keeps_canonical_wal_and_bounded_memory`: RED with the capture filter disabled (all rows returned), GREEN with it on | PASS |
| Capture output stays logical; memory/stage/publish/restart unchanged | same | journey `telemetry::variant_layouts_survive_memory_stage_publication_and_restart` | PASS |
| No regression on live reads | same | `vala-bifrost-redux` lib (898 passed), clippy `-D warnings`, `wyrd-testing --test oracle -P journey --run-ignored=only` (59 passed) | PASS |
