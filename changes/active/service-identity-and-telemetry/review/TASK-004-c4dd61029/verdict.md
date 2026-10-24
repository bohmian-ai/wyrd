# TASK-004 verdict

**FIX_REQUIRED** — seven independently validated findings remain. No specification revision is required. Route the accompanying decision-complete [remediation task](TASK-004-R1-retained-storage-authority.md) directly to `$wyrd-implement`.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`.
- Base: `3e7c59db7a0f5e9506273eab826875cc8b59f226`, the parent of the first TASK-004 implementation commit.
- Candidate: `c4dd61029a54c44017c63bd4ff469472c406bc13`.
- Actual cumulative range: `git diff 3e7c59db7...c4dd61029` (102 changed files, including the newly tracked active packet and dependency pins).
- Approved authority: [spec revision 12](../../spec.md), [table-storage supplement](../../supplements/table-storage-and-correlation.md), [original TASK-004](../../tasks/TASK-004-shared-table-storage.md), and the stronger repository/architecture authorities recorded in [subject.md](subject.md).
- First task review; no prior findings or remediation verdicts to close. Source remained immutable. Only reports were added; no implementation, commit, merge, push or deployment occurred.

## Independent review results

| Role | Result | Preserved report |
| --- | --- | --- |
| Behavior implementation review | FAIL | [task-review-behavior.md](task-review-behavior.md) |
| Invariant implementation review | FAIL | [task-review-invariants.md](task-review-invariants.md) |
| Repository standards | FAIL | [standards-review.md](standards-review.md) |
| Maintainer | FAIL | [maintainer-review.md](maintainer-review.md) |
| System resilience | FAIL | [system-review.md](system-review.md) |
| Persistent-data domain | FAIL | [domain-review-persistent-data.md](domain-review-persistent-data.md) |
| Tenancy/source-trust domain | FAIL | [domain-review-tenancy.md](domain-review-tenancy.md) |
| Concurrency domain | FAIL | [domain-review-concurrency.md](domain-review-concurrency.md) |
| Focused follow-up | RESOLVED | [followup-review.md](followup-review.md) |
| Fresh independent Ponytail validation | FIX_REQUIRED | [findings-validation.md](findings-validation.md) |

All required reports are complete. The orchestrator grouped claims in [claim-comparison.md](claim-comparison.md); it did not self-validate findings. Discovery disagreement over generation uniqueness required one fresh focused follow-up. It established that a process-local monotonic allocator does not prevent collisions against restored staged authority under another revision. The independent validator checked that trace against source and retained the finding. No further discovery pass was needed.

## Validated finding ledger

The full source evidence, sibling traces and correction decisions are in [findings-validation.md](findings-validation.md). Only independently confirmed or revised claims are included here.

| ID | Status / classification | Observable gap | Selected correction owner |
| --- | --- | --- | --- |
| FIND-TASK-004-1 | REVISED / INCORRECT | Identical retries are interpreted under current schema/spec; mutable Bloom policy also prevents recovery of valid staged work. | Retain the complete accepted binding using canonical `PhysicalLayoutWire`; reuse the existing bounded mirror and fence conflict read, with no added SQL on successful fresh ingest. Preserve canonical WAL authority when resolving cold retries. |
| FIND-TASK-004-2 | CONFIRMED / INCORRECT | Multiple IPC RecordBatches bypass the request's 32-distinct-tuple bound. | Existing native preprocessing must bound the union of complete tuples before dispatch. |
| FIND-TASK-004-3 | CONFIRMED / INCORRECT | Native compaction groups equal tuple values from different retained specs. | Add retained spec identity to the existing compaction-core partition grouping key and pin its corrected commit. |
| FIND-TASK-004-4 | CONFIRMED / INCORRECT | Promotion JSON can contradict the durable composite partition and produce incorrect manifest pruning. | Project complete existing SQL partition authority and compare it with promotion evidence before catalog mutation. |
| FIND-TASK-004-5 | CONFIRMED / INCORRECT | A restored old-revision generation can disappear from live capture when a new revision reuses its ordinal. | Preserve full `SealKey` in existing served/superseded membership. |
| FIND-TASK-004-6 | REVISED / VIOLATION | New/changed partition, WAL and test items miss the explicit rustdoc gate. | Document the validated changed items in place, with actual error/index/fixture preconditions. |
| FIND-TASK-004-7 | CONFIRMED / MISSING | Required physical geometry, actual pruning/policy equivalence and planning-cost evidence is absent. | Extend existing writer/Forge/query fixtures and record observed production telemetry and physical metadata without thresholds or tuning. |

CONC-03 is rejected: no changed production mutation caller invokes eviction, and committed-edit cache refresh is expressly assigned to TASK-005. It is omitted from remediation. The suggestion to delete Bloom membership from staged identity is also rejected: it weakens the approved accepted-layout binding. Fixed-width conversions protected by exact-length reads do not require invented input-reachable panic documentation. No rejected proposal is retained as optional advice.

## Reconciled acceptance matrix

PASS reflects inspected implementation plus credible supplied proof within this task's boundary; it does not claim tests were independently rerun.

| Obligation | Implementation / verification evidence | Result |
| --- | --- | --- |
| One bounded validator, zoned time axis, eligible transforms/types, sort/Bloom lists and defaults | `catalog/layout.rs`, wire declarations, V1/V2 | PASS |
| Creation/describe/idempotency/refusal and canonical wire projections | Catalog owner; Rust/Python/TS consumers, V2 and supplied codegen evidence | PASS |
| Typed null/type/spec distinctions and complete partition carriers | `physical_partition.rs`, seal/WAL/staged/file-list carriers, V3/V5 | PASS for carriers |
| 32/33 limit over the complete logical native request | Per-RecordBatch planner resets; V4 contains one batch | FAIL: FIND-2 |
| Original accepted schema/layout on retry and recovery | Current contract selected before duplicate resolution; missing retained Bloom recipe; V6 is sort-only | FAIL: FIND-1 |
| New sequential admissions use committed metadata | Existing catalog-backed committed revision; V6/V7 | PASS; public edit/cache orchestration remains TASK-005 |
| Local/peer lifecycle rows and tenant/source authority | Existing authenticated binding/footer/peer/capture paths; V5/V7 | PASS except capture and promotion failures below |
| Cross-revision staged/memory capture after restart | Readable source strips revision; focused startup/allocator trace | FAIL: FIND-5 |
| Stable-ID projection, rename and nullable additions in reads | Memtable and pinned DataFusion adapter; V7 | PASS; retry conformance is FIND-1 |
| Pins and retained schema/spec/order support | Catalog pin test, retained scans and promotion lookup; V7/V8 | PASS |
| Contradictory composite promotion evidence fails closed | SQL projection omits dimensions; object check reduces partition to time | FAIL: FIND-4 |
| Compaction input partitions include source spec identity | Pinned native core uses tuple-only key; V8 changes tuple arity | FAIL: FIND-3 |
| Encoded scalar sort and declared Bloom construction | Shared encoder and V9 metadata/row/filter checks | PASS |
| Actual optional-filter exactness across policies and compaction; required cost observations | V8 records plan/row counts; V9 unfiltered full read/direct Bloom probes lack requested evidence | FAIL: FIND-7 |
| Writer constants, targets and existing storage policy | Shared writer/compaction owner remains | PASS |
| Tenant SQL capabilities, fences, checksums, origin and lifecycle ownership | Standards/domain source coverage and supplied regression evidence | PASS apart from identified source-selection/retention failures; corrections must preserve these boundaries |
| Changed-item rustdoc | Validated omissions in codec, helpers, tests and accessors | FAIL: FIND-6 |
| Non-goals and task boundaries | No second engine, automatic history rewrite, release migration, tuning, public PATCH or later task implementation | PASS |

The invariant review's policy-preservation PASS does not satisfy the separate measurement obligation; the behavior review's evidence-gap FAIL is retained for that obligation. The invariant review's uninterrupted-process allocator observation does not resolve recovered staged/new-generation uniqueness; follow-up and validation establish FIND-5. Other positive tests remain useful without closing untested sibling paths.

## Verification and limits

The original task records V1–V9, 919/919 redux tests, 35/35 Scribe journeys, V8 1/1, formatting, lints, dependency, tenant-isolation and codegen passes. These are supplied executions at their recorded commits, not reruns by reviewers. Changed assertions and recorded diagnoses were examined; the existing promotion mutation fix targets the new nested layout as claimed. The orchestrator independently ran clean cumulative `git diff --check` and repeatedly verified candidate HEAD and reports-only working-tree changes.

Pinned fork source was inspected at immutable local Cargo checkout revisions. No runtime reproduction, database mutation, Cargo build/test, generator, performance experiment or outage exercise ran during this review. The seven findings are independently source-validated reachable failures or required proof deficiencies. The remediation requires focused reproductions and closure, not repetition of broad journey aggregates. Final integrated change verification remains separate.

No spec revision recommendation is necessary: the selected corrections implement approved retained-layout, identity, isolation and evidence requirements. The accompanying task makes every retained correction concrete and preserves the original non-goals.

The independent validator also completed a bounded comprehension pass over the final verdict and remediation packet, without source re-review. All seven corrections, their costs, preserved invariants, exclusions and direct proofs match its ledger; no explanation gap remained. FIND-3 closure requires an immutable corrected dependency commit/pin. Preparing its tested local change alone does not close that finding; any necessary commit/push remains caller-authorized under repository Git rules.

## Caller-directed correction revision

The caller subsequently required preserving current throughput, prohibited an additional PostgreSQL call on the normal ingest hot path, and excluded speculative guards/complexity. The earlier unconditional pre-preparation SQL lookup recommendation for FIND-1 is superseded. A bounded source scout and the independent validator traced the existing mirror, grouped fence conflict read, preparation failures and canonical WAL replay ownership to revise the correction. Fresh writes keep existing SQL operation/transaction counts and batching; accepted mirror hits use retained metadata; cold conflicts reuse the existing conflict read and strict retained-binding comparison. Only current-contract preparation failure permits an exceptional accepted-ID lookup. Complete provisional WAL attempts are resolved against existing canonical fence coordinates without weakening per-attempt corruption checks or creating another recovery authority. Tenant-group rollback and shard cancellation/completion ownership remain intact. Details are in the updated validation report and remediation task.

No findings, reviewed commits or spec behavior changed. No new cache, ledger, public protocol, abort marker or generic defensive layer is prescribed. Throughput remains unmeasured: structural constraints prevent the proposed extra fresh-write SQL call but do not prove zero CPU/metadata overhead. Matching baseline/candidate benchmark comparisons are deferred verification, not work run during this revision; the caller explicitly requested no benchmarks now. No build, test or benchmark was executed for this packet update.
