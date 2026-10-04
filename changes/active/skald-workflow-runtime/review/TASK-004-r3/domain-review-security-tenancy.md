# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Prior remediations: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md` and `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`
- Review mode: static and strictly read-only with respect to the candidate. No build, test, Cargo, or mise command was run.

The candidate was `f17726fb25df1fa513875dca8d92f0073340ee0a` when this review began and immediately before this report was written.

## Reviewed Boundary

The cumulative base-to-candidate range was traced across these security-sensitive paths:

1. default-deny HTTP authentication and construction of `Caller` from one verified tenant principal;
2. the new `workflows:run` permission, built-in Role grants, and create/replay/get/cancel decisions;
3. canonical audit coupling and the absence of run lookup or idempotency disclosure before authorization;
4. accepted-job authority after request completion and token expiry, including credential attribution, delegation, permission and Card-scope capture without bearer retention;
5. tenant- and effective-principal-qualified run ownership, idempotency, cancellation, retention, and common not-found behavior;
6. active graph resolution under the authenticated tenant's `TenantConn` and exact Card pinning;
7. `cards.get` and `bifrost.query` argument closure, object authorization, audit, tenant routing, result bounds, error redaction, and terminal integrity;
8. in-process gateway calls under captured authority with current tenant deployment, credential, capability, fallback, admission, and audit ownership;
9. tenant-assigned external gateway bindings, endpoint policy, secret resolution, reserved-header rejection, and secret redaction;
10. second-tenant Workflow, run, Card, and Bifrost evidence; and
11. dependency and lockfile changes relevant to the new server tool integration.

The fixed human decisions were applied as authority rather than reopened: the deleted Oracle graph-drain polling and supervisor idle refusal remain deleted; closing the follower grant stream is the release and the leader awaits no follower acknowledgement; and foreign-tenant journeys do not require a model step because the harness provisions gateway credential protection only for the fixture tenant.

## Authority and Source Coverage

| Boundary | Governing authority | Source and recorded evidence inspected | Assessment |
|---|---|---|---|
| Authentication and tenant derivation | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` tenant and audit rules; `architecture/wyrd-security-posture.md:9-27,141-193,246-264`; spec REQ-032/032A and INV-022 | `http/router.rs:49-80`; `components/auth/caller_extractor.rs:11-74`; `wyrd-runtime/src/principal.rs:16-38,153-219`; recorded Scenarios 1 and 3 | PASS. Workflow routes are merged into the authenticated `/v1` group. `Caller::from_authenticated` derives tenant, effective principal, permission snapshot, credential attribution, Card scope, and verified delegation from the same verifier result. It contains no bearer, refresh token, API-key secret, or renewal handle. |
| Typed Workflow RBAC | Spec REQ-032; security posture authorization model; existing typed permission owner | `wyrd-runtime/src/permission.rs:69-143,145-175,285-337,459-468`; `wyrd-runtime/src/builtin_roles.rs:51-94,122-139`; recorded principals and Workflow journey evidence | PASS. `Permission::workflow_run()` is the existing `(resource, action, scope)` mechanism: `Workflows`, `Run`, `All`. Writer and agent receive it, admin covers it through wildcard, and reader/runtime_admin do not. No separate policy engine, authorization cache, Role kind, or stale-allow mechanism was introduced. |
| Fresh create/replay/get/cancel authorization and audit | `architecture/agent-rules.md:12-14`; security posture audit integrity; spec REQ-032/033/034C, AC-010 | `components/workflow/routes.rs:24-180`; `components/workflow/host.rs:60-204`; `audit/mod.rs:211-276`; recorded Scenario 1/3 audit and audit-fault evidence | PASS. Every request is freshly authenticated. Create audits before idempotency admission; replay traverses the same decision. Get/cancel audit before parsing/lookup, so malformed, unknown, and foreign identifiers disclose no state first. Canonical standalone transaction failure refuses the operation. Terminalization performs no invented authorization audit. |
| Accepted-job authority and token lifetime | `architecture/wyrd-design.md:413-455`; `architecture/wyrd-security-posture.md:176-190`; spec REQ-032A, INV-022, AC-027 | `components/workflow/host.rs:93-123,230-399`; `components/gateway/workflow.rs:34-115`; `components/workflow/tools.rs:42-106`; `Caller`/`Principal` sources above; recorded `accepted_authority_outlives_submission_only` evidence | PASS. Preparation owns a clone of the already verified `Caller`; the run neither stores nor renews token material. Later grant changes cannot widen its immutable permission/Card-scope snapshot, while fresh HTTP requests use current authority. The context is reachable only through the run-owned gateway/tools and is dropped with run work, apart from already-issued gateway-owned settlement. |
| Run and idempotency tenancy/IDOR resistance | Security posture tenant isolation and non-disclosure; spec REQ-030/032/034C, AC-018/021/022 | `components/workflow/host.rs:91-203`; `components/workflow/runs.rs:47-115,198-209,258-344,369-415`; recorded Scenarios 2 and 6 | PASS. Idempotency is keyed by verified `(tenant, effective principal, IdempotencyKey)`. Get/cancel match both tenant and principal. Unknown, malformed, foreign-tenant, foreign-principal, expired, evicted, process-lost, and non-owning-replica cases share `WYRD_WORKFLOW_404_RUN_NOT_FOUND`; no run contents cross that boundary. |
| Registry graph and Card read tenancy | `AGENTS.md`/agent rules `TenantConn` and RLS rules; spec REQ-029/032/052, AC-009/025 | `components/workflow/host.rs:290-399`; `components/cards/resolve.rs::PinnedWorkflowGraph`; `components/cards/routes.rs:83-113,156-189`; recorded Scenarios 1 and 4 | PASS. Admission opens a tenant connection from `caller.data_tenant_id`, resolves only active exact graph identities, commits before execution, and pins those bodies. `cards.get` accepts a closed typed CardRef, inherits only the Agent space, and reuses the established exact-ref authorization/audit boundary; optional UID narrows rather than redirects the read. |
| Bifrost built-in authorization, SQL boundary, and confidentiality | Security posture object authorization; spec REQ-052, AC-020/025 | `components/workflow/tools.rs:139-215`; `query/collect.rs:40-71,73-191,204-324,353-527`; `query/service.rs:17-181,222-280`; recorded `declared_tools_use_captured_scopes_and_owned_services` evidence | PASS. The model can select SQL and bounded result/deadline values, but not tenant, principal, credential, endpoint, query class, source, path, or visibility. Input is closed and bounded; the existing Bifrost owner enforces SELECT/query admission, table-scoped authorization, tenant context, and canonical Oracle read audit. Failed, incomplete, oversized, or malformed streams return no partial rows. Model-visible failures retain only stable public metadata. |
| In-process gateway authorization and live controls | Security posture gateway decision/audit exception; spec REQ-029/032A/033/034, INV-020/021, AC-014/015/027 | `components/gateway/workflow.rs:34-115,170-315`; `components/gateway/invocation.rs:400-517`; recorded Scenarios 3 and 5 | PASS. Workflow calls pass `authorized = false`, so the gateway checks the captured invoke permission for the requested model and each fallback candidate. The gateway re-reads the current tenant snapshot and owns deployment, credential, capability, budget, fallback, admission, cancellation, capture, and non-blocking canonical audit settlement. Provider bodies and malformed success bodies are not reflected into Workflow diagnostics. |
| External gateway secret, header, and SSRF boundary | `architecture/agent-rules.md:29-30`; security posture external-network and secret rules; spec REQ-034, AC-016/023 | `config.rs:1660-1804`; `workflow/host.rs:333-357`; `wyrd-client/src/workflow/local.rs:113-159`; `skald-workflow/src/route.rs:97-199,491-540`; shared endpoint-policy owner; recorded route/security evidence | PASS. Server bindings are assigned to one tenant and selected only when that tenant's pinned graph names them. Secret references resolve at preparation into `SecretString`; debug output lists header names only, values become sensitive `HeaderValue`s, authored collisions and reserved headers fail closed, and errors omit values. The existing provider endpoint policy owns resolve/screen/pin and redirect checks. No Card, invocation input, audit event, snapshot, trace field, or model error receives a secret. |
| Public gateway fallback header | Spec REQ-036A, INV-020; existing ingress authentication and non-forwarding boundary | `components/gateway/ingress.rs:43-170,190-225`; route projections and recorded public-ingress evidence | PASS. Protocol authentication runs before handler/body processing. The fallback header has one bounded typed parser, rejects repeats/malformed/self/duplicate candidates, becomes an immutable per-call field, and is not forwarded because provider requests are rebuilt from typed request bodies rather than inbound headers. |
| Foreign-tenant evidence | Spec AC-009/018/021/025 and prior `FIND-TASK-004-5` | `pg_workflow_runs.rs:601-630,1369-1378,1617-1642,2067-2107,2656-2688`; R1 implementation evidence | PASS. The harness provisions a genuinely authenticated second tenant. Equal idempotency keys are isolated; cross-tenant run get/cancel use the common 404; local tools cannot read its Card; its token cannot query the fixture tenant's table. Its model step is intentionally unavailable because only the fixture tenant has gateway credential protection; the reviewed tenant boundaries do not rely on that step. |
| Injection and unsafe-input review | Security posture fail-closed input rules; spec REQ-030/034/052 | Typed route bodies and path parsing in `workflow/routes.rs`; `QueryArguments`, `CardsGetArguments`, gateway typed request projection, `ExternalGatewayBindings::insert`, and `PinnedWorkflowGraph` sources above | PASS. No shell or template execution was added. Path/header/Card identifiers use validated domain types. SQL remains intentionally model-supplied but enters the established SELECT-only, object-authorized Bifrost planner rather than string interpolation into application SQL. JSON inputs deny unknown fields where the contract is closed; gateway/provider parsing and endpoint/header validation fail closed. No unsafe deserialization path was found. |
| Supply chain and security-mechanism drift | Human standing direction; task/remediation non-goals | Complete cumulative diff; `crates/wyrd/wyrd-server/Cargo.toml`; `Cargo.lock`; R2 remediation diff | PASS. `skald-tool` is an existing workspace crate moved from dev-only to production because the server now implements its established tool contract. `jsonschema` is an existing workspace dependency added only for tests; the lockfile adds it to the server package dependency list without adding/upgrading a package. No bespoke security check, option, token store, renewal worker, tenant simulator, crypto, audit sink, allowlist file, compatibility path, or third-party security dependency was added. R2 changes are documentation and repository-standard import alignment, not a security mechanism. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. Optional controls or repository checks not required by the approved contract, an established Wyrd owner, or comparable accepted practice are intentionally not proposed.

### Positive Controls

- Tenant and effective-principal identity originate only from verified credentials; request bodies, route parameters, headers, model output, tool arguments, and binding names cannot choose tenancy.
- Accepted authority is a bounded clone of the verified `Caller`, not retained credential material, and replay cannot replace or widen it.
- Create/replay/get/cancel each perform a fresh typed `workflows:run` decision and canonical audit before state disclosure or mutation.
- Run ownership and idempotency are jointly tenant- and principal-qualified, with a common not-found response for inaccessible state.
- Built-in tools expose closed arguments and re-enter the established Cards and Bifrost authorization, tenant, audit, and result-integrity owners.
- Gateway calls re-enter the existing gateway decision owner and retain live tenant deployment, credential, capability, budget, fallback, and settlement controls.
- External secrets use indirection, redacted storage/debug behavior, sensitive outbound headers, exact tenant assignment, and the existing endpoint-screening implementation.
- Provider and tool failures presented to a model remove source message/details and partial data.
- The dependency change reuses existing workspace components and introduces no new package or security framework.

## Verification Evidence and Limits

- The original task records exact focused commands for Scenarios 1-7. R1 records passing focused Workflow tests and broader `test:wyrd`, principals, gateway, Bifrost, codegen, client-tier, tenant-isolation, unwrap, format, and lint lanes. R2 records the final documentation/import remediation evidence. These results were treated as recorded claims and checked against the cited current source and assertions.
- Per explicit instruction, this review ran no build, test, Cargo, or mise command. It did not independently exercise live JWT verification, Postgres RLS, audit publication, gateway credential stores, endpoint DNS behavior, or Oracle execution.
- The foreign tenant's inability to execute a model step is an approved harness constraint: gateway credential protection is seeded only for the fixture tenant. The second-tenant evidence instead directly covers authentication, registry visibility, idempotency, run ownership, Card isolation, and Bifrost table isolation.
- The fixed Oracle lifecycle decisions were not treated as missing security proof: grant-stream close is the follower release; no follower acknowledgement or deleted graph-drain/idle-refusal mechanism is required.
- No required security authority, source path, task/remediation input, or recorded verification evidence was unavailable.

## Proposed Finding Ledger

No material security, RBAC, tenancy, injection, secret-exposure, token-handling, audit, dependency, or deployment finding.

## Overall Result

**PASS**

The cumulative candidate satisfies the approved security and tenancy obligations. The server authenticates and audits the submission boundary, retains only bounded token-free execution authority, authorizes subsequent requests afresh, and routes each internal Card, Bifrost, and gateway operation through its established tenant and resource owner. No exploitable authentication bypass, IDOR, privilege widening, cross-tenant disclosure, injection path, secret leak, insecure token retention, supply-chain regression, or nonstandard security mechanism was found.
