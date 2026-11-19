# Task 004 remediation re-review

**FIX_REQUIRED — FIND-004-2. FIND-004-1 is CLOSED.**

The rustdoc remediation is complete. The required managed Rust documentation
gate still fails on four existing GitHub connection module links. No runtime,
concurrency, cleanup-redesign or approval finding is retained.

## Immutable subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Original base: `81aca5479145b2b94d5db4e0d15816d82cf40bc7`.
Prior candidate: `359a19f41597b8cfdc673373409b0d96e8298447`.
Current candidate: `b5c0db2e01268d6ad033fbe7450069247edff6ee`.
Incremental range: `git diff 359a19f41 b5c0db2e0`; original cumulative range retained.
Library root: `/home/thorrester/Documents/GitHub/bohmian-code-review`.
Original base: `d8c772de4248ce0130f79ec3b5fefda518b1431a`.
Prior candidate: `d380f57fa53429eaa4f3e7272ce651e1825a8b38`.
Current candidate: `9046d1035538df8dfb661f826b102316ee6d0b4e`.
Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 6;
explicit user approval remains authority despite stale markers.
Original Task 004 and prior remediation/verdict/ledger are retained at
`tasks/004-pinned-pr-preparation.md` and
`review/004-pinned-pr-preparation-approved/` in this packet.

Both source candidates remained unchanged. Only this new review report directory
was written. The incremental code diffs are rustdoc-only, plus the reviewed
library manifest/lock update and committed review/evidence records.

## Scope and independent coverage

[Scope](scope.md) records applicability and reasons to carry the original full
audit. Seven independent discovery reports plus focused follow-up and approved
full validation remain in the previous review directories. Their source/assertion
coverage is unchanged by documentation and pin-only changes.

This round selected scoped repository review for the reported red gate and
scoped system review for new lost-returned-key documentation evidence, followed
by mandatory fresh Ponytail validation. All selected reports are complete.
[Independent validation](findings-validation.md) checked applicability, carried
coverage, actual source, every previous finding item and the new proposal.
No new sensitive boundary, UI, SDK/language or agent runtime requires further
specialists. No missing coverage or unresolved conflict required follow-up;
[claim comparison](claim-comparison.md) records that decision.

| Review | Result and evidence |
|---|---|
| [Repository standards](standards-review.md) | FAIL: REPO-R2-001, four fatal unresolved module-doc links and mandatory red-gate rule. |
| [System recovery](system-review.md) | PASS: existing destructive removal primitive limitation established, no required supported-use regression or new workflow guarantee. |
| [Fresh validation](findings-validation.md) | FIX_REQUIRED: FIND-004-1 closed, REPO-R2-001 confirmed as FIND-004-2. |

## Reconciled acceptance and closure

| Obligations | Result |
|---|---|
| L1–L15 / REQ-006/007 / AC-004 acquisition | PASS carried: supplied pins, source/full/active comparisons, unchanged bounded context, explicit failures, cold/warm behavior and existing owner reuse. Actual library runtime code unchanged. |
| W1–W9, INV-001/002/004/005/007 | PASS carried: native create-only storage, SQL/RLS/CAS/time, guarded claim/touch and expiry, identity/rename, real-adapter proof, cache/evidence separation. Targeted recovery assessment narrows acceptance to actual supported owners. |
| N1/D1 and non-goals | PASS carried: no second index/cache/evidence engine, no invented execution IDs or new runtime service. |
| D2 / R1-H | PASS: exact reviewed library revision in manifest and seven lock sources; ancestor of local origin/main at 8adb0b7b; recorded authorized PR #7 delivery/build. |
| R1-L: fields/helper/panics/links | PASS, FIND-004-1 CLOSED: precise actual-body closure table in fresh validation; additional task-local links corrected without code change. |
| R1-W: fixture Panics | PASS: four required helper contracts match actual body failure paths. |
| R1-D: durable-write cancellation/retry | PASS: shared commit boundary and operation-specific outcomes accurately documented, including non-replayable removed-key results. |
| R1-P: docs-only | PASS: signatures, executable statements, tests, SQL, errors and preconditions unchanged in both source ranges. |
| Mandatory managed Rust documentation gate | FAIL, FIND-004-2: independent managed executions exit 101 on exactly four module links. |

FIND-004-1 remains closed under its stable ID. This is the first source-remediation
review; two unsuccessful remediation reviews have not occurred, so no convergence
escalation applies. The remaining issue is an independently observed mandatory
gate, not incomplete implementation of the prior rustdoc correction or a new
runtime-hardening expectation.

## Validated finding and smallest correction

**FIND-004-2 — CONFIRMED, VIOLATION**, discovery REPO-R2-001.
`crates/wyrd-spec/src/github_connection.rs:4,6,7` produces four bare module-doc
links to GithubConnectResponse, GithubSetupQuery, GithubCallbackQuery and
GithubConnectionView that the existing public rustdoc command cannot resolve.
All target types already exist in `crate::github_connection`. The managed gate
and crate broken-link denial make this an explicit mandatory failure; the root
AGENTS red-gate rule does not permit deferral because another task introduced it.

Qualify only the four destinations to `crate::github_connection::<Type>`,
preserving labels/prose and existing public types. This is a three-line doc
correction plus wrapping if needed, with no API/import/reexport/dependency,
warning-policy or runtime changes. Require `mise run check:docs` exit 0 and a
documentation-only diff check. Do not sweep nonfatal private-link warnings.
The [self-contained remediation](004-R2-fix-required-documentation-links.md)
contains source diagnosis, exact correction, preserved scope and focused proof;
route it directly to `$wyrd-implement`.

## Newly reported cleanup evidence

A committed destructive `ContextStore::remove_repository` can lose its returned
keys and orphan cache bytes if the reply is lost. This matches the adopted
baseline primitive/README implementation; no Contexts removal facade or
production offboarding consumer invokes it. Normal expiry deletes object before
metadata and keeps retry state. The approved task does not promise replayable
offboarding receipts, physical-erasure deadlines or a new durable offboarding
service. Prior recovery evidence is carried for its actually traced paths,
without claiming that it proves replayable removal results. No new finding,
persisted queue or cleanup redesign is justified in this docs-only remediation.

## Verification and limits

[Verification](verification.md): Wyrd/library incremental diff checks PASS;
library strict private-doc build PASS; new library revision ancestry PASS.
Orchestrator, standards reviewer and validator independently reproduced managed
`check:docs` exit 101 with the same four fatal links. Other private-link warnings
are nonfatal and excluded from remediation. Prior 11/11 adapter and 10/10 public
pipeline runtime proof carries; broad owning lanes remain recorded task evidence.
No runtime/mutation/cloud suites were repeated for docs-only changes; real GCS
remains unrun. Candidates stayed immutable. Review does not implement, commit,
push, merge or deploy.
