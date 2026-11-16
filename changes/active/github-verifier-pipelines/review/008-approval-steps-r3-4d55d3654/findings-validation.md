# TASK-008 round 3 independent findings validation

Recommendation: **PASS**. Independently validated open finding ledger: **empty**. No remediation task is required.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`. Original cumulative base: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`; prior candidate: `cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`; immutable cumulative candidate: `4d55d3654ab7361b97aeadbb72b8813707515d49`. Independently checked HEAD against that candidate. Review delta is `git diff cac4a4ce3 4d55d3654`; original task and full cumulative range remain acceptance context. No source changed during this validation.

Read the original task, its Obligations, approved spec REQ-010/011/015/016/017, Human approval contract, r1/r2 verdict and source-backed acceptance/closure evidence, R2 remediation and validation, current round scope and every selected discovery report, claim comparison and verification. Applied the task-review skill, root rules, maintainer guide and applicable owner contracts. CodeGraph preceded source navigation.

The current maintainer decision is governing authority: caller Card scope is attribution only; verification authorization is RBAC plus audit; approval requires only verifier:approve on the exact Verifier; users and service accounts are approved actors. Prior requirements and proof of Card-scope authorization are superseded. The current authority edits and replacement tests implement that decision; they are human-directed corrections, not new independently discovered FIND IDs.

## Independent coverage validation

| Contract or invariant | Required evidence | Validation |
| --- | --- | --- |
| Three remaining R2 closures and changed caller paths | Fresh remediation discovery, task-review-behavior.md | Complete. Matrix covers original obligations, closure and affected callers; source and connected assertions checked independently below. |
| Decision target refusal, original duration and pool ownership | domain-review-data.md | Complete. Covers resolver producer/consumer, SQL clock, existing Db::Conn owner, replay ordering and custody/queue adjacency. |
| Actor attribution, exact permission, tenant and audit boundary | domain-review-security.md | Complete. Covers verified actor through receipt/storage/inspection/replay, effective-principal authorization, exact Verifier, tenant binding, service callers and HTTP/MCP dispatch. |
| Cross-tier repository and projection rules | standards-review.md | Complete. Maps Rust service/tests/client/error, MCP, Python stub, TS documentation, owner text and generated error projections to rules, including changed-item rustdoc. |
| Unchanged shared state, lifecycle, failure/recovery and maintainability | r1 invariant/system/maintainer/standards reports and r2 data/validation/verdict | Carry-forward is justified by evidence, not merely unchanged filenames. Prior reports trace stable occurrence identity, asynchronous sole-state ownership, accepted disposition, Scribe ACK/fence, bounded wake/restart, supersession and thin SDK ownership. Current delta does not change those algorithms, durable layout or runtime topology. |

Separate invariant and system reviewers are not required anew: the coherent data specialist covers the changed admission lifecycle, resource hold and deadline path; r1/r2 supply source-backed unchanged engine, state, custody, queue and recovery coverage. Separate maintainer is not required anew: current changes simplify or extend existing owners without introducing an abstraction, dependency, transport, graph, cache or lifecycle owner; fresh discovery and standards cover changed shape/docs and prior maintainer report covers retained machinery. Repository and security specialists were selected because the RBAC correction reaches multiple production callers and public projections. No required specialist is missing.

All four discovery reports existed before this fresh validation began. Agreement did not substitute for source inspection. There is no material disagreement or untraced common cause requiring followup-rev, and validation discovered no new blocker requiring separate source validation.

## Independent closure ledger

| Prior finding | Source trace and observable closure proof | Disposition |
| --- | --- | --- |
| FIND-008-3 | `service/approvals.rs:129–150` distinguishes SQL failure, inner target refusal and resolved optional authored timeout. A refusal returns existing typed VerificationTargetNotFound before queue-open or custody. Only a resolved target produces min(authored,runtime). Existing SQL OPEN predicate compares original created_at plus bound duration with statement_timestamp(), not a new clock/deadline. Accepted identical replay at :110–113 precedes fresh resolution. `pg_approvals.rs:796–841` stops the worker, deletes the real Repository through normal HTTP, asserts target-not-found and null actor/decision time. Existing authored-timeout journey separately proves early acceptance and late refusal under the original shorter duration. Both ran green this round. | CLOSED |
| FIND-008-4 | Receipt creation `approvals.rs:162–177` stamps verified attribution act_principal_id; shared replay :224–236 compares actor, status and reason for retained and concurrent-winner paths. The new journey :912–969 uses ordinary bootstrap_service, exchange_rooted and delegate owners, accepts a real pending occurrence, asserts Service actor in receipt and later inspection, complete retry equality, and typed409 for direct user and another Service sharing the user. No manufactured Caller/receipt substitutes for the connected proof. It ran green this round. | CLOSED |
| FIND-008-8 | Fresh admission `approvals.rs:118–158` owns one TenantConn and passes Db::Conn(conn.transaction()) to existing resolver. Cache load :291–302 directly reads through that connection; the Db::Pool acquire arm is not selected. Retained's prior transaction commits before canonical state reading (`inspection.rs:461–476`); fresh admission commits before custody/wake. `pg_approvals.rs:855–898` restarts to empty cache, holds every app slot except one and asserts actual decision acceptance while held slots remain. It ran green this round. | CLOSED |
| FIND-008-1/2/5/6/7 | R2 verdict and independent validation provide source-backed closure of reached pending publication, withdrawn occurrence refusal/history, changed MCP Panics rustdoc, approved/rejected owner spelling and public error redaction. Current delta does not alter these producers/algorithms. Actor/action permission changes were independently checked through retained/decide and preserve their state/custody constraints. Existing current approval journeys still exercise withdrawal, ACK unavailability/retry, restart and supersession. | CLOSED, carried forward |

The correction boundaries meet the Ponytail ladder. Existing Db::Conn removes the nested resource hold without a new pool policy; preserving the resolver's existing inner result closes deadline fallback without storing another deadline or reading through a second target owner. Fresh target refusal belongs at decision admission because accepted replay must remain eligible independently. The actor correction reuses existing verified attribution and replay; the missing proof now composes existing identity facilities. No further machinery or correction burden is justified.

## Reconciled acceptance matrix

| Original obligation / constraint | Independently reconciled implementation and proof | Result |
| --- | --- | --- |
| REQ-010a closed action/message, queue_only, no approval-specific duration | R1/R2 source-backed enum/validation/declaration evidence; those producers/contracts unchanged | PASS carried |
| REQ-010b stable occurrence, no pending SQL, independent work and park | Prior engine/state identity and handoff evidence; current delta changes admission only, not reached/sole writer; closed FIND-008-1/2 preserved | PASS carried |
| REQ-010c normal authentication, only exact-Verifier approve, audit and tenancy | `retained(Action::Approve)` preflights action then exact PermissionScope::Verifier via audit::authorize before occurrence or replay. Read-only Card read gate is skipped for approval; inspection callers still pass Read. Existing approve-only second-principal test retains wrong-Verifier/viewer/foreign-tenant and audit assertions. | PASS |
| Verification direct/enqueue/evidence RBAC-only | Shared authorize_verifier retains exact PermissionCheck and one allow/deny outbox audit while removing attribution authorization once for all three callers. Direct/enqueue negative tests retain denial before staging; MCP shares service boundary. | PASS |
| Current occurrence, original expiry, replay, actor and cold-load availability | Independent closure ledger above plus unchanged withdrawal/first-wins/ACK behavior | PASS |
| REQ-011 durable receipt, same execution, accepted authority, rejection semantics, restart/ACK-to-queue repair | ApprovalDecisions::accept retains fenced staging and explicit ACK before positive receipt; wake remains best effort with bounded park repair. Current delta does not add a state writer or replace scheduler; source-backed R1/R2 proof and fresh restart/supersession/authority tests apply. | PASS |
| REQ-016 inspection/SDK/CLI/MCP/errors/docs and mandatory Rust docs | Prior thin projection evidence retained; current Rust/client/MCP/Python/TS changes are permission descriptions, no new signatures/fields/exports. Standards checked current rules, exact action dispatch and documentation. | PASS |
| INV-001–007 and non-goals | Same tenant-bound state/custody, no authority addition from receipt, no alternate login/proof/token profile/UI, new lifecycle or decision payload SQL store, deadline anchor or pending SQL | PASS |

The two unchanged publication sentences saying run permission on a subscribing Verifier “and its subject” do not establish a separate signed-Card-scope permission contract. The explicit settled decision and actual verifier-scoped permission model resolve that wording. No material implementation failure or mandatory contrary contract is demonstrated; it creates no finding or remediation work. No other rejected proposal is retained as advice.

## Verification and limits

The orchestrator's `verification.md` records observed run IDs and exact commands: 13/13 server tests (all approval journeys plus changed direct/enqueue and accepted-authority cases), 1/1 changed MCP journey, SQL migration setup, fmt:check and diff check passed. This validator inspected the actual assertions and source; it did not execute Cargo or infer success from test names. Current green tests are supplementary to source validation, not the only acceptance evidence.

Broader language/CLI suites, full lints/codegen/docs regeneration, broad aggregates and clean L1 benchmark comparison were not rerun. R1 remediation and R2 verdict explicitly reserve integrated qualification to change review, so this report does not claim full performance or whole-system qualification. No discovered failure was hidden with timeout/ignore/allow/test weakening. The user-directed removal of obsolete Card-scope authorization assertions is explained by approved current behavior.

No open findings remain; no new FIND ID, remediation task or specification revision is needed. This is a successful second remediation review, so no further failed-round correction or convergence scope is prescribed. The bounded fixes converge at their original owners and complete the remaining R2 requirements.
