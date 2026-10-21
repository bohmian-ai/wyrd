---
id: SPEC-bifrost-source-consistency
revision: 1
status: approved
---

# Reliable Bifrost reads after PR #189

## The problem and proposed change

Bifrost reads rows from two places: Scribe's live data and published files.
Today, a row moving between them can disappear from a query or appear twice.
That can leave a group Verifier's final evidence out of its judgment, because
publication alone does not schedule another group read. PR #189 has merged;
this correctness problem remains and will be fixed in a new worktree and PR.

Make Oracle, Bifrost's query engine, choose and retain both sources before it
executes a query. Oracle checks whether publication changed during that choice.
If it did, Oracle starts the selection again before returning any rows. Scribe
also prevents local source changes from interrupting its selection of live rows.
Every reader, including a group Verifier, uses this same corrected service.

For example, a query for three acknowledged evidence rows must still return
those three rows once each when the last row moves into a file during selection.
It must not silently return two or four rows. Two separate rows with identical
values remain separate; SQL `DISTINCT` would hide the bug rather than fix it.

The work includes deleting the superseded read path and qualifying the final
build against verification overhead p99 ≤500 µs and selective-query latency
p99 ≤1 ms. There is one implementation task. The reported failure remains an
accepted input; merging the original PR does not change the problem, proposed
behavior, scope or verification. A separate reproduction investigation is not
a prerequisite.

## Execution base and worktree handoff

Start the follow-up from current `origin/main` containing PR #189, not from the
old feature branch. The merge observed when preparing this handoff is
`1c5edf72958af8f47c63eb8ce39eb316672e170a`; use a later main commit if appropriate
and record the actual base before implementation and baseline measurement.
Recheck source paths, commands and existing evidence against that base.

Carry this specification, its single task and the maintainer-readability edits
to `wyrd-plan`, `wyrd-spec` and the spec-driven-development reference into the
new worktree. Those are currently uncommitted edits and are not in the merged
PR or the separate local commit below. Preserve the old worktree until the
transferred files are verified; worktree creation and Git delivery remain
caller-owned.

Keep local commit `9a93d9310f49ca01f62b0ab032a36afe8bc8b519`, which is not in
main at this handoff. It corrects an asynchronous span-permit test by waiting
for all permits with a deadline. Carry it forward as a separate test-only commit,
not as the Bifrost fix. Record it in the baseline if it is applied before
measurement. If a later main already contains the equivalent correction, do not
apply it twice. It adds no production behavior or scenario to this task.

## How the proposed read works

**Publication** means recording a Scribe file in the catalog so queries can
read it. A **published cut** is the fixed selection of Iceberg data and committed
Scribe files used by one query. A **live capture** retains Scribe's selected
memory batches and staged files, Scribe's local Parquet data. Existing file
leases prevent their deletion until the query releases them. A writer epoch
identifies a writer incarnation, so a restarted writer cannot inherit an old
capture's authority.

1. Oracle reads a fresh published cut and its publication revision from the
   database together. The revision is a per-table counter changed by publication.
2. Oracle discovers live data and asks every selected online Scribe to retain
   its matching rows. A Scribe with no matching live rows explicitly says so;
   disappearance of its final live partition cannot silently skip this step.
3. Each Scribe selects its memory batches and staged files while briefly
   excluding local ownership changes. In particular, publication cannot be
   halfway between catalog commit and local advancement, and writable memory
   cannot freeze into staging halfway through this selection.
4. After all captures complete, Oracle checks the table revisions and writer
   identities again using authoritative evidence. It executes only if the
   acquisition remains valid. Otherwise it releases the entire selection and
   retries within the original query deadline.
5. Execution reads the retained sources. Later publication or cleanup cannot
   invalidate them. Completion, cancellation, failure or expiry releases them.

The ordering closes both races. Publication before the first catalog read is
already represented by the file selection, and Scribe finishes its local change
before capture. Publication during acquisition changes the revision, so that
attempt is discarded. Publication after validation cannot remove retained data.
The local exclusion also prevents a memory batch from being selected again after
it freezes into staging, even when no publication occurs.

## Why this approach, and its costs

This extends Oracle's existing query ownership and Scribe's existing memory
references and file leases. It avoids physical-filename comparisons, which
become unreliable when compaction replaces files. Re-reading groups would leave
other readers exposed. A cluster-wide write pause would coordinate more work
than the source handoff requires.

The cost is query startup coordination: a fresh initial catalog read, a final
validation read, and live capture before the first result row. Required Scribe
captures run with bounded parallelism. Publication conflicts repeat acquisition;
sustained conflicts can exhaust the query deadline. Selective queries and
`LIMIT` may pay more startup cost because they previously opened live sources
only when consumed. These costs must fit the measured latency targets below.

Appends continue. A local rotation or publication may briefly wait for capture,
but the guard does not cover upload, query execution, lease drainage or cleanup.
No catalog round trip is added to each append acknowledgement.

## What is promised, and what is outside this change

The promise covers qualifying acknowledged rows held by the selected online
Scribes when acquisition begins, together with the query's published data.
Tenant, table, partition, predicates and ordinary SQL semantics still bound
what the query returns. Later appends need not be included.

Scribes do not all stop at one instant. Writers unavailable before selection,
accepted outbox loss, post-ACK scheduling-notification loss and retention limits
keep their existing contracts. This work adds no unavailable-writer recovery,
new WAL, evidence ledger or history scan. A selected source that cannot complete
capture cannot be silently treated as a complete successful acquisition.

## Required behavior

- **REQ-001 — Rows survive source changes.** Interactive and analytical reads,
  including group evidence reads, do not omit or duplicate rows because they
  move between memory, staging and published storage.
- **REQ-002 — Select before executing.** Oracle follows the acquisition protocol
  above. Discovery belongs inside validation. It discards a conflicting attempt
  before execution or output; execution never combines different attempts.
- **REQ-003 — Scribe resolves ownership before capture.** A possibly committed
  publication, incomplete local advancement or cancellation cannot leave a
  contradictory source selection readable. Capture reconciles that uncertainty
  or refuses it. Recovery follows the same rule.
- **REQ-004 — Preserve the merged verification workflows.** The existing
  late-evidence group journey passes through corrected Bifrost reads. Keep the
  merged workflows covered by the regression checklist below, without reopening
  PR #189, reimplementing its delivered work or reusing old measurements as
  final proof of this fix.
- **REQ-005 — Remove superseded behavior.** Delete execution-time live
  reacquisition and the stale cache/discovery logic that supported it. Remove
  unused contracts, callers, assertions and documentation left by that replacement.
  Keep useful lazy decoding, metadata caching and independent integrity checks.
- **REQ-006 — Qualify actual capacity.** Preserve a comparable baseline, enforce
  the performance contract below, and run the capacity workloads on the final
  build. Fix failures without weakening correctness, workload or thresholds.

## Rules that must remain true

| Invariant | Promise and failure prevented |
| --- | --- |
| **INV-001 — One server-owned read service** | Oracle owns reads, Scribe owns source authority and Vala SQL owns catalog state. Clients and group scheduling gain no alternate read path or workaround. |
| **INV-002 — Captures belong to one authority** | Tenant, table, query, deadline and writer epoch bind each capture and signed peer assignment. A foreign, substituted or expired reference cannot authorize a read. |
| **INV-003 — Ingestion continues** | No cluster-wide append pause or remotely held publication barrier. Append acknowledgement retains its existing durable boundary and gains no per-append catalog request. |
| **INV-004 — Revisions reflect committed publication** | New publication advances its revision in the same catalog transaction. Identical replay does not invent another publication. Notification-cache age cannot validate a read. |
| **INV-005 — Invalid acquisition is not success** | An unresolved selection emits no rows. Owner loss or deadline exhaustion cannot produce complete success; loss during execution fails rather than reopening live data. |
| **INV-006 — Retention has a bounded lifetime** | Captures use existing query admission, memory, lease, cancellation and deadline budgets. They release on completion, cancelled/failed acquisition or expiry; cleanup cannot delete sources still in use. |
| **INV-007 — Measurements include the real cost** | Refusals, wrong answers and missing required samples fail qualification. Query latency includes acquisition; no samples or coordination time are removed to meet a target. |

## Technical boundaries being approved

Add tenant-scoped durable publication revision state, initialized for existing
catalog rows through repository migration conventions. Both catalog acquisition
and final validation use fresh database evidence. Preserve the existing coherent
hot/Iceberg reconciliation and pinned-file protection across Forge promotion
and compaction. Do not delete retained analytical data.

Extend existing internal contracts and authenticated peer assignments with
capture identity. Public query inputs, SDK APIs and existing structured
error/terminal vocabulary stay unchanged. A lost capture fails its execution;
it does not open a replacement snapshot. Servers roll out the one internal
protocol together; no mixed-version compatibility path is required.

Keep ordinary group scheduling, immutable execution inputs, identity
deduplication and genuine missing-input handling. Source-transition consistency
alone does not prove application-level completeness across input/result tables.
No matching-snapshots loop or additional group scheduling operation is added.
Tenancy, authorization and non-blocking audit retain their existing rules.

## Performance contract and final qualification

| Workload | Required result | Measurement conditions |
| --- | --- | --- |
| Direct verification | Server overhead p99 **≤500 µs** | Existing 1/4/16/64-client points, ≥1,000 samples each, correct verdicts, zero refusals and required memory headroom. Verifier execution remains excluded from this existing overhead metric. |
| Selective Bifrost queries | Client request latency p99 **≤1,000 µs** | Every selective case at the existing one-client latency point, in both hot and Parquet tiers. Includes source acquisition. Preserve peak selective throughput ≥1,200 QPS and existing answer/error checks. |
| Other cases | Existing capacity criteria pass | Keep write throughput, full query sweeps, reads while writing, queue pressure, scans and background verification drain checks. Background verification latency remains reported rather than newly gated. |

Equality passes the two new limits. Replace the old selective 7/10 ms gates with
the p99 criterion; keep reporting p50/p95. Use unrounded values for decisions
and enough output precision to distinguish 1,000 from 1,001 µs. Preserve other
workloads' strict limits. Broad scans and saturated-concurrency queries are not
promised to finish in one millisecond.

Compare the same host fingerprint, release/profiling build, dataset, fixture,
tiers and concurrency. Preserve reports, raw samples, logs and profiles outside
directories overwritten by the harness. Include latency distributions,
throughput, errors, memory and acquisition retries/contention in the evidence.

After all changes and correctness/static checks, run write capacity,
verification capacity, then Bifrost query capacity. The query benchmark is the
last capacity run. An implementation change after qualification requires the
affected correctness checks and final capacity sequence again.

## Acceptance: what evidence establishes success

| Criterion | Observation required |
| --- | --- |
| **AC-001 — Exact row coverage** | A real-client/server query across publication returns the expected row identities once, including when the last live partition disappears. Retained sources survive later publication. A Scribe proof checks the same promise across memory rollover. |
| **AC-002 — Invalid selections stay contained** | Publication/epoch conflicts retry before output; invalid capture authority is refused. Publication uncertainty never supplies contradictory rows. Cancellation, failure and expiry release retained resources. |
| **AC-003 — Group evidence reaches its verdict** | The existing late-evidence group journey passes on this build. Regressions for the merged workflows and first-class client/agent surfaces are accounted for at integrated review. |
| **AC-004 — One current implementation remains** | Diff and documentation checks show the superseded read path and publication-race allowance are gone. Independent integrity checks and historical review evidence remain. |
| **AC-005 — Benchmark verdicts are accurate** | Report tests accept the exact inclusive limits and reject the next representable value/bucket above them while preserving throughput, sample, refusal and background-reporting rules. |
| **AC-006 — Final capacity meets the targets** | The final capacity commands pass on the identified candidate with comparable retained artifacts. Bifrost query capacity runs last. Independent task/change review owns approval of this follow-up change. |

## Regression checklist from merged PR #189

The merged functionality is the starting point. Preserve its behavior under
the corrected reader, with the publication and performance changes stated here.
These are regression obligations for the follow-up, not unfinished delivery or
approval requirements on the already-merged PR:

| Merged behavior | Evidence needed for this follow-up |
| --- | --- |
| Grouped workflows: resumed conversations, delayed refunds, coding without tracing and application-sampled media | Existing Rust/Python/TypeScript journeys preserve ordered inputs, null outcomes for absent results, later-evidence revisions, immutable past judgments and verdict-change reactions. The final-evidence regression passes on the corrected read path. |
| Pressure and existing platform behavior | Existing checks preserve refusal before excess staging, tenant progress, ordinary ungrouped verification, scheduled Drift and Operators. No workaround changes group scheduling or accepted loss limits. |
| Public contracts and source eligibility | SDK/HTTP/MCP paths agree on group IDs, source references, trigger duration and inspection. Wrong tenant/owner/subject/version cannot contribute; ownerless calls keep their existing behavior. Served OpenAPI, runtime MCP schemas, authored stubs, generated types and current docs agree. |
| Scheduled samples and producer attribution | Existing proof shows every declared samples slot is evaluated, insufficient data remains inconclusive, invalid binding inputs are rejected, and recording authenticated owner identity adds no synchronous producer SQL. Eligibility remains background work. |
| Capacity | Final write, verification and query workloads pass the targets and conditions above. Earlier verification reports judged the old limit and do not qualify this build. |
| Independent review | Obtain task and integrated change review for the new fix against its recorded merged-main base. Preserve original delivery/review evidence as history; do not require reapproval of the old tasks or treat the original merge as approval of this fix. |

Integrated review owns the complete Bifrost gate (`mise run verify:bifrost`) and
any required contract/SDK/documentation proof outside that gate, once on the
integrated candidate. The implementation task runs its focused proofs and final
capacity sequence. Preserve old evidence as history; record fresh results where
this change affects it. Branch/worktree creation, push and merging the follow-up
remain caller-owned actions; PR #189 is already merged.

## Status, history and authority

Revision 1 was explicitly approved by the maintainer on 2026-10-21. Approval
covers the proposed protocol, limits, performance targets and verification
obligations. No technical decision is intentionally deferred.

The [long-horizon specification, revision 21](../long-horizon-verification/spec.md)
continues to govern existing grouping functionality. Approval of this change
replaces its publication-race allowances and tightens direct verification's
performance target; its other obligations remain. Update current architecture
and product prose accordingly, preserving historical evidence and that packet.

| Revision | Status | Change |
| --- | --- | --- |
| 1 | approved | Reliable source acquisition and scoped deletion; clarified for maintainer review and execution from merged main in a new worktree. PR #189's merge changes delivery context only, not behavior, scope, targets or proof. Explicitly approved by the maintainer on 2026-10-21. |

- [Repository standards](../../../AGENTS.md), [agent rules](../../../architecture/agent-rules.md).
- [Wyrd design](../../../architecture/wyrd-design.md), [Bifrost design](../../../architecture/bifrost-design.md).
- [OLAP serving](../../../architecture/references/domain/olap-serving.md), [analytical reliability](../../../architecture/references/domain/analytical-operations-reliability.md).
- [Spec-driven development](../../../architecture/references/languages/spec-driven-development.md).
- [Capacity tasks](../../../mise.toml): `bench:verification:capacity`, `bench:bifrost:write-capacity`, `bench:bifrost:query-capacity`.
