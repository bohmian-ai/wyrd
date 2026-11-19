> **Deprecated for future work — 2026-11-09.** The user requested a fresh design of the remaining capabilities. Continue in [GitHub Verifier pipelines design](../../github-verifier-pipelines/design/notes.md). This packet is retained as history; its prior approvals and evidence do not approve the replacement.

# Verification pipeline task packet

**Approved GitHub correction, revision 24:** the original TASK-004 is
[deprecated](TASK-004-github-repository-automation.md). The active replacement is
[TASK-004 — Install Wyrd's GitHub App and receive PR Checks](TASK-004-installable-github-app.md).
Teams install one deployment-configured App, select repos, connect their tenant
and receive ordinary Verifier Checks. SaaS/self-hosting share code; App keys
belong to the operator. Keep/extend the generic independent library branch at
`9fdf6dc`; no Wyrd-specific callback, credential, tenant or workflow logic goes
into it. The 22 rejected Wyrd commits remain reversed locally; no old PASS table
is acceptance of the new task.

Tasks 005 and 006 now consume/teach that exact connection contract. TASK-007 was
checked for GitHub impact and only clarifies that resumption reuses the existing
installation, and GitHub consent is not Wyrd human approval. Its pre-existing
unapproved wait/approval policies are separate from this GitHub amendment and
are not silently resolved by it. All revised GitHub decisions are fixed; public
fields/routes and selected protocol are in [public contracts](../public-contracts.md).

The approved TASK-003 revision-22 amendment is recorded in [the spec](../spec.md):
retain shared process/per-execution admission without tenant quotas; retire
acceptance-time limits reporting; keep server recovery fault injection in Rust;
and close shared-client cancellation errors and the forced cancellation-writer
race in [TASK-003 scenarios 5–6](TASK-003-generic-orchestration.md#scenario-5--waiting-on-a-cancelled-execution-reports-cancellation).
TASK-003 remains incomplete pending that executable work and verification.
No separate remediation task or SDK harness expansion is introduced.

The approved implementation baseline is revision 18, incorporating
[durability supplement revision 3](../execution-state-and-durability.md) and
the explicitly approved uniform terminal-settlement amendment. The current [spec with approved amendments](../spec.md) records the approved suspension,
SubmissionSink resume-admission and stable-execution/new-delivery decisions.
[TASK-007](TASK-007-durable-waits-and-approval.md) owns the new capability.
Remaining material contracts are explicit in [wait/approval](../wait-and-approval.md);
TASK-007 is proposed, not ready while those remain open. R4 is not blocked.

TASK-001 is implemented. Its remaining foundation contract is recorded in
[TASK-001-R4](../review/task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md)
and is executed through the active replacement handoff
[TASK-001-R5](../review/task-001-r4-0d317ecdf-working/TASK-001-R5-single-terminal-settlement.md).
R5 replaces the rejected R4 implementation without dropping R4 obligations.
References to R4 in unchanged tasks identify that contract, not its rejected
implementation; use R5 as the active prerequisite.
The original TASK-001 document is requirements/history, not an active owner of
new work. Current order: **001-R5 → 002 → 003 → 004 → 005 → 006 → 007**.
Renumbering changes navigation only; architecture, scope, proofs and real
prerequisites remain unchanged. Tasks 002–007 remain proposed; implementation
and independent acceptance are distinct, so TASK-001's remaining findings are
not closed by its implemented status.

## Implementation order

| Task | Outcome | Required predecessors |
| --- | --- | --- |
| [001-R5 — Single terminal settlement](../review/task-001-r4-0d317ecdf-working/TASK-001-R5-single-terminal-settlement.md) | Active replacement for rejected R4 implementation, retaining its contract: record every queued terminal decision in Bifrost, receive batch-commit ACK, then retire queue delivery; close validated findings16/17/19/22/23/26 and original hosted-dependent proof. TASK-001 and implemented R3 remain historical. | Implemented R3 at d89356985. |
| [002 — Agent tools](TASK-002-agent-tooling.md) | fs.write/publish contracts, selection, permissions and real Agent-loop use of foundation state/storage. | 001 |
| [003 — Generic orchestration](TASK-003-generic-orchestration.md) | Ready successors, bounded fanout/repeat/conditions/speculation and recoverable control state. | 001, 002 |
| [004 — Installable Wyrd App](TASK-004-installable-github-app.md) | Operator-configured App, both installation entry points, verified tenant/repository connection, trusted configuration, ordinary PR execution and independent Checks; generic library branch kept/extended. | 001-R5 |
| [005 — Pull-request extraction](TASK-005-pull-request-extraction.md) | Whole public library pipeline, pinned complete evidence, intended adapters and library maintenance. | 001, 002, 003, 004 |
| [006 — Inspection and education](TASK-006-inspection-and-developer-education.md) | UI, bounded safe inspection, complete examples and dedicated docsite. | 001, 002, 003, 004, 005 |
| [007 — Durable wait, approval and suspension](TASK-007-durable-waits-and-approval.md) | Delay/action/human waits release active resources; SubmissionSink admits a new queue item restoring the same execution. Includes timing, identity, auth, durable handoff and feature-specific client/inspection/example closure. | 001-R4, 003, 006 |

TASK-001-R5 closes the existing foundation under R4’s retained contract before dependent capability work.
References to prerequisite 001 below mean its remediated, accepted foundation,
not reopening its original task. Task 002 exposes Agent tools;
Task 005 retains extraction evidence through the engine, not a publish call.
Tasks 004 and 005 extend the same library GitHubClient and immutable dependency
lineage. Task 004 uses an ordinary no-input Verifier and does not require
extraction or Agent tools. It owns connection UI and operator/team setup docs;
Task006 teaches the subsequent full-review story rather than deferring onboarding.
Task005 extends Task004's verified immutable upstream descendant, never a Wyrd
App implementation in the independent library. Planning does not approve or implement tasks.

## Obligation ownership

### New wait/approval capability — TASK-007

[TASK-007](TASK-007-durable-waits-and-approval.md) is the new implementation
home for delay/wait, timer/action-triggered resumption, human approval and
suspension. The selected owners are the existing engine, canonical Bifrost
records, SubmissionSink for admission, and existing queue/runner/lease owners.
A resume delivery has its own queue identity and retains the original
ExecutionId. Bifrost never writes PostgreSQL or performs admission.

This feature is not R4 scope. TASK-003 owns existing long-running limits and
renewal, not suspension. TASK-006 supplies the existing inspection/education
baseline; TASK-007 closes its added client/auth/queue/GitHub/UI/example behavior
without assigning work back to implemented TASK-001. The task cannot be marked
ready while the supplement's material decisions remain unresolved.

Each task acceptance table maps obligations to observable proof. Primary
ownership does not excuse necessary consumer or negative-case closure.

| Obligations | Primary proof owner |
| --- | --- |
| REQ-001–009/045/051; INV-001/005/011/016; AC-003–004/020/023/038/043–045 | 003: validation, readiness, fanout, joins, repeat, speculation/recovery, existing shared admission and cancellation-writer race closure. |
| REQ-010–015/031–032/046–049; INV-004/009/015; AC-005–009/024–025/039–041 | 001-R4: remaining backend state, mounts, cancellation, history and page corrections against implemented 001. 002 closes real Agent publication/receipt proof. |
| REQ-021/023; INV-003/006–007; AC-018 | 002: hosted selection, per-call authority, bounds and provenance. |
| REQ-016–018/020/041–043; INV-014; AC-010/012/033–036 | 005: whole pipeline, selector/bundle, adapters, completeness, shared recovery, budgets and library maintenance. |
| AC-017 | 005: fork/source and integrated hosted-tool revocation. 002 supplies tool authority; 004 supplies repository policy. |
| REQ-024–030/034–038/040; INV-002/012–013; AC-001–002/011/014–016/019/026–028/030–032 | 004: deployment App/onboarding, generic provider protocol, verified tenant association/access, trusted config, ingress/generation/delivery and backlog. 005/006 close real review integration. |
| REQ-033/044/050; AC-021–022/037/042 | 006: bounded safe browser inspection, canonical examples, combined-surface regression and rendered-docsite/fresh-reader proof. |
| REQ-052–059; INV-017–019; AC-046–051; DUR-001–010 | 001-R4: all remaining foundation/remediation and approved research-follow-up proof. 002–006 close their task-local consumer seams. |
| REQ-032 cancellation contribution | 003: exact shared SDK cancelled-wait error and public SDK assertions; server stored status and response shapes unchanged. |
| REQ-060–068; INV-020–026; AC-052–060 | 007: delay/action/human wait declarations, release/resume, separate queue delivery identity, permission/recent-login, durable handoff and feature-specific surface closure. |
| INV-010 | Every task uses portable shared behavior; 006 confirms existing CI/platform/deployment closure. |

Public surfaces remain HTTP, shared Rust client, Rust/Python/TypeScript SDKs,
CLI and MCP. Each introducing task owns its operations and negative cases;
006 verifies the combined story rather than absorbing earlier missing behavior.

## One execution-state architecture

Direct verify stays queue-free. Submit, schedule and group activation use
existing PostgreSQL admission/claims/lease renewal/retry. Both paths enter the
same engine, restore exact accepted Bifrost state or establish absence, and run
from memory. Complete logical checkpoints use the existing shared writer and
WAL acknowledgement. Preserve normal lease/cancel/deadline controls and generic
accepted-data/replay integrity, with the user's accepted rare delayed-old-worker
overlap interval; do not add a new SQL ownership protocol. Unavailable accepted
state causes wait/failure, not older/fresh fallback. Direct final-record
delivery retains its approved asynchronous fast-return exception.

There is no PostgreSQL invocation/publication payload mirror, archive handoff,
extra writer or lease scheduler. Preserve names/receipts, exact descriptors,
grants, budgets, original deadlines, settled/interrupted semantics, independent
delivery and the distinction between queue retry and execution recovery.

## Historical evidence and supersession

- Original TASK-004 is deprecated; its full evidence is preserved in
  [the rejected archive](../old/TASK-004-github-repository-automation-rejected-487ccb0b6.md).
  Only TASK-004-installable-github-app.md is an active TASK-004. Its proof is new,
  not carried forward from either the rejected implementation or unfinished review.

- Current TASK-001 is implemented, not an owner for new edits/proofs. Its
  remaining independent-review and approved research obligations are assigned
  only to R4; preserve the original evidence as historical.

- [Original revision-16 Task 001](../review/task-001-r1-365bfc94b/TASK-001-original-durable-agent-work.md)
  is archived with its original ID and reported implementation context.
- [Historical Task 001-R1](../review/task-001-r1-365bfc94b/TASK-001-R1-durable-execution-guarantees.md)
  remains superseded. Original fixes and scenario 12 are reported implemented;
  13–17 were not. Implemented TASK-001 replaced their obsolete lifecycle
  mechanisms; R4 owns all remaining safety/replay/publication/delivery work.
- Original finding IDs, immutable reviews and comprehension-r17 remain unchanged.
  They do not certify the new architecture. The readiness review predates
  renumbering; each renamed task's `previous_id` preserves that reference mapping.
- Replace architecture-specific test assertions deliberately; retain valid
  coverage rather than deleting tests to make a gate green.

## Proof and completion

Tasks 002–007 use end-to-end user journeys as scenarios, with supporting
negative/integration cases beneath them. TASK-001 is implemented; new proof and
remaining corrections are recorded in R4. Tasks retain Behavior/RED/GREEN/REFACTOR
proof discipline and exact planned selectors
in existing targets. [Shared verification](verification.md) owns repeated setup
and owner checks. Planning runs no runtime tests or throughput benchmark.
Run focused proof during implementation and integrated public-surface/platform
journeys at change review. Independently verify the reported original fixes.
[Public contracts](../public-contracts.md) remain the packet's shared API and
permission contract. No task renumbering changes test identities or behavior.

## Revision-24 planning closure

The replacement TASK-004, TASK-005 and TASK-006 have complete GitHub contracts
and explicit Behavior/RED/GREEN/REFACTOR scenarios. Their named selectors are
planned implementation proof, not passing evidence. Independent comprehension
of the current spec/contracts/task packet found one stale resource-limit
administration phrase; it was resolved by retaining existing deployment and
Verifier budget owners, and the reader confirmed closure. No new limits API,
repository snapshot policy, tenant App mode or Wyrd-specific library behavior
is authorized. Local links/scenario structure were checked; implementation
and runtime verification have not been performed by planning.

TASK-007's previously deferred non-GitHub approval/wait contracts remain outside
this GitHub planning closure. No pending GitHub decision is delegated to it.
