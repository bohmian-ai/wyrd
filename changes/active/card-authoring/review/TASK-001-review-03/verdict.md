# TASK-001-R3 verdict: FIX_REQUIRED

The eight independently validated revision-4 findings are
**FIND-TASK-001-4 through FIND-TASK-001-11**.
Original **FIND-TASK-001-1/2/3 are closed**. All existing focused checks passed;
the retained failures are outside those assertions or explicit repository gates.

## Subject and scope

Repository: /Users/stevenforrester/Documents/GitHub/wyrd.
Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
Prior reviewed revision-3 candidate: `c0a06eacdc70249e9c32e44b7670314cf1aefb3a`.
Immediate R3 base: `a387f2a47a145a867c8a4948966fcbd136af9961`.
Candidate: `37b3c2f96b1bd425bf1f0923a1950d0363c40907`.

Reviewed the complete immediate diff and affected owners/callers under approved
spec revision 4 and TASK-001-R3, retaining original task, revision-3 approval
snapshot, earlier verdicts/remediations and source-backed acceptance context.
Ancestry is preserved; no correspondence reconstruction was needed.
[subject.md](subject.md) records exact ranges, authority paths/hashes,
navigation and scope expansion. The new staging/public input/declaration
paths required fresh review; unrelated server/executor internals did not.

HEAD, tracked source and packet hashes remained unchanged. Packet/reports are
locally untracked. Review wrote reports and the next remediation task only;
no implementation, staging, commit, merge or push.

## Independent roles and coverage

| Role | Report | Result within assigned scope |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL: missing edits and Artifact root adaptation |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL: canonical equality, malformed raw spec and selection intent |
| repo-rev | [standards-review.md](standards-review.md) | FAIL: finite changed-item docs and exact IO-code proof |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL: Artifact root workflow; no unearned second owner established |
| system-rev | [system-review.md](system-review.md) | PASS for existing failure/recovery ownership, qualified by separate local-admission defects |
| domain identity-publication | [domain-review-identity-publication.md](domain-review-identity-publication.md) | FAIL: local Artifact complete admission and root adaptation |
| focused followup-rev | [followup-review.md](followup-review.md) | Selection materiality resolved; setter representation question routed to validation |
| independent ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; eight retained, no validator-origin blocker |

The independent validator confirmed all selected reports and specialist
applicability/carry-forward. No required role is missing. Prior security,
tenancy, exact-subject and executor evidence carries only at unchanged
owners after fresh dispatch/source traces confirmed the same client,
authorization, tenant resolution and planner/engine consumers. No new
principal/route/security or executor state warranted separate fresh discovery.
Current identity/publication and failure paths received fresh domain/system
coverage.

[claim-comparison.md](claim-comparison.md) groups claims by shared cause,
not by reviewer agreement. Follow-up was justified by the descriptor selection
materiality disagreement and constructor/setter collision. Independent
validation narrowed the former to explicit construction/snapshot parity and
resolved the latter against authority reserving representation choices.
No new grammar or public-call change is required: correction stays within
the approved calls and existing owners. No further follow-up is needed.

## Validated ledger and correction boundaries

Full producer-to-consumer traces, exact locations, correction burden and
focused proof are preserved in [findings-validation.md](findings-validation.md).

| ID / decision | Classification and exact failing boundary | Required correction |
|---|---|---|
| FIND-TASK-001-4 CONFIRMED; INV-R3-001 | INCORRECT: raw same_content at authoring/mod.rs:978–982 rejects canonical-equal omitted versus explicit-empty same-identity Cards | Use existing successful canonical decoding/projection for equality, retaining traversal/invalid diagnostics/true conflicts. |
| FIND-TASK-001-5 CONFIRMED; INV-R3-003 | INCORRECT: CardDefinition::new at mod.rs:834–838 drops non-object raw specs into valid empty Service content | Preserve raw input until the existing shared decoder refuses it; preserve intentional empty-object staging. |
| FIND-TASK-001-6 REVISED; INV-R3-002/follow-up | INCORRECT: Artifact.file at kinds.rs:268–273 retains a sibling capture but emits external selection, breaking explicit snapshot intent/root ordering | Reuse the existing Artifact conversion's value together with captures. No unproved executable-byte claim or new upload. |
| FIND-TASK-001-7 REVISED; IDPUB-001 | INCORRECT: local Artifact graph branch at mod.rs:913–968/kinds.rs:280–301 omits nested captures/shared Ref admission and cross-partition conflicts | Visit current local captures dependency-first and run shared canonical admission/identity checks before returning publication plan, preserving separate requests. |
| FIND-TASK-001-8 REVISED; BR3-002/M3-001/IDPUB-002 | MISSING: Artifact lacks named-root declaration adapters; local validate produces empty ordinary graph | Complete root admission with full metadata/captures/source and actual existing sole-publication receipt, distinct from field-reference lowering. |
| FIND-TASK-001-9 REVISED; BR3-001/follow-up | MISSING: Action.from/agent and Check.each/agent edits absent at verifier.rs:245–419 | Complete exact approved calls through existing Body/canonical conversion; separate constructor lookup/editable receiver where needed, without new grammar or trait imports. |
| FIND-TASK-001-10 REVISED; REPO-R3-001 | VIOLATION: rewritten main lacks workflow/Errors; Model/Data/Experiment docs are name-only; private submit omits durable cancellation contract | Finite meaningful docs using existing domain/lifecycle owners; batch with functional remediation. |
| FIND-TASK-001-11 CONFIRMED; REPO-R3-002 | VIOLATION: deferred-source test at tests.rs:573–578 accepts wrong code or substring IO | Assert exactly WYRD_LOADER_400_IO at existing diagnostic proof; no production mapping change. |

No duplicate-workflow DRIFT finding was validated: raw local staging is earned
by deferred input/capture behavior, while canonical parsing, graph ordering,
registry, upload and hydration retain their existing owners. The missing
same-named setters do not justify inventing renamed APIs or waiving the
approved table. Concrete return/staging representation remains implementation-owned.

## Prior closure and reconciled acceptance

Original FIND-1's obsolete-current-capture bug is closed through Body field
replacement and current conversion tests. Original FIND-2's revision-3
inventory/nullability proof remains supplied; new revision-4 field/root gaps
are separately numbered. Original FIND-3's five wrapper contracts are resolved:
three no longer inherit early panics after deferred construction; the other
two have the actual panic docs. New rewritten-item docs are FIND-10, not a
retrospective reopening of those five locations.

| Requirement / acceptance | Result and evidence |
|---|---|
| REQ-001/002, AC-007 all named families and edits | FAIL: Artifact named root and four field edits missing, FIND-8/9; exercised inventory/operators otherwise pass. |
| REQ-002/004, AC-002 malformed-input preservation/diagnostic parity | FAIL: raw shape erased, FIND-5; existing local deferred/diagnostic tests pass. |
| REQ-002/008 current replacement, duplicates and snapshots | PASS for existing operations, with original capture defect closed; newly missing Agent edits remain FIND-9. |
| REQ-008 canonical-equal reuse and true conflicts | FAIL: FIND-4/7; raw equality and separate local identity partition omit required behavior. |
| REQ-004/008 complete graph admission | FAIL: FIND-7, local Artifact captures/Ref admission omitted. |
| REQ-005/008 fixed-reference intent | FAIL: FIND-6; local/registered cases work, descriptor snapshot loses sibling intent. |
| REQ-003 required/nullable slots | PASS: public matrix and fresh native null/omission regressions. |
| REQ-004 planning/concurrency/dependent outputs | PASS: unchanged canonical owners, fresh exact regression/dependency journey and carried source evidence. |
| REQ-005/006, AC-003 existing publication/scratch/source maps/errors/retention | PASS existing lifecycle/journey, but surface completeness blocked by FIND-7/8. |
| REQ-006/008, AC-001/008 path/native/Card, client/options/hydration | Existing dispatch/parity/path-state behavior PASS; Artifact root gap FIND-8 blocks full acceptance. |
| REQ-007, AC-004 two-component case matrix/attribution | PASS: fresh component/dependent journeys and validated security carry-forward. |
| Runnable finance example simplification | PASS behavior: one writer, Service-level three requirements and exact real-binary output; item docs remain FIND-10. |
| INV-001–005 / non-goals | PASS: no wire/server/auth/schema/dependency/feature/scheduler/foreign-runtime implementation change or new owner established. |
| Mandatory item documentation and exact negative proof | FAIL: FIND-10/11 despite green compiler/doc checks. |

REQ-009/010 and AC-005/006 native language projection work remains outside this
common-Rust task, as the approved task/index state; this is not a missing
Python/TypeScript implementation finding.

## Verification and limits

[verification.md](verification.md) records all exact fresh commands/run IDs:
six authoring local tests, public inventory, three native scheduling/slot tests,
five SDK journeys and the real finance example journey passed: **16 behavioral
tests**, plus both migration tests. Server build, scoped all-target Clippy,
warning-free client/loader/SDK documentation, format and immutable-range
whitespace checks passed.

Linking emitted the previously observed compact-unwind-size warning without
failing builds/tests. Broader workspace/dependency/mutation results in task
evidence remain historical, not fresh review runs. No full gate, cross-platform
execution, live-provider quality assessment or fault injection was performed.
Mutation sensitivity does not establish every original pre-implementation RED;
no extra behavioral finding was manufactured from that historical limitation.

The green suite does not cover the retained failing cases. Material risk is
incorrect local admission/parity and incomplete promised authoring operations,
including missing publication dependencies and acceptance of unintended empty
specs. Existing server permission/tenant and durable lifecycle owners remain intact.

## Convergence and next bounded review

Prior corrections are closed. This round implements a human-approved
revision-4 rewrite rather than another attempt at the old helper prose.
The new gaps concern newly changed canonical staging, Artifact
admission/root/selection and approved edits, plus finite new mandatory
documentation/proof omissions. No optional hardening, server behavior,
scheduler or security expectation was added.

One ready task packages the validated corrections:
[TASK-001-R4-canonical-admission-and-artifact-roots.md](TASK-001-R4-canonical-admission-and-artifact-roots.md),
routed directly to `$wyrd-implement`. It preserves approved calls and existing
owners; no specification revision is necessary.

Next review checks FIND-4–11 closure, the actual new diff and affected
canonical/publication/public consumers. Carry unaffected accepted rows and
closed original IDs forward. Use focused local/public proof and one existing
Artifact journey for changed nested/root publication; do not restart this
entire team or full journey suite absent a materially changed boundary.

**Verdict: FIX_REQUIRED.**
