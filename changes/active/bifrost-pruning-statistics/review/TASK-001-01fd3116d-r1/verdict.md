# TASK-001 review verdict

Verdict: **FIX_REQUIRED**

The statistics correction and its direct consumers have credible acceptance
evidence and no independently established material runtime defect. Three explicit
repository obligations remain: mandatory fork delivery checks, accurate changed-item
rustdoc, and module placement of newly introduced imports. TASK-001 does not pass
its completion gate yet.

## Immutable subject and round

- Wyrd repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-pruning-statistics`.
- Wyrd base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
- Wyrd candidate: `01fd3116d2501d89317073a2abb547818a23b4a8`.
- DataFusion repository: `/home/thorrester/Documents/GitHub/datafusion-pruning-statistics`.
- Dependency cumulative base: `19796a13ed46877cf998013886947b28a6f774b3`.
- Dependency candidate: `9c724e501b613ea1e3de6a7e8d9fb700eed65b25`.
- Specification: [approved revision 3](../../spec.md).
- Original task and available execution evidence: [TASK-001](../../tasks/TASK-001-dependency-owned-pruning.md).

This is the first implementation review, covering both complete committed
base-to-candidate ranges. No prior implementation acceptance or finding closure
is carried forward; the earlier readiness review is not implementation proof.
Both candidates remained unchanged through final review. Only report artifacts
were written; the dependency working tree remained clean.

## Reviewer selection, independence and coverage

The orchestrator supplied [one starting map and scope](scope.md); fresh reviewers
expanded it independently from actual source and governing authorities.

| Selected reviewer | Scope | Report and result |
|---|---|---|
| behavior-rev | Complete task acceptance, consumers/producers, failure paths, ownership, reuse and maintainability | [Behavior review](task-review-behavior.md), FAIL on BEH-001 |
| repo-rev | Both repository authorities, changed-item documentation/imports, manifests and mandatory checks | [Standards review](standards-review.md), FAIL on REPO-001/002/003 |
| domain-rev, statistics | Typed partial evidence, parameterized values, uncertainty/nulls, safe pruning/full-match/sizing and diagnostics | [Statistics domain review](domain-review-statistics.md), PASS |
| ponytail-rev | Fresh independent source/materiality/correction validation of every proposal and clean-area coverage | [Findings validation](findings-validation.md), FIX_REQUIRED |

No separate invariant/lifecycle/system/maintainer specialist was selected:
the changes add no lifecycle, shared-state authority or substantial ownership
abstraction. The behavior and domain reviews covered the changed in-process
semantics, caller failure containment and existing capture/dispatch paths.
No selected report is missing.

No focused follow-up was necessary. BEH-001 and REPO-001 describe the same gate
gap and were consolidated by independent validation. There is no unresolved
material conflict. The behavior reviewer rejected its homogeneous schema-mismatch
hypothesis after tracing the caller schema contract and deliberate downstream
statistics casts; it did not propose that hypothesis as a finding. Validation
discovered no new blocker requiring separate source validation.

## Reconciled acceptance matrix

| Obligation | Implementation and proof | Result |
|---|---|---|
| REQ-001/003, AC-001 | Existing dependency owner retains exact values and typed unknown slots; both-order primitive/timestamp/string/decimal matrix and restored W1 | PASS |
| REQ-002/004, AC-002 | No-evidence preflight, conservative conversion failure, value-free bound/column/type reason; D1–D4 source/proof | PASS |
| REQ-005, AC-003, INV-001/002 | Shared file/batch/route conversion, separate exclusion/full-match/sizing assertions and reference rows; exact zero-null requirement retained | PASS |
| Empty route selection and partition shares | W2 asserts route counts and captured warnings; unchanged dispatch loop cannot construct or send a fragment for zero routes | PASS |
| REQ-006, AC-004, INV-003 | Real SDK journey over persisted/live spans with predicate families, SQL-null/negative cases, tenant-separated readers, reference rows and diagnostic evidence; footer refusal retained | PASS |
| INV-004/005 | Eleven compatible pins, lockfile and peer revision agree; no new public/persisted contract, IO, source authority, dependency or lifecycle machinery | PASS |
| AC-005 and task dependency delivery checks | Selected fork package proof does not satisfy independent mandatory format/contributor lint/Clippy/extended checks | FAIL: FIND-TASK-001-1 |
| AC-006 and non-goals | Bounded producer/adapter/consumer audit, conservative fallback, no speculative replacement reader, collector or benchmark framework | PASS |
| Mandatory Wyrd changed-item rustdoc | Stale zero-batch method description and four undocumented new constants | FAIL: FIND-TASK-001-2 |
| Mandatory Wyrd import placement | Newly introduced function-scoped concrete imports in W1, W2 and J1 helper | FAIL: FIND-TASK-001-3 |

Full source/evidence mappings and counter-hypothesis dispositions remain in the
independent reports and reconciled validation ledger above.

## Validated finding ledger

| Stable ID | Discovery IDs | Validated disposition | Classification | Required outcome |
|---|---|---|---|---|
| FIND-TASK-001-1 | BEH-001, REPO-001 | REVISED, duplicate cause consolidated | VIOLATION | Satisfy and record already-required fork delivery checks; repair actual failures and consistently repin only if the delivered revision changes |
| FIND-TASK-001-2 | REPO-002 | CONFIRMED | VIOLATION | Correct the empty-input rustdoc and document the four existing fixture constants |
| FIND-TASK-001-3 | REPO-003 | CONFIRMED | VIOLATION | Move only newly introduced local imports to their existing owning module import groups |

Only independently confirmed/revised findings are retained. None requires a
new product, public interface, persistent-data, concurrency or architecture
decision. No specification revision is requested. No optional improvement or
unrelated existing source cleanup is prescribed.

## Verification and limits

[Review-time verification](verification.md) independently reran:

- Seven selected Wyrd new-regression/source-proof tests: all PASS.
- Dependency common statistics module: 13 tests PASS.
- Dependency empty-container real-predicate regression: one test PASS.

The source and task evidence support historical RED/GREEN, the real SDK journey,
published-time journey, bounded work/memory observations and recorded Wyrd checks.
Those journeys and broad checks were not rerun during this review. Review-time
fork commands did not override the toolchain and are not substitutes for the
separately required pinned-toolchain delivery checks. No full fork extended
suite or benchmark was executed during review. Existing benchmark applicability
assessment does not justify creating a new benchmark harness.

The fork's unresolved format drift and omitted required checks are an acceptance
gap even if they originate before the correction. They are not evidence of a
runtime statistics defect. Controlled incident-shaped reproduction does not
recover the historical requests' route counts; the task correctly preserves that
limit. No implementation, merge, push or deployment was performed in review.

## Remediation handoff

One self-contained task batches all validated obligations:
[TASK-001-R1-close-required-gates.md](TASK-001-R1-close-required-gates.md).
It is ready for `$wyrd-implement` within approved revision 3. The next task review
checks these finding closures, the delta from the recorded candidates, and affected
paths; unaffected acceptance evidence carries forward unless the repairs change it.
This is the first unsuccessful review, so no repeated-round convergence assessment
is applicable.
