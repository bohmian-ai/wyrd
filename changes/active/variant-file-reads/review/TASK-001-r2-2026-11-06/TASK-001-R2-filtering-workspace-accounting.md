---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-variant-file-reads
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-1]
---

# Charge filtering workspace through the existing core budget

## Authority and immutable context

- [Approved spec revision 2](../../spec.md),
  [original task](../../tasks/TASK-001-memory-variant-execution.md).
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Prior candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- Current reviewed candidate: `96fea41e147c799354f797469260391d2fc237ff`.
- [Implemented R1/evidence](../TASK-001-r1-2026-11-06/TASK-001-R1-memory-execution-corrections.md),
  [current verdict](verdict.md), [independently validated ledger](findings-validation.md).

Implement this one remaining part of FIND-TASK-001-1 under `wyrd-implement`.
Findings 2–5 and retained-output/core-linkage portions of finding 1 are closed;
preserve them. No product or architecture decision is open for this correction.
Commits/pushes remain caller-authorized actions.

## Diagnosis and intended outcome

Live reads now correctly reserve their returned computed arrays from the actual
follower core-backed pool. They also correctly preserve original generation
charges and keep the returned reservations alive through source/stream lifetime.
The remaining gap is the current batch used while evaluating an exact filter.

For supported `COUNT(*) WHERE value > 0`, the input closure contains `value`,
while outputs are empty. Choose a batch containing both matching and nonmatching
values so the filter is partial. Adaptation succeeds and the evaluator creates
a copied filtered physical RecordBatch. It skips logical fallback, the only path
that resizes the workspace reservation. After deriving the surviving row count,
the evaluator returns an empty-column batch and drops the filtered input.
`project` then reserves zero bytes for that empty output. The core budget never
records the temporary copied batch, and the query can succeed even when remaining
capacity cannot cover it.

Evidence: `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs`,
`ReadableSource::project:2335–2358,2394–2399` and
`AdaptedRequirements::evaluate:2640–2665,2710–2716`. Pinned Arrow's partial primitive
filter creates selected-value buffers; this is a RecordBatch held by Wyrd's own
evaluator, not merely an invisible dependency kernel allocation. Existing P1 uses
`filter: None`; existing filter-only correctness tests use unbounded pools. Neither
proves this workspace obeys the budget. Final zero counters also cannot prove
that it was charged while held.

This violates the original governed temporary-memory requirement, R1's explicit
current-batch workspace obligation and the maintainer's single core accounting
direction. Limiting retention to one batch does not establish budget enforcement.
The outcome is that physical/filter workspace participates in the same core
accounting as the already-correct returned allocations, with refusal and release
at the existing ownership boundary.

## Selected minimal correction

Complete workspace ownership in the existing `ReadableSource::project` /
`AdaptedRequirements::evaluate` path. Reuse its supplied core-backed reservation
and existing alias-aware `unshared_arrow_bytes` sizing. Account the newly
materialized physical/filter batch for the period Wyrd holds it; refuse growth
the current query view/core root cannot cover. Release temporary ownership when
it ends, and reconcile/transfer ownership when outputs retain shared allocations
so a buffer is counted once. Preserve the logical-fallback workspace path and
the existing retained-output owner and original source charges.

The pool is already proven to originate from `ScribeResources::follower_execution`
and the process's core root. Do not add a tracker, budget, pool, resource root,
allocator interception, source workflow or independent byte total. Query ceilings
and attribution remain views/handles backed by the one core accounting source.
Do not alter generation accounting or charge only final emitted arrays.

This is a local completion of an existing reservation mechanism plus a focused
test branch. Counting final outputs or cloning source charges cannot cover a
temporary input that has already disappeared. Do not special-case only the named
COUNT query while leaving the same materialized workspace unowned in other
output shapes. Private implementation details and minimum adjacent test support
remain with the implementer.

## Preserved behavior and non-goals

Keep exact filter results, SQL null/type/order/cardinality, explicit zero-column
row counts, learned leaf execution, full-root/residual fallback, conservative
statistics and original source/tenant/assignment proof. Preserve all four closed
fixes, retained-output accounting, sharing exclusion, original generation charges,
staged leases, source clone/rewrites/stream lifetime and cancellation/error cleanup.

No new planner, public API, record/persisted format, learning/ingest/publication
policy, dependency, general allocation framework or arbitrary Arrow scratch
instrumentation is required. File-specific TASK-002 and L1 TASK-003 remain outside
this correction. The prior scalar-output public-regression claim is withdrawn;
do not add SQL/planner work or manufacture SDK RED to defend it.

## Acceptance and focused proof

All rows close residual FIND-TASK-001-1:

| Situation | Required observation |
|---|---|
| Adapted partial filter, empty outputs, insufficient remaining view/root capacity | Refusal instead of an uncharged successful COUNT; no partial provider, pool reservation residue or core accounting residue. |
| Same filter with sufficient capacity | Exact surviving row count with zero output columns; positive core/pool accounting while the copied workspace is held, followed by release. A zero final snapshot alone is insufficient. |
| Outputs retain some allocations also held by workspace | One charge per unique allocation, correct handoff/release, and unchanged exclusion of original captured buffers. |
| Existing computed multi-batch, sharing, refusal/drop and logical fallback cases | Their retained ownership and exact results remain correct; no change to original generation charge semantics. |

Extend the existing core-backed owner test with the partial-filter/empty-output
branch. Use the existing composed resources, batches and requirements; choose
remaining capacity below the copied input allocation and a fitting case. Observe
the actual core tracker and pool at the relevant owner boundary, using the
smallest focused test support needed. No new server journey, harness or dependency
is required.

First run the new assertion against the current code and confirm RED for the
uncharged-success path. Apply the bounded correction, confirm GREEN, then simplify
only while the affected tests remain green. Record source and proof per finding.

**Focused existing owner test, extended with workspace refusal/accounting.**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::computed_live_outputs_respect_query_memory_budget)'
```

**Existing result/leaf/fallback/cardinality regression.**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::memtable::tests::covered_variant_paths_avoid_unrelated_reconstruction)'
```

**Narrow affected owner families and required static checks, sequentially.**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::(memtable|tail_rpc)::/) or test(/^oracle::memtable_source::/)'
mise run fmt
mise run lints
```

Those owner tests require no Postgres. If the correction actually changes a
different owner, add only its narrow affected proof after confirming the current
selector/setup; no broad journey, language, `gate` or benchmark run is required
here. Do not change already-correct core resource semantics to avoid a refusal.
Document invariants/errors/panics on changed Rust items. Finish with
`git diff --check` and a diff audit; preserve this review's candidate and reports
for the next focused closure review.

## Implementation evidence

Candidate: `96fea41e1..625b7b84c` (one commit, `scribe/memtable.rs` only).

| Situation (FIND-TASK-001-1 residual) | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Partial filter, empty outputs, insufficient capacity | `ReadableSource::project` builds one `hold` closure per batch that resizes the existing `workspace` reservation (child of the core-backed `query`) to `unshared_arrow_bytes(held, &[captured])`. `AdaptedRequirements::evaluate` takes `hold` and calls it after logical rows are built (filter fallback and output fallback) and after the exact filter copies `physical`/`restored`. The no-requirements pruning filter also holds its `kept` batch. Refusal → `ScribeError::Internal` (existing open-failure path). | RED on 96fea41e1: the count succeeded with peak 0 ("the copied survivors are charged while held"). GREEN: a ceiling of half one survivor copy refuses the count; pool 0 and root `oracle_total_bytes` 0 afterwards. | PASS |
| Same filter, sufficient capacity | Same. | Three batches of `(0 columns, 512 rows)`; `PeakPool` (test pass-through over the real follower `GovernedMemoryView`) records a peak ≥ one survivor copy; pool 0 after the read, root back to 0 after drop. | PASS |
| Outputs keep allocations the workspace held | `workspace.free()` runs after the output exists and before `query.try_grow`, so the output's retained allocations move from workspace to query and are counted once; captured buffers stay excluded through `&[captured]`. | Bare `value` output under the filter: `pool.reserved()` and the root equal the outputs' unique bytes, and the peak equals them exactly. Mutation (free after grow) fails this assertion. | PASS |
| Existing computed, sharing, refusal/drop, logical fallback | The logical-fallback workspace charge moved from the `logical` closure into `evaluate`'s `hold` calls; retained outputs and generation charges are unchanged. | Earlier branches of `computed_live_outputs_respect_query_memory_budget` unchanged and passing; `covered_variant_paths_avoid_unrelated_reconstruction` PASS. | PASS |

### Correction to a new assertion

- **Symptom:** the first GREEN run failed `each survivor once`: 12696 vs 12288.
- **Cause:** the expected value was a guessed `3 × 512 × 8`. The read reserved exactly what `unshared_arrow_bytes` measures for the kept output buffers, which is 136 bytes more per batch.
- **Fix:** the test now measures the retained outputs with the same helper, as the existing `fresh` check does. The assertion was written in this remediation and was never committed.

### Commands

| Command | Result |
|---|---|
| `cargo nextest … -E 'test(=scribe::memtable::tests::computed_live_outputs_respect_query_memory_budget)'` | RED before, PASS after |
| `cargo nextest … -E 'test(=scribe::memtable::tests::covered_variant_paths_avoid_unrelated_reconstruction)'` | PASS |
| `cargo nextest … -E 'test(/^scribe::(memtable\|tail_rpc)::/) or test(/^oracle::memtable_source::/)'` | 46/46 PASS |
| `mise run fmt`, `mise run lints`, default-feature `cargo clippy -p vala-bifrost-redux -- -D warnings` | PASS |
| `git diff --check` | PASS |

### Non-goals and limits

No new tracker, pool, budget, resource root, allocator hook, public API, format or dependency. Generation charges and retained-output accounting are unchanged. Findings 2–5 are untouched. The boolean filter mask and arrays computed through adapted expressions are Arrow scratch that is created and consumed within one batch; they are not charged, as the task allows. No journey suite, `gate` or benchmark was run.
