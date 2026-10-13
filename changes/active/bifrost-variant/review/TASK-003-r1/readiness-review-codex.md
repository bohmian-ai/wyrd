Verdict: **NOT_READY**

Review ID: `TASK-003-R1-readiness`  
Target: `changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md`  
Gate: Task handoff  
Architecture axis: **FAIL**  
Executability axis: **FAIL**  
Critical: **1**  
Major: **3**  
Task coverage: All six groups, including D1 and D2  
Cold rehearsal: **FAIL**

## Critical findings

### CRIT-1 — Recovery may overwrite an already-published immutable object

- **Type:** `ARCHITECTURE_CONFLICT`
- **Location:** [TASK-003-R1-review-fixes.md:187](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:187)
- **Issue:** FIND-003-14 requires verify-first, but then requires both a missing object and contradictory content to fall through to PUT. Its proposed test explicitly expects one PUT for the contradictory case.
- **Impact:** Scribe recovery can replay a publication manifest after the catalog commit succeeded but before cleanup completed. In that state the key is already published. Overwriting contradictory bytes mutates an object referenced by an existing Iceberg snapshot, creating a credible persisted-data corruption path.
- **Evidence:**
  - REQ-034 distinguishes unpublished overwrite from published immutability: a second upload may overwrite an **unpublished** key, but “a published key is never uploaded again” ([spec.md:1065](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/spec.md:1065)).
  - Recovery explicitly preserves the manifest across cancellation or ambiguous commit and calls `upload_file_verified` before its idempotent publication reconciliation ([persistence.rs:1585](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/src/scribe/persistence.rs:1585), [persistence.rs:1609](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/src/scribe/persistence.rs:1609)).
  - The current uploader contract says contradictory content fails closed ([object_uploader.rs:152](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/vala/vala-bifrost-redux/src/parquet/object_uploader.rs:152)).
- **Decision needed:** Define how recovery determines publication state before mutation. For an already-published key, matching bytes may be accepted, but missing or contradictory content must fail closed unless the specification introduces an explicit repair protocol using a new object identity and catalog publication. Tests must separately cover committed-but-not-cleaned matching, missing, and contradictory objects. The current blanket `0/1/1` PUT expectations are unsafe.

## Major findings

### MAJ-1 — D2 lacks a complete lifecycle contract for closing the actual executed pools

- **Type:** `MATERIAL_DECISION_GAP`
- **Location:** [TASK-003-R1-review-fixes.md:107](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:107)
- **Issue:** The packet says the `DistributedExec` output stream closes every connection pool “in its plan,” but does not define the required lifecycle invariants around lazy plan construction.
- **Impact:** An implementation can close the base or visualization plan while missing pools in the actual executed head stage, or allow pools to be created after the output has already been dropped. Either leaves reader tasks, remote work, and charged memory alive—the exact failure D2 is intended to fix.
- **Evidence:**
  - `DistributedExec` holds distinct `base_plan`, `plan_for_viz`, and `head_stage` values ([distributed.rs:29](/home/thorrester/Documents/GitHub/datafusion-distributed/src/coordinator/distributed.rs:29)).
  - The actual `head_stage` is produced lazily inside the spawned execution task and stored only after preparation ([distributed.rs:194](/home/thorrester/Documents/GitHub/datafusion-distributed/src/coordinator/distributed.rs:194), [distributed.rs:220](/home/thorrester/Documents/GitHub/datafusion-distributed/src/coordinator/distributed.rs:220)).
  - Pools are private fields of each shuffle, coalesce, and broadcast execution node, not owned by `DistributedExec` ([worker_connection_pool.rs:25](/home/thorrester/Documents/GitHub/datafusion-distributed/src/worker/worker_connection_pool.rs:25)).
- **Decision needed:** State lifecycle invariants—not necessarily a particular private implementation:
  1. closure targets every pool belonging to the actual executed head plan;
  2. dropping the root before or during preparation prevents later stream creation;
  3. closure occurs before coordinator draining can wait on those streams;
  4. normal EOS, execution error, preparation error, cancellation, and root drop all converge on the same idempotent close;
  5. independent sibling queries remain unaffected.

### MAJ-2 — D2 does not handle close racing unresolved connection futures or define observable completion

- **Type:** `MATERIAL_DECISION_GAP` / `INVALID_VERIFICATION_RECIPE`
- **Location:** [TASK-003-R1-review-fixes.md:118](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:118)
- **Issue:** `close()` is specified only as dropping retained inner streams. The current pool can already have an initialized but unresolved shared future, and the packet does not say what happens if it resolves after close. It also requires immediate proof that tasks exited and memory is zero without defining a completion signal.
- **Impact:** A late `execute_task` result can install live streams after closure, recreating the leak. Tests can also become scheduler-dependent by observing `reserved() == 0` before the cancellation task has unwound.
- **Evidence:**
  - Each connection is currently a `OnceLock<SharedBoxFuture<StreamGroup>>`; the future may remain pending after `execute()` returns its proxy ([worker_connection_pool.rs:25](/home/thorrester/Documents/GitHub/datafusion-distributed/src/worker/worker_connection_pool.rs:25), [worker_connection_pool.rs:78](/home/thorrester/Documents/GitHub/datafusion-distributed/src/worker/worker_connection_pool.rs:78)).
  - Cancellation is asynchronous and observed by a spawned gRPC reader task ([worker_client.rs:150](/home/thorrester/Documents/GitHub/datafusion-distributed/src/protocol/grpc/worker_client.rs:150), [worker_client.rs:162](/home/thorrester/Documents/GitHub/datafusion-distributed/src/protocol/grpc/worker_client.rs:162)).
  - Memory is released when queued messages and the shared reservation are dropped, not synchronously merely because cancellation was requested ([worker_client.rs:253](/home/thorrester/Documents/GitHub/datafusion-distributed/src/protocol/grpc/worker_client.rs:253)).
- **Decision needed:** Define a monotonic closed state checked both before and after connection resolution, the post-close behavior of `execute()`, and how pending RPC/future creation is cancelled or discarded. Define an awaitable or otherwise deterministic completion observation, bounded by a test timeout, before asserting reader-task exit and zero memory reservation.

### MAJ-3 — FIND-003-20 reverses the required hostile-input validation order

- **Type:** `ARCHITECTURE_CONFLICT`
- **Location:** [TASK-003-R1-review-fixes.md:257](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/review/TASK-003-r1/TASK-003-R1-review-fixes.md:257)
- **Issue:** The packet mandates `unshred_variant` followed by Wyrd validation, while leaving pre-unshred depth protection conditional and unplanned.
- **Impact:** A renderer can process or allocate from hostile shredded input before applying Wyrd’s bounded size, encoding, numeric, and depth rules. This can change failure ordering and weaken the bounded-processing guarantee.
- **Evidence:**
  - Bifrost’s design requires iterative bounded validation before decoding so a hostile cell becomes an error rather than amplified work ([bifrost-design.md:173](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/architecture/bifrost-design.md:173)).
  - Revision 26 likewise specifies that the DataFusion adapter validates bytes and **then** calls `unshred_variant` ([spec-revision-26-datafusion-variant.md:77](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/changes/active/bifrost-variant/spec-revision-26-datafusion-variant.md:77)).
  - The current Wyrd walk exists specifically to validate every shredded residual before upstream decoding ([variant.rs:500](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/shared/wyrd-queue/src/variant.rs:500)).
  - Arrow’s `unshred_variant` immediately constructs recursive row builders and an output builder; it is not documented as enforcing Wyrd’s size/depth/failure-order contract ([unshred_variant.rs:61](/home/thorrester/Documents/GitHub/arrow-rs/parquet-variant-compute/src/unshred_variant.rs:61)).
- **Decision needed:** Make pre-unshred bounded validation mandatory and assign its owner. If arrow-rs owns it, specify the fork change, its exact guarantees, pin update, hostile-depth/malformed-residual tests, and Wyrd verification. Otherwise retain a Wyrd-side validation step using an existing public primitive. A happy-path fixture adjustment alone is insufficient proof.

## Architecture and execution readiness

D1’s source claims are valid: each network boundary’s `children()` already exposes its local stage, so separately traversing `input_stage()` double-counts that subtree. Its “aggregated by name” wording is sufficiently fixed by DataFusion’s existing `MetricsSet::aggregate_by_name` behavior, which removes partition dimensions and aggregates each metric name. D1 does not independently block readiness.

The remaining groups are generally source-backed and locally implementable. The blockers are concentrated in three load-bearing boundaries:

- published-object immutability during crash recovery;
- distributed stream ownership and asynchronous cancellation;
- validation-before-decoding for hostile Variant cells.

The independent deployed-system pass and cold rehearsal both failed first at D2’s lifecycle boundary. Normal EOS is not the difficult case; interruption during lazy preparation, unresolved RPC creation, and root-stream drop is.

## Adversarial analysis

The smaller safe designs are:

- For recovery, query/reconcile durable publication state before any upload; do not turn overwrite into implicit repair.
- For D2, introduce one query-scoped close state shared by every pool created for the executed plan, rather than relying on best-effort traversal after termination.
- For Variant rendering, keep validation before unshredding; moving the implementation to arrow-rs is acceptable only if the fork owns and proves the same bounds.

These alternatives use existing owners and add no Wyrd-specific DataFusion mechanism.

## Static limitations

This was a read-only source review. No files were modified and no builds or tests were run. The normal skill review artifact and validator were intentionally skipped because the user prohibited file modification.

Risk: implementing the packet unchanged can overwrite published analytical data, leak distributed query resources after cancellation, or decode hostile Variant input before enforcing Wyrd’s bounds.