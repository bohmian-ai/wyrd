---
id: TASK-008-R1
kind: remediation
status: ready
spec: SPEC-github-verifier-pipelines
spec_revision: 8
parent_task: TASK-008
remediates: [FIND-008-1, FIND-008-2, FIND-008-3, FIND-008-4, FIND-008-5, FIND-008-6, FIND-008-7]
---

# Make pending approval inspection and decision acceptance match the execution

An approver must be able to find a reached approval while independent work continues, decide only a current unexpired occurrence, and receive a correctly attributed durable receipt. Correct the seven independently validated gaps below through the existing state, permission, custody and deadline owners. This task also batches the two bounded documentation corrections required by the same review.

Use `$wyrd-implement`. Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 8. Public contract: `changes/active/github-verifier-pipelines/contracts.md`, Human approval. Original task: `changes/active/github-verifier-pipelines/tasks/008-approval-steps.md`.

Review base: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`. Reviewed candidate / remediation baseline: `798b6b0e83595356c0d2328fa541119267e84fb2`. Findings and full caller/source evidence: sibling `findings-validation.md`; verdict: sibling `verdict.md`. No previous remediation exists. Preserve FIND-008-1 through FIND-008-7 in closure evidence.

## Reached occurrences must enter the sole state owner before all-blocked park

**FIND-008-1 — MISSING (revised during independent validation).** TASK-008 Input → pending and REQ-010/016 require a reached pending identity/message in live state while independent branches continue. The engine produces an occurrence at `crates/vala/vala-verify/src/blocks/engine.rs:1543–1558`, but `drive` keeps it only in local awaiting state (`1094–1098`) until `tasks.is_empty()` (`1069–1082`). Runner `verification/runner.rs:579–581` then calls the sole identity/state producer, `RunState::wait` (`verification/state.rs:814–843`). Inspection and acceptance use only accepted state. A long independent Agent/code action therefore prevents any approval ID from being offered for the entire time it runs, potentially leaving no time for a decision. Current tests inspect only after that action finishes; passing them does not establish the required parallel journey.

Record each reached occurrence through the existing engine checkpoint/live RunState boundary at the pending transition, retaining stable execution/step/item/iteration identity and the existing asynchronous background persistence. Keep accepted-state inspection and decision acceptance authoritative. Preserve the accepted decision for the sole execution owner to consume through the same-execution continuation after live independent work reaches that boundary; the run-start receipt snapshot must not lose the early accepted fact. Independent work must neither be interrupted nor replayed to achieve this.

Keep all-blocked compute release and acknowledged background park custody separate. Do not add another writer, scheduler, notification framework, approval wait service, engine durability wait or pending-step SQL. Exposing an engine-local map is insufficient because decisions require accepted identity; waiting for all-blocked reproduces the defect. This is a bounded correction to the existing producer/state seam and its consumers. It does **not** require approval-dependent work to start before the held independent peer finishes.

## Discarded history must not remain eligible for a new decision

**FIND-008-2 — INCORRECT.** REQ-010, the task and Human approval contract explicitly require stale/non-pending refusal. `components/verification/service/approvals.rs:75–87` accepts historical membership and `124–133` checks only execution-level eligibility. `RunState::wait` appends history; `service/inspection.rs:279–321` projects every undecided occurrence as pending while the execution is open, ignoring accepted disposition.

A speculative approval can reach pending while its guard producer waits for a deferred input. After the guard resolves false, the engine records Discarded, while another deferred branch keeps the execution open. The retained approval remains listed as pending and a first approve/reject gets a positive receipt although the engine will never use it. Existing old-head tests cancel the whole execution and miss this supported still-open case. The defect is a stale receipt and misleading pending state; existing speculation fences already prevent discarded work from authorizing output.

Use accepted execution lifecycle/disposition (`ExecutionState::speculation` and existing status mechanisms) as the authority for current undecided eligibility in inspection/waiting projection and fresh decision acceptance. Preserve occurrence history and already accepted identical receipt replay. Accepted Discarded/Blocked occurrences must stop appearing pending and refuse a new decision with existing APPROVAL_NOT_PENDING. Preserve genuinely pending approvals. These boundary checks enforce already accepted lifecycle; do not reevaluate guards, duplicate speculation or add a lifecycle store/public status. An execution-only terminal check cannot close this case, and deleting history would remove required inspection.

## Acceptance must enforce the actual original execution ceiling

**FIND-008-3 — INCORRECT.** REQ-010/011 require decisions after the original execution expiration to refuse. `service/approvals.rs:124–129` binds only the runtime timeout to `wyrd-sql/.../verifier_executions/approvals.rs:20–24`. Runner `verification/runner.rs:1449–1460` instead caps runtime remaining time by the exact Verifier's authored `limits.timeout_seconds`, both measured from admission.

With a short authored limit under a longer operator ceiling, park and stop the worker. After the authored expiration the queue is still unfinished, so the endpoint accepts a durable decision until the wider runtime ceiling. Restart correctly times the execution out, but cannot undo its invalid positive receipt. Current expiry proof waits for terminal state under the runtime ceiling and misses this deterministic gap; no cross-store timing race is needed.

Use the runner's same minimum effective admission-relative execution duration for fresh decision admission. Resolve the accepted exact Verifier through the existing target/cache owner and supply that duration to the existing tenant-scoped SQL predicate, retaining PostgreSQL's statement clock. Do not resolve a newer policy version, reset age, create an approval deadline/clock anchor/store, or wait for eventual worker settlement. Preserve already-accepted identical receipt replay, authorization, Scribe ACK and wake recovery. The current SQL parameter already carries the necessary duration; no schema or deadline protocol is required.

## Receipt identity must use the authenticated actor

**FIND-008-4 — INCORRECT.** REQ-010/contracts require server actor provenance and same-actor retry. `service/approvals.rs:138` stores `caller.principal.id`; `:201` uses it for replay, including the raced-acceptance replay at `:162`. Under supported normal delegation this principal is effective subject S, while `Caller::attribution()` already derives actual current actor A. Another authorized actor B for S, or direct S, can therefore replay A's receipt and the stored actor is wrong. Direct-principal tests cannot reveal this. This violates provenance/replay, without implying permission escalation.

Use existing `Caller::attribution().act_principal_id` consistently for receipt actor and replay identity. Preserve permission evaluation on the effective principal, managed subject/actor attribution, tenant isolation, first-wins fence and continuation. Both normal and raced replay must use this identity. This bounded local correction is sufficient; no token restriction, new attribution abstraction, login flow or storage field is authorized.

## Batch mandatory documentation and error-boundary corrections

**FIND-008-5 — VIOLATION.** New MCP test helper `Opened::start` at `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/github.rs:386–455` has assertion panics (`419`, `439`) and Errors rustdoc but no mandatory Panics contract. Root AGENTS and the Rust guide explicitly include test helpers. Document the actual connect/delivery assertions' panic conditions; retain assertions and fixture behavior. No new test or fixture redesign is necessary.

**FIND-008-6 — INCORRECT.** The newly added `architecture/wyrd-design.md:1647` says wire decision `approve | reject`, while approved `contracts.md:299–300` and closed `ApprovalDecision` serde/schema accept `approved | rejected`. A literal reader receives 400. Correct those owner values to the approved contract. Do not add aliases, enum variants or change the API/spec. This correction is authorized by the task's required accurate architecture amendment, not by implementation preference.

**FIND-008-7 — VIOLATION.** New `verification/approvals.rs::unavailable:226–230` embeds Display(error) in public ServiceUnavailable.message. Tenant connection/fence-query failures reach this helper through `ApprovalDecisions::decided:88/91`, retained inspection (`service/inspection.rs:520–522`) and decide; HTTP/MCP project the unredacted error. Supported database outages expose internal diagnostics in violation of `agent_docs/errors.md`. Runner logging does not sanitize public sibling consumers. The similar pre-existing evidence helper is not this task's correction scope.

Log the diagnostic with structured tracing at the approval custody owner and return a stable safe public message. Keep ServiceUnavailable code/status, safe table detail, retryability and the unavailable-versus-absence distinction. Follow existing redaction conventions such as `http/error.rs::internal_failure` without changing to an unrelated status. A local projection check is enough; no broad error migration, retry layer, dependency or harness is needed.

## Preserved behavior and exclusions

Keep ordinary authentication, exact Verifier approve plus subject authorization and audit. Keep queue-only derivation at registration, closed wire types and thin public projections. Keep canonical Bifrost first-wins decisions, ACK-before-receipt, unavailable accepted-state refusal, replay/conflict semantics and best-effort queue wake repaired by park recheck. Preserve accepted authority, exact source/policy, stable identity, settled predecessors, remaining attempts/spend, original expiration, rejection-as-data and supersession/cancellation fences.

No UI, recent-login/proof requirement, credential-type shortcut, new public operation, generic workflow owner, database payload mirror, per-approval clock, polling service, speculative guard evaluator or fault/crash hook. Do not strengthen the admitted-request/ACK overlap into an unapproved cross-store atomicity protocol. No requirement for immediate approval-dependent work while a peer is active. No unrelated evidence/error cleanup or style-only review round.

## Acceptance and focused proof

| Finding | Required observable closure |
|---|---|
| FIND-008-1 | Hold an ordinary independent action active. Reach approval, inspect its accepted ID/message and obtain a durable decision before releasing the peer. Release it; same execution consumes the receipt without replaying settled work or renewing deadline/spend. Existing all-blocked release and stable-ID assertions remain. |
| FIND-008-2 | Park speculative approval behind an unresolved deferred guard; resolve guard false while another branch remains waiting. History is retained but no longer pending; first decision for its old ID returns typed 409. Other branch and identity remain intact. |
| FIND-008-3 | Short authored timeout under longer runtime ceiling: before-expiry decision works; after parking/stopping worker and authored expiry, fresh decision returns typed 409 with no accepted decision. Restart observes timeout. Existing runtime-ceiling behavior remains. |
| FIND-008-4 | Use ordinary real token exchange for actor A on behalf of S. Receipt/inspection names A; same A retry is exact; authorized B sharing S and direct S conflict. Existing direct and scope-refusal cases remain. |
| FIND-008-5 | Complete helper rustdoc documents the actual assertion panics; assertions unchanged. |
| FIND-008-6 | Owner sentence matches approved contract and closed serde/schema `approved/rejected`; public behavior unchanged. |
| FIND-008-7 | An internal diagnostic marker is absent from the public problem projection; stable unavailable code/status/safe table detail remain. Existing unavailable/ACK retry behavior stays correct. |

Place focused proof in existing engine, server approval and identity/journey owners; reuse their fixtures and ordinary restart/retry/refusal controls. Do not invent a new test harness. Existing focused commands (run through mise; keep Cargo sequential):

- `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::approvals_hold_only_their_dependents_until_decided)'`, plus exact selectors for added/changed engine proof.
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::each_approval_occurrence_has_its_own_stable_id)'`, plus exact selectors for changed state/error projection proof.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_approvals::/)"'`, extending this existing selection with exact new identity integration proof only if placed outside pg_approvals.
- `mise run fmt:check`; use only owner lint/check lanes needed by the actual write set. Run tenant/dependency checks if those boundaries change; regenerate/check contracts only if their sources actually change. No public contract change is prescribed here.

Record exact new test selectors and observed assertions/results with each finding's closure. Do not require full language journey suites, a broad gate, SQL-wide tests or performance experiments in this remediation. Full integrated qualification, including clean full L1 verification comparison for shared hot-path changes, remains a change-review concern. The earlier review's unrun broad lanes are limits, not permission to weaken any mandatory gate or expand this bounded task.

## Closure evidence

Candidate: `798b6b0e8..b570026f6` (commits `3b5356075`, `5e569882d`, `ba8ff6196`, `b570026f6`). Postgres selectors ran under `scripts/postgres/with-test-postgres.sh` after `mise run db:migrate:all:inner`.

| Finding | Implementation | Proof | RED → GREEN | Result |
|---|---|---|---|---|
| FIND-008-1 | `Checkpoints::reached` (`vala-verify/src/blocks.rs`) is called from `drive` at each `Done::Awaiting` (`blocks/engine.rs`); `RunState::reached` records the occurrence through `self.change` and the shared `record_approval` used by `wait` (`verification/state.rs`), keeping background persistence and the stable `approval_id`. The runner's `Parked` branch rereads decisions for the parked occurrences and parks for `Duration::ZERO` when one is already decided (`verification/runner.rs`), because a decision accepted while `running` has no pending row for `wake_decided` to touch. | `blocks::engine::tests::a_reached_approval_is_recorded_while_independent_work_runs`: the occurrence is recorded while the independent step hangs, and only that step ran. `pg_github_pull_request_extraction::an_approval_beside_running_work_is_decided_before_it_parks`: GitHub holds the extraction's pull request read; the execution is `running` and lists `signoff` pending with its message; an editor approves it (200, `approved`) while it is still `running`; after release it completes and passes within 30 s (not the 1 min recheck), shows `approved`, and reads `/compare/` once (no replay). | Engine test without the `reached` call: fails at `tests.rs:4419` "not recorded while the step ran". Journey without the `reached` call: fails at line 1424 "not offered while the extraction runs" (32 s). Journey without the park check: fails at line 1465, status `pending` waiting on the recheck. All GREEN. | PASS |
| FIND-008-2 | `Retained::withdrawn` (Discarded/Blocked via `ExecutionState::speculation`) excludes withdrawn occurrences from `undecided()` and projects them `expired`; decide refuses a fresh decision of one with `APPROVAL_NOT_PENDING` after replay is checked (`service/inspection.rs`, `service/approvals.rs`). | `pg_approvals::a_discarded_speculative_approval_refuses_a_decision`: rejecting `gate` leaves `signoff` listed `expired` with no actor; the execution waits for `["ci"]`; approving `signoff` returns 409 `expired`. | Before the fix it failed at `pg_approvals.rs:161` (never served `expired`, 123 s). GREEN. | PASS |
| FIND-008-3 | Decide resolves the exact Verifier through `verifier_cards().resolve` and caps the runtime timeout by `Target::authored_timeout()`, which the runner shares (`verification/cache.rs`, `runner.rs`); the existing `open_for_decision` SQL predicate receives the minimum. | `pg_approvals::a_decision_after_the_authored_timeout_is_refused`: authored 8 s under 5 min; the early decision is 200; with the runtime stopped, a decision after 8 s is 409 `expired`; the restarted runtime reports `timed_out` with no actor. | Before the fix it failed at `pg_approvals.rs:656` (200, not 409). GREEN. | PASS |
| FIND-008-4 | Receipts and both replay paths use `caller.attribution().act_principal_id` (`service/approvals.rs`). | `components::verification::service::approvals::tests::only_the_deciding_actor_replays_a_decision`: A acting for S is stored, not S; A's retry replays exactly; B acting for S and S directly get 409. | Before the fix it failed at `approvals.rs:288`. GREEN. Deviation: an end-to-end token exchange cannot reach a pull request subject, because the actor must be a Service rooted at the subject and a Service cannot root at a Repository Card (`WYRD_AUTH_401_WORKLOAD_UID_REFUSED`). The unit test pins the identity rule at its owner. | PASS |
| FIND-008-5 | `Opened::start` documents its connect and delivery assertion panics (`wyrd-mcp/tests/bifrost/mcp/github.rs`); assertions unchanged. | Content review. | n/a (documentation). | PASS |
| FIND-008-6 | `architecture/wyrd-design.md` states `{decision: approved \| rejected, reason?}`, matching `contracts.md` and `ApprovalDecision`. | Content review. | n/a (documentation). | PASS |
| FIND-008-7 | `unavailable()` logs the cause with `tracing::warn!(cause = %error)` and returns the fixed message "approval decisions cannot be read now; try again", keeping `ServiceUnavailable` and the table detail (`verification/approvals.rs`). | `verification::approvals::tests::an_unavailable_read_hides_its_cause`: the public message omits the cause, and the code and status are kept. The unavailable and ACK retry journey `pg_approvals::an_unacknowledged_decision_is_unavailable_and_its_retry_replays` stays green. | Projection check. GREEN. | PASS |

Regression: `pg_approvals::`, `pg_github_pull_request_extraction::` and `pg_deferred_evidence::` pass (37/37). The wyrd-server lib `verification::` tests pass (81/81 with Postgres), and so does the vala-verify lib (81/81). `mise run fmt`, clippy `-D warnings` for wyrd-server and vala-verify, `check:tenant-isolation` and `git diff --check` are clean. The discarded-speculation journey now waits for `waiting` after observing `expired`, because occurrences are recorded during the run and the withdrawn state can show while the execution still runs toward its park. The assertions are unchanged in substance.
