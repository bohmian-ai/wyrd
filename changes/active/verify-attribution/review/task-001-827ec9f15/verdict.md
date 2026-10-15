# TASK-001 implementation verdict

**FIX_REQUIRED**

## Immutable subject

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`.
- Candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`.
- Candidate tree: `9df6ae0e34c646e6ef2c32a1c030e2a800683644`.
- Approved spec: `changes/active/verify-attribution/spec.md`, revision 3.
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`.

The complete cumulative base-to-candidate implementation, recorded task
evidence, applicable architecture authority, callers, tests, and first-class
SDK projections were reviewed. The candidate commit remained immutable; review
artifacts are outside its tree.

## Reconciled result

The implementation's core design passes: one sampling/routing rule, one
request continuation entry point, verified tenant binding, catalog projection,
one Scribe writer, stable queued contexts, tenant-scoped one-hop reads, and
Rust/Python/TypeScript projection. The accepted localized `SpanData` clone is
simple, bounded, and not a material violation.

Four independently validated gaps remain:

| Finding | Result | Smallest correction |
| --- | --- | --- |
| `FIND-TASK-001-1` | Producer-context allocations bypass the queue's sole byte budget. | Charge and transfer those bytes through the existing `ClientByteBudget`; add one retained-retry accounting test. |
| `FIND-TASK-001-2` | Timeline identity deduplication removes only adjacent duplicates. | Keep the sort and retain first identity with a local set; strengthen the existing fixture. |
| `FIND-TASK-001-3` | The named Scenario 7 same-outbox recovery and sibling-availability proof is absent. | Add that one existing-task integration test; change production only if it fails. |
| `FIND-TASK-001-4` | The modified `stored` helper lost its required rustdoc contract. | Move the existing documentation to the correct item. |

The full evidence, consequences, rejected proposals, and empty-claim
validation are in `findings-validation.md`. In particular, preserving producer
`trace_state` or non-sampled flag bits was rejected because it would change
operator content contrary to INV-004. No finding requires a specification
revision or migration.

## Verdict

**FIX_REQUIRED**

One decision-complete remediation task covers all four findings:
`TASK-001-R1-close-trace-continuity-gaps.md`. It adds no new task split,
architecture, public option, compatibility path, or speculative mechanism and
is routed directly to `$wyrd-implement`.

No Cargo, mise, database, code-generation, Python, or TypeScript lane was
rerun during the parallel source review. The candidate's recorded green lanes
were assessed as evidence; the remediation task names the missing focused
closure proofs.
