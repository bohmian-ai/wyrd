# TASK-008 round 2 independent validation

Recommendation: **FIX_REQUIRED**. Five prior findings are closed. FIND-008-3 has an incomplete deadline correction; FIND-008-4 has correct production code but lacks its required connected proof. One remediation-induced availability regression is retained as FIND-008-8.

## Immutable subject and independence

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`. Remediation base: `798b6b0e83595356c0d2328fa541119267e84fb2`; candidate: `cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`. HEAD was checked as the candidate. Cumulative original base remains `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`.

This is a fresh validator, independent of implementation and discovery. Inputs: original task `../../tasks/008-approval-steps.md`, approved packet spec revision 8 and Human approval contracts, R1 verdict/validation/remediation, actual remediation diff, [scope.md](scope.md), all three selected discovery reports, [claim-comparison.md](claim-comparison.md), and [verification.md](verification.md). Applicable root and SQL rules, maintainer/Rust/error guidance, and owning Verifier/accepted-authority/delegation contracts govern. CodeGraph preceded source inspection; targeted reads filled gaps omitted by capped queries. No Cargo command or production edit was made by this validator.

## Specialist applicability and carried coverage

The selected remediation discovery reviewer covers changed behavior, Rust items/tests, owner reuse and docs. The data-domain reviewer covers the coherent changed persistent-state/lifecycle boundary: reached occurrence, background writer, accepted withdrawal, runner continuation, authored deadline resolution and database resource hold. The security-domain reviewer covers changed actor attribution/replay and public error projection. Reports are complete and their cited boundaries were independently inspected.

Separate broad repository and maintainer reviewers are unnecessary in this bounded round: no SDK/wire/schema/feature/dependency changes or substantial replacement owner were introduced; discovery covers the changed Rust and documentation surfaces and compares existing ownership. Separate invariant/system passes would duplicate the scoped data review and prior source-backed invariant/system coverage of unchanged writer custody, queue settlement and recovery. No new UI, provider, deployment or storage-schema domain is affected. These omissions are validated for this remediation scope, not for the original full task or integrated change review.

Carry forward R1 verdict/validation matrices and behavior, invariant, maintainer, standards, security, data and system reports for unchanged closed authoring action, queue_only registration, stable logical IDs/fanout/repeat, public SDK/CLI/MCP projections, tenant/RBAC/audit, first-wins canonical Scribe custody, ACK/retry, settled-work/spend restoration, supersession and all-blocked handoff. Actual changed dependencies reopen only the occurrence reach/withdrawal consumers, target/deadline resolution, pool acquisition, actor receipt/replay and safe errors. No acceptance row is carried merely because its file is unchanged.

## Proposal decisions

| Discovery proposals | Validation | Final ID / rationale |
|---|---|---|
| DATA-R2-001; behavior ready-target closure qualification | **REVISED** | FIND-008-3 remains open. The ready-target journey is valid but `.ok()` erases refused targets and widens the deadline. Same prior expiration invariant; no new ID. |
| SEC-R2-1; behavior delegated-proof caveat | **REVISED** | FIND-008-4 remains open only for explicitly required connected proof. Production receipt and shared normal/raced replay now use the correct actor. |
| DISC-008-8, DATA-R2-002 | **CONFIRMED** | FIND-008-8: same new nested app-pool acquisition cause, one correction. |
| FIND-008-1/2/5/6/7 closure | **CONFIRMED closed** | Source and observable assertions support closure below. |

No proposal is retained solely because reviewers agree. No additional blocker originated in validation, so no separate validator-finding source validation is required. No follow-up is necessary: ready-target success and refused-target failure describe different inputs; source resolves the apparent disagreement. Duplicate pool proposals consolidate into one finding. Rejected proposals: none.

## Prior-finding observable closure

| Finding | Independently validated implementation and proof | Closure |
|---|---|---|
| FIND-008-1 | `blocks/engine.rs:1109–1115` records each Done::Awaiting occurrence through Checkpoints::reached; `verification/state.rs:1097–1105` uses the sole memory state mutation owner and shared record_approval, preserving background persistence and deterministic identity. `runner.rs:578–614` rereads decisions at park and makes already-decided parked work immediately due. New engine assertion sees the exact occurrence while the peer hangs. New extraction journey (`pg_github_pull_request_extraction.rs:1401–1485`) holds an ordinary extraction read, inspects running + pending ID/message, receives approved before releasing the peer, then observes same execution passed and exactly one compare read. No deadline/spend field is mutated by reached recording; existing restore/ID/handoff evidence carries forward. Root executed both focused engine tests, stable-ID test and journey successfully. | **CLOSED** |
| FIND-008-2 | `Retained::withdrawn` (`inspection.rs:289–293`) reuses accepted speculation disposition. Undecided and approval projection exclude Discarded/Blocked; fresh decide refuses after existing accepted replay. `pg_approvals.rs:734–790` rejects gate, sees signoff history expired, keeps ci waiting, refuses old signoff with exact APPROVAL_NOT_PENDING/expired and retains waiting execution. Absent-decision projection has null actor. Root executed the journey successfully. | **CLOSED** |
| FIND-008-3 | Shared Target::authored_timeout and minimum duration fix the ready-target path; new stopped-worker test (`pg_approvals.rs:656–712`) observes early 200, late typed409 and restart timed_out with null actor. Root executed it. Refused resolution is nevertheless erased to no authored cap (`service/approvals.rs:145–148`), so the original expiration invariant is incomplete; details below. | **OPEN — incomplete implementation** |
| FIND-008-4 | `service/approvals.rs:163` and `replay:227` correctly use attribution actor; both initial and raced replay call that comparator. New test (`:295–326`) manually constructs Caller and the expected receipt and tests replay only. It does not establish authenticated production receipt creation, persistence or inspection; it would pass if production creation regressed to subject ID. Required real exchange is feasible on generic queued Service-subject approval. Root executed the unit test, not that missing journey. | **OPEN — required connected proof missing** |
| FIND-008-5 | MCP `Opened::start` (`github.rs:386–460`) now has Panics documenting actual browser 200 and webhook202 assertions. Assertions and fallible behavior remain intact. | **CLOSED** |
| FIND-008-6 | Verifier owner at `architecture/wyrd-design.md:1647` now states `approved \| rejected`, matching approved Human approval contract and the unchanged closed ApprovalDecision enum. No aliases/API change. | **CLOSED** |
| FIND-008-7 | `verification/approvals.rs:226–232` internally traces the diagnostic and returns only fixed safe message + table in unchanged ServiceUnavailable. Local test passes; root also reran unavailable/ACK retry. Public mapper projects typed fields only, so discarded diagnostic cannot reappear. Stable503/code follow unchanged variant; no broad error cleanup required. | **CLOSED** |

## Final deduplicated ledger

### FIND-008-3 — A refused exact target silently removes the original authored cap

Status **REVISED**; classification **INCORRECT**; discovery source DATA-R2-001; continuing prior FIND-008-3.

Obligation: REQ-010/011, INV-005, Human approval and R1 remediation require fresh decision admission to enforce the same original effective admission-relative duration as the execution, without widening it during worker downtime.

Exact location: `crates/wyrd/wyrd-server/src/components/verification/service/approvals.rs:134–148`, particularly `.ok().and_then(|target| target.authored_timeout())`.

The resolver has two result layers: SQL failure and a successful read that refuses the target (`cache.rs:230–240`). `Target::resolved:94–103` applies `VerificationTarget::refusal`; the latter rejects absent/deleted subject or Verifier and non-ready Verifier (`wyrd-sql/src/queries/verification_target.rs:46–58`). `.ok()` converts this inner refusal to the same None as a successfully resolved Verifier with no authored timeout. `ceiling` therefore becomes the longer runtime duration.

Reachable supported path: manually enqueue an approval Verifier against an ordinary subject Card with no visible inbound Card references; reach and accept pending state; stop its worker. Delete the subject through the normal Card lifecycle, then make the request after its shorter authored expiration and before the runtime ceiling. Soft deletion (`queries/cards/delete.rs:89–166`) checks Card references and updates status; neither it nor `finish_card_delete` cancels the queue row. Cache invalidation removes targets naming that subject (`cache.rs:358–394`); a cold/restarted process also reads the refusal. Accepted retained state still identifies the pending occurrence (`inspection.rs:442–535`), while authorize_subject (`service.rs:1017–1054`) checks permissions/signed scope, not current Card readiness. SQL open (`verifier_executions/approvals.rs:20–24`) sees only unfinished queue status and the supplied duration. The erased refusal thus admits a durable receipt after the actual original expiration. This does not rely on a deadline/ACK nanosecond race or require a PR-specific deletion sequence.

Materiality: ordinary registry lifecycle plus stopped-worker recovery widens an explicit server-owned integrity boundary; later timeout cannot undo the invalid positive receipt. The new ready-target test never exercises the refused-result branch.

Selected smallest correction: keep exact target resolution at its existing owner but propagate its refusal before fresh decision admission. Return the existing typed `WyrdError::VerificationTargetNotFound` with safe exact-target details, following `VerificationControl::resolve` at `service.rs:963–971`, rather than treating unresolved original limits as absent. Preserve accepted identical replay before this check. A successfully resolved target whose authored limit is genuinely absent still uses runtime duration. SQL failures retain existing typed registry unavailability. No new error code, registry reader, stored deadline, timer, guard evaluator or stronger cross-store protocol is required.

Correction burden: one local refused-result handling correction at the original duration source. Deletion is unsafe because it would restore runtime-only admission. Existing resolver and typed target refusal are sufficient; there is no need to change the runner, SQL clock or schema.

Focused proof: extend existing stopped-worker authored-expiry proof using ordinary subject deletion and an actual cold/invalidated target lookup. After authored expiry, assert the existing typed target refusal and no newly accepted approval/actor; do not fabricate a second expiration timestamp to force an expired projection. Keep the ready-target before-expiry success, after-expiry typed409, restart timeout and accepted-replay tests. No executed reproduction of this added branch is claimed in round2.

### FIND-008-4 — Required real delegated decision acceptance and inspection proof is absent

Status **REVISED**; classification **MISSING**; discovery source SEC-R2-1 and behavior proof caveat; continuing prior FIND-008-4.

Obligation: R1's explicit observable closure, REQ-010 provenance/same-actor replay, Human approval, and root's user-facing connected-test rule. Correct source alone does not waive that prescribed test.

Exact location: synthetic proof at `components/verification/service/approvals.rs:261–326`; R1 remediation's FIND-008-4 closure-evidence row claims completion by that substitute.

The production correction is correct: receipt creation stamps act_principal_id (`:163`), shared replay compares it (`:227`), and both retained/raced acceptance paths reuse replay (`:116–119`, `:177–189`). Authorization remains on the effective principal. The new unit test builds User Callers directly and builds held.actor_id from the expected attribution, then invokes replay. It cannot fail for a broken production receipt creation, dropped verified chain on the route, or wrong persisted inspection actor.

The claimed Repository fixture restriction does not make the required ordinary journey unreachable. The approved task/spec supports manual queued approval. Generic enqueue (`service.rs:741–856`) accepts VerificationRequest and authorizes request.card_uid; no PR/Repository restriction exists. Normal exchange (`wyrd-auth/src/exchange_api_key.rs:277–321,372–401`) requires a rooted Service actor and narrows effective authority; it does not require a Repository subject. Existing `pg_verification_routes::enqueue_admits_only_signed_scope_subjects:714–783` already combines Service credentials, real token exchange and generic enqueue. Use matching Service-root subject scope and grants instead of that test's unrelated-root refusal case. Caller attribution correctly preserves the verified effective subject/outer actor split (`caller_extractor.rs:55–64,93–104`).

Materiality: this is the specifically diagnosed connected identity/provenance boundary, and the mandated test observes actual authenticated storage/inspection behavior. This is a proof gap, not a claim of production permission escalation or a new credential rule.

Selected correction: retain the two correct production actor uses. Add one focused ordinary real-exchange server journey using existing Service credential/delegation and generic queued approval fixtures. Actor A acting for S obtains the durable receipt and inspection actor A; identical A retry returns the exact receipt; separately authorized actor B sharing S and direct S receive exact typed409 for the same decision/reason. Preserve direct-principal and scope-refusal tests. No new auth flow, token profile, helper abstraction or harness is justified.

Burden: bounded test/fixture composition at existing owners. Synthetic caller construction cannot prove route-to-custody attribution; full language suites and external IdP setup are unnecessary for the repository's own normal token exchange. No executed real-exchange approval result is claimed in round2.

### FIND-008-8 — A cold approval resolution holds one pool slot while acquiring another

Status **CONFIRMED**; classification **REGRESSION**; discovery sources DISC-008-8 and DATA-R2-002.

Obligation: preserve supported cold-load decision availability under bounded database capacity; REQ-017 preserves necessary cold loads and short tenant-scoped ownership. This finding concerns a new same-pool resource hold, not a demand for unbounded throughput or synthetic host load.

Exact location: `components/verification/service/approvals.rs:124–145`. The new path acquires `TenantConn` first and retains it until commit at156, then resolves with `Db::Pool(self.state.postgres.wyrd())`. On a cache miss, `VerifierCardCache::load` acquires `postgres.conn()` (`cache.rs:291–297`). `WyrdPostgres::conn:182–188` and `tenant_conn:195–201` both acquire the same app pool. The caller's first connection cannot be released until its inner resolve finishes.

Supported failing path: during cold cache/restart or listener loss (cache serves memory only while live, `cache.rs:310–325,406–428`), ordinary concurrent cold decisions take the available app connections as outer tenant transactions, then wait for a second slot from the same bounded pool. Even one request with only one serving slot available holds the slot it needs. Acquisition times out and fresh decisions return registry unavailable despite the held connection being sufficient for the read. Other pool users/listeners are normal supported traffic; no artificial host load is needed. Warm-cache journeys leave this branch untested.

Materiality: this added nested acquisition makes normal control requests fail during cold loading and retains scarce transactions throughout the wait. The regression is independent of the original deadline fallback; it persists even if inner refusal handling is fixed.

Selected smallest correction: use the resolver's existing `Db::Conn(conn.transaction())` variant for the already-open tenant transaction. Cache `load:298–302` directly runs the same target read on it; VerificationTargets explicitly supports a transaction already bound to that same tenant. Existing runner/acceptance consumers use this mechanism. Preserve exact target IDs, tenant binding, eligibility SQL/commit, receipt ACK and wake order. Do not add a semaphore, retry, separate pool, cache-prewarm prerequisite or new connection owner. Resolving before opening the transaction could also avoid nesting, but reuse of the existing connection is the selected sufficient local correction.

Burden: a small existing-capability substitution plus focused constrained-pool proof. Deleting the target read loses required authored timeout; the existing Db::Conn arm solves the resource hold without new machinery.

Focused proof: use existing pool/approval integration fixtures to make target resolution cold with only one serving connection available after the request reaches this section, and show fresh decision proceeds without waiting for a second slot. Keep scope limited to the relevant acquisition boundary; do not add synthetic host load or a broad benchmark. No executed constrained-pool reproduction is claimed in this review.

## Reconciled acceptance and limits

| Original obligation / constraint | Reconciled evidence | Result |
|---|---|---|
| REQ-010a closed action, queue_only, no approval duration | Unchanged R1 validated authoring/registration/contract evidence | PASS carried |
| REQ-010b/016 reached accepted identity while peer continues, stable occurrence, no pending SQL | New reached/RunState/inspection source + focused engine and held-peer journey assertions, root observed green | PASS |
| Current occurrence/stale refusal | Accepted disposition reused at inspection and fresh boundary; discarded journey root observed green | PASS |
| REQ-010/011 actual original expiration on fresh decision | Ready-target authored-timeout journey green; erased refused target still widens ceiling | FAIL FIND-008-3 |
| Actor provenance/exact actor replay | Production fixed; required real-exchange persisted receipt/inspection proof absent | FAIL FIND-008-4 |
| Tenant/RBAC/audit, canonical first-wins/ACK, unavailable versus absent, accepted replay | Unchanged custody/authorization evidence + green direct/unavailable/restart approval journeys | PASS carried and checked at changed call sites |
| Supported cold target loading | Same-pool nested acquisition introduced | FAIL FIND-008-8 |
| Same ID/settled work/spend/authority, all-blocked park and wake recovery | Same state/scheduler owners, changed early-decision reread, prior recovery/handoff evidence + root green focused assertions | PASS within scoped proof |
| Accurate public owner and mandatory changed Rust docs | Correct approved/rejected spelling; Panics docs; changed owner methods/tests carry intent and Errors/Panics where relevant | PASS |
| Public error redaction | Diagnostic discarded at typed helper, safe fields only, unavailable/retry test green | PASS |
| INV-001–007/non-goals/public language projections | Existing owners extended; no new lifecycle store, scheduler, auth restriction, expiry clock, public API/schema or UI | PASS within carried scope |

Wait inventory is validated: reached's awaited host callback performs only short memory mutation and existing background notification; no pending-step SQL/Scribe/storage/Vault wait. Fresh decision adds a sanctioned cold target SQL read, but unnecessarily nested acquisition is FIND-008-8. Background queued runner adds an accepted-decision reread through existing system query/fence/Bifrost owners before park; read failure falls back to existing bounded recheck and never authorizes absent data. Existing background park ACK and lease custody remain. No new direct-verify SQL/ACK wait or direct storage/Vault wait was found.

Root observed 13/13 focused tests, fmt:check and diff check green; [verification.md](verification.md) records exact commands, run IDs, skipped lanes and the disclosed fmt/Cargo serialization caveat. These tests do not exercise refused-target expiry, constrained-pool resolution, or real exchanged approval custody. The R1 remediation expressly leaves clean full L1 comparison and broad integrated language/gate qualification to change review; none is claimed here. Runtime restart proof is not a complete server/Scribe process restart.

All retained corrections fit existing approved behavior and owners. No new public/security/concurrency/persistent-data decision or specification revision is needed. This is the first unsuccessful remediation review; the mandatory two-unsuccessful-remediation convergence assessment is not yet triggered. Next scope remains these three bounded causes plus affected paths, not a renewed whole-task audit.
