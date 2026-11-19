# TASK-001 verdict

## Current disposition — PostgreSQL audit, 2026-11-06

**FIX_REQUIRED.** The [focused PostgreSQL audit](../task-001-postgres-cd905f4cd/verdict.md)
and its [remediation task](../task-001-postgres-cd905f4cd/TASK-001-R3-complete-execution-state-remediation.md)
supersede the earlier specification-decision stop and relocation-only correction.
The user rejects the SQL ownership/checkpoint-head table itself (USER-PG-001)
and accepts the specified rare delayed-worker interval; finding20 must not
require a new queue-to-Scribe protocol. Finding18 remains withdrawn. Findings
16/17/19 are carried into the PG task;24/25 are subsumed by verified removal,
not repaired to preserve the rejected owner. Findings21–23 are now included
in the same consolidated R3 handoff. No implementation finding is closed by
writing this addendum or combining the tasks.

## Earlier disposition — superseded where noted above

**SPEC_REVISION_REQUIRED** — nine retained findings after the user-directed removal of finding 18. The material
stop is FIND-TASK-001-20: the approved outcome requires queue cancellation and
checkpoint acceptance to have an authoritative order, but the declared
queue-to-generic-Scribe authority handoff is missing. That decision also
constrains canonical custody of queue-owned terminal facts in finding 17.

## User correction — direct fast return

On 2026-11-06, the user confirmed that direct verification intentionally returns
results without waiting for Bifrost acknowledgement of final input/output/result
writes and instructed removal of FIND-TASK-001-18. It is withdrawn from the
active ledger and creates no remediation work. The original discovery reports
remain historical evidence; their contrary interpretation does not govern this
review. Other findings and the specification-decision verdict remain unchanged.

## Immutable subject and round scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
- Base: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb` (`a9f0b6261^`).
- Candidate: `cd905f4cd929785bda25696326d58b4a4ae88dee`.
- Complete first audit of the six-commit base...candidate range: 60 files,
  +4127/-3472. The final Revision 1 commit alone is not the reviewed subject.
- Authority: approved [spec revision 18](../../spec.md),
  [durability supplement revision 2](../../execution-state-and-durability.md),
  original [TASK-001 including Revision 1](../../tasks/TASK-001-bifrost-execution-state.md),
  applicable architecture, root/nested AGENTS and topic guides.

HEAD, implementation source and recorded authority hashes remained unchanged.
The packet and architecture amendments were uncommitted authority inputs at
establishment, as recorded in [subject.md](subject.md) and
[authority-sha256.txt](authority-sha256.txt). Only review reports were written.

The older spec-16 review at `task-001-r1-365bfc94b` is preserved historical
context. No acceptance or closure rows carry forward: this replacement-state
contract changes the owners and recovery representation. The validator compared
historical causes and reserved IDs 1–15; the retained replacement-state findings use
IDs 16–17 and 19–25; ID 18 is withdrawn and is not reassigned. Human-directed Revision 1 is acceptance scope, not a retroactive
reviewer finding. This is not an unsuccessful remediation round, so the
multi-round convergence assessment is inapplicable.

## Independent reviewers and validated coverage

The orchestrator established the subject, routed evidence, ran focused proof,
compared claims and wrote this verdict from the independent validation ledger.
It did not validate its own findings.

| Selected role | Coverage / independent report | Discovery result |
| --- | --- | --- |
| behavior-rev | Full original task and Revision 1 caller-to-result acceptance, negative flows and non-goals: [behavior](task-review-behavior.md) | FAIL proposals |
| invariant-rev | Live/pending/accepted state, attempts, publications, terminal selection and sibling consumers: [invariants](task-review-invariants.md) | FAIL proposals |
| repo-rev | All 60 changed paths and applicable Rust/SQL/proto/generated docs/TS rules, capability signatures and rustdoc: [standards](standards-review.md) | FAIL proposals |
| maintainer-rev | Whole-workflow reuse and new state/fence versus removed SQL owners; preparation and tests: [maintainer](maintainer-review.md) | FAIL proposals |
| system-rev | Direct/queued failures, outage/restart, partial delivery, capability availability and independent lifetimes: [system](system-review.md) | FAIL proposals |
| domain-data | State-head/RLS/batch transaction, WAL replay, exact reads, migration custody and page cuts: [data](domain-review-data.md) | PASS, empty proposal ledger |
| domain-concurrency | Fresh replacement: tickets/shared futures, cancellation/expiry/takeover, replay receipts and lanes: [concurrency](domain-review-concurrency-independent.md) | FAIL proposals |
| domain-security | Fresh replacement: exact grants, internal data capability, tenant/File custody, revocation and retained identity: [security](domain-review-security-independent.md) | FAIL proposals |
| followup-rev | Focused direct-request SQL authority and queue-to-fence correction readiness: [follow-up](followup-review.md) | Applicability resolved; material handoff decision unresolved |
| ponytail-rev | Fresh source/materiality/correction/coverage validation of every selected report: [validation](findings-validation.md) | SPEC_REVISION_REQUIRED |

All selected reports exist. Applicability and omissions were independently
validated: no changed UI/provider/OIDC or Python/Node lifetime boundary warrants
another specialty; generated TypeScript remains covered by standards, and
shared public execution semantics by behavior/security. No applicable specialty
is missing. The data report's empty ledger was validated with its stated
limits; its complete-state assessment does not override missing identity/error
facts established at actual consumers.

The first concurrency/security reviewers exchanged a conclusion after their
initial tracing. Their original reports are retained for provenance but were
excluded from selected independent evidence. Fresh reviewers received only the
original subject/scope, examined source independently and wrote the replacement
reports above. Neither their earlier agreement nor duplicated proposals count
as proof.

## Claim comparison and follow-up

[Claim comparison](claim-comparison.md) groups proposals by violated behavior,
not line or reviewer count. The validator consolidated duplicates, separated
new-write acceptance from active publication replay, narrowed canonical-state
requirements to approved retained facts, and rejected the redundant-tenant-filter
allegation as a mandatory rule imported from a sibling directory. No rejected
proposal creates optional work.

One focused follow-up was justified by a material authority/correction conflict:
permitted internal Bifrost fence SQL does not waive the no-request-owned direct
Postgres mutation boundary; cancellation-after-staging also needs a declared
queue-to-acceptance handoff. The fresh validator independently checked both,
identified a bounded existing-owner relocation for direct registration, and
retained the precise unresolved material decision for queue invalidation. No
extra discovery pass was needed merely for a unique proposal. Validation found
no new blocker requiring separate source validation.

## Reconciled acceptance

The full source/test evidence and every proposal disposition are in
[findings-validation.md](findings-validation.md).

| Task obligation | Reconciled result |
| --- | --- |
| Scenario 1: common direct/queued engine/state, in-memory dependencies, distinct direct IDs | Composition and existing engine concurrency PASS. Retained direct inspection and request-owned head registration FAIL: 16/25. Hosted A/B-to-C dependency observation remains incomplete proof. |
| Scenario 2: exact accepted compatible state, authoritative absence or explicit unavailability | Implemented head/exact-row recovery PASS. Required retained identities and all terminal facts are incomplete: 16/17. |
| Scenario 3: current writer, expected revision and rejection surviving WAL replay | Generic fence/takeover/replay PASS. Queue cancellation/current lease integration at acceptance FAIL and requires a spec decision: 20. |
| Scenario 4: settled work, spent interrupted attempts, original bounds and no-spend refusal | Inspected engine/adapter mechanics and focused regression PASS. Already selected terminal recovery at the final attempt remains FAIL: 19. |
| Scenario 5: publication identity/replay/accounting, exact File associations and mounts | Custody/budgets/content mechanics PASS. Covered stale replay bypasses active execution authority: 21. New-write invalidation is 20. |
| Scenario 6: final delivery, all statuses/full errors and retained public inspection | FAIL: immutable identity/reader resolution 16, terminal facts 17, final-attempt reconciliation 19; direct final delivery is intentionally asynchronous. Normal queued receipt and ordinary retry selection work. |
| Scenario 7: group/replay/binding/Operator continuity and independent lifetimes | Inspected existing owners and recorded suites support continuity. No new owner/cleanup/scheduler is required; shared terminal findings remain applicable. |
| Scenario 8: format safety and old custody | Fail-closed migration/runtime source and incompatible-format proof PASS. Legacy progressed-row migration refusal was not dynamically exercised. |
| R1.1–R1.3: short lock, coalescing, exact coverage, surviving waiters, own-frame ACK and execution lanes | Implemented mechanics and fresh focused tests PASS. Full JSON/Variant preparation still bypasses the bounded writer owner: 22. Hosted settlement-to-dependent proof and required byte/concurrent-run measurements remain incomplete. |
| Repository constraints, generated parity, isolation and non-goals | Existing engine/outbox/storage reuse is supported; no validated DRIFT finding. Mandatory changed-item rustdoc and named-column decoding FAIL: 23/24. Direct request-owned SQL also violates owner authority: 25. |

## Validated findings

All findings below are independently CONFIRMED or REVISED, with producer and
consumer tracing, materiality, correction burden and focused closure proof in
the validation ledger. That ledger defines the correction boundaries; the table
is a navigation summary, not an implementation checklist.

| ID | Classification / validated issue | Correction status |
| --- | --- | --- |
| FIND-TASK-001-16 | MISSING: retained identities do not drive source-deleted and direct execution inspection | Complete canonical metadata and existing shared readers; preserve current scoped authorization and queue coordination. |
| FIND-TASK-001-17 | MISSING: execution errors/cancellation/other non-verdict terminal facts lack canonical producers | Complete existing terminal representation/producers; queue-owned custody depends on decision 20. |
| FIND-TASK-001-19 | INCORRECT: queue exhaustion can suppress an already selected durable result | Existing queue-to-runner accepted-state reconciliation; conserve computation attempts/deadline and selected identity. |
| FIND-TASK-001-20 | INCORRECT: queue invalidation is absent from authoritative checkpoint acceptance | SPEC_REVISION_REQUIRED: declare the bounded queue-authority handoff/order and its failure semantics. |
| FIND-TASK-001-21 | REGRESSION: covered publication replay bypasses current execution authority | Existing lease confirmation at active publication success/replay, outside the lock; no new checkpoint for valid replay. |
| FIND-TASK-001-22 | VIOLATION: JSON/Variant checkpoint preparation bypasses bounded writer preparation | Extend existing bounded preparation, preserving immutable snapshot, coverage, byte accounting and retries. |
| FIND-TASK-001-23 | VIOLATION: mandatory changed-item rustdoc omissions | Bounded documentation of the verified new tests/helpers, batched with behavioral work. |
| FIND-TASK-001-24 | VIOLATION: state-head result schemas decode by ordinal | Existing SQLx named-column patterns; preserve guards/RLS/capabilities. No tenant-filter removal prescribed. |
| FIND-TASK-001-25 | VIOLATION: direct registration mutates PostgreSQL on the request path | Relocate existing registration through background Scribe/outbox handoff; preserve the claim/fence semantics. |

## Required specification decision

For finding 20, define the declared handoff between PostgreSQL queue authority
and generic Scribe checkpoint acceptance. Fix the authoritative order and
failure/recovery semantics for:

- explicit cancellation and principal-revocation acknowledgement;
- renewal and expiry visibility at acceptance;
- crash and ambiguous-commit reconciliation;
- canonical delivery of queue-owned terminal facts required by finding 17.

Preserve progress accepted first, reject progress when invalidation wins,
and preserve that decision through WAL replay. PostgreSQL remains the one
queue lease owner; Bifrost remains the generic checkpoint acceptance/recovery
owner. No payload mirror, second lease scheduler, private cross-tier transaction,
token epoch or new persistent bridge is selected by this review. A caller-side
recheck or notification does not by itself close the proven ordering gap.

This is the exact unresolved decision, not permission to redesign execution.
The existing material stop condition applies. No implementation remediation task
is issued: doing so would pretend this authority handoff is already
implementation-ready. Once approved authority fixes that boundary, package the
validated corrections with the existing owners and focused proofs from the
ledger; preserve local completed behavior.

## Verification and limits

Fresh sequential verification is recorded with commands and results in
[verification.md](verification.md):

- 25/25 server state/outbox unit tests PASS.
- 1/1 exact unchanged engine checkpoint restoration test PASS.
- 68/68 server durable-work/runtime/verifier integration tests PASS; isolated
  setup also passed both migration apply/idempotence checks.
- `mise run fmt:check` and immutable-range `git diff --check` PASS.

These results close the reported clean-rerun gaps but do not exercise all
retained failure paths. No new fault test/source was written during review.
Encoded bytes/concurrent-run throughput, legacy progressed-row migration
refusal, and the requested hosted A/B settlements to dependent C observation
remain execution-proof limits. Clippy/codegen/docs/boundary and Scribe/SQL/SDK
journey historical evidence is distinguished from fresh runs; integrated
language journeys and aggregate gates were not repeated here.

No source, specification, task evidence or architecture was changed; no commit,
merge, push or deployment occurred. The implementation is not accepted until
this material decision and the validated corrections/proof are resolved and a
later task review passes.
