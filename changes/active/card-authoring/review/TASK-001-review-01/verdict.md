# TASK-001 verdict: FIX_REQUIRED

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `5234961d51798581a026834cfffd5efaadc226b4`.
Candidate: `790da3369c1b0107765db234d72c1ae0c4680446`.
First review: full `git diff <base>...<candidate>`; no previous findings, remediation rounds or carried-forward evidence. Approved spec revision 3 and original TASK-001 are identified with unchanged packet hashes in [subject.md](subject.md). Reviewed source remained immutable and HEAD stayed at the candidate. Packet/reports are locally untracked; no staging, commit or push was performed.

## Independent coverage

All eight selected discovery reviewers and the fresh independent validator completed their assigned reports. Selection and source-backed reasons are in [subject.md](subject.md); independent applicability/coverage validation is in [findings-validation.md](findings-validation.md). No required specialist is missing or omitted.

| Reviewer | Report | Independent result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL: replacement capture; explicit primitive/slot proof |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL: replacement capture |
| repo-rev | [standards-review.md](standards-review.md) | FAIL: mandatory rustdoc |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL: replacement capture and mandatory rustdoc |
| system-rev | [system-review.md](system-review.md) | PASS within runtime/recovery scope |
| domain security | [domain-review-security.md](domain-review-security.md) | PASS within permissions/tenancy/subject boundary |
| domain persistence | [domain-review-persistence.md](domain-review-persistence.md) | PASS within identity/publication boundary |
| domain concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS within planner/executor boundary |
| ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; three independently validated findings |

The orchestrator compared claims by common invariant in [claim-comparison.md](claim-comparison.md). A focused follow-up was unnecessary: duplicate replacement claims share one traced source, no competing material diagnosis remains, and unique proof/rustdoc proposals were directly validated. The validator found no new blocker requiring another independent discovery pass. No agreement count substitutes for source validation.

## Validated ledger

The full source trace, materiality, correction burden, exact locations and focused closure proof for each retained finding are authoritative in [findings-validation.md](findings-validation.md).

| Stable ID | Status / classification | Violated obligation and consequence | Required correction |
|---|---|---|---|
| FIND-TASK-001-1 | CONFIRMED / INCORRECT; BEHAVIOR-001, INV-01, M-001 | REQ-002/008, AC-002: `Binding::set_runs_on` and AgentCall workspace replacement overwrite canonical slots but retain obsolete captured dependencies. Graph can falsely reject same-identity replacements; Cards/state can publish obsolete local files or fail reading an irrelevant path. | Synchronize capture ownership with current replacement positions in Binding/AgentCall; preserve other live/shared/nested captures and genuine conflicts. |
| FIND-TASK-001-2 | REVISED / MISSING; BEHAVIOR-002 | AC-007 and explicit native/slot proof obligations: missing all-operator, Source/reference, Trigger/Operator binding, remaining typed Spec/public SDK and required/nullable coverage. No unavailable-export or schema-conversion bug was proven. | Complete bounded canonical/public construction and slot-adapter proof using existing test and validation owners; no speculative exports/wrappers. |
| FIND-TASK-001-3 | CONFIRMED / VIOLATION; REPO-001, M-002 | Mandatory all-visibility changed-item rustdoc gate: private tests/helpers/fields and applicable error/panic/cancellation contracts missing; explicit-only planner prose contradicts inferred staging. | Batch a meaningful rustdoc pass with the behavioral correction; preserve execution and existing lifecycle semantics. |

Rejected proposals create no work: new value collection/loader adapters do not duplicate existing graph validation or registration machinery. The UID discussion does not establish a material persistence defect. Authored graph references do not transmit the upload return's UID; stronger existing authority assigns UID binding before hash/persistence to the server, and exact authenticated tenant/kind/name/version resolution and transactional UID rechecks remain intact. This is explicitly not a claim that the outgoing request contains the upload UID. See independent reconciliation in findings-validation.md.

## Reconciled acceptance

| Requirement / acceptance / constraint | Result | Evidence |
|---|---|---|
| REQ-001 / AC-007, complete native and public primitive parity | FAIL | FIND-TASK-001-2; story construction works, named full coverage does not yet exist. |
| REQ-002/008 / AC-002, snapshots, edits and constructor parity | FAIL | FIND-TASK-001-1; existing value capture, duplicate-ID/alias refusal and active-conflict checks otherwise pass. |
| REQ-003/007, required versus nullable authored inputs | FAIL for explicit proof | FIND-TASK-001-2; optional omission passes, schema conversion defect not established. |
| REQ-004, references, explicit/inferred stages and prerequisite outputs | PASS for runtime | Native plan assertions, fresh bounded-concurrency regression and dependent-output journey; misleading docs remain FIND-TASK-001-3. |
| REQ-005/006 / AC-003, staged Artifact publication, reuse, source isolation, retained state, versions/errors | PASS for current references | Fresh Artifact journey and traced existing owners; obsolete captured references are the single FIND-TASK-001-1 exception. |
| REQ-006/008 / AC-001, receipt/state, existing path flow, canonical and registered parity | PASS for current references | Local file parity and fresh parity/state journeys; shared pull/hydration tail. |
| REQ-007 / AC-004, both components, outcomes, uncertainty, exact attribution/refusals | PASS | Fresh component journey, source-backed behavior/security reports. |
| INV-001–005 and task non-goals | PASS within scoped boundaries | Existing registration/upload/loader/planner/server identity owners retained; no new endpoint/schema/dependency/feature, transport, scheduler, durable workflow, auth/tenant change or foreign-runtime implementation. |
| Mandatory changed-item documentation | FAIL | FIND-TASK-001-3. Other standards coverage passes. |

## Verification and limitations

[verification.md](verification.md) records exact commands and run IDs. Fresh review runs passed all three local authoring tests, the exact existing bounded-concurrency regression, server build, all four authored journeys, existing ready-state declaration journey, both migration tests and range whitespace check. Real journeys used the repository-managed Postgres lifecycle and local stand-in provider. The linker emitted a compact-unwind-size warning during server/integration linking; builds and tests passed.

The original task records broader format/lint/dependency/docs gates passing. Those broad gates, other platforms, fault injection and live model quality were not independently rerun. Static owner tracing supports unchanged interruption/security/persistence boundaries. Passing baseline tests do not cover replacement capture, fill explicit proof gaps or waive private/test rustdoc. Residual behavior risk is obsolete dependency publication/refusal until FIND-TASK-001-1 is closed.

## Remediation and next review

One ready, self-contained remediation task covers the shared behavior correction, bounded proof and documentation: [TASK-001-R1-current-dependencies-and-proof.md](TASK-001-R1-current-dependencies-and-proof.md), routed directly to `$wyrd-implement`. Review does not execute it.

There are no prior findings to close and no convergence assessment is required on this first round. The next review retains the original task/spec, this verdict/ledger and candidate; checks the remediation diff, producer ownership and affected consumers/tests, public proof additions and documentation; and carries accepted unchanged domain evidence forward. It need not recreate the eight-reviewer team or rerun all journeys absent a material boundary change.

**Verdict: FIX_REQUIRED.**
