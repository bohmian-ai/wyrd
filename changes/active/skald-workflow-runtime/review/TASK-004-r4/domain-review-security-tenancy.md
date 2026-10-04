# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: `TASK-004-R1-close-accepted-job-gaps.md`, `TASK-004-R2-align-revision-and-source-contracts.md`, and `TASK-004-R3-close-step-attempt-and-import-gaps.md`
- Review mode: static, cumulative base-to-candidate review. No build, test, Cargo, or mise command was run.

The candidate resolved to the requested commit at the start of review and immediately before this report was written. `.codegraph/` is absent, so immutable Git diff inspection, `rg`, and direct source reading were used.

## Reviewed Boundary

The cumulative change was traced end to end across:

1. default-deny HTTP authentication and construction of `Caller` from one verified tenant principal;
2. the typed `workflows:run` permission and built-in Role grants;
3. create/replay/get/cancel authorization, canonical audit, and lookup ordering;
4. accepted-job capture of principal attribution, permission/Card scopes, credential attribution, and verified delegation without retaining bearer or refresh material;
5. tenant- and effective-principal-qualified run ownership, idempotency, replay, cancellation, retention, and common not-found behavior;
6. exact active graph resolution through the authenticated tenant's `TenantConn`;
7. `cards.get` and `bifrost.query` closed inputs, per-call authorization/audit, tenant routing, bounds, and non-disclosing failures;
8. in-process gateway calls under captured authority while current gateway deployment, credential, capability, fallback, admission, and audit remain gateway-owned;
9. tenant-assigned external bindings, secret indirection, endpoint policy, reserved-header rejection, and credential non-disclosure;
10. authenticated second-tenant Workflow, run, Card, and Bifrost journeys; and
11. the R3 correction from `f17726fb25df1fa513875dca8d92f0073340ee0a` to the candidate, including its effect on externally visible step state.

The fixed human decisions were applied as authority and not reopened: deleted Oracle graph-drain polling and supervisor idle refusal stay deleted; follower release is grant-stream close without a leader acknowledgement; a foreign-tenant model step is not required because the harness seeds gateway credentials only for the fixture tenant; and a published `Running` step reserves attempt one and settles `Cancelled` if interrupted.

## Authority and Source Coverage

| Boundary | Governing authority | Source and recorded evidence inspected | Assessment |
|---|---|---|---|
| Authentication and tenant derivation | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` tenant/audit rules; `architecture/wyrd-security-posture.md:141-193`; spec REQ-032/032A and INV-022 | `http/router.rs:49-80`; `components/auth/caller_extractor.rs:11-74`; `wyrd-runtime/src/principal.rs`; recorded Scenarios 1 and 3 | **PASS.** Workflow routes are inside the authenticated `/v1` group. `Caller::from_authenticated` derives tenant, effective principal, permission/Card scope, non-secret credential attribution, and delegation from the verified result and contains no bearer, refresh token, API-key secret, or renewal handle. |
| Typed Workflow RBAC | Spec REQ-032; existing runtime permission owner | `wyrd-runtime/src/permission.rs:69-175,419-468`; `builtin_roles.rs:51-94,122-139`; recorded role evidence | **PASS.** `Permission::workflow_run()` reuses the established resource/action/scope model. Writer and agent receive it, admin covers it through wildcard, and reader/runtime_admin do not. No parallel role, policy engine, authorization cache, or compatibility permission was added. |
| Fresh create/replay/get/cancel authorization and audit | Agent rules canonical audit requirements; spec REQ-032/033/034C and AC-010 | `workflow/routes.rs:24-180`; `workflow/host.rs:60-204`; `audit/mod.rs:211-276`; recorded admission/audit-fault evidence | **PASS.** Every request authenticates afresh. Create and replay audit before idempotency admission; get/cancel audit before parsing and lookup. The canonical standalone audit transaction fails closed, and no terminal authorization event is invented when no permission is evaluated. |
| Accepted authority and token lifetime | `wyrd-design.md` accepted-job contract; `wyrd-security-posture.md:181-190`; spec REQ-032A, INV-022, AC-027 | `workflow/host.rs:230-399`; `gateway/workflow.rs:34-131`; `workflow/tools.rs:42-107`; recorded `accepted_authority_outlives_submission_only` journey at `pg_workflow_runs.rs:1699-1830` | **PASS.** The tracked preparation clones an already verified `Caller`, not credentials. Later grant changes cannot widen its immutable snapshot, while later HTTP requests authenticate and authorize independently. Current gateway credential/deployment eligibility is still evaluated per call. |
| Run ownership, replay, and IDOR resistance | Security posture tenant isolation/non-disclosure; spec REQ-030/032/034C, AC-018/021/022 | `workflow/host.rs:91-203`; `workflow/runs.rs:47-115,198-209,258-415`; recorded Scenarios 2 and 6 | **PASS.** Idempotency is keyed by verified `(tenant, effective principal, IdempotencyKey)`, and run get/cancel require the same tenant and principal. Unknown, malformed, foreign-tenant, foreign-principal, expired, evicted, lost, and non-owning-replica identifiers share `WYRD_WORKFLOW_404_RUN_NOT_FOUND`. Replay returns the existing run and cannot replace captured authority. |
| Registry graph and Card tenancy | Agent rules `TenantConn`/RLS boundary; spec REQ-029/032/052, AC-009/025 | `workflow/host.rs:290-399`; `cards/resolve.rs` `PinnedWorkflowGraph`; `cards/routes.rs:156-189`; recorded Scenarios 1 and 4 | **PASS.** Admission opens a registry transaction from `caller.data_tenant_id`, pins only the exact active graph, and commits before execution. `cards.get` accepts a closed typed exact reference, inherits only the executing Agent's space when omitted, and reuses the canonical Card read authorization/audit owner. |
| Bifrost tool authorization and confidentiality | Security posture object authorization; spec REQ-052, AC-020/025 | `workflow/tools.rs:139-215`; `query/collect.rs`; existing query service; recorded `declared_tools_use_captured_scopes_and_owned_services` at `pg_workflow_runs.rs:1852-2200` | **PASS.** Model-supplied arguments cannot select tenant, principal, credential, endpoint, query class, source, path, or visibility. The established Bifrost owner enforces read-only SQL admission, object authorization, tenant context, and canonical Oracle audit. Failed, incomplete, oversized, or malformed streams return no partial rows, and model-visible errors omit source messages/details. |
| Governed gateway calls | Security posture gateway decision/audit contract; spec REQ-029/032A/033/034, INV-020/021, AC-014/015/027 | `gateway/workflow.rs:54-131,170-315`; existing `GatewayInvocation`; recorded Scenarios 3 and 5 | **PASS.** Workflow calls pass `authorized = false`; the gateway checks captured invoke permission for the requested model and fallback candidates and retains current tenant deployment, credential, capability, budget, admission, cancellation, capture, and non-blocking audit ownership. Upstream refusal bodies and malformed success bodies are not reflected to the Workflow. |
| External binding, secret, and network boundary | Agent rules SSRF requirements; security posture secret/network rules; spec REQ-034, AC-016/023 | `config.rs:1697-1804`; `workflow/host.rs:333-357`; `wyrd-client/src/workflow/local.rs:113-158`; existing Skald endpoint policy; recorded external-ownership journey at `pg_workflow_runs.rs:2226` | **PASS.** Server bindings are assigned to exactly one tenant and resolved only when the authenticated tenant's pinned graph selects them. Secrets stay behind `SecretRef`/`SecretString`, outbound headers are sensitive, authored/reserved collisions fail closed, endpoint screening remains with the existing resolve/screen/pin owner, and errors do not include secret values. |
| Foreign-tenant proof | Spec AC-009/018/021/025 and closed `FIND-TASK-004-5` | `pg_workflow_runs.rs:603-650,1369-1378,1617-1642,2069-2109,2667-2688`; recorded R1 evidence | **PASS.** The harness provisions and authenticates a real second tenant. Submission cannot resolve the fixture tenant's Workflow, equal idempotency keys remain isolated, cross-tenant get/cancel use the common 404, Cards cannot cross tenant registry scope, and the foreign token cannot query the fixture tenant's table. The absent foreign model step is the approved credential-fixture limit, not a missing isolation path. |
| Injection and unsafe inputs | Security posture fail-closed input rules; spec REQ-030/034/052 | Typed route bodies/path parsing; `QueryArguments`; `CardsGetArguments`; gateway typed projection; binding configuration and endpoint policy | **PASS.** No command or template execution was introduced. Path/header/Card identifiers use validated types. SQL is intentionally model-supplied but enters the existing SELECT-only, object-authorized Bifrost planner rather than application string interpolation. Tool inputs deny unknown fields, and provider/header/endpoint parsing fails closed. No unsafe deserialization or caller-controlled filesystem path was added. |
| R3 security impact and mechanism drift | R3 preserved-behavior/non-goal clauses; human standing direction | Final remediation diff in `skald-workflow/src/{run,workflow}.rs` and import-only sites; complete cumulative diff | **PASS.** Reserving attempt one before publishing `Running` and settling interrupted published work as `Cancelled` changes no identity, permission, tenancy, credential, audit, secret, or network decision. The remaining edits move existing types/traits into module imports. No bespoke security check, file, option, setting, token store, renewal worker, tenant simulator, crypto, audit sink, compatibility surface, dependency, or remediation-only guard was introduced. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. Optional controls or repository checks not required by approved authority, established Wyrd behavior, or comparable standard practice are intentionally not proposed.

### Positive Controls

- Tenant and effective-principal identity originate only from verified credentials; request bodies, route values, model output, tool arguments, and binding names cannot choose tenancy.
- Accepted authority is a bounded, token-free clone of verified caller context, and replay cannot replace or widen it.
- Create/replay/get/cancel each make a fresh typed `workflows:run` decision and canonical audit before state disclosure or mutation.
- Run ownership and idempotency are jointly tenant- and principal-qualified, with a common inaccessible/not-found response.
- Cards, Bifrost, and gateway calls re-enter their established per-call authorization, audit, tenant, and admission owners.
- Built-in tools expose closed arguments and redact failures and partial data before presenting them to a model.
- External credentials use indirection, tenant assignment, redacted secret-bearing types, sensitive outbound headers, and the existing endpoint-screening transport.
- The R3 lifecycle correction preserves the bounded snapshot contract without adding a security mechanism or widening authority.

## Recorded Evidence and Limits

- The original task records focused commands for Scenarios 1-7. R1 records the second-tenant, accepted-authority, negative tool/query, gateway, lifecycle, and graph-bound closures plus broader Wyrd, principals, gateway, Bifrost, codegen, client-tier, tenant-isolation, unwrap, format, and lint lanes. R2 records source/authority corrections. R3 records the exact attempt-lifecycle tests plus Skald, Wyrd, Oracle, format, and lint lanes. These were treated as recorded claims and checked against current source and assertions.
- Per explicit instruction, this review ran no build, test, Cargo, or mise command. It did not independently execute JWT verification, Postgres RLS, audit publication, secret stores, endpoint DNS screening, gateway credentials, or Oracle queries.
- The foreign tenant's inability to execute a model step is the approved harness constraint: gateway credential protection is seeded only for the fixture tenant. The recorded journey directly covers credential-derived tenant selection, registry visibility, idempotency, run ownership, Card isolation, and Bifrost table isolation.
- The fixed Oracle lifecycle decisions were not treated as missing security controls: grant-stream close is follower release, and no release acknowledgement, graph-drain polling, or supervisor idle refusal is required.
- No required security authority, source path, task/remediation input, or recorded evidence was unavailable.

## Material Proposed Findings

None. No exploitable authentication bypass, authorization gap, IDOR, privilege widening, cross-tenant disclosure, injection path, secret exposure, unsafe token retention, supply-chain regression, deployment hazard, or unsupported bespoke security mechanism was found in the cumulative candidate.

## Overall Result

**PASS**

The cumulative candidate satisfies the approved security, RBAC, and tenancy obligations. Submission is authenticated and audited; accepted execution retains only bounded token-free authority; later HTTP requests authorize afresh; and every internal Card, Bifrost, gateway, and external-binding operation stays inside its established tenant, permission, audit, credential, and disclosure boundary.
