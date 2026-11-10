# TASK-001 R3 closure discovery

**Result: FAIL.** Residual proposals preserve FIND-TASK-001-16, FIND-TASK-001-17 and FIND-TASK-001-19. This is discovery, not final independent validation.

## Subject and scope

Original base `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`; prior candidate `cd905f4cd929785bda25696326d58b4a4ae88dee`; candidate and inspected HEAD `d89356985566ab3492699682942438c25c8322fd`. Inspected actual R3 diff/source with cumulative original task acceptance. Only this report is written. No Cargo, runtime, migration, source, authority or Git mutation was performed.

Read task-review skill, subject, original task scenarios/acceptance, R3 obligations, current spec REQ-052–059/AC-046–051, durability supplement execution/recovery/boundary sections, root instructions, SQL AGENTS, spec-driven-development, maintainer/Rust guides and relevant Wyrd execution/inspection and Bifrost checkpoint authority. CodeGraph exploration preceded code searches; its multi-file query returned unrelated symbols, so precise source/diff inspection supplied missing navigation. No other R3 discovery report was read. Implementation evidence is not accepted as closure proof.

Carried evidence: `../task-001-bifrost-cd905f4cd/{verdict.md,task-review-behavior.md,findings-validation.md,verification.md}` and focused PG audit validation. The original independent acceptance review ran 25 state/outbox units, the pure engine restoration test and 68 server integration tests. These prove the prior candidate only. R3 changes reopen recovery/fences, terminal/error reconciliation, retained inspection, preparation and final-attempt recovery. Existing engine scheduling, upload/storage ownership and group/Operator machinery retain source-backed prior acceptance except where changed terminal behavior affects them. Current test bodies were inspected; no fresh run is claimed. Root separately reported the current 25 units passed and the server rerun ongoing; that is not independent runtime execution by this reviewer.

## Original acceptance matrix

| Obligation | Implementation evidence | Verification / carry-forward | Result |
| --- | --- | --- | --- |
| Scenario 1; REQ-052/053; common engine/state, distinct queue-free direct requests | service run_direct installs RunState Checkpoints; runner state/dispatch uses same adapter and existing engine; direct fresh identity inserts no queue row | Prior behavior rows 1–2; current direct_and_queued_use_bifrost_run_state | PASS composition |
| Scenario 2; exact checkpoint, authoritative absence/failure | state.rs accepted_revision probes deterministic generic batch fences; accepted reads precisely that revision, refuses invisible/malformed/incompatible state | Current lifecycle exact-state and format tests; prior unchanged generic proof; selected data/system reviewers cover changed source coverage | PASS inspected loading; exhausted consumer FAIL (19) |
| Scenario 3; REQ-057 as currently qualified; normal lease checks and batch conflicts | revision_batch/revision_fence, StateScribe confirmed, durable; generic commit classify and shard conflict refusal | Current batched recovery/lifecycle test bodies; generic WAL proof carried; data/invariant coverage remains necessary | PASS inspected policy; rare old-worker interval accepted |
| Scenario 4; original invocation bounds and settlement reuse | Checkpoints begin/forget/settle retain logical attempts; engine owns recovery; selected terminal bypasses dispatch only after target/state setup | Prior pure engine proof; invocations_resume_from_bifrost_with_original_bounds; final-attempt test | FAIL recovery branches (17/19) |
| Scenario 5; publication replay/accounting and exact File custody | accept_publication and receipt_once_accepted; retained objects; inspection file selects recorded descriptor through published_object | Prior publication/mount/budget proof; changed stale-first replay test; retained reader traced | PASS inspected behavior |
| Scenario 6; full errors and origin-neutral retained result/status/pages/content | retained authorizes immutable verifier UID without Cards/queue; get_execution uses exact selected result and refs; pages/select/file share reader | New cancellation/failure test bodies remove queue/Cards but exercise successful manual canonical writes, not failed real producers | FAIL 16/17/19 |
| Scenario 7; schedule/group/Operator/admission continuity | Existing queue, group comparison and dispatch owners retained; no new scheduler/cleanup owner | Prior behavior row 7 and recorded runtime/group/Operator suites | PASS unchanged owners; terminal gaps remain |
| Scenario 8; format/custody | Decode rejects unsupported state; worker format unchanged; additive retirement migration preserves historical migrations and old payload refusal | Prior static migration assessment; current incompatible-state test; dynamic legacy progressed-row refusal not established here | PASS static fail-closed contract; proof limitation |
| R1.1/R1.2; short lock, shared pending, exact coverage, waiter survival, lanes and own-frame ACK | Held/Pending/durable; acceptance outside lock; existing outbox frame delivery | Prior 25-unit verification and changed function/test inspection; invariant specialist covers shared changes | PASS inspected mechanics |
| R1.3; exact batch and dependent release | Existing engine awaits settlement; current integration restores accepted begin records and rejects conflicting revision | Pure engine proof; batched_checkpoints_recover_exactly_the_accepted_batch calls begin, not actual A/B settlements consumed by C | PASS source boundary; requested hosted A/B-to-C observation remains proof gap |
| Direct final-return exception | run_direct awaits consequential state.select, then stage_direct stages final inputs/result without receipt wait | service.rs:604–623,786–855; explicit user authority overrides contrary historical final-record ACK expectation | PASS final-record delivery exception; FIND-18 remains withdrawn |
| Non-goals/SQL ownership | Head module/query/fields/proto removed; additive retirement migration; accepted position uses generic batch metadata | Actual R3 diff and accepted_revision; selected data/standards reviewers supply complete boundary coverage | PASS inspected removal |

## Closure and user-work matrix

| Item | Evidence / disposition |
| --- | --- |
| FIND-16 | Exact refs and selected result reader remove live-Card/direct fallback; common pages/content reader correct. PARTIAL: pre-claim cancellation discards group/owner metadata, below. |
| FIND-17 | TerminalFailure/fail added. OPEN: producers bypass or best-effort the write and recovery ignores accepted failure, below. |
| FIND-19 | SQL now permits final-attempt reconciliation. OPEN: transient recovery/admission failure still becomes permanent exhaustion, below. |
| FIND-21 | CLOSED for original cause: receipt_once_accepted reconfirms lease outside lock even for covered replay; valid replay adds no revision. Current replay test exercises stale-first JSON then File. |
| FIND-22 | CLOSED for original inline preparation cause: Checkpoint typed snapshot/OnceLock reaches Part::Checkpoint encoding in existing spawn_blocking delivery. Stable weight is used for admission/release. Detailed accounting/lifecycle remains invariant/system scope. |
| FIND-23 | Added docs/deleted head-only items inspected; complete mandatory changed-item rustdoc closure belongs to selected standards review, not inferred from clippy. |
| FIND-18/20 | Withdrawn final-record ACK finding / superseded stronger cancellation fence. No remediation or reopening. |
| FIND-24/25; USER-PG-001 | Deletion closure source-supported: head/registration paths removed; retirement migration; ordinary queue and generic metadata retained. Data review must verify migration/replay/source coverage. |
| USER-DURABILITY-001 identity | Stable execution/revision batch identity feeds outbox digest and fences; invocation identities restored through checkpoint adapter. Provider integrations unchanged; no supported provider idempotency capability established, so possible repeat is accepted and no invented headers/protocol required. |
| USER-DURABILITY-001 formats/lost replies | Decode refuses incompatible/malformed data; frozen row and deterministic batch identity support retries. Lifecycle/format tests are relevant, but no claim that a combined lost-checkpoint-ACK plus process restart was dynamically exercised here. |
| USER-DURABILITY-001 retention/cost | No new policy/journal authorized. Generic fences have no removal path. Contextual byte/latency/retention observations require root evidence; static inspection and prior green tests do not supply measurements. |

## Residual proposals

### CLOSURE-16 — FIND-TASK-001-16: cancellation loses retained group identity

**Classification:** MISSING. **Obligation:** REQ-054/056, AC-048, scenario 6 preserve public metadata after queue/source removal.

At `service/inspection.rs:703–719`, cancellation fallback identity unconditionally sets group_id and owner_card_uid to None. Both are already available in status.state: `VerifierExecutionQueue::execution_status`, `verifier_executions.rs:3231–3238`, fills them from the admitted row. Cancel a grouped/binding execution before its first checkpoint: retain_cancellation installs incomplete identity. After queue removal, get_execution (`service.rs:238–256`) derives group/owner solely from it; those fields disappear and group-source lookup is skipped. This is a supported pre-claim cancellation path. The new cancellation test has no group metadata and cannot detect it.

**Minimum correction:** copy existing status group/owner into fallback identity, preserving an identity already accepted. No new lookup/store/type. **Focused proof:** cancel grouped execution before checkpoint, remove queue/source Cards, compare retained group ID/owner/source snapshot to pre-removal response under current grants.

### CLOSURE-17 — FIND-TASK-001-17: terminal failures remain an incomplete lifecycle

**Classification:** MISSING/INCORRECT. **Obligation:** REQ-054/056/058, AC-048, scenario 6 and supplement's explicit terminal result/error replay before steps.

Two related breaks remain at the same terminal-state boundary:

1. `runner::execute:397–404` returns target/SYSTEM errors before RunState. The new retained helper (`runner.rs:1371–1405`) logs failed canonical writes and still settles the queue. `cancel_principal` (`verifier_executions.rs:3174–3192`, reached from admission fence_principals:2049–2073) cancels only SQL. Explicit retain_cancellation (`inspection.rs:744–799`) also logs failed persistence after commit with no reconciliation owner. Once the queue row is absent, inspection answers prior nonterminal state as running or has no identity. Full terminal fact remains solely SQL custody on supported paths. This is canonical terminal durability, not stronger distributed cancellation fencing.
2. A successfully recorded TerminalFailure is ignored by recovery: execute:418–423 checks only snapshot.terminal, then dispatches engine. RunState::select (`state.rs:612–635`) ignores existing failure and can select success alongside it. If engine termination is retained but its SQL termination COMMIT fails, a nonfinal reclaim runs engine again and can replace the observed error with success; Retained::status (`inspection.rs:231–239`) prioritizes terminal outcome. The canonical failure does not stop execution or preserve the original decision.

The failure-readback test (`pg_durable_verifier_work.rs:1839–1920`) manually calls run.fail then independently terminates SQL; it does not exercise these producers or failed termination COMMIT recovery.

**Minimum correction boundary:** finish terminal production/reconciliation at existing runner/control/admission owners; replay accepted failure before dispatch, and make RunState outcome/failure selection exclusive. Preserve SQL coordination/revocation commit guarantees, direct final-record exception and accepted rare overlap. Use existing identity/status custody; add no head, archive/finalization service, lease system or universal exactly-once protocol. A reader fallback cannot retain facts after queue deletion or prevent provider reruns. If existing owners cannot cover a required producer, expose the precise authority conflict instead of silently inventing an exception.

**Focused proof:** real engine terminal error → accepted failure → injected termination COMMIT failure → reclaim; assert original error, zero engine entries and no success selection. Exercise pre-target refusal/principal cancellation and temporarily unavailable terminal persistence, then remove queue/Cards. Preserve sanitized errors; no stronger distributed-order test required.

### CLOSURE-19 — FIND-TASK-001-19: exhausted recovery settles without knowing accepted outcome

**Classification:** INCORRECT. **Obligation:** REQ-056/058, AC-049 and scenario 6 forbid replacing an unknown accepted result with exhaustion.

CLAIM_SQL correctly returns expired final-attempt work without spending another attempt. But `runner.rs:1036–1041` transforms every Retry/Defer on an exhausted claim into exhausted(). Before reading canonical state, execute:405–413 can defer because reserved result staging is full. state_failure:1229–1248 defers unavailable or incompatible state. The transformation replaces either with permanent lease_expired and settles SQL. A durable selected result whose completion COMMIT failed is therefore abandoned whenever recovery encounters ordinary transient capacity/source failure; errored queue status prevents another delivery claim. Incompatible state also becomes terminal despite ordinary recovery deliberately preserving it for a compatible worker. The new final-attempt test (`pg_verification_runtime.rs:2040–2140`) has available state/free staging and misses this branch.

**Minimum correction:** exhaust only after an authoritative compatible read proves there is no selected terminal outcome/error. Preserve lookup/delivery retry without spending computation attempts, using existing lease/queue release or expiry policy rather than converting unknown state to a terminal fact. No engine dispatch, replenished budgets/deadline, SQL state mirror or extra scheduler. **Focused proof:** extend failed-final-settlement recovery with unavailable state and separately full staging; no permanent exhaustion while blocked, then same result ID/completed status/unchanged attempts/zero engine entries after recovery. Incompatible accepted state must retain custody.

## Convergence and limits

These are incomplete closure of existing findings, not expanded hardening requirements. New fields and happy reconciliation branches left early exits, terminal COMMIT retries and control/admission producers on old paths. Next bounded scope: these terminal producers/consumers and grouped pre-claim cancellation metadata with focused fault proof. Do not restart whole-repository discovery or revive rejected SQL ownership/fencing.

No fresh runtime verification or measurements were performed. Selected standards/data/security/invariant/system/maintainer reports and fresh independent final validation remain required before final task acceptance.
