# TASK-001 acceptance verdict

**FIX_REQUIRED** — five independently validated findings remain. This review
does not authorize implementation, merge, push or deployment.

## Immutable subject and round

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/variant-file-reads`.
- Base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- Range: complete first review, `git diff <base>...<candidate>`.
- Authority: [approved spec revision 2](../../spec.md) and
  [original TASK-001](../../tasks/TASK-001-memory-variant-execution.md).
- No prior verdict/findings, carried-forward acceptance or remediation context.
  A convergence assessment is not applicable to this first round.
- Source candidate and spec remained unchanged. The maintainer supplied an
  independent diagnosis report inside the task during review. Removing only
  that evidence/disposition reproduces the original task hash; approved task
  instructions are unchanged. [Scope](scope.md) records both hashes and the
  evidence-only correspondence. The supplied report was independently checked.
- Pre-existing benchmark-scaling deletions and the packet's untracked state are
  outside the committed source subject. Review writes are reports only.

## Independent coverage and results

[Scope and applicability](scope.md) records owners, producers, consumers,
specialist selection/omission and navigation before discovery. All required
selected reports are present. The fresh validator independently confirmed their
substantive coverage and that no additional sensitive-domain role was required.

| Reviewer | Assigned coverage | Discovery result and report |
|---|---|---|
| behavior-rev | Full task behavior, caller-to-result semantics and negative/regression flows | PASS, no proposals; [behavior report](task-review-behavior.md). |
| invariant-rev | Captured generations, Arrow ownership, query allocation, statistics and release | FAIL, INV-001; [invariant report](task-review-invariants.md). |
| repo-rev | All changed surfaces and applicable repository rules, including rustdoc/test diagnosis | FAIL, RSR-001/002; [standards report](standards-review.md). Initial RSR-003 evidence concern closed. |
| maintainer-rev | Changed owners/abstractions, whole-workflow reuse and maintainability | PASS, no proposals; [maintainer report](maintainer-review.md). |
| system-rev | Deployed local/remote paths, query failure, version skew and recovery | FAIL, SYS-001; [system report](system-review.md). |
| domain-rev-security | Authenticated expressions, schema closure, tenancy and pre-IO refusal | FAIL, SEC-001; [security report](domain-review-security.md). |
| ponytail-rev | Independent source/materiality/correction validation, applicability and reconciled acceptance | FIX_REQUIRED, five retained findings; [final validation ledger](findings-validation.md). |

No separate persistence reviewer is needed: no WAL, writer, SQL/catalog,
publication protocol or persisted-format change. No separate concurrency reviewer
is needed: no new locking/scheduling protocol; shared state and cancellation are
covered by invariant/system reviewers. No Python/TypeScript specialist is needed:
no binding, SDK, generated or public query contract change. Runtime Rust visibility
was explicitly reviewed rather than treating unchanged SDK paths as sufficient.

No focused discovery follow-up was required. All proposals had concrete traced
paths; the public-surface disagreement was resolved by the validator from the
baseline resolver seam and approved boundary. The validator discovered no new
blocker requiring separate source validation. Agreement was not treated as proof.

## Reconciled acceptance

The [validation matrix](findings-validation.md#reconciled-acceptance-matrix)
contains the complete obligation/source/proof disposition. The following condenses
its task acceptance without treating broad reviewer PASS results as overriding
specific validated failures.

| Obligation | Evidence and disposition | Result |
|---|---|---|
| Establish baseline cause and use existing source/adapter owners | Recorded V2 RED plus source trace through root reconstruction; existing DataFusion adapter, live boundary and file readers are reused. | PASS |
| REQ-001: ordered local/remote requirements, output schema/order, exact-versus-pruning meaning | Payload/resolver/capture source plus V1/V3/V4 cover ordinary expressions, repeated/reordered outputs and exact filtering. Successful absorbed scalar outputs lose runtime values. | FAIL — FIND-TASK-001-2 |
| REQ-002; memory AC-001/002: learned leaves and exact edge/fallback/cardinality | V2/V3 and source establish covered scalar/nested/list/null/IN paths, complete roots/residuals, mixed canonical/learned sources and zero-column row counts. Supported successful scalar output regression violates preservation. | FAIL — FIND-TASK-001-2 |
| REQ-004; memory AC-003: tenant/assignment proof, permissions, generations, deadlines/cancellation | Existing authenticated proof, original charges/leases and V1/V3/V5 are preserved. New expression validation is incomplete before tail access; new retained query buffers are not governed. | FAIL — FIND-TASK-001-1/3 |
| REQ-005; task-local memory AC-003 | V6 and unchanged scan-evidence producers preserve unknown/zero and non-file IO meanings. File-attempt completion accounting remains TASK-002. | PASS |
| Preserve staged/published siblings while extending common requirements | Existing tenant-bound Parquet reader, staged lease and exact wrappers remain. Full file acceptance is assigned to TASK-002. | PASS for preservation |
| No new public execution API; private source wiring and test support remain bounded | Existing public resolver is not itself new, but new ordinary public parser/builder/writable payload are unnecessary. | FAIL — FIND-TASK-001-4 |
| Mandatory changed-item Rust documentation | New panic/schema invariants and changed leaf statistics workflow have concrete documentation gaps. | FAIL — FIND-TASK-001-5 |
| Learning, ingest/WAL, persisted format, resident representation and dependency non-goals | No such implementation change in the range. L1 performance completion is not claimed by this task. | PASS |
| Assertion changes retain protected behavior and diagnosis evidence | Supplied independent diagnosis matches encoding; original result, fragment/footer and both literal assertions remain. | PASS |

## Validated finding ledger

Only independently CONFIRMED/REVISED findings are retained. Detailed source paths,
producer/sibling tracing, consequences, correction burden, selected correction
and closure proof are in [findings-validation.md](findings-validation.md).

| Stable ID | Discovery / validation | Classification and required correction |
|---|---|---|
| FIND-TASK-001-1 | INV-001 / REVISED | VIOLATION: eager retained computed/filter/fallback buffers bypass the admitted query memory pool. Reuse existing query reservations at the shared memory-read owner; retain their lifetime separately from generation charges. |
| FIND-TASK-001-2 | SYS-001 / CONFIRMED | REGRESSION: absorbed scalar-subquery outputs serialize runtime references without execution snapshotting. Snapshot every absorbed output through the existing execution-time mechanism, including no-predicate reads. |
| FIND-TASK-001-3 | SEC-001 / CONFIRMED | INCORRECT: invalid nested/filter columns can cross the promised pre-tail closure boundary. Validate every referenced ordinal and propagate filter type errors in signed_requirements before opening sources. |
| FIND-TASK-001-4 | RSR-001 / REVISED | VIOLATION: newly public private-payload parser/builder/contents exceed intended scope. Keep needed nominal public transport type for the existing resolver, narrow unnecessary operations, and gate test observation with existing test-support. |
| FIND-TASK-001-5 | RSR-002 / CONFIRMED | VIOLATION: mandatory changed-item panic invariants and leaf schema/statistics rustdoc are incomplete. Correct them alongside the behavioral remediation. |

Post-review maintainer clarification for FIND-TASK-001-1: all affected bytes must
flow through the core Bifrost-owned budget and tracker, with one authoritative
accounting source per process. Query ceilings and role/tenant/table attribution
must be core-backed views/handles, not a second memory tracker or budget.
The remediation now explicitly requires core snapshot/attribution and no-double-
charge proof. The independent validation report is preserved unchanged; this
clarification is human direction consistent with its existing-root correction,
not an additional independently discovered finding.

The broad proposal to redesign/privatize the existing resolver facade is rejected.
Resource comments from multiple specialists share one finding. The initial
missing diagnosis report proposal is closed by supplied evidence. No optional
style, speculative hardening, new cross-tenant exploit or performance finding is
included. No specification revision is needed for the selected corrections.

## Verification limits and next review

The original task records V1–V6, 99/99 owner-family tests, 63/63 Oracle journeys,
format/lints/dependency/tenant checks and diff hygiene passing. Reviewers inspected
their assertions and consumers but did not independently rerun builds, tests,
fault injection or benchmarks. These GREEN claims do not cover the retained
allocation, successful scalar-output or nested malformed-expression paths. Forced
adapter-failure fallback and mixed-build refusal have static proof only.

All approved semantics and task instructions remain unchanged. Source is pinned
to the recorded candidate; future work must not rewrite this review's subject.
The next review checks these stable findings and affected paths against the new
candidate while carrying forward accepted unaffected rows, not restarting the
whole first-round discovery.

Route [TASK-001-R1](TASK-001-R1-memory-execution-corrections.md) directly to
`wyrd-implement`. This is one bounded remediation task, including documentation;
no new design plan, extra approval stage or implementation action occurs here.
