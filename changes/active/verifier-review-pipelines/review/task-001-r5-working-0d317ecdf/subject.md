# R5 focused task review

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Committed HEAD/prior R4 candidate: `0d317ecdf2d28d2e77b6f394ebe0f443d256d580`.
R3 restart baseline: `d89356985566ab3492699682942438c25c8322fd`.
Original base: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`.

The user explicitly submitted the uncommitted R5 working tree for review.
Accordingly, candidate identity is HEAD plus the working-source and authority
hashes in `subject-sha256.txt` and packet hashes in `packet-sha256.txt`, rather
than a fabricated candidate commit. This is the user-requested working-tree
exception to the skill's committed-subject convention. No commit, staging,
source archive or patch is created. A changed pinned file blocks this review.
Review reports are excluded from the packet pin. Read `git diff HEAD -- <path>`
for the remediation delta, and `git diff d89356985 -- <path>` for R4/R5 context;
the original base remains available for unchanged Task 001 acceptance context.

Authority: AGENTS.md; SQL nested AGENTS.md; changes/spec-driven-development.md;
changes/implementation-execution.md; agent_docs/maintainer-style.md; TESTING.md;
applicable architecture sections on verification state, terminal delivery,
Scribe ACK/dedup, security revocation and recovery. Packet paths below are
relative to `changes/active/verifier-review-pipelines/`:

- spec.md revision 21's approved terminal amendment and unchanged revision 18;
  unrelated draft wait/approval choices remain excluded;
- execution-state-and-durability.md approved revision 3;
- tasks/TASK-001-bifrost-execution-state.md (original outcome context);
- review/task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md;
- review/task-001-r4-0d317ecdf-working/TASK-001-R5-single-terminal-settlement.md.

## Scope and carry-forward

This is remediation closure, not another comprehensive Task 001 discovery pass.
Check the single terminal recording/ACK/settlement workflow, acknowledged failure
retirement, actual producer custody, selection/result delivery, exhausted
reconciliation, matching delivery fences, rollback/migration preservation and
affected regressions. Reconcile the explicit capacity-proof obligation and the
meaning of selected-result replay/dedup. Do not assume either is a blocker or
acceptable merely from implementation claims.

Carry forward source-backed unchanged acceptance from
`review/task-001-r3-d89356985/verdict.md`, `findings-validation.md` and the
referenced concurrency/security/data reports: accepted source-aware recovery,
publication/File custody and permissions, generic batch fence/replay semantics,
short memory locks/shared waiters, original bounds, direct final fast return,
removed SQL heads and unchanged provider/platform boundaries. Recheck affected
dependencies; prior acceptance of R4 terminal orchestration is not carried.
The earlier working R4 maintainer/verification reports are historical context,
not an independent completed R4 verdict or evidence for the R5 candidate.
Open R4 obligations 16/17/19/22/23/26 and original hosted-dependent proof require
closure against current source/test evidence.

Available verification evidence: R5's appended exact RED/GREEN observations and
module/gate evidence; earlier root focused runs are historical because source
changed afterwards. Discovery reviewers must inspect test assertions and static
authority rather than receive the implementor's chat summary as conclusions.
No reviewer runs Cargo or changes production/test/authority/task files. The
orchestrator owns any justified focused execution, avoiding concurrent builds.

## Common navigation map

- server verification/runner.rs: execute, record, conclude, result, deliver,
  attempt, deliver_owed, settle, settle_delivery, Frozen, Work, Offer, custody;
- verification/state.rs: accepted/recover/at, select/fail/decide/durable,
  StateScribe, DECISION_TIMEOUT; shared scribe_outbox batching/ACK/charge;
- components/verification/service/inspection.rs: cancel and retained reader;
  auth/revoke.rs plus admission/scheduler/group producers;
- SQL queries/verifier_executions.rs: TerminalDelivery/PendingDelivery,
  terminate/retry/cancel/cancel_principal/fence_principals/refuse_group,
  claim_deliveries/delivered/redeliver/SETTLE_DELIVERY_SQL and tenant scans;
  immutable 20261108000000 terminal-delivery migration;
- verification/results.rs builder and existing completion/group/Operator owners;
- server pg_verification_runtime P1/P2/P3/P5/P6; pg_durable_verifier_work P4/T3/T6;
  pg_verification_groups support and SQL pg_verifier_executions fence proof;
  scribe_outbox T5 and protobuf reservation static closure.

The map is a starting point; each reviewer traces its boundary end to end.

## Specialist applicability and assignments

| Material contract | Producers/consumers and reason | Fresh reviewer/report |
|---|---|---|
| Shared decision/ACK/settlement lifecycle and exhausted reconciliation | execute/record/conclude and attempt/delivery settlers materially consolidate shared state paths; explicit proof gap is in this boundary | invariant-rev: task-review-invariants.md |
| One workflow and changed ownership/mandatory Rust documentation | Removed state/request recording plus Frozen/Offer/Work and existing builder reuse; substantial ownership change needs focused maintainer review | maintainer-rev: maintainer-review.md |
| Persistent tenant queue coordination and claim fencing | All terminal SQL writers, newly fenced delivery claims/retirement, frozen identity and unchanged migration; persistent-data/concurrency boundary materially changed | domain-rev data: domain-review-data.md |
| Restart/outage/late ACK and result replay effects | Common operation changes recovery across runtime, Bifrost and SQL, with cancel/revoke stopping computation independently; check actual consumers | system-rev: system-review.md |
| Cross-tier/API/provider/UI/native lifetimes | No changed SDK, wire or platform boundary; prior R3 standards/security acceptance carries where unaffected; touched Rust/SQL gates covered above | Omit broad repo/extra behavior/security specialists; targeted gaps go to relevant owner above |

Remediation default independent validator: a fresh ponytail-rev, assigned only
after discovery/any justified focused follow-up, writes findings-validation.md.
It checks closure, selection/coverage, materiality and every proposed correction.
A validator-discovered blocker requires separate focused source validation.
No finding quota, optional refactor, unrelated hardening or new review round is
authorized. Scope must stay bounded even if acceptance remains incomplete.
