# Task 004 review verdict

**BLOCKED** — approval of the affected specification revision is not established.
Independently validated technical ledger: **FIND-004-1** (mandatory rustdoc gate).
Without the authority limitation, technical disposition would be FIX_REQUIRED.

## Immutable subject and review scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Wyrd base: `81aca5479145b2b94d5db4e0d15816d82cf40bc7`.
Wyrd candidate: `359a19f41597b8cfdc673373409b0d96e8298447`.
First review range: `git diff 81aca5479...359a19f41`.
Library root: `/home/thorrester/Documents/GitHub/bohmian-code-review`.
Library base: `d8c772de4248ce0130f79ec3b5fefda518b1431a`.
Library candidate: `d380f57fa53429eaa4f3e7272ce651e1825a8b38`.
Task: `changes/active/github-verifier-pipelines/tasks/004-pinned-pr-preparation.md`;
original at base, candidate adds obligations and evidence. No prior review or
finding closure is carried forward; no convergence assessment is applicable.
Both candidates remained unchanged. Only this new review report directory was
written; source, tests and migrations were kept immutable.

## Authority limitation

`spec.md` revision 6 and Task 004 still explicitly require renewed approval.
The prior visible instruction directed updating design/tasks for the storage
exception; the present instruction asks for review. Neither establishes approval
to implement the material persisted-contract amendment. Last approved revision 5
(`0f08e6c9f`) prohibits new PostgreSQL tables beyond installations. The bounded
direct implementation exception requires an explicit implementation instruction;
implementation ledger assertions cannot supply it.

This is an existing approval gap, not a need to redesign or write revision 7.
Revision 6 already records the directed outcome. Do not infer approval, claim
retroactive approval or undo the implementation. The technical audit and its
retained finding remain useful independently of the approval limitation.

## Independent coverage and results

[Scope and specialist applicability](scope.md) records selection before discovery.
[Independent validation](findings-validation.md) validates all selected coverage
and omitted roles against actual source; no required report is missing.

| Reviewer / report | Independent result and reconciliation |
|---|---|
| [Behavior](task-review-behavior.md) | BLOCKED for authority; comprehensive matrix. B-004-1 proposed, then rejected by independent validation. |
| [Invariants](task-review-invariants.md) | PASS; shared-state/lifecycle producer-to-consumer tracing. |
| [Repository standards](standards-review.md) | FAIL; REPO-004-001 revised and retained as FIND-004-1. Direct-implementation authority assumption rejected by validation. |
| [Maintainer](maintainer-review.md) | PASS; complete existing pipeline/storage/SQL owners reused, no unearned replacement workflow. |
| [System resilience](system-review.md) | PASS; partial writes, uncertain commit, cleanup/retry and failure containment. |
| [Persistent data](domain-review-data.md) | PASS; SQL/CAS/RLS/time/expiry and collection contracts. |
| [Security/tenancy](domain-review-security.md) | PASS; scoped keys, identities, operator expiry and evidence separation. |
| [Focused follow-up](followup-review.md) | RESOLVED; no supported producer failure for B-004-1 under unchanged grace/precondition semantics. |
| [Fresh Ponytail validation](findings-validation.md) | BLOCKED on approval; one revised mandatory documentation finding. |

No UI, Python, TypeScript, SDK public operation or model/agent runtime changes
require additional specialists. The behavior and domain reports cover the
public library API and real Wyrd adapter seams. Task 005/006 own eventual queued
extraction/SDK proof and are not silently added to Task 004 scope.

## Reconciled acceptance matrix

Detailed source and actual assertion citations are in the behavior matrix and
specialist reports; validation checks them rather than treating test names or
agreement as proof.

| Obligations | Reconciled result / evidence |
|---|---|
| L1–L5: supplied pins, moved live PR, merge-base/full/active comparisons, unusable baseline | PASS technically; real public facade journeys and exact comparison assertions. |
| L6–L12: changed files/source/pins/context, pagination/truncation, omissions, empty active | PASS technically; library source and journey assertions, including reconstructed trees and explicit errors. |
| L13–L15: cold/warm reuse, independent library ownership, existing workflows | PASS technically; archive-count assertions and complete owner/caller review. |
| W1–W3: native create-only storage, metadata migrations/RLS, pointer CAS | PASS technically; object adapter tests, forced-RLS assertions and competing publication. |
| W4: collection/touch/publication, grace and retry | PASS within the adopted library protocol. Refresh/claim arbitration plus documented grace longer than longest build protects supported publishers; no new arbitrary direct-CAS guarantee is introduced. |
| W5–W7: stamps/expiry, identity/rename, SQL capability/time/IO boundaries | PASS technically; real SQL/server assertions and domain source tracing. |
| W8–W9: object bytes/evidence separation and actual pipeline over Wyrd adapters | PASS technically; isolated cache namespace/schema and real pipeline integration. Accepted execution proof remains Task 005. |
| N1, D1: no second evidence/index/coordination service; architecture custody | PASS technically; schema/source/custody changes and existing-owner review. |
| D2: delivered reviewed library revision and normal pin | PASS technically; exact manifest/lock source, pinned ancestor of local origin/main at 445a70bf and committed delivery record. |
| Incidental storage journey repair and failure-clearing tests | PASS; standards report independently traces root-relative authentication, production GCS conditional create and stable collection grace. Assertions are retained. |
| Mandatory changed-item rustdoc | FAIL: FIND-004-1. |
| Approved affected specification | BLOCKED: revision 6 remains draft; existing approval must be established. |

## Claim comparison and validated ledger

[Claim comparison](claim-comparison.md) records why focused follow-up was needed.
B-004-1 is REJECTED: isolated pointer SQL lacks a claim lock, but its proposed
claim-then-direct-publication failure bypasses the adopted producer/grace
contract. Reference PostgreSQL/memory stores share those semantics. No supported
failure justifies stronger locking or altered CAS outcomes. It creates no work.

**FIND-004-1 — REVISED, VIOLATION**, discovered as REPO-004-001. New library
fields/helpers/tests and new Wyrd test helpers omit mandatory rustdoc/Panics;
new durable write methods omit required uncertain-completion/retry contracts;
a new fixture rustdoc link is broken. Actual affected items and exact source
locations are recorded in the [validated ledger](findings-validation.md#find-004-1).
Green checks do not waive the explicit mandatory documentation gate. The minimum
correction is one bounded changed-item rustdoc pass at existing owners, retaining
all runtime behavior and using shared contracts/precise links where sufficient.
No test harness, abstraction, schema or concurrency change is justified.

[Task 004-R1](004-R1-complete-changed-item-rustdoc.md) packages the full diagnosis,
exact established scope, correction and focused proof. It is diagnostic and
proposed, subordinate to this BLOCKED verdict; no implementation, publication
or dependency delivery is authorized by this review. Route the bounded correction
to `$wyrd-implement` with the pending authority status explicit.

## Verification and limits

Independent runs recorded in [verification.md](verification.md): SQL/server
adapter selector 11/11, library pipeline journeys 10/10, dependency and tenant
checks PASS; cumulative diff check PASS. Broad checks and mutation evidence
remain the committed implementation record, not new runs by every reviewer.
No real GCS cloud check was run. Future collection callers must honor the
existing grace/non-overlap preconditions. No accepted execution evidence or
Task 005 consumer is claimed shipped by this task. Review does not commit,
push, merge, implement or deploy.
