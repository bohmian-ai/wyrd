# Security, RBAC, and tenancy domain review

Result: **FAIL**. Immutable subject: base `f2a726c90edbf8956c7b91c843379175ee20eac0`, candidate `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`. HEAD still equals candidate. No implementation edits.

## Boundary and authority coverage

Reviewed authenticated HTTP/MCP verification admission, evidence upload/read namespaces, accepted authority through acknowledgment and queue persistence, hosted Agent tools and model/Card/query reads, tenant isolation, and Oracle subject/actor attribution. Authority: AGENTS.md §§2/3/9/11, architecture/agent-rules.md (RLS, nonblocking audit and verified identity), architecture/wyrd-security-posture.md (workload scope, accepted authority, delegation), architecture/wyrd-design.md (signed Card scope), approved spec r8 REQ-007/010–013, INV-003/004, AC-005/006, original TASK-005 and its material stops. Reference router and architecture-constraints, patterns, agent-harness, evaluation, spec-driven-development, maintainer-style references were consulted. CodeGraph was the first code-location mechanism.

| Surface | Sources and actual path inspected | Assessment |
|---|---|---|
| External admission | components/auth/caller_extractor.rs; components/verification/service.rs and routes.rs; mcp/verification.rs | Caller derives tenant and principal from verified credentials. HTTP and MCP share control owner. Exact verifier permission is checked/audited; queued subject scope is missing (SEC-01). |
| File planning/upload | service.rs evidence/upload_local_evidence; wyrd-storage service.rs evidence_upload_plan/upload_local_evidence; tenant_path; wyrd-client verify/mod.rs pin and storage upload dispatch | Tenant comes from caller, digest/body bounds checked, storage key tenant-qualified. Exact-verifier-only callers cannot prepare files (SEC-02). Local upload introduces public route outside approved list (SEC-03). |
| Queue handoff | verification/submissions.rs; wyrd-sql queries/verifier_executions.rs Submission/accept/insert/ResolvedTarget; runner.rs dispatch | Accepted principal and chain retained, no credential; RLS tenant connection; runner calls Caller::accepted with SYSTEM attribution, not SYSTEM permissions. Availability check does not repair missing signed scope. |
| Protected tools | hosted/blocks.rs, tools.rs Invocation::call, registry.rs, evidence.rs; Card route helpers; query/service.rs, scheduled.rs; gateway caller/invocation | Exact tool invoke decision precedes dispatch; underlying Card, catalog, query and gateway owners retain authorization. Workspace tools operate pinned capabilities. No tool grant inferred from Agent declaration. Existing owners are extended rather than duplicated for domain authorization. |
| Evidence consumers | TenantEvidence::read/artifact/stored; hosted/traces.rs; blocks engine | Artifact uses authorized registry/inventory; file hashes/lengths/sniffed format checked; traces use caller's ordinary Oracle authorization. No cross-tenant arbitrary object path accepted. |
| Reserved storage and audit | Bifrost gate/mod.rs diff; oracle/mod.rs AuthorizedQueryContext/query_audit_event; query/service.rs oracle_context and denial path | Public ingest refuses execution_inputs along with result tables. executed_by is attribution-only and survives forwarding; effective principal remains permission authority. Per-statement request IDs prevent concurrent query identity collision without replacing subject/history. Shared Scribe audit path retained. |
| Proof | pg_verification_routes.rs; hosted evidence/tool tests; supplied gate and SDK journey inventory | Existing refusal tests cover no verifier grant and unrelated verifier UID. Direct test deliberately preserves base scope exception. Queued out-of-scope subject and scoped-grant file upload are not covered. |

## Proposed findings

### SEC-01 — REGRESSION: queued verification no longer enforces signed subject Card scope

**Obligation:** INV-003 preserves signed Card scope; security posture says workload Card scope bounds which Cards it may emit for and verify. Base manual queue admission explicitly checked it. This finding concerns background submission, not the pre-existing direct-execution scope exception.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:569` and `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:1153`.

**Evidence and reachability:** Base `VerificationControl::start_run` checked `subject_in_scope` after exact verifier permission; base helper used `Principal::card_attribution` and `authorizes_card` and returned a denied audited decision for a bound workload outside its signed scope. Candidate enqueue checks only `verifier:run`, then snapshots authority and stages inputs. Its queue writer resolves Cards/readiness in the tenant, but `ResolvedTarget::refusal` checks only readiness and subject availability. Runner::dispatch builds Caller::accepted from that snapshot and runs the supplied JSON/ordinary assert Verifier; this path needs no later protected tool read that could happen to deny it. A Service/Agent with workload's `verifier:run` can submit against another active subject Card in the same tenant outside its signed scope. ResultRun then attributes retained verification to that unauthorized subject. RLS confines this to a tenant; it does not enforce the narrower signed scope.

**Smallest correction boundary:** Restore the queued-admission scope invariant once at VerificationControl::enqueue before staging. Reuse `Principal::card_attribution` and signed `CardRefScope::uids()` to compare the requested exact subject UID for a scoped principal, preserving AnyRegistered behavior for humans/admins/unbound services. This requires no Postgres request write or read and no new scope model. Record the resulting allowed/denied admission decision on the existing Scribe audit stage. Preserve exact verifier permission checks, accepted principal/delegation, deferred target availability/replay decisions, the direct execution behavior, and scheduling authority. Do not add downstream checks in every tool, runner, or storage consumer.

**Focused closure proof:** Existing real route/SDK fixture with bound workload A, same-tenant unrelated active subject B, and a JSON assert verifier: enqueue B is refused with stable RBAC code and denied audit, stages no inputs/queue work, and invokes no model; A or an in-scope component succeeds. Retain an unbound principal case. Exercise the shared control through the existing HTTP/MCP owning tests rather than introducing a new harness.

### SEC-02 — INCORRECT: exact verifier grants cannot use the required File workflow

**Obligation:** INV-003's exact `verifier:run` rule, REQ-010 shared file upload workflow, REQ-011's equal Run entry behavior, and first-class file journeys. Exact verifier UID permission is an expressly supported authorization mode, not a request for a new role.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:326` and `:403`.

**Evidence and reachability:** Both evidence planning and local upload ask the normal RBAC checker for `Permission::verifier_run_any()`, which means an all-Verifiers grant. RbacCheck uses PermissionSet::contains/covers; an exact UID grant cannot cover All. By contrast, execute/enqueue ask for `Permission::verifier_run(request.verifier_card_uid)`. The shared client `verify/mod.rs:517–535` always calls evidence planning when pinning local File inputs and uploads missing files before executing/submitting. Thus a principal authorized for verifier V, with otherwise sufficient Card reads, can verify inline input but its File workflow fails 403 before V executes. MCP's `may_run_verifiers` already intentionally exposes these write tools to an exact UID grant using `covers_operation`.

**Smallest correction boundary:** At generic evidence planning/local-upload admission, reuse the existing operation-level any-verifier-grant decision represented by PermissionSet::covers_operation(Resource::Verifier, Action::Run), as MCP already does. Stage its allow/deny decision through the existing audit path. Keep the exact selected verifier check at execute/enqueue and all tenant/digest/size/storage checks. This operation holds no requested verifier identity, and granting access to file preparation for a caller with one verifier grant does not grant invocation of any other verifier. Do not require granting all verifier UIDs or adding verifier identity to the file wire contract solely for this gap.

**Focused closure proof:** With only V's exact verifier grant plus necessary Card/domain reads, use the real SDK/server File workflow for verify and submit, including a missing file on the local backend; ensure planning/upload succeed, V completes, another verifier remains denied, and a principal with no verifier grant still receives an audited 403. Add the equivalent narrow planning check for cloud-plan behavior using existing storage fixtures where applicable.

### SEC-03 — DRIFT / material decision: local upload is a new authenticated public API

**Obligation:** TASK-005 material stop forbids a new public API; approved REQ-012 enumerates evidence planning/execute/enqueue/status and preserves binding read. A material public boundary change requires approved spec revision under the review skill.

**Location:** `crates/wyrd/wyrd-server/src/components/verification/routes.rs:165` (`PUT /verification/evidence/local/{sha256}`), mounting at :50–51, and `crates/wyrd/wyrd-storage/src/service.rs:518`.

**Evidence:** This is an externally invokable, OpenAPI-published authenticated write route with a newly selected digest identity and authorization behavior, generated in evidence plans and consumed by the shared SDK storage client. It does not use the existing Card upload capability ID and metadata lifecycle. Reusing UploadPlan::LocalFs's enum shape does not make the receiving route private or previously approved. The implementation diagnosis correctly shows why file:// PUT cannot satisfy local SDK file uploads, but necessity does not waive the material stop.

**Required outcome:** Obtain a spec revision explicitly approving the local evidence transport/public boundary and its authorization contract before accepting this route. The existing Card upload owner was considered: it is keyed by a minted upload ID with upload metadata, contrary to REQ-010's metadata-free evidence plan, so mechanically substituting it is not a bounded private correction. Do not prescribe a new capability or retrofit the Card upload lifecycle without approval. If the owner elects to keep current behavior, approval should name the route and preserve tenant/digest/bounds checks and the scoped-grant correction in SEC-02. This finding is a spec-revision decision, not an instruction to delete local support and break the required File workflow.

**Closure proof after approval:** Served OpenAPI and existing local File journey plus unauthenticated/no-grant, digest mismatch, oversized body and tenant separation cases. The existing integration test publishing the sixth route proves exposure, not approval.

## Proof and limits

Static cumulative review only; relied on supplied passing `mise run gate` and source of the relevant route/journey tests, without rerunning the broad gate or live providers. The review found no new cross-tenant query/object escape in the inspected paths; this does not excuse narrower same-tenant Card scope regression. No secrets or real-provider credentials were used. No optional hardening or unrelated debt is proposed. Findings remain proposals for the mandatory independent validation pass.
