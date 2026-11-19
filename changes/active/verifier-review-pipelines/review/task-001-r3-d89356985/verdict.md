# TASK-001 and R3 acceptance verdict

**SPEC_REVISION_REQUIRED.** TASK-001 and TASK-001-R3 do not pass. Six material causes remain independently validated: FIND-TASK-001-16, FIND-TASK-001-17, FIND-TASK-001-19, FIND-TASK-001-22, FIND-TASK-001-23 and FIND-TASK-001-26.

The design conflict is narrow: cancellation/revocation/refusal can commit a terminal queue decision and stop its workers before Bifrost records the decision. A crash before staging loses that canonical history. The contract requires durable canonical terminal history while prohibiting a PostgreSQL event projection backlog, and no current declared owner bridges that interval. This review does not invent that owner or authorize another execution-state store.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
- Original base: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`.
- Prior candidate: `cd905f4cd929785bda25696326d58b4a4ae88dee`.
- Candidate: `d89356985566ab3492699682942438c25c8322fd`.
- Remediation: `git diff cd905f4cd d89356985`; cumulative context: `git diff 0e8a31b14...d89356985`.
- Original task: [TASK-001](../../tasks/TASK-001-bifrost-execution-state.md), including Revision 1.
- Remediation reviewed: [TASK-001-R3](../task-001-postgres-cd905f4cd/TASK-001-R3-complete-execution-state-remediation.md).
- Authority: approved revision18 baseline plus explicit user corrections, [durability supplement](../../execution-state-and-durability.md), narrow architecture and repository rules. Current [spec](../../spec.md) revision20 is draft for new wait/approval work; those TASK007 proposals are excluded.

This is cumulative TASK001 acceptance with remediation closure, affected callers/invariants/tests and targeted prior gaps. It is not a new repository-wide audit. Prior acceptance/validation in [replacement-state review](../task-001-bifrost-cd905f4cd/verdict.md) and [PostgreSQL audit](../task-001-postgres-cd905f4cd/verdict.md) carries forward only where source-backed and unaffected. SQL-head necessity/ownership acceptance is not carried forward. Candidate and pinned authority hashes remained unchanged; source, tasks, architecture and historical evidence were not edited. Only this review directory was written.

## Independent coverage and results

The applicability record and common navigation map were written in [subject.md](subject.md) before discovery. Every selected report exists. Reviewers were fresh, independent and assigned one role each; discovery did not receive other discovery conclusions or an intended verdict. The orchestrator grouped claims; a fresh validator independently inspected source and corrections.

| Role | Report | Independent result / coverage |
|---|---|---|
| Remediation closure discovery | [task-review-closure.md](task-review-closure.md) | FAIL proposals; original acceptance, all R3 closure, affected paths and prior gaps |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL proposals; shared checkpoint/terminal state, byte accounting and concurrency mechanics |
| Repository standards | [standards-review.md](standards-review.md) | FAIL proposals; all42 changed paths, Rust/SQL/proto/generated docs/TS, nested rules and mandatory rustdoc |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL proposal; complete owner reuse, bounded preparation, charge invariant |
| System resilience | [system-review.md](system-review.md) | FAIL proposals; outages, terminal recovery, exhausted reconciliation, shared capability lifetimes |
| Persistent data | [domain-review-data.md](domain-review-data.md) | Scoped PASS; head removal, generic fences, exact accepted-prefix recovery, replay and custody |
| Security/RBAC/tenancy | [domain-review-security.md](domain-review-security.md) | FAIL proposal; retained grants/identity/File boundary and principal cancellation |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED; terminal commit-to-canonical custody correction boundary requires a material decision |
| Final independent validator | [findings-validation.md](findings-validation.md) | SPEC_REVISION_REQUIRED; six retained deduplicated causes; applicability/coverage and correction burden validated |

No additional concurrency reviewer was required: previous independent ticket/shared-future/lane/waiter evidence is carried, current invariant review rechecks changed preparation/receipt paths, and current data/security/system coverage handles persistence and normal authority. The validator independently checked this omission. There is no changed provider/OIDC/UI/native-lifetime boundary; standards covers generated projections and closure covers the required provider-support limitation. The focused follow-up was added only after validation exposed a material correction-boundary uncertainty; it did not restart discovery or decide by reviewer votes. No validator-only new blocker needed a separate validation pass.

## Validated findings and smallest correction boundaries

The complete ledger, exact source evidence, producer/sibling/consumer traces, materiality, correction burden and focused proofs are in [independent validation](findings-validation.md#retained-deduplicated-ledger), incorporated here. Rejected proposals do not create optional work.

| Finding | Disposition / classification | Plain finding | Minimum correction within approved behavior |
|---|---|---|---|
| FIND-TASK-001-16 | CONFIRMED / MISSING | Cancellation before the first checkpoint drops admitted group and owner metadata. | Propagate the already available queue status fields into the existing fallback identity; retain accepted identity precedence and exact canonical group-input selection. |
| FIND-TASK-001-17 | REVISED / MISSING + INCORRECT | Some terminal decisions remain SQL-only or best effort, and recovery can rerun an already recorded terminal failure. | Locally honor accepted failure before dispatch/exhaustion and enforce success/failure exclusivity at RunState. Complete actual terminal producers; their queue-commit-to-Bifrost crash custody requires the bounded specification decision below. |
| FIND-TASK-001-19 | CONFIRMED / INCORRECT | Temporary recovery/capacity failure permanently exhausts an already completed final attempt. | Preserve exhausted non-computing reconciliation through existing no-settlement/lease-expiry reclaim; do not refund/spend attempts or authorize more computation. Exhaust only with authoritative readable unfinished state. |
| FIND-TASK-001-22 | CONFIRMED / REGRESSION | Checkpoint byte charges omit variable settlement and outcome payloads. | Conservatively account for complete retained variable values and prepared representation with stable admission/release charge; keep bounded deferred preparation and existing outbox. |
| FIND-TASK-001-23 | CONFIRMED / VIOLATION | Three surviving changed Rust items still miss required Errors/Panics documentation. | Local rustdoc on Checkpoint::prepare, test execution_state and Leased::writer; batch with behavioral corrections. |
| FIND-TASK-001-26 | CONFIRMED / VIOLATION | Removed protobuf tag10 and name state remain reusable. | Reserve only that tag/name in the existing message, preserving old reservations. No compatibility implementation or runtime feature. |

INVAR-R3-26 is merged into existing FIND17's terminal lifecycle; it is not separately numbered. Stable new FIND26 applies only to the protobuf retirement cause. No universal exactly-once provider capability, remote replication, retention policy, new journal, service, scheduler, execution SQL head or stronger stale-worker fencing is prescribed.

## Prior closure and reconciled acceptance

| Obligation / prior work | Reconciled result and evidence |
|---|---|
| USER-PG-001, prior24/25 | PASS by removal: rejected head/query/registration/advance/wire/replay authority gone; ordered retirement preserves applied migrations. Generic existing batch/catalog metadata remains legitimate. Data/standards reports and validation establish this boundary. |
| Exact accepted revision/absence/unavailability and replay | PASS source boundary: deterministic generic fence prefix followed by exact compatible row lookup; missing fenced row refuses rather than using older/fresh state. Data/system reports record dynamic proof limits. |
| Direct/queued shared state; successful direct final-record fast return | PASS composition and user exception; direct remains queue-free, final records asynchronous. Consequential selection/checkpoint boundaries remain separate. Withdrawn18 is not reinstated. |
| Short lock, one pending revision, coalescing/shared waiters, frame receipts/lanes | PASS mechanics through prior independent evidence and current unit/source checks; byte-accounting preservation remains FAIL22. |
| Invocation bounds and terminal recovery | Existing settled invocation recovery remains source-backed; terminal-failure replay and exhausted outage handling FAIL17/19. |
| Publication replay, exact File custody, ordinary authority checks | PASS inspected boundary;21 CLOSED by common replay lease confirmation outside lock. Valid replay adds no revision. |
| Canonical lifecycle and retained metadata/inspection | PARTIAL/FAIL16/17/19; common reader/exact refs/current grants fixed, but actual producer and metadata gaps remain. |
| Schedules/groups/Operators and independent lifetimes | Prior unchanged source-backed evidence carried; shared terminal defects remain applicable. No whole-workflow reimplementation required. |
| Full snapshot preparation placement | PASS: existing bounded writer and OnceLock; preserved memory admission FAIL22. |
| Format/migration custody | PASS static refusal/preservation and current format tests; migration setup proves fresh apply/idempotence, not populated legacy refusal or populated-head upgrade. |
| Required hosted concurrent A/B settlements consumed by C | INCOMPLETE observation: current batched recovery test records begins, not a hosted dependent consuming both settlements. Carry original focused proof obligation, no new runtime finding or performance threshold. |
| USER-DURABILITY-001 | Logical identity/unsupported-provider possible-repeat limitation retained; format and retry/restart source evidence retained. Lost-ACK transport and real checkpoint restart are separate proofs, not a combined executed checkpoint-lost-ACK restart. Cost/retention observations are recorded limitations, not an accounting correctness certificate or new policy. |
| Mandatory documentation/schema retirement | FAIL23/26. Green compilation does not waive explicit static obligations. |
| Historical18/20 |18 WITHDRAWN;20 stronger ordering SUPERSEDED by accepted rare old-worker interval. No remediation for them. |

IDs1–15 remain historical reserved identities from earlier pre-replacement work; no obsolete architecture proof is revived. The complete reconciled original matrix and per-finding closure map are preserved in findings-validation.md.

## Exact specification decision required

The unresolved decision is: **which durable owner carries an atomically committed queue-owned terminal decision until Bifrost accepts it, and when is that custody discharged?**

Current actual sequence is queue cancellation/revocation/refusal COMMIT, then optional/best-effort canonical staging. Death between them leaves nothing in Bifrost to replay and no runnable lease. Revocation must remain immediate and atomic with queue cancellation. Scribe only retries already staged/accepted work; SubmissionSink handles input-ACK-to-admission, Completions handles the inverse already-canonical-result-to-queue completion, and existing claims exclude terminal rows. They do not currently own this gap.

A terminal-row scan in the existing runner would project SQL's sole historical terminal decision into Bifrost after computation has ended. That is the event projection backlog excluded by `architecture/bifrost-design.md:469–474` and the original task; a bounded eligibility/ACK/discharge scheme would declare a new cross-owner durable handoff under SQL AGENTS. Calling it queue coordination or avoiding a new table does not make that decision already approved. Prewriting final cancellation before COMMIT can record rolled-back cancellation; waiting for Bifrost to retire security authority changes availability. A tentative record/commit proof is also new machinery.

Authority must explicitly select either a narrowly defined terminal-only committed handoff in existing queue/runner owners with durable discoverability/retry/discharge/identity custody, or qualify the full-retention promise for these queue-owned decisions as best effort. This review approves neither and does not revive the superseded rare-worker ordering concern. Local fixes alone cannot close full FIND17. Therefore no mechanical R4 remediation task is issued against a missing decision. After that bounded approved amendment, one consolidated remediation can package all six validated causes and the existing hosted-dependency proof gap.

## Verification, limitations and convergence

Independent executions on the candidate, detailed in [verification.md](verification.md):

- 25/25 state/outbox library tests passed.
- 71/71 server durable-work/runtime integration tests passed.
- Wyrd and Vala migration apply/idempotence each1/1 passed.
- `git diff --check` passed; candidate/authority hash verification passed.

The exact current server selector registers71 tests; the summary's76 count was not independently reproduced. Existing green cases do not exercise the retained failed-settlement/cancellation crash/source outage/large settlement charge/group-fallback gaps. No retrospective RED was manufactured. Scribe/SQL batch suites, full language journeys, clippy/codegen/docs and performance measurements were not independently repeated here; recorded R3 evidence and previous source-backed proof retain their actual limits. Broad aggregate checks are not requested for this bounded review.

Convergence: R3 removes the rejected SQL authority and materially improves exact recovery, publication replay and bounded preparation.16/17/19/23 remain incomplete closure; ignored accepted failure and undercharged snapshots are fix-induced regressions;26 is a missed explicit protocol-retirement obligation. These are consolidated causes, not expanding expectations or a request to rediscover every caller. The next bounded pass must resolve the terminal custody contract, exercise focused failure proof and affected siblings, then carry unchanged accepted evidence. No implementation, staging, commit, push or deployment was performed.

## Subsequent approved decision — review verdict preserved

After this review, the user explicitly approved one uniform queued terminal
mechanism: canonical Bifrost record → existing WAL/batch-commit ACK → queue
delivery settlement. Cancellation/revocation stops computation immediately and
the existing queue retains durable pending terminal delivery until ACK. The
previous terminal-custody specification decision is therefore resolved by
[approved supplement3](../../execution-state-and-durability.md#approved-terminal-settlement--revision-3).
The verdict above remains historical evidence for d89356985 under its reviewed
authority, not a new PASS or a rewritten independent finding. All six causes
and original missing hosted observation are consolidated in
[TASK-001-R4](TASK-001-R4-uniform-terminal-settlement.md). No code has been changed
or accepted by this approval; R4 requires implementation and closure review.
