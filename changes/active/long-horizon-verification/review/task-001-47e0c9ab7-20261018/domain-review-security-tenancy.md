# Security and tenancy domain review

Result: **PASS** (static domain review; runtime verification limits below).

## Subject and boundary

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
Base: `ef3a57b07`; immutable candidate: `47e0c9ab7a5cf91cb8d292bdd96a86bdd966a3f9`.
HEAD was checked at the start and end and remained the candidate. Reviewed the
cumulative base-to-candidate change, using the preceding candidate only to
navigate remediation. No source or test edits and no tests were run.

Authority is the human-approved revision 13 of
`changes/active/long-horizon-verification/spec.md`, original
`tasks/TASK-001-grouped-verification.md`, and the preceding review's validated
findings/remediation. The draft revision 14 header is not approval of a new
exception. The accepted newest-active discovery page ceiling and in-finality
discovery replace the former scan-progress prescription. TASK-002 schedule
inputs and TASK-003 staging bounds/pressure/progress are excluded.

Reviewed boundary: authenticated effective principal → no-IO owner stamping →
same-owner exact source registration → tenant-scoped discovery and frozen
payload reads → accepted execution authority → check-Agent tool invocation →
result inspection. Also reviewed the new shared artifact-manifest validator
and authoritative server validation before registration writes.

## Authority and source coverage

Read AGENTS.md, agent rules, the reference router, spec-driven-development and
maintainer guidance; the protocol design, doctrine, Bifrost design and security
posture; architecture constraints, agent-harness, evaluation, telemetry and
testing guidance. CodeGraph was used first for navigation and expanded for the
authority, scheduled query, caller and tool paths.

| Boundary / obligation | Source evidence and consumers | Assessment |
|---|---|---|
| REQ-001/014: authenticated owner, no producer identity lookup | `wyrd-runtime/src/principal.rs:196–217`; `components/auth/caller_extractor.rs::Caller`; `components/verification/service.rs:580–650,stage_direct`; `verification/results.rs::ResultRun`; `runner.rs::result` | PASS. Only a resolved Service principal's verified Service Card UID is copied. Request fields carry no owner override. Agent, human, unbound and SYSTEM identities remain ownerless; no parent lookup is added. Enqueue remains authorization plus in-memory staging with no new SQL read/write. |
| REQ-013 / INV-003: registration resolves declared sources within the owner and exact Card versions | `components/cards/resolve.rs::EffectiveSpecs::{resolve,validate_binding,validate_sources,reaches,load}`, `GroupSite::ambiguous`, `source_binding`; `components/cards/service.rs::BindingProjector::{collect,group_sources}` | PASS. Source names resolve to exactly one binding at the same site; slot/requirement existence, cycles and ambiguous recorded tuples are refused. Projected sources freeze exact subject and Verifier UIDs from the tenant registry. No cross-owner or name-only runtime lookup replaces these pins. |
| INV-003 / REQ-011: accepted owner grants govern every evidence read | `verification/groups.rs:119–150`, `BindingPoll::{candidates,counts,identities,snapshot,payloads,run}`; `verification/authority.rs::BackgroundAuthority::resolve`; `query/scheduled.rs::{authenticated,dispatch}`; `query/service.rs::{QueryAuthority,oracle_context,stream_query}` | PASS. Recovered owner authority is retained for all source statements. Query capability admission and Oracle object authorization apply, with ordinary denied/allowed audit ownership. SYSTEM is executor attribution only. See prior finding closure below. |
| INV-003: tenant, owner, source-tuple and group isolation | `groups.rs::Sources::{new,any,surfaces,identities,payloads}`; `ObservationWindow::text`; `wyrd-sql/src/queries/verifier_executions.rs::{claim_due_group,group_states,selected_results,refused_submissions,enqueue_group}`; binding/execution migrations | PASS. Tenant SQL uses TenantConn and caller-owned commit. Bifrost context carries the same principal tenant. Source reads combine owner UID, exact subject/Verifier pair and group; payloads further restrict selected execution IDs and finality. SQL literal escaping applies to opaque group text. Tenant-qualified outstanding index and RLS are retained. Cross-tenant queue discovery remains OperatorPool. |
| REQ-002/011: live span analysis retains ordinary tool and payload permissions | `vala-verify/src/blocks/agent.rs::AgentExecutor::invoke`; `card/verifier/validate.rs` check-Agent validation; `verification/hosted/tools.rs::{HostedTools::open,Invocation::call}`; `query/collect.rs::BoundedQuery`; `hosted/traces.rs::CallerTraces` | PASS. Both static and runtime checks permit only declared `bifrost.query` for a check and prohibit its workspace. Tool invoke is authorized/audited before query dispatch; BoundedQuery retains the accepted caller, ordinary read authorization and bounded results. Existing action tools keep their invocation workspace and deadline. Trace sources contribute the recorded trace ID; they do not implicitly fetch spans. |
| REQ-011: headless inspection and write-scope protection | `components/verification/service.rs::{authorize_run,get_binding,get_execution,read_result}`; `VerificationExecutionState`; Rust/Python/TS Observe/verify projections; MCP verification journey; OpenAPI contract test | PASS. Group identity is correlation, not authority. Existing verifier-run checks precede execute/enqueue. Metadata reads retain cards permission plus tenant SQL; retained results use the authenticated query path. MCP's unscoped enqueue and foreign-tenant inspection refusals exercise these boundaries. |
| Server registration is the authority even when a client bypasses the loader | `components/cards/service.rs::validate_request` (1550 onward); `wyrd-spec/src/registry/submission.rs::validate_artifact_manifest`; `wyrd-loader/src/validate.rs::check_duplicate_artifact_paths`; `pg_card_registration_route.rs::{conflicting_artifact_paths_are_refused,composite_with_manifest_rejects_before_writes}` | PASS. Server validates decoded Verifiers and shared duplicate/nested-path rules before database registration. Loader reuses the same pure rule for early diagnostics. Multi-submission artifacts remain prohibited, so replacing request-wide loader path tracking with per-manifest checking does not create a cross-submission publication path. Existing RelativeArtifactPath validation owns path grammar. The existing catalogued artifact-path error is reused. |

## Prior finding closure

**FIND-TASK-001-5 (SEC-001 / FUP-002): closed in source.** The prior group path
manufactured SYSTEM grants for `execution_inputs` and `results`, independently
of recovered owner permissions. `GroupPoller::poll` now constructs
`Caller::accepted(authority.authority(), authority.attribution().act_principal_id)`
and `ScheduledQueryCaller::authenticated`; there is no remaining grouped-path
`SystemReadAuthority::resolve` call. All candidate, count, identity and payload
queries run through this one reader. Its dispatch reaches ordinary
`stream_query`, whose capability denial and object denial are audited through
`QueryAuthority`, while successful immutable visibility decisions remain
Oracle-owned. `Caller::accepted` retains the effective principal and delegation
chain and changes only executor attribution. The scheduler still recovers the
binding owner's current grants through `BackgroundAuthority` before polling
and freezes them in queued work. Scheduled Drift's separate approved SYSTEM
read exception remains unchanged.

The added real-server Rust journey
`verify_in_real_time::grouped_reads_need_the_owner_grant` (line 2800) retains an
active owner with producer grants, first removes query capability, then grants
query access scoped only to `vala.traces`, and asserts no review. It checks a
retained owner query denial, restores the ordinary workload role and verifies
the next review's complete input and outcome lists. This directly targets the
prior consumer's trust boundary, unlike the earlier live-tool denial alone.
Its fixture adds no replacement query or authorization implementation.

**FIND-TASK-001-7, security-relevant authority synchronization:** current
evaluation/protocol guidance permits the approved check-Agent live-query tool
while retaining ordinary tool/query grants and the no-workspace restriction.
No additional tool exception was inferred from draft revision 14.

## Verification evidence and limits

Inspected the original grouped journeys and their Rust, Python, TypeScript and
MCP negative paths: same group across tenants/owners and a newer source version,
ownerless calls, denied producer invocation, unscoped MCP enqueue, foreign-tenant
execution inspection and live-query/tool grant denial. Read the new owner-grant
journey and direct HTTP registration manifest-refusal test. The latter bypasses
the loader and asserts the stable public error; the existing request-validation
ordering establishes refusal before registration writes.

No tests or Cargo lanes were run by this reviewer, per the orchestrator's
sequential verification coordination. Appended task/remediation command results
are retained implementation claims, not independent execution by this report;
root reruns are forthcoming. The domain PASS means no source-backed material
security or tenancy defect remains in the inspected boundary. Final task
acceptance still requires the orchestrator's immutable-subject check and
verification reconciliation.

## Material findings

None. No speculative hardening or additional permission model is requested.
