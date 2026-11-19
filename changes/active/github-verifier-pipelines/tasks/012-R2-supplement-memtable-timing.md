---
id: TASK-012-R2-S1
status: proposed
approved_spec_revision: 17
supplements: TASK-012-R2
obligations: [REQ-018, REQ-019, REQ-022, REQ-023, REQ-024]
---

# Continue TASK-012-R2: lazy memtable reads and timing qualification

## Current state and pickup point

Continue [TASK-012-R2](012-R2-hot-read-optimization.md); do not restart its
baseline implementation. The original UI SQL catalog, typed answer checking,
default/10× fixtures, learned-layout arm and benchmark selectors already exist.
TASK-012-R1's integrated memory/CPU owners are available and its
maintainer-completed status is not reopened.

The previous implementor recorded these full clean, unprofiled baselines at
`6fc40b230`, each reported VALID with zero wrong answers:

| Experiment | Existing baseline record |
|---|---|
| Default | `benchmarks/history/bifrost/ui-sql/20261119T010259Z-6fc40b230.json` |
| 10× | `benchmarks/history/bifrost/ui-sql-10x/20261119T010539Z-6fc40b230.json` |
| Learned | `benchmarks/history/bifrost/ui-sql-learned/20261119T010926Z-6fc40b230.json` |

These are the comparison starting points. Read them through the supported
readers before scheduling another run. Older withdrawn records are not evidence.
The parent task retains gaps: scenario 2's placement/journey proof is marked
unrun, and scenarios 4–7 have no recorded completion evidence. Baseline success
does not close those scenarios. Check the actual checkout and updated parent
evidence before editing; preserve any work completed since this handoff.

The recorded baseline reports lack executed-plan scanned rows/bytes, despite
their VALID verdicts. Parent scenario 3 says missing required source facts make
measurement INVALID. Resolve that discrepancy without silently discarding the
completed baselines or treating absent facts as zero. Reuse existing timing and
profile evidence where available; verify that the parent's pre-edit attribution
checkpoint is satisfied before production read edits.

Repository inspection identified the immediate execution gap:

- Capture selection already retains fixed source references and statistics.
- `tail_rpc` opening calls `LiveCapture::read`, which eagerly projects sources
  into a complete result vector. `ReadableSource::project` performs row work.
- The Oracle follower also materializes/project-collects before constructing
  its source. `MemtableSource` already has some pull-driven filtering; reuse it.
- Shared source charges, query workspace/output reservations and stream-owned
  lifetimes already exist. Extend those owners rather than replacing them.
- No batch merging was found in these owners. Memtable physical scan bytes are
  intentionally absent when that field means Parquet bytes.

The review supplied with this handoff did not rerun benchmarks or independently
verify the proposal's numerical cost attribution. Read recorded evidence before
using its totals to justify a change.

## Authority and decision

Read and follow these sources before implementation:

- [Parent task](012-R2-hot-read-optimization.md), including existing evidence
  and remaining scenarios; [approved revision 17](../spec-revisions/17-hot-read-optimization.md)
  and [selected design](../design/task-012-r2-hot-reads.md).
- [`benchmarks/AGENTS.md`](../../../../benchmarks/AGENTS.md) is the source of
  truth for benchmark execution, evidence validity, reading, comparison and
  reporting. The repository directory is plural: `benchmarks/`. Read its linked
  README for supported reader commands and flags. If this supplement's command
  examples conflict with that owner, follow the owner and correct this document.
- Root/directory `AGENTS.md`, the owning Bifrost design sections, and existing
  implementation/testing guidance.

Implement descriptor-only opening, demand-driven bounded execution and the
required conservative UTF8/fixed-binary identity bounds. Measure the lazy-read
change before choosing additional batching machinery. Query-time batch
coalescing is deferred; it is not implied by “adapt once per generation.”

This supplement supplies continuation order and review clarifications within
revision 17. It does not replace the parent task, create a prerequisite that the
parent first complete, revise its contract or claim implementation approval.
Record assertions, actual commands, RED/GREEN and performance evidence in the
parent's existing scenario entries; do not create another evidence ledger here.

## Ordered continuation

### 1. Complete measurement attribution and scan facts

Locate whether missing scan facts originate in execution, transport or the
report reader, and fix that existing owner. Reuse the metrics facade and
completion/reporting path; do not introduce another recorder.

Define decoded memtable rows/bytes separately from Parquet physical bytes.
State whether counters describe examined input before filtering or emitted
output, and count actual work without draining unused input. Preserve the
existing meaning of `physical_bytes_scanned`; do not populate it with Arrow
memory bytes. Test the counters and their connected reporting path.

If the records contain the facts but the reader omits them, repair the reader
and reread the existing baseline. If required facts were never recorded, retain
the originals as historical evidence with the explicit limitation and obtain
the missing baseline evidence on the unoptimized read path. Do not repeat
baseline setup or measurements that already answer the question. A needed clean
source checkpoint remains maintainer-owned; do not commit automatically.

Confirm the required diagnostic attribution on baseline code/data. Separate
CPU, waits, layout, scanning and allocation costs and connect them to the
selected changes. Aggregate totals over different query counts do not establish
comparable per-query cost or rule out planning as a contributor.

### 2. Make work follow demand

First add a failing test through the actual DataFusion source: opening a
multi-batch capture performs no row expression evaluation or reconstruction.
Then extend the existing `scribe/memtable.rs`, `scribe/tail_rpc.rs`,
`oracle/follower.rs` and `oracle/memtable_source.rs` flow:

1. Retain fixed source descriptors, statistics, generation/schema metadata and
   resource owners during opening; metadata-only selection is allowed.
2. Remove eager result collection in Scribe opening and the Oracle follower.
3. Reuse expression adaptation per generation; filter, project and reconstruct
   only when the execution stream requests the next bounded batch.
4. Reuse existing byte-sized partitions, typed paths, zero-copy buffers,
   CPU lanes and memory reservations. Keep large appends bounded.
5. Keep the exact captured snapshot through later writes/publication. Never
   reopen current state to resume a capture or recover from a failure.

Moving eager work to another thread does not satisfy this outcome. Do not add
a scanner service, cache, pool, scheduler, dependency or configuration option.

### 3. Prove the connected execution and safety behavior

Exercise canonical and learned sources through actual consumers. Map proof to
parent scenarios 2, 4 and 6 rather than repeating their contract here:

- Pulling yields exact expected rows; covered typed paths avoid unnecessary
  root reconstruction and large retained appends remain bounded.
- Where execution can stop early, dropping prevents evaluation of later
  batches. Sorted top-N may legitimately consume all qualifying input.
- Writes/publication after capture do not change that capture's result.
- Local and remote paths preserve failure/terminal behavior for drop,
  cancellation, expiry, memory exhaustion, later expression failure and
  capture loss. No successful partial result or fresh-state reopen is allowed.
- Source buffers, workspace, outputs and staged leases retain their charges
  for actual lifetimes, including late children, and release exactly once.

Retain existing exact-filter, static/all-match pruning, unknown fallback,
filter/fetch/projection rewrite, dynamic/volatile expression, field-ID/schema,
mixed-source and publication-deduplication coverage. Reuse existing proof where
behavior is already correct; do not manufacture RED for unchanged behavior.

### 4. Measure lazy execution before adding other machinery

Smoke-run each UI selector separately, then run its full experiment one at a
time under the benchmark owner's rules:

```sh
mise run bench:l1:bifrost:ui
mise run bench:l1:bifrost:ui:10x
mise run bench:l1:bifrost:ui:learned
```

Use the unchanged corpus, binds, schedules, host and settings. Compare with the
matching baseline using `mise run bench:results` and its documented arguments.
Dirty variants are diagnostic, not qualification. Do not create commits to
qualify them. Normalize stage costs by comparable query/workload populations;
do not add nested timings or subtract percentiles.

Report what improved and the remaining material costs. A faster open timer
alone is insufficient. If evidence contradicts the selected approach, report
what failed, why and the options before redesigning. Continue the already
approved identity-bounds work unless the contradiction affects that approach.

### 5. Add required identity bounds and measure their cost

Extend existing scalar statistics with conservative UTF8/fixed-binary identity
bounds. This is mandatory scope, not conditional on the lazy-read result.

Use parent scenario 5's test-first cases: catalog ID/time/child-match predicates,
complete/missing/all-null/schema-adapted bounds, safe exclusion, conservative
unknowns, native ordering/null semantics and unchanged pushdown interactions.
Measure additional append CPU/work and retained memory. Repeat the original UI
experiments after this change, preserving comparable evidence between variants.

### 6. Decide any remaining batching work from evidence

Do not concatenate batches solely because their count is high. If tiny batches
remain a material contributor after the selected changes, report normalized
execution costs, allocation/memory evidence and a bounded coalescing proposal
before implementing that redesign. Account for retained original buffers,
workspace and output together; an output-byte bound alone does not bound memory.

Unresolved material slowdown keeps the parent task incomplete unless the
maintainer accepts the evidenced limit. Do not silently expand scope or claim
that completing this supplement alone closes TASK-012-R2.

## Verification and completion

Add actual new test names and exact focused commands to the parent scenario
entries before RED. Use existing owner modules and targets. Run:

```sh
mise run test:bifrost:integration:redux
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:scribe
mise run fmt
mise run lints
```

Retain affected SDK/HTTP/MCP terminal proof; run additional surface lanes where
existing proof no longer covers the changed path. No default L2, broad gate,
frontend build or new test binary is required by this supplement.

After owner checks pass, qualify clean, unprofiled full L1 runs sequentially:

```sh
mise run bench:l1:bifrost
mise run bench:l1:gateway
mise run bench:l1:verification
```

Follow `benchmarks/AGENTS.md` exactly: supported `mise run bench:*` runners,
quiet host, no custom run/retry loops or record parsers, supported report/profile
readers, and comparison against compatible history. Every benchmark report has
only Question / Run / Report / Comparison / Answer / Unproven, in that order,
with the supported report output pasted unedited. Changes inside repetition
spread are “no measured change.” Smoke/profile/subset/dirty runs are diagnostic.

Parent completion requires exact answers across residences/layouts; complete
hot-path improvement beyond repetition spread; reported server/client latency,
first batch, full bundles, throughput, CPU, memory, refusals and scan work; no
correctness, memory, refusal or write-performance regression; and an evidenced
stage/owner/cost/disposition for every material residual contributor. Keep the
inherited Gateway/Verification goals visible without claiming this task alone
achieves them. No new absolute latency or hot/Parquet ratio is introduced.

Finish with the parent evidence updated, remaining limitations explicit and
`git diff --check` clean. Do not commit or push unless asked.
