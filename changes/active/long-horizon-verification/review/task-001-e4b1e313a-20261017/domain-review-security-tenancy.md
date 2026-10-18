# Security and tenancy domain review

Result: **FAIL**.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
Base: `ef3a57b07`; candidate: `e4b1e313aacab50d7eb0f0a2060b8547bf19a632`.
Reviewed the cumulative range, not only the last implementation commit. HEAD remained the candidate during this review.
Authority: approved revision 13 of `changes/active/long-horizon-verification/spec.md` and original `tasks/TASK-001-grouped-verification.md`. The spec header discrepancy is being handled by the orchestrator; this report relies on the approved requirements' text, not an inference of a new approval.

Boundary: verified request identity → no-IO owner attribution → same-owner pinned source registration → tenant-qualified discovery/snapshot reads → accepted group execution authority → check-Agent tools and execution inspection. TASK-002 schedule-binding inputs and TASK-003 outbox admission/pressure/progress are excluded. No source or tests were changed and no tests were run by this reviewer.

## Authority and source coverage

| Boundary | Governing authority | Inspected source and consumers | Assessment |
|---|---|---|---|
| Authentication and owner stamping | AGENTS §§2/9; security posture Security principles, Principal and credential lifecycle; REQ-001/014 | `wyrd-runtime/src/principal.rs::owner_service_uid`; `components/auth/caller_extractor.rs::Caller`; `components/verification/service.rs::{execute,enqueue,stage_direct}`; `verification/results.rs`; `verification/runner.rs::result`; SDK Observe/wire additions | Owner comes from the verified effective principal, only a resolved Service Card, without parent lookup or caller attribution fields. Agent, unbound and human identities remain ownerless. |
| Source registration and identity isolation | REQ-013; INV-003; AGENTS tenant and exact-version rules | `components/cards/resolve.rs::{EffectiveSpecs,source_binding}` and new source/cycle/ambiguity validation; `components/cards/service.rs::BindingProjector::{collect,group_sources}`; typed `GroupSource`; `groups.rs::Sources` | Source membership pins the same occurrence's exact subject and Verifier UIDs. Runtime filter requires matching owner, subject, Verifier and non-null group. Typed/escaped literals avoid treating group text as SQL. |
| SQL isolation and activation | Agent rules TenantConn/RLS; security posture trust boundaries | `verification/scheduler.rs::poll_groups`; `wyrd-sql/src/queries/verifier_executions.rs::{claim_due_group,group_states,selected_results,refused_submissions,enqueue_group}`; binding/execution migrations; `BackgroundAuthority::resolve`; issuer `recover` | New tenant queries use TenantConn; caller commits. Owner authority is recovered from tenant-owned current grants and frozen. Outstanding-group index includes tenant. No new raw pool or payload-selected tenant boundary found. |
| Discovery and recorded payload reads | REQ-010, INV-003; security posture SYSTEM read limitations and accepted authority | `groups.rs::GroupPoller::poll`, `BindingPoll::{candidates,identities,snapshot,payloads,run}`; `authority.rs::SystemReadAuthority::{resolve,serving}`; `query/scheduled.rs::{authenticated,new,dispatch}`; `query/service.rs::{QueryAuthority,oracle_context}` | **FAIL: SEC-001 below.** |
| Agent live span reads | REQ-002/010; existing tool and Bifrost permissions | `vala-verify/src/blocks/agent.rs::AgentExecutor::invoke`; toolset seam; `verification/hosted/tools.rs::{HostedTools::open,Invocation::call}`; `hosted/traces.rs::CallerTraces`; runner `Caller::accepted`; `hosted/mod.rs::statement_caller` | Check calls allow only declared bifrost.query and no workspace. Tool invoke is checked before bounded query; underlying query retains accepted effective principal and SYSTEM only as actor. No new tool bypass found. |
| Inspection / agent surfaces | REQ-010/011; MCP explicit write scopes; TenantConn/RLS | `components/verification/service.rs::{get_binding,get_execution,read_result}`; execution status projection; MCP grouped journey and SDK Rust/Python/TS journeys; OpenAPI schema test | Metadata uses cards:read under tenant SQL; result payload read uses authenticated ordinary Bifrost caller. MCP added grouped tests cover unscoped enqueue refusal and foreign tenant execution not-found. |

Read applicable repository rules, reference router, spec-driven development and maintainer guidance, security posture, doctrine architecture constraints, telemetry observation and agent-harness guidance; compared source under the protocol and Bifrost authorities' tenant/query ownership rules. CodeGraph was used before source search and expanded for group/authority/tool owners.

## SEC-001 — Group discovery and snapshot manufacture read authority independent of the accepted owner

Classification: **VIOLATION** (security / accepted authority). Blocking.

Violated obligations: specification REQ-010 (`spec.md:524–528`) requires existing authorization on every evidence read, states a group reference cannot widen the accepted execution's permission to another table or payload, and requires existing permissions on sensitive recorded inputs/results. INV-003 preserves accepted authority for injected reads. The security posture explicitly limits the credentialless SYSTEM table-read exception to scheduled Drift's `vala.drift.observations`; SYSTEM adds no authority to agent-check calls or other accepted work.

Exact changed location: `crates/wyrd/wyrd-server/src/verification/groups.rs:130–150`, particularly `SystemReadAuthority::resolve` over `ExecutionInputsTable` and `ResultsTable` and subsequent `ScheduledQueryCaller::new`.

Evidence and reachable producer-to-consumer path:

1. `VerificationScheduler::poll_groups` (`scheduler.rs:276–288`) resolves the active binding owner with `BackgroundAuthority::resolve`. The latter calls `TenantTokenIssuer::recover`; that recovers the owner's current permissions and stores them in the `ExecutionAuthority`. A Service can remain active with verifier/model permissions while its Bifrost grants are revoked or scoped to another table. Removing a query grant does not suspend that principal or eliminate binding activity.
2. `GroupPoller::poll` does not use those recovered permissions for any source statement. It calls `SystemReadAuthority::resolve` with the SYSTEM actor and the two verification tables. `authority.rs:104–124` manufactures fresh BifrostQuery/Read table-UID grants, then builds a SYSTEM `Principal` containing them.
3. `.serving(authority.authority())` changes served-subject attribution and delegation history only (`authority.rs:178–189`); it neither intersects nor checks the owner's grants. The `effective_permissions.is_empty()` branch therefore tests the manufactured table grants, not the owner authority. Registered tables make it proceed regardless of the owner's read permission.
4. `ScheduledQueryCaller::new` holds no authenticated caller. Its `dispatch` uses `state.bifrost.query_sql(self.context.clone(), ...)`, so Oracle authorizes SYSTEM's new grants. All discovery, identity and payload statements share that context. `payloads` selects original `inputs` and `outcome`, projects the selected values, and `snapshot` pins them into group inputs.
5. `enqueue_group` stores the snapshot alongside the separately recovered owner authority. `VerifierRunner` then executes those already-read values as the owner. The missing grant can no longer protect the read: sensitive source text is already available to assertions or a model judge, and can influence a result/Operator. Correct owner/tuple/tenant filters prevent mixing owners but do not substitute for table/payload authorization.

Sibling consumers and existing mechanism: result inspection (`VerificationControl::read_result`) and trace evidence (`CallerTraces::fetch`) construct `ScheduledQueryCaller::authenticated` from the accepted caller, retaining normal capability admission, Oracle object authorization and audits. Check-Agent `bifrost.query` uses the same accepted caller through `BoundedQuery`, checking tool permission separately. `DriftEngine::authority` is the other production `SystemReadAuthority` consumer, and its explicitly approved narrow observation-table exception must remain intact. No change to producer stamping, SQL source identity selection, outbox, or Drift authorization is needed.

Observable consequence: an active owner denied read access to `vala.verification.execution_inputs` or `vala.verification.results` can still have group judgments consume those protected recorded values. A scoped grant to unrelated tables also cannot confine this path. The query's audit will reflect the manufactured SYSTEM authority, not an ordinary denial under the owner grants.

Smallest testable correction: remove the manufactured SYSTEM source-read authority from the grouped path and reuse the existing authenticated scheduled-query mechanism with the recovered accepted owner caller and SYSTEM executor attribution. Apply this consistently to discovery, identities and payload snapshots so denied reads cannot enqueue partially authorized evidence. Keep the exact tuple/group filtering, bound/read settlement, background cursor retry behavior, and scheduled Drift's separate approved SYSTEM authority. This is an implementation correction under the existing approved permission boundary, not a request for new permission types or a new security policy.

Focused closure proof: one real-server grouped journey variant should retain active owner identity plus invocation/model grants, deny the owner's verification-table read (both no query capability and a scoped grant excluding a source table are relevant), and assert no protected source values reach a group snapshot/judge or yield a successful protected judgment. Verify the ordinary audited denial. Then grant the required existing source-table read and show the same eligible group can be judged on a later tick. Exercise input-slot and requirement-outcome reads without introducing a new harness. The current live-query denial scenario proves a different consumer and cannot close this gap.

## Verification limits

Task-appended green commands and selected counts are implementer claims, not independently executed evidence in this domain pass. This reviewer inspected the new Rust grouped coding journey's grant/revocation path (`verify_in_real_time.rs:1668–1687`, `1821–1854`): it proves denial of the Agent's live span tool, while the group input/results snapshot remains authorized through SYSTEM. The MCP negative tests concern producer invocation scopes and cross-tenant inspection, also different boundaries. No new tests were run, in accordance with the orchestrator's sequential Cargo coordination.

No additional material security/tenancy findings were identified. This domain result does not substitute for task acceptance, persistence/concurrency, standards, or independent findings validation.
