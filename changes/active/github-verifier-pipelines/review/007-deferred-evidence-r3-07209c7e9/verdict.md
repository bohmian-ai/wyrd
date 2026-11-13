# Task 007 round-3 verdict

**REMEDIATION_REQUIRED** (skill disposition: **FIX_REQUIRED**). Only FIND-007-3 remains, for incomplete regression proof. Current production behavior is source-correct. FIND-007-1 and FIND-007-6 close; FIND-007-2 remains rejected by the maintainer; FIND-007-4/5 remain closed.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-007`.
Original base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Prior candidate: `74fc2190e255a3e3cf7515091ff45ab86a288f6b`.
Candidate HEAD: `07209c7e9af1664a30bf548e64be08937c4f88df`.
Approved authority: packet spec revision 7 and original task `tasks/007-deferred-evidence-and-resumption.md`, including approved amendments. Remediation context: R2 `007-R2-finish-publication-regression-proof.md`, appended Obligations/Evidence and prior R1/R2 reports. Review range is prior candidate to candidate; prior is an ancestor, no rebase correspondence needed.

Review covers prior-finding closure, pure receipt extraction, changed Artifact journey and shared fixture, affected service callers and consumers. Unchanged acceptance carries forward through R2 source/assertion matrices and cited R1 specialist reports. Added prior-review documents are context. No unrelated product change was found. Maintainer rejection of delegated runtime proof and accepted Artifact-read INPUT_INVALID mapping were honored.

## Independent reports and coverage

- [scope.md](scope.md): contracts, producer/consumer navigation, selection and omission rationale.
- [task-review-behavior.md](task-review-behavior.md): fresh remediation discovery, FAIL proposal R3-BEH-1.
- [findings-validation.md](findings-validation.md): separate fresh independent validator, CONFIRMED FIND-007-3 and validated coverage/closure.
- [claim-comparison.md](claim-comparison.md): claim grouping and routing. No follow-up needed: proposal independently resolved, no new validator-discovered blocker or material conflict.
- [ledger-check.md](ledger-check.md): orchestrator's independent ledger audit, reconciled by validator.
- [verification.md](verification.md): fresh commands versus recorded/carry-forward evidence.

Separate fresh discovery and validation roles completed. Additional security/data, invariant/system, repo and maintainer reviewers were omitted for source-backed bounded reasons independently validated: there is no changed trust, durability, concurrency, lifecycle or ownership protocol, only equivalent receipt calculation and connected proof additions. Prior R1/R2 specialist acceptance covers unchanged sensitive boundaries, lifecycle and cross-tier projections. No required report is missing.

## Reconciled acceptance and closure

| Obligation | Evidence | Result |
|---|---|---|
| FIND-007-1 Artifact denial before retention and same-execution consumption | Changed journey seeds run-only publisher, asserts denial; valid identical publication is accepted; real extract requirement passes for exact waiting ID | CLOSED; source plus recorded connected PASS |
| FIND-007-6 stable Artifact refusal codes/status | All four negative cases assert exact code and status; accepted read-denial mapping preserved | CLOSED |
| FIND-007-3 losing-offer canonical receipt regression | Correct service transfer remains at evidence.rs:155–156; new unit tests only already-correct held/false arguments | Production PASS; required proof FAIL |
| FIND-007-2 delegated runtime test | Maintainer rejection and recorded reason; existing caller.attribution reuse accepted | REJECTED; no work |
| FIND-007-4 normal lifecycle test routing; FIND-007-5 named SQL decoding | Prior R2 validated closure, unaffected | CLOSED carried forward |
| REQ-008 typed deferred slots, derived queue-only/direct refusal, independent work | R1/R2 registration/engine/direct source and assertion matrices | PASS carried forward |
| REQ-009 matching, source/policy pins, unmatched/history, custody ACK/conflicts, sequential replay | R1/R2 behavior/security/data matrices and retained connected assertions | PASS except focused race proof |
| REQ-011 / AC-007 compute/claim, attempts/budget/deadline, same-ID restart/running consumer | Prior invariant/system/behavior matrices and retained lifecycle assertions | PASS carried forward |
| INV-004 unavailable never absent/fresh | Approved source trace plus custody refusal/retry assertions | PASS carried forward |
| INV-005/006 authority, supersession/revocation, no terminal reopening | Prior security/system/invariant coverage | PASS carried forward |
| REQ-016 / AC-011 SDK/CLI/MCP/HTTP, source inference, schemas/docs/example | Prior cross-tier coverage; shared service callers unchanged | PASS carried forward |
| Changed-item docs, shared fixture and non-goals | Inline helper/test intent docs; report-gate retains max64/413 proof despite removal from unused shared report slot; no new owner/table/hook/dependency/API | PASS |

## Validated finding and ledger audit

Retained ledger: **FIND-007-3 / CONFIRMED / MISSING**, discovery R3-BEH-1, `components/verification/service/evidence.rs:145–162,403–442`. A losing fresh offer must become canonical held/replay state before response. The new test constructs that state itself. Deleting canonical state transfer or fresh clearing does not affect its assertions, although it restores the original receipt defect. Full producer-to-consumer evidence, materiality, burden and minimum safe correction are in independent validation. No new production bug is alleged.

The smallest correction is deterministic proof of the production-used losing-offer transition and resulting canonical receipt at the existing publication owner. No new protocol, harness or prescribed private helper shape is required. [007-R3-prove-losing-offer-reconciliation.md](007-R3-prove-losing-offer-reconciliation.md) packages this sole finding directly for `$wyrd-implement`.

Ledger audit: FIND-007-1/6 duplicate valid-after-refusal acceptance; original REQ-008/009 share publication-starts-nothing, REQ-011/AC-007 share same-ID durability, and INV-004 shares custody/recovery proof. Shared assertions suffice. All remediation Source cells self-reference; their diagnoses resolve the real REQ-009/Rust-test authorities, so no row is wholly unsupported. FIND-007-3's “extracted production receipt” wording prescribes a private proof shape and is not contractual; actual transition coverage decides acceptance. Its Evidence overclaims reconciliation/identity proof. FIND-007-2 is rejected, not an open or failed obligation. Ledger normalization itself creates no additional remediation.

## Verification, convergence and limits

Fresh exact unit selector: 1 passed, 631 skipped. Format and range whitespace checks pass. Artifact connected exact-case/module PASS (9/9), custody and sequential replay PASS and focused Clippy are implementation-recorded evidence. Docker is unavailable; no fresh connected execution was attempted or claimed. Broad suites, generated-contract checks and benchmarks were not repeated for this bounded proof diff.

This is the second unsuccessful remediation review. The sole remaining cause is incomplete proof of the original receipt repair: the test boundary excludes the repaired transition. There is no fix-induced production regression, new original defect or expanding product expectation. Next review is bounded to that transition and focused regression; retain all other closures and maintainer decisions.

HEAD was rechecked unchanged, tracked source/index clean. Only reports in this requested directory were written. No source mutation, commit, push or Git configuration.

REMEDIATION_REQUIRED
