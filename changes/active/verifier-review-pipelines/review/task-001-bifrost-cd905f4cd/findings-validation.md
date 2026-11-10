# Independent findings validation — TASK-001

## Current authority addendum — 2026-11-06

The fresh [PostgreSQL audit validation](../task-001-postgres-cd905f4cd/findings-validation.md)
and [remediation task](../task-001-postgres-cd905f4cd/TASK-001-R3-complete-execution-state-remediation.md)
supersede this report's former acceptance of SQL head metadata, finding20's
new-handoff approval stop, and finding25's relocation-only correction. The user
rejects that SQL owner and accepts the specified rare delayed-worker risk.
USER-PG-001 is separately directed removal, not retroactive review credit.
The consolidated R3 task covers open findings16/17/19/21–23 and deletion proof
for24/25;18 remains withdrawn. Current disposition is FIX_REQUIRED, with
no new SQL protocol or approval gate prescribed. The original source reasoning
below is historical where it conflicts with this addendum; no implementation
closure is claimed.

## Original recommendation and validation — superseded where noted above

Recommendation: **SPEC_REVISION_REQUIRED**. The candidate has material implementation gaps, but its queue-to-Bifrost authority integration also requires a declared cross-owner concurrency/persistent-authority decision before that correction can be packaged as implementation work. This is not an invitation to replace either owner.

## User-directed correction — 2026-11-06

The user confirmed direct verification's intended fast-return contract: it does
not wait for Bifrost acknowledgement of final input/output/result writes. Former
FIND-TASK-001-18 is withdrawn, its correction and closure proof are removed, and
its ID remains reserved. Nine findings remain. This correction supersedes the
original validation of INV-1/SYS-1; it is a user-authority correction, not another
independent validation pass. Historical source observations and discovery reports
remain unchanged.

## Subject and independence

Validated the complete immutable range `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb...cd905f4cd929785bda25696326d58b4a4ae88dee`, against approved specification revision 18, durability supplement revision 2, and TASK-001 scenarios 1–8 plus Revision 1. HEAD remains the candidate. This is a fresh independent validator, not a discovery reviewer or the orchestrator. Only this report was written; no source, authority, test, task, or other report was changed, and no Cargo command was run.

Read every selected report: behavior, invariants, standards, maintainer, system, persistent-data, replacement independent concurrency, replacement independent security, and focused follow-up; also subject, claim comparison and final verification. The superseded concurrency/security reports were excluded. Historical spec-16 findings were consulted only for identity/cause comparison, with no acceptance carried forward. IDs 1–15 remain reserved. The findings below concern replacement-state causes/newly introduced omissions rather than genuinely retained instances of those historical causes, so new numbering starts at 16. In particular, historical causes were not automatically relabeled as replacement-state findings; the direct-final-delivery proposal was subsequently withdrawn under the user correction above.

CodeGraph was consulted before code navigation. Independently read the complete new state contract and adapter methods, relevant direct/status/inspection methods, runner execution/state/delivery/settlement methods, publication backend methods, head SQL and acceptance caller, queue claim/retry/termination/cancellation/release/renewal paths, affected readers and proof bodies. Authority included root rules, relevant nested rules, Rust/maintainer guidance, spec-driven workflow, and the owning verification, Bifrost acceptance and tenant/security passages. Findings below identify actual source paths, not reviewer agreement.

Authority SHA-256 at validation:

| Input | SHA-256 |
| --- | --- |
| `AGENTS.md` | `c9b27d281657129247d7ecdf90a50127cac89885ff902a7c0d9b7ae20da56a4b` |
| `architecture/wyrd-design.md` | `1887a2fe659a2a652b60998c7fa05c845e26f4976c0ae051eee138a79a896fc8` |
| `architecture/bifrost-design.md` | `f876727f4f0c0a59bd53729cbaaeb87908a5fe7cba180bcd1f281fb361d3f094` |
| `architecture/wyrd-security-posture.md` | `01d3475bdff78814ca33785bd4ecfd94db907e51ba1356e7095274ad405244ea` |
| `architecture/operations/reliability-and-recovery.md` | `6f138d2130bcff7da437e04b88ef0361091866a6da0ea2e908079d7baef01aca` |
| `spec.md` | `830a24787671f190ba50ccecf16d72b4d76d24df561fbb5313fa46d2cc0d7d74` |
| `execution-state-and-durability.md` | `c03ebbc755caf9ef6fba23134ddc6b3ff1d08eeb2da0a9d7af2a0593ac3a6d5b` |
| `tasks/TASK-001-bifrost-execution-state.md` | `87fde0bd7a8e8adf49467bd57cebe9a8ecffddfab36085fe96a37538a6d07649` |

Packet paths in the last three rows are relative to `changes/active/verifier-review-pipelines/`.

## Proposal dispositions

| Proposal | Disposition | Independent resolution |
| --- | --- | --- |
| BEH-1 | REVISED | FIND-TASK-001-16. Missing recorded identity is proven. Distinguish canonical recovery content from legitimate external queue coordination/input custody; do not require serializing runtime capabilities or duplicating admission. |
| BEH-2 | REVISED | FIND-TASK-001-17. Non-verdict terminal facts are missing. Cancellation/revocation/expiry delivery cannot be prescribed as a stale worker checkpoint; its authority integration depends on finding 20. |
| BEH-3 | CONFIRMED | FIND-TASK-001-22, one bounded checkpoint-preparation correction. |
| INV-1 | REJECTED following user correction | Direct final delivery intentionally does not await Bifrost acknowledgement; former finding 18 is withdrawn. |
| INV-2 | REVISED | Split into findings 16 and 17: distinct immutable-identity and terminal-fact producers/consumers. No extra third finding for the same canonical-state omission. |
| INV-3 | REVISED | FIND-TASK-001-19. Final-attempt loss of already durable result/queue completion proves it. Do not resolve ambiguous undelivered-result deadline policy by silently extending time. |
| STD-001 | CONFIRMED | FIND-TASK-001-23, bounded changed-item rustdoc gate. |
| STD-002 | REVISED | FIND-TASK-001-24 retains ordinal decoding under the applicable global Rust mapping rule. The separate redundant-tenant-predicate allegation is REJECTED as a mandatory gate here: the cited nested `wyrd-sql/AGENTS.md` does not automatically govern the sibling `vala-sql` tree. No tenant leak was proved and no predicate deletion is required by this finding. |
| STD-003 | REVISED | FIND-TASK-001-25. Actual request-owned mutation violates the execution owner; relocating existing direct registration to background Scribe preserves its semantics. |
| MNT-001 | CONFIRMED | FIND-TASK-001-22; duplicates BEH-3, not additional drift machinery. |
| MNT-002 | CONFIRMED | FIND-TASK-001-23; same format-test omission as STD-001. |
| SYS-1 | REJECTED following user correction | Same withdrawn direct final-acknowledgement proposal; no remediation. |
| SYS-2 | REVISED | FIND-TASK-001-20; defect confirmed, local-remediation readiness rejected. |
| CONC-I-1 | REVISED | FIND-TASK-001-20; same staged-write/current-authority cause. |
| CONC-I-2 | CONFIRMED | FIND-TASK-001-21; covered receipt replay is distinct from accepting new progress. |
| SEC-I-01 | REVISED | FIND-TASK-001-20; same acceptance cause, including principal cancellation. |
| SEC-I-02 | CONFIRMED | FIND-TASK-001-16; direct inspection queue prerequisite and source-deletion read failures share recorded-metadata resolution. |
| Persistent-data empty ledger | VALIDATED WITH QUALIFICATION | No additional data finding is manufactured. Its fence/replay/exact-revision/migration/page conclusions have source support. Its complete-state PASS cannot cover missing exact identities or non-verdict errors; findings 16/17 override that row. Its explicit exclusion of cancellation/lease-gap certification leaves that coverage to the selected specialists and finding 20. |
| Follow-up STD-003 | REVISED | Authority conclusion confirmed; minimal relocation is finding 25, preserving existing claim semantics rather than inventing initialize-on-advance. |
| Follow-up SYS-2 | CONFIRMED | Cancellation path and absent declared handoff independently confirmed; the required decision is specified under finding 20. Expiry is narrowed to supported lease/deadline orderings; the original deadline predicate is present. |

## Validated ledger

All locations below are repository-relative. Each correction was checked against deletion/reuse before accepting new machinery. No rejected proposal produces optional remediation work.

### FIND-TASK-001-16 — REVISED / MISSING — retained execution identity does not drive historical or direct inspection

Sources: BEH-1, identity portion of INV-2, SEC-I-02. Obligations: REQ-052/054/056, AC-048, task scenarios 1/2/6, and `architecture/wyrd-design.md:1924–1929` dedicated reads independent of source Cards/leftover queue rows.

Locations: `crates/wyrd-spec/src/verification/state.rs:30–43`; server `verification/state.rs::StateRow/StateScribe::write`; `components/verification/service.rs::get_execution/direct_execution/card`; `service/inspection.rs::authorized/accepted`; `verification/inputs.rs::state/direct/result`.

Source trace: the checkpoint payload retains invocations, publications and a result selection, but no exact Verifier/subject references. Row dimensions retain UIDs and attribution; they are not the missing kind/name/version identity. Main queued GET still fetches both Cards through `Self::card`; `cards/get.rs::SELECT_BY_UID` excludes deleted rows. With either absent, it returns status without result. Direct GET also fetches both and returns `None`, becoming execution-not-found. Separately, `inspection::accepted` first calls `authorized`, whose unconditional `queue.execution_status(...).ok_or_else(...)` rejects a direct execution before reading its accepted state. Invocation pages and all publication/content/download siblings share that gate. Direct executions intentionally have no queue row.

Reachable consequence/materiality: ordinary completed verify/submit followed by supported subject deletion hides retained result identity/body; a successful nonempty direct execution cannot inspect its accepted invocation history at all. These are explicit supported acceptance flows, not speculative retention hardening. A reader-side not-found fallback cannot reconstruct facts that were never retained.

Minimal correction/burden: use the existing canonical execution input/state representation to retain the exact immutable execution references and required pinned reconstruction facts once, then use `InputResolver`, accepted-head reads and the existing narrow SYSTEM `records` capability for status and inspection identity resolution. Authorize/audit current caller grants against the recorded exact Verifier UID before exposing content. Queue rows remain authoritative for live coordination and dispatch projection, but do not define retained evidence existence. Preserve ordinary Card deletion/read behavior, exact File custody and existing result-table meanings. No new tombstones, source registry, archive, queue rows for direct work or general permission grant. Completing canonical data and adapting its shared readers is necessary; per-reader fallback patches are insufficient.

External accepted input custody and queue claims/renewal are explicitly allowed by the supplement. This finding does not require copying all their coordination fields into every payload merely because the state is called complete. It requires retained pinned reconstruction facts and immutable read identities, which the actual readers demonstrably lack. Exact declarations/evidence/bounds must be reconstructable under the approved canonical format, without changing queue ownership or serializing runtime handles.

Focused proof: extend existing direct/queued execution inspection proof with a nonempty plan and accepted publication; read invocation/publication/status before and after subject deletion and without a direct queue row. Preserve exact references/result ID/outcome and current scoped/cross-tenant denial. Keep required-source unavailability distinct from authoritative absence. This is one shared metadata/read correction, not a new harness.

### FIND-TASK-001-17 — REVISED / MISSING — non-verdict terminal decisions/errors have no canonical producer

Sources: BEH-2 and terminal-fact portion of INV-2. Obligations: REQ-054/056, AC-048, task scenario 6; supplement state table and terminal recovery; owning canonical verification data requires complete sanitized errors for every outcome.

Locations: spec `verification/state.rs::TerminalSelection`; server `verification/runner.rs::execute/settle/deliver`, direct `service.rs::run_direct`, inspection `cancel`; SQL `verifier_executions.rs::{terminate,retry,cancel,cancel_principal}`, claim exhaustion.

Source trace: `TerminalSelection` requires `VerificationOutcome` and a result ID. It cannot describe errored/timed-out/cancelled execution status. Runner `execute` converts `EngineOutcome::Terminal` straight into `Transition::Terminate`; `settle` writes queue status/error only. Failure before state creation, final retry exhaustion, and direct engine errors also bypass terminal state selection. Explicit cancellation and principal revocation update only queue error/status. The status reader gets the full error from `ExecutionStatus` decoded from that row. Removing old event projection leaves no sibling canonical error producer.

Reachable consequence/materiality: ordinary invalid evidence, unavailable capability, timeout or cancellation ends execution while its canonical checkpoint remains running or absent. Bifrost cannot reproduce the promised terminal error independently of the coordination payload. A failed *verdict* test exercises a completed `VerificationOutcome`, not this execution-error boundary.

Minimal correction/burden: complete the existing versioned canonical terminal representation and wire existing terminal producers to retain their existing sanitized typed status/error. Reuse result/error projection and the common state reader; retain queue control transitions and existing public error meanings. No new result format, independent finalizer, payload mirror or Operator lifecycle changes. Engine-owned terminal failures can use the existing state writer while authority permits. Queue-owned cancellation/revocation/expiry and pre-engine termination must reach canonical custody through an authorized owner handoff, not by allowing a cancelled/expired worker to write. That portion depends on the material authority decision in finding 20; this report does not prescribe an unapproved persistent bridge under cover of a type extension.

Focused proof: existing real-server negative-flow seams must retain/reconstruct invalid-evidence and timed-out errors and repeated cancellation/principal-revocation decisions from canonical data, including no previously accepted checkpoint, while preserving completed failed/inconclusive verdict semantics. Assert full sanitized errors, exact identity and retryable required-source failure. Choose concrete handoff failure/acknowledgement proof only after finding 20's decision is approved.

### FIND-TASK-001-19 — REVISED / INCORRECT — queue exhaustion can suppress an already selected durable result

Source: INV-3. Obligations: REQ-055/058, DUR-007, scenario 6, supplement terminal-result/queue-reply-loss recovery.

Locations: server `verification/runner.rs::execute/attempt/deliver/settle`; SQL `verifier_executions.rs::CLAIM_SQL/retry`; proof `pg_verification_runtime::reclaimed_attempt_after_a_failed_settlement_redelivers_the_selected_result`.

Source trace: `execute` correctly checks recovered `state.terminal` before dispatch/deadline handling, but only after receiving a queue claim. `CLAIM_SQL` first terminalizes expired running rows with `attempts >= max_attempts`; only lower-attempt expired rows can be handed to the runner. `retry` also terminates at the ceiling. Thus on the final allowed attempt, accept selection and every result frame, then lose/roll back queue completion. After lease expiry, the selected outcome is never loaded and status becomes errored without its result. Existing failure-injection proof uses attempt 1→2 and therefore misses this boundary. Its deferred trigger demonstrates completion COMMIT failure is an existing supported fault seam, not a hypothetical provider edge.

Consequence/materiality: fully durable selected result is stranded by treating completion bookkeeping as unfinished engine work. Required recovery cannot finish and public polling yields a different terminal error. No new provider execution or deadline extension is needed to demonstrate the defect.

Minimal correction/burden: extend the existing queue-claim→runner accepted-state reconciliation path so exhaustion does not permanently terminalize selected work before that reconciliation. Preserve exhausted behavior for genuinely unfinished computation, provider attempt counts, original deadline, cancellation/current-writer fencing, independent dispatch completion, and the exact selected result. Reuse the existing loader/result delivery/completion owners; no second finalizer/store, unbounded retry budget or private cross-tier query. The existing queue-to-runner claim handoff is the safe boundary for this distinction: SQL must not query Bifrost. This is local completion/reconciliation adaptation to the approved terminal contract, not permission to replenish computation budgets.

Narrowing: `deliver` also turns refusal/timeout after selection into ordinary retry/termination. Do not prescribe arbitrary delivery time beyond the original bound to resolve that policy. The already-durable-result/failed-completion final-attempt case independently establishes this finding and its correction. Any materially different undelivered-result/deadline policy must be reconciled with specification authority before expanding the correction.

Focused proof: adapt the existing failed-settlement test to the last permitted attempt with selection and all result records durable before the injected COMMIT failure. Reclaim/reconcile and require identical result ID/outcome and zero engine/provider entries, including original deadline elapsed with no new computation. Preserve unfinished-work exhaustion and cancellation. No broad journey rerun is needed for this bounded closure proof.

### FIND-TASK-001-20 — REVISED / INCORRECT — queue invalidation is absent from authoritative checkpoint acceptance

Sources: SYS-2, CONC-I-1, SEC-I-01 and follow-up SYS-2. Obligations: REQ-015/057, DUR-009/010, scenario 3 and R1 preservation; supplement expressly rejects acceptance by stale/expired workers merely because no replacement has run.

Locations: server `verification/state.rs::StateScribe::{confirmed,write}/RunState::durable`; `runner.rs::state`; Vala SQL `scribe_state_heads.rs::claim/advance`, `scribe_batch_commits.rs::record`; Wyrd SQL queue cancellation/revocation/renewal/claim/settlement owners.

Source trace: `durable` confirms queue lease outside the memory lock, then freezes and stages a stable outbox-owned write. `StateWriter.not_after` is the original execution deadline. `advance` checks only head writer, exactly-next revision and offered absolute bound. Commit explicit cancellation while the staged frame is held: `CANCEL_SQL` changes status/lease expiry only; principal cancellation and `wyrd-auth::revoke::cancel_executions` do the same. The head writer/revision/deadline remain eligible. Releasing the frame accepts new recoverable progress after cancellation won. Outbox ownership survives waiter cancellation. Replay then sees a legitimately accepted batch fence, so rejecting unfenced replay cannot repair this acceptance error.

Sibling writers traced: queue claim/reclaim installs a token and deadline; renewal's `wyrd.renew_verifier_leases` checks PostgreSQL expiry and running state; complete/retry/terminate/release end running authority; `cancel_principal` is called both by revocation and admission's principal fence. None supplies queue invalidation/lease validity to generic head acceptance. Replacement does install a new head, so takeover protection works; cancellation without replacement remains broken. The original deadline check exists and is not reported absent. Expiry reproductions must use a real supported lease shorter than the remaining deadline, rather than assume defaults (ten-minute lease/five-minute timeout) exercise it. Explicit cancellation alone proves ordinary supported reachability/materiality.

Consequence/materiality: start/settlement/publication/terminal progress becomes canonical after committed cancellation or revocation, violating required authority ordering even if no waiter receives its receipt. This is not a demand to recall external effects.

Correction readiness: **SPEC_REVISION_REQUIRED**. The current generic API has claim/head/advance but no committed queue-authority handoff. Existing Scribe batch-fence atomicity orders head updates, not queue invalidation. PostgreSQL remains the lease owner. A preflight recheck leaves the same race; notification alone can arrive after acceptance; checking private Wyrd queue SQL from Vala crosses owner boundaries; extending caller transactions through private queries violates the task and SQL transaction authority; duplicating lease scheduling is forbidden. No existing declared committed handoff found in the traced producers/consumers supplies the missing ordering. The follow-up's “extend the boundary” is an outcome, not a decision-complete implementation recommendation.

Required bounded decision: define the declared queue-authority→generic Scribe acceptance handoff and its authoritative order with checkpoint acceptance, including cancellation/revocation acknowledgement, renewal/expiry visibility and crash/ambiguous-commit reconciliation. It must preserve acceptance-first progress and reject invalidation-first progress through replay, with one queue lease owner and no payload mirror/new scheduler. Decide how queue-owned terminal decisions are delivered canonically as required by finding 17 at that same boundary. This report selects no new persistent bridge, token epoch, cross-schema transaction or fail-whole-service policy. The task's material stop condition and review skill require returning this decision to approved specification authority before code remediation.

After approval, focused proof uses the existing HeldRevisions/retained-volume seams: stage after confirmation, commit explicit cancellation and separately principal revocation, release without replacement and require unchanged accepted head/state/publications. Restart and retain rejection; preserve acceptance-first inheritance. Test real queue expiry and renewal with documented bounds. No synthetic host load or independent recovery harness.

### FIND-TASK-001-21 — CONFIRMED / REGRESSION — covered publication replay bypasses current execution authority

Source: CONC-I-2. Obligations: preserved hosted publication stale-lease/replay contract, REQ-057/058 and DUR-008/009, task scenario 5. This is running tool authority, not historical read authority.

Locations: server `verification/state.rs::publication_receipt/accept_publication/receipt_once_accepted/durable`; hosted `publications.rs::json/path/receipt/accept`.

Source trace: an accepted call finds its receipt and supplies `held.changes` to `durable`; restored state starts with `changes = covered = 0`, and fully accepted live state likewise has covered tickets. `durable` returns immediately at `held.covered >= ticket`, before lease confirmation. After cancellation or takeover, old state is not broken until a fresh write attempts confirmation. Replaying the old JSON call as its first stale operation therefore succeeds; file `path` returns through preflight `publication_receipt` before scratch/storage. Base SQL publication receipt/acceptance checked running/token/expiry/deadline before returning its existing holder. The retained stale-replay test first attempts a new stale publication, which sets `broken` and masks this path.

Consequence/materiality: an invocation that no longer owns the execution receives successful hosted publish replay in an explicit lost-reply/stale-caller scenario, contrary to preserved backend behavior. No new bytes need be written for this contract regression to occur.

Minimal correction/burden: reuse `StateScribe::confirmed`/HeldLease confirmation at the common publication-success/replay boundary, outside the memory lock, even for covered tickets. Preserve valid replay without allocating a new revision; both JSON acceptance and file preflight must use it. This consumer owns active tool receipt authority, so its guard is justified. Historical inspection must remain independent of a running lease. This local guard does not repair finding 20's new-write acceptance race, and is not a second head/lease scheduler.

Focused proof: accepted JSON/file call, then cancellation and separately takeover, then same-call replay as the first stale operation: refuse both paths. A live replay returns the same receipt with no new revision. Reuse the existing publication replay fixture and remove the masking order only in this focused scenario.

### FIND-TASK-001-22 — CONFIRMED / VIOLATION — checkpoint JSON/Variant preparation bypasses the bounded writer owner

Sources: BEH-3, MNT-001. Obligations: R1 selected correction 6; applicable Rust async blocking strategy.

Locations: server `verification/state.rs::StateScribe::write`, `verification/results.rs::checkpoint`, `scribe_outbox.rs::submit/deliver/weight`.

Source trace: every frozen revision calls `serde_json::to_value`, constructs checkpoint row and encodes Variant synchronously before `reserve_until` first awaits. Existing Scribe `submit` bounds concurrent chunks by `FRAMES_IN_FLIGHT`; `deliver` uses `spawn_blocking` only for subsequent Arrow chunk encoding. Thus the full-state conversion/allocation precedes writer admission on the polling request/runner Tokio task. Starts/settlements/publications/terminal selection all converge here. Whole retained settlements/publications are supported, so growing snapshots make this a real explicit resource-placement violation; no latency/starvation measurement is inferred.

Minimal correction/burden: extend the existing bounded Scribe preparation owner to prepare frozen JSON/Variant as well as Arrow. Retain one immutable pending snapshot, encode once per frozen revision, stable retry identity, exact coverage, byte accounting/reserved capacity, deadlines, receipts and shutdown/drain. Moving work out of the memory mutex is already done; an unbounded per-mutation blocking spawn does not satisfy the selected bounded-owner contract. No new writer/actor/task service is needed. Some adapter plumbing is warranted by the explicit requirement; no speculative format optimization is warranted.

Focused proof: existing controlled writer seam observes bounded preparation admission/concurrent execution progress; retain coverage/cancelled-waiter/refusal/own-frame tests. Record actual snapshot/encoded byte counts and revision counts as R1 requires, without a new benchmark framework or synthetic load.

### FIND-TASK-001-23 — CONFIRMED / VIOLATION — changed-item rustdoc gate is incomplete

Sources: STD-001, MNT-002. Obligation: root mandatory Rust item rustdoc and Rust guide, including private tests/helpers, Errors/Panics as applicable.

Verified changed items and complete bodies/callees: spec `verification/state.rs:139::only_the_current_format_decodes` has no rustdoc and contains expect/assert; Vala SQL test `pg_scribe_batch_commits.rs:402::claim_state` omits Panics while expecting acquire/query/commit; server test `pg_durable_verifier_work.rs:155::Leased::writer` calls panic-capable `query` and renewal `hold` without Panics; lifecycle test `scribe/lifecycle.rs:619::take_over` propagates the inner `Result<RunState, StateError>` without Errors; server `verification/state.rs:834::held_state` calls panic-capable `uid` and runtime-spawning outbox construction without Panics.

Consequence/materiality: explicit merge gate, independent of probability or lint success. Minimal correction/burden: document these existing items with precise intent and relevant error/panic contracts, batching prose with behavioral work; no new abstraction, checker or behavioral test. Static bounded inspection and normal format/lints close it. Do not expand to unrelated old items.

### FIND-TASK-001-24 — REVISED / VIOLATION — new state-head result schemas decode by ordinal

Source: retained portion of STD-002. Applicable authority: global `agent_docs/rust.md:146–148` requires columns/fields mapped by name when schemas can diverge. SQL result columns and the separately authored Rust tuple shape are the independent representations here. The sibling directory's extra tenant-predicate prohibition is not imported as a global rule.

Locations: Vala SQL `queries/scribe_state_heads.rs::{claim,head,advance}` at lines 73/89, 109/118, 137/179–185. All three full bodies inspected; production callers are RunState claim/accepted and batch-fence `record`. Results use `query_as` tuples and positional destructuring for revision/batch and table/batch classification. The migration correctly forces RLS; this is not evidence of a current leak or a presently incorrect accepted value.

Minimal correction/burden: use existing SQLx named-column decoding/FromRow patterns at this owner, retaining every identity/writer/revision/deadline guard and TenantConn signature. Existing `ScribeStateHead` can serve its matching result shape; no generic mapper or new capability abstraction. The explicit applicable mapping rule makes this a bounded gate correction rather than personal preference. No requirement to delete redundant tenant predicates is retained. Static query/decoder inspection and existing state-fence integration proof, including cross-tenant refusal, suffice.

### FIND-TASK-001-25 — REVISED / VIOLATION — direct registration mutates PostgreSQL on the request path

Sources: STD-003 and focused follow-up STD-003. Obligation: `architecture/wyrd-design.md:1801–1828` verification execution entry point ownership. The paragraph explicitly names verify/submit and queue handoff; it is not unrelated application-Run prose.

Locations: server `service.rs::run_direct` → `RunState::fresh` → `RunState::claim`; Vala SQL `scribe_state_heads.rs::claim`.

Source trace: admitted direct request awaits fresh; fresh awaits claim; claim acquires a tenant connection, inserts/updates the generic head and commits before returning. That is a request-owned mutation/failure dependency on every otherwise valid direct verify. No-direct-queue-row proof does not prove this broader constraint. Generic head metadata is permitted Bifrost fence SQL; its existence is not the violation.

Minimal correction/burden: retain the exact existing fresh-writer registration and expected-next claim semantics, but drive registration via the existing background Scribe/outbox handoff and receipt before accepted progress. Keep direct distinct identity/queue-free/drop lifecycle, accepted absence, queued takeover and exact reads. Internal Scribe fence SQL remains authorized by Bifrost durability authority. Do not delete fencing, implicitly accept unclaimed revisions or change initialize-on-advance semantics. The existing background SQL/fence owner and outbox can execute the same registration; relocating it requires bounded handoff plumbing, not a new writer or a material persistent protocol choice. If implementation reveals a genuinely necessary semantics change, apply the task stop condition rather than expanding this recommendation.

Focused proof: distinguish request execution from background Scribe and demonstrate no direct request-owned INSERT/UPDATE/commit, while first checkpoint still has one registered writer, exact-next acceptance and a receipt. Retain direct queue-free/distinct-ID and stale-writer proofs.

## Specialist applicability and coverage validation

| Selected evidence | Coverage accepted and limits |
| --- | --- |
| Behavior | Required first full acceptance review covers direct/queued/state/publication/inspection/control/format scenarios and R1. Its broad canonical-state language is narrowed above; its A→B and A/B→C proof gaps remain real proof limits. |
| Invariants | Necessary shared live/pending/covered lifecycle and final-selection recovery coverage. Shared-future/ticket/receipt reasoning matches source; final-attempt proposal independently traced above. |
| Repository standards | Necessary 60-file Rust/SQL/proto/generated-doc/TS/test gate coverage. Actual capability signatures and generated source ownership covered; no unsupported language layer is substituted. STD-002 authority narrowed; other named rule coverage remains valid. |
| Maintainer | Necessary new adapter/fence and removed payload-owner workflow coverage. Existing engine, upload, outbox, queue and descriptor owners are reused as workflows. No independently supported duplicate workflow/DRIFT finding. Preparation and rustdoc proposals validated. |
| System | Necessary request/worker/shared-process/outage/replay/partial delivery coverage. Local failures do not require process-wide shutdown. Direct ACK and cancellation acceptance paths source-confirmed; retained-volume guarantee is preserved. |
| Persistent data | Necessary RLS/head/fence atomicity/replay/exact reads/migration/page coverage; explicitly empty proposed ledger considered. Not a certificate for missing state fields or lease/cancellation integration. Its general batch replay remains distinct from state-table replay. |
| Independent concurrency replacement | Necessary tickets/shared future/cancellation/takeover/current-authority boundaries. Complete assigned coverage, including distinct replay-success fast path. Superseded report is not corroboration. |
| Independent security replacement | Necessary exact feature grant, SYSTEM serving capability, tenant/File descriptors, peer trust/audit/revocation and historical identity coverage. Complete assigned boundary; superseded report excluded. |
| Follow-up | Justified material uncertainty about direct request SQL and queue/fence correction readiness. It resolves applicability, narrows expiry defaults and establishes a precise unresolved decision; it is not another vote. |

No applicable required specialty was omitted. No UI/provider/OIDC behavior or Python/Node lifetime behavior changes in this range justify separate such reviewers. Generated TypeScript catalog parity is a standards surface; public retained execution semantics remain covered by behavior/security. This does not waive affected integrated language journeys at change review. No selected report is missing, and no extra discovery pass is required for an agreed or unique proposal. This validator discovered no additional blocker requiring separate focused source validation.

## Reconciled acceptance and proof

| Requirement/scenario | Reconciled result |
| --- | --- |
| Shared engine/state adapter and direct distinct IDs/no queue; PostgreSQL remains queue coordinator | PASS for composition and identity. FAIL for direct historical inspection (16) and request-owned head mutation (25). External queue/input context is allowed; it cannot substitute for missing required immutable retained facts. |
| Complete canonical recoverable data and all terminal states/full errors | FAIL (16/17). Complete checkpoints of the represented invocations/publications are not the complete approved context/terminal contract. No requirement to serialize runtime handles or replace admission. |
| Exact accepted compatible state/authoritative absence/unavailable | PASS for implemented revision/head loader: revision zero authoritative, positive exact row mandatory; no older/fresh fallback. Source-deletion/status/inspection failures are separate (16). |
| Current writer, expected revision, accepted retry and refused WAL replay | PASS for existing generic head/batch transaction and takeover/replay. FAIL for queued invalidation/current lease at acceptance (20); original deadline predicate is present. |
| Started/spent attempts, settled restoration, no-spend refusal, original bounds | PASS for inspected engine/adapter semantics and focused existing proof; selected result reconciliation at queue exhaustion remains FAIL (19). |
| Publication custody, budgets, immutable File descriptors, downstream mounts | PASS for retained descriptor/replay/name/budget mechanics and actual consumers. FAIL for first stale replay authority (21), not historical retained readability. New-write authority failure belongs to 20. |
| Final selection and delivery, partial delivery, public selected result | FAIL for non-verdict terminal custody (17), final-attempt queue completion loss (19), and retained identity reads (16); direct final delivery intentionally does not await Bifrost acknowledgement. Normal queued result receipt and ordinary attempt-1→2 same-ID recovery work. |
| Paging/permissions/tenant isolation/audit/peer trust | PASS for inspected cut binding, exact associations, existing scoped grants and nonblocking audit after identity resolves; resolution failure is 16. No new security disclosure claim inferred from redundant SQL filters. |
| Group/replay/binding/Operator continuity and independent lifetimes | PASS for inspected unchanged coordination/delivery owners and recorded existing suites; not a waiver of shared canonical/terminal findings. No extra cleanup/archive/serialization workflow required. |
| Format/migration custody | PASS on fail-closed source and incompatible-format proof. New isolated migration setup does not dynamically prove refusal of preexisting legacy progressed rows; record that execution limit without inventing a data-loss finding. |
| R1 short memory locks, one pending immutable revision, exact tickets, surviving waiters, own-frame ACK, execution lanes | PASS for code and reproduced focused units. FAIL for bounded full JSON/Variant preparation (22). |
| R1 required hosted dependency/settlement observations and measurements | INCOMPLETE PROOF. `batched_checkpoints_recover_exactly_the_accepted_batch` only calls `begin`; its “combine” task never consumes settlements. Pure engine restoration plus adapter tests establish useful components but do not observe the requested concurrent A/B settlements→dependent C and storage-read behavior in hosted composition. Encoded bytes/concurrent-run throughput observations remain unmeasured. Close the specific planned observations with existing seams; no broad new benchmark or speculative runtime defect is inferred. These gaps were recorded by discovery, not newly self-validated findings. |
| Mandatory changed-item and SQL schema-mapping gates | FAIL (23/24); generated error projections/source layering/platform boundaries otherwise covered by selected standards evidence. |

Final `verification.md` records independently run 25/25 state/outbox units, 1/1 engine restoration unit, and 68/68 server integration tests, plus fmt and range whitespace PASS. The integration setup passed Wyrd/Vala migration apply/idempotence. These are credible positive results for their actual assertions, not fault proofs for the retained findings. Historical targeted Scribe/SQL/SDK proof and lint/codegen/docs/boundary claims remain distinguished from fresh execution. Integrated language journeys and aggregate gates are change-review work; review did not rerun them or generate load.

The reviewed private task generally follows the approved state architecture and existing owners. Its mechanics cannot override no-request-owned PostgreSQL mutation, complete canonical custody, queued final delivery acknowledgements, or the cross-owner committed-handoff constraint. No accepted historical row is carried forward from spec 16, and this first replacement audit needs no remediation-convergence assessment.

## Recommendation

**SPEC_REVISION_REQUIRED**, specifically for FIND-TASK-001-20's queue-authority→generic acceptance ordering/handoff and the queue-owned terminal-fact integration it constrains in 17. The retained findings are FIND-TASK-001-16–17 and FIND-TASK-001-19–25; finding 18 is withdrawn. Do not turn the unresolved authority decision into an open-ended implementation remediation or invent a private cross-tier transaction. Once that bounded decision is approved, local corrections can use the existing owners specified above.

Skipped: Cargo/runtime reproductions, broad language journeys and throughput benchmarks; no source edits.
Risk: the green focused suites miss staged cancellation and terminal-recovery/retained-read failures; prescribing a new queue/head protocol as a helper fix would create unapproved durable authority.
