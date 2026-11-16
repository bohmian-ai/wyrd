# System resilience review — TASK-004

Result: **FAIL**. Base `dfd19aa129830696d68cf8d4a09c144805847345`; committed candidate `1beb1aa1980cb03495dfb0778a353604670ba935`; authorized immutable WIP tree `226fdafc56bacc707b9167d1887be9b72f5e6d53`. Locations below refer to that tree. No prior acceptance carried forward; no other reviewer reports or implementation summaries read.

## Scope and deployed paths

Read the full task-review skill, review scope, replacement TASK-004, applicable spec REQ-028/030/054–059, public connection/revocation contracts, execution-state supplement, root/SQL instructions, maintainer style, workflow, testing map, architecture map and operations readiness/recovery sections. Used CodeGraph first, then immutable Git blobs/diffs. Traced ingress → tenant SQL queue → runtime → adoption → ordinary enqueue → runner/state/cancellation owners, and searched the candidate for all Check writers/consumers.

Every composed verification runtime with an App adds a GitHub worker (`verification/mod.rs:558–565`); multiple replicas coordinate through SQL claims. The existing supervisor isolates capability tasks and restarts exits/panics (`:350–401`). Provider errors are logged/retried by this component rather than crashing siblings. An absent optional App composes no worker; invalid configured keys fail typed boot validation.

| Obligation / failure path | Evidence and proof assessment | Result |
|---|---|---|
| Persist signed delivery before ACK; refuse store failure | `github/routes.rs:597–611`, `automation.rs::accept_webhook`: verify/decode/resolve/save/commit precede 202; SQL dedups delivery identity | PASS by source |
| Process loss after claim and later reclaim; short transactions | `worker.rs:114–121` commits before IO; `github_deliveries.rs:39–67` reclaims expired rows and token-fences settlement | PASS for settlement only |
| Provider stall, takeover, shutdown and unrelated progress | Whole deliveries execute serially without attempt cancellation or lease ownership | FAIL SYS004-01 |
| Exact subject across trusted adoption, admission and Check association | Worker pins once; ordinary enqueue fetches a new pin | FAIL SYS004-04 |
| Terminal Check delivery, ambiguity, key rotation and retained-state uncertainty | Only SQL record/list operations exist; no delivery consumer | FAIL SYS004-02 |
| Current PR generation and affected-work cancellation | Required events fall through; control operations only update access | FAIL SYS004-03 |
| Existing repository execution recovery and terminal ACK retention | Runner preserves `LeaseRenewal`, `RunState::recover`, retained delivery and repository identity; `runner.rs:1383–1415` stops on lost lease | No separate defect found in inspected changes; comprehensive foundation qualification remains outside this specialist scope |

The `repository_verification` journey asserts static provider pins, ordinary execute/enqueue, target validation and permission refusals. It does not drive signed ingress, provider stalls, takeover or a remote Check. `pg_github_repository.rs` asserts binding revisions/retirement, not delivery recovery. No runtime checks were executed during this review.

## Proposed findings

### SYS004-01 — INCORRECT / VIOLATION: bespoke serial worker loses bounded claim ownership

**Obligation:** TASK-004 requires reuse of existing queue/claim/lease/cancellation owners, unrelated repository progress, bounded attempts, and refusal of stale admission/effects. Operations reliability requires bounded external dependencies.

**Location:** `components/github/worker.rs:80–139`, particularly `pass` awaiting `next` at :102 and unfenced `process` at :121; `queries/github_deliveries.rs:39–67`; runtime composition borrows the 10-minute runner lease at `verification/mod.rs:559–563`.

**Failure evidence:** a valid delivery performs sequential token, PR, tree, dependency-blob, declaration and admission calls. `automation.rs:158–175` fetches dependency blobs serially. Each HTTP request is bounded (`github/mod.rs:296–335`), but the whole delivery is not. Slow successful calls can exceed the lease. Replica B reclaims while A continues declaring Cards, reconciling bindings and admitting work; only final settlement checks A's token. B can adopt current configuration before A later overwrites bindings from older configuration. The serial wait also blocks every later tenant/repository on that replica. Shutdown is checked only between deliveries and after the pass, so it cannot interrupt processing; a claim completing after shutdown still executes. The eight-error cap applies after processing, while SQL continues incrementing reclaimed attempts without an exhaustion admission boundary.

**Materiality:** supported multi-replica deployment, ordinary local imports and rolling shutdown reach this path; an infinitely stalled provider is unnecessary.

**Smallest correction:** adapt this queue to existing `ClaimLoop` ownership instead of another polling/draining lifecycle. Preserve bounded capacity, drain, cancellation and late-claim release. Follow the existing Operator finite-attempt pattern or existing renewable lease owner for genuinely long attempts; authority must cover binding mutations/admission, not just final settlement. Existing runner `LeaseRenewal`/`HeldLease` demonstrates loss cancellation. A local timeout alone does not fence a worker resumed after a pause. Keep the queue-specific adapter; do not add another scheduler, global lock, execution-state mirror or process-wide failure.

**Focused proof:** delay one provider operation while another repository progresses; stop during processing and observe bounded release; reclaim an expired delivery then resume the old worker and assert no stale binding/admission/effect. Use the existing fake provider and repository Postgres, without synthetic host load.

### SYS004-02 — MISSING: accepted executions never produce or recover a GitHub Check

**Obligation:** REQ-030 and TASK-004 scenarios 3–4 require an actual exact-head Check, independently retried from authoritative selected state, ambiguous-effect reconciliation, monotonic terminal status and visible delivery failure without rerunning verification.

**Location:** `worker.rs:233–263,286–316`; complete `queries/github_checks.rs` (record/list only); composition `verification/mod.rs:553–565`.

**Failure evidence:** signed PR work can enqueue a Verifier and record a permanently queued SQL Check (`delivery=pending`, `check_run_id=NULL`). Candidate-wide source search finds this worker as the sole server `github_checks` caller, and no provider Check create/update consumer. Configuration failures likewise only write SQL. Nothing projects execution results, observes unavailable retained state or delivers pending Checks after restart/provider recovery/key rotation. This is one incomplete Check-delivery outcome, not separate findings for each absent conclusion/recovery variant.

**Smallest correction:** connect the existing independent delivery/effect boundary to generic library Check operations and the authoritative retained-state reader. Keep original execution/head, wait on unavailable state, reconcile ambiguous creation and expose exhausted delivery on execution inspection. Do not re-execute verification or mirror results into SQL. If TASK-004 is reduced to onboarding, explicitly move this complete outcome and recovery proof to its later task rather than claim it complete here.

**Focused proof:** signed ingress → ordinary Verifier → observed remote Check; lose the create response and restart, retaining one association; make exact retained state unavailable, showing no success/no rerun, then recover and deliver the original conclusion/head.

### SYS004-03 — MISSING: PR and access lifecycle changes do not stop affected work

**Obligation:** REQ-028, public disable/disconnect/revoke contracts and TASK-004 require shared current PR generation ordering, supersession/closure cancellation and cancellation of affected manual/queued/automated repository work after access loss, preserving evidence and unrelated work.

**Location:** `worker.rs:161–204`; `service.rs:846–865,1051–1094`; `queries/github.rs::deactivate_connection/revoke_repository`. Existing correction owner: `VerifierExecutionQueue::cancel/cancel_principal` and its pending terminal delivery.

**Failure evidence:** close/rerequest/push/installation/access events are saved then settled as successful no-ops. New-head PRs enqueue additional work without cancelling unfinished older work. Disable/disconnect/revoke update settings only. An already queued/running manual or automatic repository execution continues under accepted authority: runner changes carry repository identity but attach no access transition to cancellation. Principal revocation cannot substitute because revoking one repository need not revoke its principal and must preserve unrelated work. This is one missing event/control lifecycle, not a finding per event.

**Smallest correction:** connect required repository/PR transitions to the existing queue cancellation/fencing owner and current logical generation shared by ordinary/rerequest writers. Include manual repository work; retain terminal delivery until Bifrost ACK. Reconcile access events through the existing connection/access owner. Preserve pending-only default-push maintenance. Do not introduce a result store, scheduler or global lock. If onboarding and runtime are split, move this outcome intact with runtime.

**Focused proof:** admit active/queued work, revoke/disconnect, observe fenced cancellation and retained evidence while another repository progresses. Close/supersede/rerequest proof must show old events cannot cancel newer work and one deliberate rerequest survives redelivery as its same generation.

### SYS004-04 — INCORRECT: one event adopts one pin but admits another

**Obligation:** TASK-004 exact configuration/execution/Check association, REQ-028 logical identity includes base/head, and REQ-030 old results never claim a different head.

**Location:** `worker.rs:177–180,286–316`; `components/verification/service.rs:727–756`.

**Failure evidence:** worker pins PR base/head A, adopts A's trusted suite and derives the execution key and Check subject from A. `VerificationControl::enqueue` receives the unpinned target and fetches the PR again, admitting current base/head B. A normal synchronize or retarget between those calls associates A's configuration and Check with execution B. Different suite members can see different heads. This is already wrong before adding remote publication: durable Check rows link mismatched execution subjects. Static-pin assertions do not test this race.

**Smallest correction:** preserve server pinning for external manual requests, but carry automation's already authorized pinned subject through the existing internal admission owner; derive logical identity and Check association from that same pin. Preserve full-suite preflight and authorization. Do not expose caller-trusted pins publicly or duplicate admission.

**Focused proof:** advance/retarget the fake PR after its first response; assert all admitted subjects, adopted configuration and Check associations use one selected pin and replay preserves those execution IDs.

## Task scope recommendation and limits

Split verified install/connect/manage from PR automation/Checks. Keep repository execution/trusted adoption with the runtime task that consumes it, and give moved cancellation/delivery/recovery outcomes explicit owners and proof. This is planning advice, not approval to weaken product behavior. Extend existing claims, cancellation, admission and retained-state owners rather than turning every unfinished ledger row into new machinery.

No Cargo, lint, codegen, Postgres or provider tests ran; source/task files remained unchanged. Source proves reachable paths and absent consumers, not measured throughput or cross-platform qualification.
