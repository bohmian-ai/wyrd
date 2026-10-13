# Readiness re-review: TASK-003-R1

## Verdict: NOT_READY

| Axis | Result |
|---|---|
| Architecture soundness | **FAIL** |
| Decision completeness | **FAIL** |
| Fresh-implementer executability | **FAIL** |
| Cold implementation rehearsal | **FAIL** |
| Critical findings | **1** |
| Major findings | **3** |

The revision substantially improves the packet, but four implementation-defining decisions remain unresolved. Two are data-safety issues: recovery can resurrect retired publications, and canonical whole-column Variant reads can still bypass validation.

## Status of prior readiness findings

| Prior finding | Status | Assessment |
|---|---|---|
| **CRIT-1 — Recovery may overwrite a published immutable object** | **PARTIALLY CLOSED** | FIND-003-14 now correctly requires a catalog check before object-store PUT and a zero-PUT path for an exact committed match. It still lacks an authoritative way to distinguish “never committed” from “previously committed and subsequently retired from `vala.file_list`,” and it does not define partial or contradictory catalog states. |
| **MAJ-1 — D2 does not cover the lifecycle of the pools actually executed** | **PARTIALLY CLOSED** | The coordinator-head lifecycle is now concrete: one close state, propagation through `TaskContext`, monotonic closure, post-close rejection, and `wait_closed()`. Remote worker stages construct separate task contexts and may own their own network pools without passing through `DistributedExec::execute`; those pools remain outside the proposed lifecycle. |
| **MAJ-2 — D2 leaves late connection futures and completion undefined** | **CLOSED** | The revised D2 specifies checks before and after connection establishment, rejection of post-close execution, a monotonic close transition, `wait_closed()`, and timeout-bounded tests for late and never-finishing connection attempts. These decisions are sufficient for every pool reached by that lifecycle. |
| **MAJ-3 — Validation occurs after unshredding and has unclear ownership** | **PARTIALLY CLOSED** | The ownership and ordering decision is corrected: generic Variant validation belongs in DataFusion/arrow-rs and must precede unshredding. However, FIND-003-20 assumes the current adapter covers all production reads. Equal-schema canonical Variant batches bypass that adapter, and the active Bifrost architecture still assigns read-side bounded validation to Wyrd. |

## Critical findings

### CRIT-1 — Recovery has no durable proof that an absent catalog entry was never committed

- **Location:** FIND-003-14, especially [TASK-003-R1-review-fixes.md:225](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:225)
- **Type:** Data integrity / incomplete recovery protocol

**Issue**

The packet treats an absent current catalog entry as permission to upload. That is insufficient because `vala.file_list` is lifecycle state, not permanent publication history.

A valid sequence is:

1. Scribe commits a hot publication.
2. The publishing process crashes while retaining its local recovery manifest.
3. Forge promotes, compacts, or expires the publication.
4. Terminal cleanup removes the old `vala.file_list` row.
5. The old publisher restarts.
6. The proposed pre-check sees no current row and classifies the publication as uncommitted.
7. It uploads and republishes data already represented by an Iceberg descendant.

That can reintroduce already-published rows into the hot read path.

The current publication flow uploads before calling the catalog publisher at [persistence.rs:1590](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/src/scribe/persistence.rs:1590). The catalog operation inserts rows before validating the complete set at [file_list_writer.rs:487](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/src/scribe/file_list_writer.rs:487) and [file_list_writer.rs:527](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/src/scribe/file_list_writer.rs:527). Forge eventually deletes terminal file-list rows at [forge_tasks.rs:1261](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-sql/src/queries/forge_tasks.rs:1261).

This also leaves partial catalog state undefined. “Committed match,” “not committed,” and “contradictory” cannot be safely inferred by first attempting the existing mutating insert.

**Impact**

A delayed recovery can resurrect a retired publication, duplicate analytical rows, or overwrite an immutable key. This violates REQ-034’s requirement that published keys are never uploaded again: [spec.md:1065](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/spec.md:1065).

**Decision needed**

Define a durable, read-only recovery authority with at least these outcomes:

- **Exact committed generation:** verify-only success, zero PUT.
- **Proven never committed:** upload is permitted.
- **Partial, mismatched, or historically committed generation:** fail closed, zero PUT.

The authority must survive Forge promotion, compaction, expiration, and deletion of the current `file_list` row. This could be a permanent publication-generation fence/tombstone or an equivalent retained ledger, but the task must choose the owner and persistence semantics.

Add explicit tests for:

- partial catalog rows;
- mismatched checksums or object identities;
- committed publication followed by Forge retirement and delayed recovery;
- zero PUT on every exact, partial, contradictory, or historically committed state.

## Major findings

### MAJ-1 — D2 closes coordinator-head pools but not remote worker-stage pools

- **Location:** D2, [TASK-003-R1-review-fixes.md:118](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:118)
- **Type:** Architecture / resource lifecycle

**Issue**

The design creates the query close state in coordinator-side `DistributedExec::execute` and propagates it through that `TaskContext`. Remote workers do not execute through this path.

A worker creates a separate `SessionState` and `TaskContext` while decoding a task at [impl_coordinator_channel.rs:45](/home/thorrester/Documents/GitHub/datafusion-distributed/src/worker/impl_coordinator_channel.rs:45). It then executes the stored physical plan directly at [impl_execute_task.rs:19](/home/thorrester/Documents/GitHub/datafusion-distributed/src/worker/impl_execute_task.rs:19).

Those remote plans can themselves own connection pools:

- `NetworkShuffleExec`: [network_shuffle.rs:103](/home/thorrester/Documents/GitHub/datafusion-distributed/src/execution_plans/network_shuffle.rs:103)
- `NetworkCoalesceExec`: [network_coalesce.rs:77](/home/thorrester/Documents/GitHub/datafusion-distributed/src/execution_plans/network_coalesce.rs:77)
- `NetworkBroadcastExec`: [network_broadcast.rs:120](/home/thorrester/Documents/GitHub/datafusion-distributed/src/execution_plans/network_broadcast.rs:120)

The existing Wyrd wrapper applies cancellation around each worker `execute_task` call at [analytical_transport.rs:1565](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs:1565). Removing that wrapper before the fork owns stage-local shutdown leaves lower-stage pools uncovered.

**Impact**

Early root-stream drop can close the coordinator head while remote stage-to-stage connections, readers, metrics entries, or reservations remain live. Under repeated cancellation this becomes an availability and tenant-isolation risk.

**Decision needed**

Extend D2 to define lifecycle ownership for every remotely executed stage or task:

- where its stage-local close state is created;
- how it reaches every stage-owned connection pool;
- which stream/task termination paths close it;
- how task-data and metric cleanup wait for or observe terminal closure.

Add a multi-stage cancellation test that drops the root stream early and proves all coordinator and worker-stage pools, readers, reservations, and metrics return to zero while a sibling query continues normally.

---

### MAJ-2 — FIND-003-20 relies on a validation path that canonical reads bypass

- **Location:** FIND-003-20, [TASK-003-R1-review-fixes.md:306](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:306)
- **Type:** Architecture conflict / unsafe read boundary

**Issue**

The packet says the DataFusion adapter validates and unshreds all production Variant data. That is not true for canonical unshredded columns whose source and target schemas already match.

DataFusion invokes Variant conversion only when source and target data types differ at [nested_struct.rs:226](/home/thorrester/Documents/GitHub/datafusion/datafusion/common/src/nested_struct.rs:226). Equal-schema batches are returned unchanged at [nested_struct.rs:3561](/home/thorrester/Documents/GitHub/datafusion/datafusion/common/src/nested_struct.rs:3561). The generic validation at [variant.rs:56](/home/thorrester/Documents/GitHub/datafusion/datafusion/common/src/variant.rs:56) therefore does not run for that path.

This matters because arrow-rs documents that `VariantArray::try_value` does not deeply validate unshredded bytes and may panic for malformed input: [variant_array.rs:457](/home/thorrester/Documents/GitHub/arrow-rs/parquet-variant-compute/src/variant_array.rs:457). Wyrd’s renderer calls it at [wyrd-queue/src/variant.rs:379](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/shared/wyrd-queue/src/variant.rs:379).

Revision 26 requires validation before unshredding and a stable error rather than panic: [spec-revision-26-datafusion-variant.md:77](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/spec-revision-26-datafusion-variant.md:77).

There is also an unresolved authority conflict. The active Bifrost design still says stored readers and renderers apply Bifrost’s iterative bounded validation and fixed error order: [bifrost-design.md:173](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/architecture/bifrost-design.md:173). The revised task instead removes that read-side Wyrd validation.

**Impact**

A malformed canonical Variant column can pass through a whole-column `SELECT v` without validation and later panic in rendering. UDF tests do not cover this path because the UDF decoder performs its own validation.

**Decision needed**

Following the user’s ownership rule, put the generic fix in DataFusion or arrow-rs, not Wyrd. The task must identify the actual read boundary that validates both:

- equal-layout canonical Variant columns;
- differing/shredded layouts before unshredding.

Require whole-column Parquet tests for malformed metadata, malformed values, and depth overflow, alongside shredded and UDF cases. Retain Wyrd renderer tests proving malformed arrays return stable errors and never panic.

Update `architecture/bifrost-design.md` so its authority matches the chosen boundary: Bifrost owns write-time Wyrd limits; DataFusion/arrow-rs owns generic persisted-read validation and unshredding.

---

### MAJ-3 — Group 7 has no objective benchmark pass condition

- **Location:** Group 7, [TASK-003-R1-review-fixes.md:417](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:417)
- **Type:** Invalid verification recipe

**Issue**

“Planning time is equal” and “Wyrd remains within measured noise” are not executable gates. The packet does not define:

- whether the Wyrd comparison covers hot reads, published reads, or both;
- how “equal” and “noise” are calculated;
- the number of repetitions or treatment of warm-up/outliers;
- which p50/p95 result constitutes failure;
- a correctness oracle proving both layouts and engines return identical logical rows before timing.

The existing benchmark distinguishes hot and published paths at [bifrost_reads/main.rs:6](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/benches/bifrost_reads/main.rs:6), while the existing report only prints samples and percentiles without a pass gate at [report.rs:65](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/benches/bifrost_reads/report.rs:65). The parent plan requires Wyrd overhead evidence for both hot and published reads: [TASK-003-native-datafusion-read-path-plan.md:689](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/tasks/TASK-003-native-datafusion-read-path-plan.md:689).

**Impact**

Two implementers can run the same benchmark, obtain the same measurements, and reach opposite completion decisions. The benchmark also cannot prove that a fast result was semantically correct.

**Decision needed**

Specify:

- hot, published, or both—consistent with the parent requirement;
- an exact row/result equivalence check across canonical and shredded layouts before timing;
- sample and warm-up protocol;
- planning and execution statistics;
- an objective threshold or statistical comparison for “equal” and “within noise”;
- explicit failure criteria, including nonzero Variant misses or fallback use.

Retain or explicitly replace the no-Variant and Struct-baseline evidence required by Revision 26.

## Cold-rehearsal result

A fresh implementer can execute Groups 1, 3, and 4 from the packet. Group 2 stops at the missing permanent recovery authority; D2 stops at remote-stage lifecycle ownership; Groups 5–6 stop at the canonical validation bypass and conflicting architecture authority; Group 7 stops at undefined acceptance gates. These are implementation choices with materially different correctness outcomes, not research details safely delegated to the implementer.

Skipped: no files were modified, and no builds, tests, or benchmarks were run because this was a static readiness review.
Risk: conclusions reflect the repository and fork revisions supplied; runtime behavior remains unverified.