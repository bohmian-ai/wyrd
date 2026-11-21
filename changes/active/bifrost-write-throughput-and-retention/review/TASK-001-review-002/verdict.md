# TASK-001: FIX_REQUIRED

The spec and architecture are corrected to the maintainer-approved R1 behavior.
A fresh independent review closes the prior contract hold. Two implementation
defects remain; no further contract clarification is needed.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `66f44aee193077ef3565aa76140a942d8eb593f9`.
- Runtime candidate and prior candidate: `bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`.
- Cumulative runtime range: `66f44aee1..bf438a8d7`, 19 commits.
- Fixed documentation/rustdoc overlay: tracked `git diff` SHA-256
  `da4f96b4380cfed6c829a75ddb87e67518637f4c03ede8c02d80a4027a597cc3`.
- Approved authority: [spec revision 2](../../spec.md), original
  [TASK-001](../../tasks/TASK-001-fence-lifecycle.md), corrected Bifrost and
  verification architecture, applicable AGENTS/workflow/testing rules.

No executable code or test changed. This review explicitly covers the pinned
runtime commit plus the frozen documentation overlay, not a new commit. No Git
commit or push was requested or performed.

## Review and coverage

Fresh reviewer `review_002` independently checked the authority correction,
comprehension, actual overlay, prior finding closure and affected source paths.
The [validation report](findings-validation.md) contains the single canonical
REQ-001–009 acceptance matrix and complete retained findings. That matrix is
incorporated into this verdict by reference.

The prior primary, recovery/custody specialist, SQL/concurrency/lifecycle specialist
and independent validator reports in [review 001](../TASK-001-review-001/verdict.md)
remain applicable to unchanged runtime boundaries. No additional specialist is
needed for this documentation-only follow-up. No new finding was introduced.

The correction records Bifrost-only reads, newest-readable recovery or fresh state
on no readable checkpoint, temporary absence of accepted-but-unreadable records,
fixed-identity conflict protection, acknowledged saves before settlement and
unavailability on query failure. The maintainer's instruction authorizes that
correction; the previous `SPEC_REVISION_REQUIRED` hold is closed.

## Remaining findings and remediation

- **FIND-TASK-001-1 — open:** a live second pod's retry mirror can acknowledge a
  receipt after the owning pod deletes it, violating REQ-002.
- **FIND-TASK-001-2 — open:** a pending initial cleanup sweep can hold recovered
  service startup/readiness, violating the REQ-008–009 admission/availability boundary.

Both were independently retraced, retaining their stable IDs and correction bounds.
Implement [TASK-001-R2](TASK-001-R2-mirror-expiry-and-cleanup-readiness.md) to close
them. The revised R2 references spec revision 2 and preserves the approved R1
semantics. This review does not start implementation or TASK-002.

The maintainer subsequently requested decision-complete remediation. R2 now
selects the exact owner changes: exclude `spent_batch_ids` from mirror population
and remove the synchronous cleanup sweep from successful replay, retaining the
existing worker and health gate. It specifies connected documentation, three
regression-test flows, runners and the cache fast-path tradeoff. A fresh
[independent correction check](remediation-decision-validation.md) validated those
decisions against source; this strengthens the task without claiming implemented
fixes or changing the acceptance verdict.

## Evidence and limits

Existing assertions and recorded task/R1 execution remain credible for unchanged
paths. No fresh Cargo/Postgres tests, lints or benchmarks ran for the prose-only
correction. `git diff --check` passed, and both HEAD and overlay hash remained
unchanged through independent review. The two failures are independently
source-traced; existing execution does not cover their failing variants.

Recorded post-implementation tests remain TDD violations separately from behavioral
verification. The disclosed hot-read regression remains open by maintainer decision;
diagnostic subsets do not establish full performance qualification. No performance
PASS is claimed. Full qualification remains the recorded CPU/integrated-review work.

This follow-up corrects the contract record, not an implementation attempt at the
two findings; no repeated-failed-remediation escalation applies. TASK-001 remains
unaccepted until those settled defects are corrected and verified.
