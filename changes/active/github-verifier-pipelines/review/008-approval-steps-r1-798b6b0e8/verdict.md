# TASK-008 verdict

**FIX_REQUIRED**

## Immutable subject and round

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`.
- Candidate: `798b6b0e83595356c0d2328fa541119267e84fb2`.
- Reviewed first complete `git diff cd0b8e18b...798b6b0e8` (77 files), including tests, fixtures, contracts, architecture and public projections.
- Original task: [008-approval-steps.md](../../tasks/008-approval-steps.md).
- Approved authority: [spec.md](../../spec.md), revision 8, and [contracts.md](../../contracts.md), Human approval, under applicable repository/architecture authority. Task revision-5 metadata does not supersede later approved amendments. Its maintainer clarification resolves queued-only approval registration consistently with REQ-015.
- No prior TASK-008 review, accepted coverage, findings or remediation carried forward. Prior-finding closure and convergence assessment are not applicable to this first round.
- HEAD remained the candidate. Final tracked diff was empty; only this new review directory was added. Production source was not edited; nothing was committed or pushed.

## Independent coverage and claim reconciliation

[scope.md](scope.md) records the navigation map and selection before discovery. The orchestrator used seven separate fresh discovery agents and one fresh independent validator. Each discovery reviewer received the same immutable subject/map and its scope, without other discovery conclusions or an intended verdict. The validator independently checked applicability, coverage, source, materiality and correction burden.

| Role and report | Why required | Discovery result |
|---|---|---|
| behavior-rev: [task-review-behavior.md](task-review-behavior.md) | Complete first task acceptance and public caller-to-result audit | FAIL |
| invariant-rev: [task-review-invariants.md](task-review-invariants.md) | Shared engine/occurrence/park/restoration state | FAIL |
| repo-rev: [standards-review.md](standards-review.md) | Broad cross-tier rules, declarations and mandatory documentation | FAIL |
| maintainer-rev: [maintainer-review.md](maintainer-review.md) | New custody owner and changed engine abstractions; existing-owner comparison | PASS |
| system-rev: [system-review.md](system-review.md) | ACK, outage, lost wake, restart and compute release | FAIL |
| domain-rev-security: [domain-review-security.md](domain-review-security.md) | Approval permission, tenancy, subject/actor provenance | FAIL |
| domain-rev-data: [domain-review-data.md](domain-review-data.md) | Canonical decision custody, fences, SQL/deadline concurrency | FAIL |
| Independent ponytail-rev: [findings-validation.md](findings-validation.md) | Source validation, deduplication, correction burden and coverage | FIX_REQUIRED recommendation |

All selected reports are complete; no required specialist is missing. Concurrency is covered by the coherent persistent-data review and engine invariant review; a separate duplicate role was unnecessary. No UI or new deployment/provider domain was introduced.

[claim-comparison.md](claim-comparison.md) groups claims by violated invariant. No followup-rev was required: the discarded-output fence and stale-decision refusal are distinct promises, not contradictory findings. Duplicate pending-visibility and deadline findings were consolidated. The validator discovered no new blocker requiring another independent validation. It revised the pending-visibility correction to exclude an unsupported promise that dependents must start before an independent peer settles.

Complete-owner comparison validates extending common Scribe fence/ACK transport while retaining separate evidence/approval domain projections. No duplicate scheduler or full-state writer warrants DRIFT. The existing assert check over `status == approved` provides require-approved behavior without a new evaluator. All retained corrections fit approved behavior; no specification revision is required.

## Validated finding ledger

The full source evidence, affected callers, materiality, burden, selected correction and focused closure proof are preserved in [findings-validation.md](findings-validation.md). Only independently confirmed/revised findings are retained here.

| Finding | Status / classification | Violated obligation and exact location | Consequence / correction boundary |
|---|---|---|---|
| FIND-008-1 | REVISED / MISSING | REQ-010/016 reached pending identity; `blocks/engine.rs:1069–1098`, `verification/runner.rs:579–581`, `verification/state.rs:814–843` | Pending identity is never staged while independent work runs. Record through the existing checkpoint/RunState owner when reached; retain accepted decision for same-execution consumption. |
| FIND-008-2 | CONFIRMED / INCORRECT | REQ-010 stale/non-pending refusal; `service/approvals.rs:75–87,124–133`, `service/inspection.rs:279–321` | Discarded history remains pending and accepts a first decision in an open execution. Reuse accepted disposition/lifecycle for inspection and fresh eligibility; preserve history/replay. |
| FIND-008-3 | CONFIRMED / INCORRECT | REQ-010/011 original expiry; `service/approvals.rs:124–129`, `verifier_executions/approvals.rs:20–24` | Stopped worker allows decisions after a shorter authored execution timeout. Reuse runner's effective admission-relative ceiling and exact Verifier with PostgreSQL clock. |
| FIND-008-4 | CONFIRMED / INCORRECT | Actor provenance/same-actor replay; `service/approvals.rs:138,201` | Delegated subject is recorded as actor; different actors sharing it replay. Use the existing authenticated attribution actor in receipt and replay. |
| FIND-008-5 | CONFIRMED / VIOLATION | Mandatory changed-item rustdoc; MCP `github.rs:386–455`, `Opened::start` | Assertion panics lack required documentation. Document actual Panics; preserve assertions. |
| FIND-008-6 | CONFIRMED / INCORRECT | Accurate approved wire authority; `architecture/wyrd-design.md:1647` | New owner text says `approve/reject`, served contract accepts `approved/rejected`. Correct owner wording only. |
| FIND-008-7 | CONFIRMED / VIOLATION | Mandatory public error redaction; `verification/approvals.rs:226–230` | Raw database diagnostics reach public inspection/decide errors. Trace internally and return a safe typed unavailable message. |

## Reconciled acceptance matrix

| Obligation / constraint | Implementation and verification evidence | Result |
|---|---|---|
| REQ-010a closed action/message, no approval duration, queue_only registration | Spec enum/validation and declaration source assertions; 855 spec/runtime unit tests passed | PASS |
| REQ-010b stable execution/step/item/iteration IDs, distinct fanout/repeat, no pending SQL | Pure state ID producer; focused ID and engine tests passed; engine has no pending SQL path | PASS for identity/no-SQL; FAIL timely recording, FIND-008-1 |
| Independent branches and all-blocked compute-free park | Existing engine/runner park owners retained; engine assertion proves independent settlement once | PASS for work/park; preserve during correction |
| REQ-010c exact tenant/Verifier approve/subject auth and audit | Shared authorization/tenant owners; negative pg journey assertions and observed five pg tests | PASS ordinary auth/tenancy; FAIL delegated attribution, FIND-008-4 |
| Current/non-pending/expired acceptance refusal | Source-proven historical-membership and authored-deadline gaps | FAIL, FIND-008-2/3 |
| Durable ACK, first-wins, identical/conflicting retry, canonical unavailable | Fence/ACK/recovery source traced; held-ACK/retry pg proof passed | PASS custody; FAIL safe public error, FIND-008-7 |
| REQ-011 same execution, predecessors/budgets/authority, rejection data and wake recovery | Existing sole state owner, checkpoint replay and park recheck; engine and pg tests passed | PASS for inspected ordinary paths; preserve in remediation |
| REQ-016 typed SDK/CLI/MCP inspection/decision, docs/errors | Thin shared-client projection and public journey assertions source-checked; native/declaration coverage complete | PASS ordinary projection; FAIL new owner spelling, FIND-008-6 |
| Mandatory changed Rust item documentation | Full changed helper inspected | FAIL, FIND-008-5 |
| INV-001–007/non-goals | Bifrost owns decisions; no new SQL payload store/service/UI/login/proof restriction; accepted authority and supersession fences remain | PASS inspected boundaries, subject to findings above |

## Verification and limits

[verification.md](verification.md) records executed commands/results separately from implementation claims. Five approval Postgres tests, focused engine/ID tests, 855 spec/runtime tests, format, dependency, tenant-isolation and diff checks passed. These do not cover the retained failing scenarios. The initial uv cache permission failure was resolved using a writable temporary cache; corrected checks passed.

Full SDK/CLI/MCP journey lanes, Clippy, Python/TypeScript typing, SQL-wide tests, regenerating codegen/docs checks and docs build were not rerun. The runtime-restart test is not a complete server/Scribe process restart; no broader executed recovery result is claimed.

System review inventories every new/moved wait and its sanctioned boundary: approval fence SQL, conditional decision query, decision-open SQL/commit, bounded explicit ACK, wake SQL/commit and existing background park ACK. No pending-step SQL, direct-verify SQL/Scribe wait, new Vault wait or direct object-storage wait was found. Shared direct-engine code nevertheless changed; a clean full L1 verification benchmark and same-host history comparison remain unqualified. This verdict does not claim performance acceptance or create a broad remediation suite. Integrated qualification remains for change review.

## Handoff

Implement [008-R1-close-approval-acceptance-gaps.md](008-R1-close-approval-acceptance-gaps.md) with `$wyrd-implement`. Retain this immutable candidate, task, reports and stable finding IDs in the next remediation review; check closure, the candidate-to-candidate diff and affected paths rather than restarting whole-task discovery.

**FIX_REQUIRED**
