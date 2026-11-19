# Full TASK-003 / SDK review — FIX_REQUIRED

## Immutable subject

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Clean source checkout: `/tmp/wyrd-task003-review-d7685d129`.

- Requested full range: `634e19cbc517293ed2458f30ce73401714f3447d` through
  `d7685d129ffee870ab1c497cdcb1206a89bc80c4`, six commits / 88 paths.
- Prior substantive candidate: `480251a70f799b8581ca5a4be77983d6f345d5e3`;
  original cumulative TASK-003 base:
  `5775c3795ebd6557d080b34d55dd2a67b8dbad63`.
- Authority: committed approved SPEC-positioning revision 7,
  SPEC-service-identity-and-telemetry revision 17, applicable repository rules
  and owning architecture. Revision 7 explicitly replaces the prior Rust
  configure/state.flush requirement.
- Original task / prior R1 / prior R2 content pins:
  `6846032793910170e76c0cc0ec9de2f50cb6ad6f`,
  `6f3484e808b5015efa78abdf0c2b52863082fea5`,
  `49fd0a6a3cdbcb23bd13c80ff7b8c6b834090e31`.
  Full paths are in [review-scope.md](review-scope.md).

The user explicitly requested a full review of the expanded six-commit range.
This review covered it, retaining original task/remediation/prior acceptance
context rather than silently redefining scope from the summary. No ancestry
rewrite occurred. Stale task revision-5 metadata does not override approved
revision 7 or imply execution of the older task mechanics.

Primary worktree had additional lifecycle/error/test/generated/planning edits;
all were excluded. Source review and fresh checks used the detached candidate,
whose HEAD and clean status remained unchanged. Task content pins were
rechecked. No reviewed source, authority, staging or commit was modified.

## Independent selection and coverage

The pre-discovery map and specialist applicability are in review-scope.md.
The fresh validator independently confirmed every selection, omission and
coverage obligation.

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL; full four-outcome/public-SDK/reader matrix; retention claim proposal |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL; startup concurrency/failure/cancellation/shutdown and producer context; TS cold-span proposal |
| repo-rev | [standards-review.md](standards-review.md) | FAIL; all 88 paths/languages/layers, prior closure, rustdoc, outbox claims and wait inventory |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL; existing-owner workflow comparison; cold-span proposal, no unearned writer/parser/provider replacement |
| system-rev | [system-review.md](system-review.md) | FAIL; startup/provider failure boundaries, recovery and outbox durability; retention proposal |
| evidence domain-rev | [domain-review-evidence.md](domain-review-evidence.md) | FAIL; producer/Run/subject/tenant/evidence integrity, native outcomes and visibility; both substantive proposals |
| Separate fresh ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; independently traced/materiality-checked all proposals and corrections, reconciled acceptance and specialist coverage |

No separate RBAC/security or SQL/storage-engine reviewer was required: their
implementation boundaries were not changed. Domain review covers ordinary
grants/tenant/evidence consumers; invariant/system cover the coherent start
concurrency and recovery domain. The validator confirmed this is sufficient,
not an excuse inferred from changed filenames.

[claim-comparison.md](claim-comparison.md) groups proposals into three causes.
No material conflict or untraced uncertainty required followup-rev; agreement
and unique findings were still independently validated. No validator-origin
blocker required separate validation. Rejected older provider-race and stub-
layout proposals were not reopened without new material evidence.

## Reconciled acceptance and prior closure

The complete source/proof matrix is in findings-validation.md. Source-backed
unchanged judging, retained reads/versions, tenant refusal, scheduling and
original tutorial evidence carry forward after changed dependencies were
checked; they are not conclusions based only on unchanged filenames.

| Obligation | Result |
|---|---|
| Request waits for verdict, releases only pass, holds non-pass/error; one state per FastAPI lifespan | PASS |
| Actual complete/skipped workflow, equal plausible replies, original evidence and native missing-trace inconclusive | PASS |
| Real scheduled fast/slow/empty windows and exact identities; no client scheduler | PASS data/judgment |
| Retained failed requirement/original evidence/versions, no rejudge, native unknown/foreign refusal | PASS behavior; prose fails FIND-6 |
| Reader-native commands/dependencies, canonical guides/dispositions and bounded handoff | PASS; FIND-1 CLOSED |
| Trace marker model/mapping/string lowering through existing canonical owner | PASS |
| Default managed configure/opt-out, TS async, frozen repeat/failure, provider ownership | PASS |
| Rust export setup under revised AC-016 | PASS; FIND-3 SUPERSEDED/CLOSED under replacement authority |
| TS missing optional-package public-configure proof | PASS; FIND-4 CLOSED |
| Writerless/closed/started flush; first-use shared writer, failure/cancellation retry and terminal shutdown | PASS runtime within disclosed proof limits |
| TypeScript cold and warm sample enqueue PRODUCER evidence | FAIL cold branch, FIND-5 |
| Truthful result staging, linger, graceful loss and visibility claims | FAIL, FIND-6 |
| Mandatory changed-item async/durable cancellation documentation | FAIL, revised FIND-2; prior seven named sites CLOSED |

## Independently validated ledger

| Stable ID | Decision / discovery sources | Minimum correction |
|---|---|---|
| FIND-TASK-003-5 | REVISED / INV-FULL-001, MAINT-FULL-001, EV-FULL-1 | Preserve the Node-created producer context across cold Window staging to actual enqueue, retaining caller context for startup; extend existing cold/warm timeline and record/link proof. |
| FIND-TASK-003-6 | REVISED / BEH-001, SYS-1, REPO-FULL-001, EV-FULL-2 | Correct three coupled paragraphs/generated text: batching linger is no durable deadline, graceful drain has a budget, terminal/deadline/crash loss is possible, and missing evidence does not promise future arrival. |
| FIND-TASK-003-2 | REVISED / REPO-FULL-002 | Add cancellation/progress/retry rustdoc to start_gated and lifecycle/public flush, without changing runtime behavior. |

The detailed ledger contains exact locations, authority, producer/consumer and
sibling traces, supported consequences, materiality, correction burden and
focused closure. The producer issue loses causal span evidence, not proved
sample-row or tenant integrity. The retention issue is false published
guarantees, not a requirement to strengthen the existing outbox. Rustdoc is an
explicit mandatory gate, not an inferred runtime failure.

No new public/product/security/concurrency/persistence decision or specification
revision is needed for these corrections. No optional rejected work is added.

## Verification and hot paths

[verification.md](verification.md) records fresh clean-candidate passes: five
exact Rust tests (five selected/passed), Rust format and cumulative whitespace,
102 docs tests, reader commands, links, static build/Pagefind, 42-page
accessibility and Svelte check with zero errors/warnings. The table-concurrency
Rust test starts its writer explicitly and does not prove the cold Node trace
branch. Broader supplied language units/integrations/example journeys/lints/
types/generated-sync evidence was inspected against actual assertions, not
independently rerun wholesale. Green checks do not close the semantic defects.

The detached docs dependencies were installed from the locked local cache
after pnpm refused reuse through a symlink; the normal build then passed.
No source/lockfile change or check bypass was used. No full gate, full journey
aggregate, full docs generation aggregate, live provider or benchmark ran.

The validator independently traced the defined VerificationEngine/direct
verify/gateway/Oracle/Scribe request owners and actual callers. None of their
request implementations changed. Warm samples use a short phase snapshot and
existing synchronous enqueue, bypassing the async start gate. Added/moved
external waits inside designated engine hot paths: **none**.

The relevant non-engine waits are cold start-gate contention, existing
credential/client/channel connection, existing declared/fixed/named table
describes, managed setup imports/provider construction, and explicit provider/
producer drain barriers. REQ-017 authorizes first-use startup. Complete wait
inventories are in standards/invariant/validation reports. The conditional
clean comparative engine benchmark gate was not activated, rather than waived.

## Convergence and next bounded scope

This is the second unsuccessful substantive remediation review; the earlier
subject-only block does not count. Previous reader/rustdoc corrections and TS
negative proof landed, and approved revision 7 resolved the Rust-call demand.
The new cold-span defect comes from the authorized lazy-start expansion; the
retention claims were introduced in the reopened range. The remaining rustdoc
sites are newly assessed async/durable operations under the existing gate.
These causes do not justify another startup/architecture redesign or an
expanding checklist.

**FIX_REQUIRED**. Route
[TASK-003-R3-preserve-first-sample-evidence.md](TASK-003-R3-preserve-first-sample-evidence.md)
directly to `$wyrd-implement`. It contains the existing producer handoff and
one cold/warm timeline proof, three coupled prose corrections/generated text,
and three local documentation additions. Next review checks closure, actual
delta and affected consumers, carrying accepted tutorial/provider/scheduler/
tenant evidence forward unless new material changes invalidate it.

Review does not implement, stage, commit, merge, push or deploy.

Skipped: full journey/gate/benchmark aggregates and dirty primary followups.
Risk: candidate's cold TypeScript trace and published retention guarantees
remain incorrect; cancellation documentation also remains merge-blocking.
