# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R5 in their preceding review directories
- Review mode: static, cumulative base-to-candidate review. Per direction, no
  build, test, Cargo, mise, or source-edit command was run.

The candidate resolved to the requested commit at the start of review and
immediately before this report was written. `.codegraph/` is absent, so this
review used immutable Git diff inspection, repository search, and direct source
reading.

## Reviewed Boundary

The security-sensitive Workflow boundary was traced through:

1. default-deny HTTP authentication and construction of `Caller` from one
   verified tenant principal;
2. typed `workflows:run` grants and the create/replay/get/cancel authorization
   and canonical audit sequence;
3. accepted capture of effective principal, permissions and Card scope,
   credential attribution, and delegation without bearer, refresh, or API-key
   retention;
4. tenant- and principal-qualified idempotency, lookup, cancellation,
   retention, and non-disclosing not-found behavior;
5. exact active-graph resolution and dynamic `cards.get` and `bifrost.query`
   decisions under the captured caller;
6. governed gateway calls under captured scopes while current deployment,
   provider credential, capability, fallback, limit, and audit decisions remain
   with the gateway owner;
7. tenant-qualified external bindings, selected secret resolution, reserved
   header rejection, exact-origin matching, DNS/SSRF screening, address use,
   redirect refusal, and redacted failures;
8. Revision 14's optional Prompt dispatch target and its native, governed
   gateway, external gateway, Gemini, and Vertex projections; and
9. the recorded authenticated second-tenant, permission-denial, audit-failure,
   and secret-nondisclosure journeys.

The fixed human decisions were treated as authority and not reopened. In
particular, the foreign-tenant harness remains unchanged and does not need a
model step or credentials; a follower graph release is the grant-stream close;
published `Running` reserves attempt one and interrupted published work settles
`Cancelled`; query tools receive the prepared deadline once, with a shorter
`deadline_ms` winning; and Revision 14's one-variant-per-wire-schema provider
model, Vertex-as-Google-body representation, and unchanged local Vertex refusal
stand.

## Authority and Source Coverage

| Boundary | Governing authority | Source and recorded evidence inspected | Assessment |
|---|---|---|---|
| Authentication and tenant derivation | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; spec REQ-032/032A, INV-006/022 | `http/router.rs:49-80`; `components/auth/caller_extractor.rs:11-74`; recorded Scenarios 1 and 3 | **PASS.** Workflow routes are under the default-deny `/v1` verifier. Tenant, effective principal, credential attribution, permissions/Card scope, and delegation all come from one verified principal. Request bodies, paths, tool arguments, model output, and bindings cannot select tenancy. |
| Workflow RBAC and canonical audit | Canonical audit rules; spec REQ-032/033/034C, AC-010 | `wyrd-runtime/src/permission.rs:69-175`; `builtin_roles.rs:51-94,122-139`; `workflow/routes.rs:24-180`; `workflow/host.rs:60-204`; `audit/mod.rs:211-270`; recorded admission and audit-fault evidence | **PASS.** `workflows:run` is typed and limited to admin/writer/agent. Create and replay, get, and cancel each make and commit a fresh fail-closed audit decision before idempotency or run-state disclosure. A malformed run id receives the same post-audit not-found result as an inaccessible id. |
| Accepted execution authority | Security posture accepted-job exception; spec REQ-032A, INV-022, AC-027 | `workflow/host.rs:230-400`; `workflow/tools.rs:43-124`; `gateway/workflow.rs:34-131`; recorded accepted-authority journey | **PASS.** Preparation clones `Caller`, which contains no bearer, refresh token, API key, secret, renewal capability, or live grant resolver. Later grant changes cannot widen or replace the accepted snapshot; later HTTP requests authenticate independently. Dynamic resource calls still evaluate their own permission boundary. |
| Idempotency, ownership, and IDOR resistance | Tenant isolation rules; spec REQ-030/032/034C, AC-018/021/022 | `workflow/host.rs:91-203`; `workflow/runs.rs:47-57,89-115,198-209,258-415`; recorded replay, foreign-principal, foreign-tenant, eviction, and restart evidence | **PASS.** Keys and runs are scoped by verified `(tenant, effective principal)`. Unknown, malformed, foreign, expired, evicted, lost, and non-owning-replica IDs converge on `WYRD_WORKFLOW_404_RUN_NOT_FOUND`. Replay cannot refresh captured authority or duplicate execution. |
| Pinned graph and Cards tool | `TenantConn`/RLS rules; spec REQ-029/032/052, INV-005/006, AC-009/025 | `workflow/host.rs:290-400`; `components/cards/resolve.rs`; `components/cards/routes.rs:156-189`; `workflow/tools.rs:236-351`; recorded Cards denial/audit evidence | **PASS.** Preparation pins the exact active graph inside the verified tenant transaction. `cards.get` accepts one closed exact Card reference, inherits only the executing Agent's space when omitted, and reuses the canonical Cards authorization/audit boundary. Model-visible failures carry no underlying message or details. |
| Bifrost query tool | SQL and object-authorization rules; spec REQ-052, AC-020/025 | `workflow/tools.rs:156-233`; `query/collect.rs:173-324,353-651`; existing query service and Oracle controls; recorded permission, foreign-table, non-SELECT, bound, malformed-terminal, cancellation, and forwarded-query evidence | **PASS.** Closed arguments expose no tenant, principal, credential, endpoint, path, source, visibility, or query-class selector. SQL enters the established SELECT-only, object-authorized, tenant-routed query owner. Complete results require trustworthy terminal and EOF proof; partial rows and source diagnostics are withheld on failure. |
| Governed gateway decision | Gateway authorization/audit authority; spec REQ-029/032A/033/034, AC-014/015/027 | `gateway/workflow.rs:34-305`; existing `GatewayInvocation`; recorded gateway ownership, credential-revocation, and provider-path evidence | **PASS.** Calls use `authorized = false`, so each requested model and fallback is authorized and audited by the gateway under the captured scopes. Current deployment and credential eligibility remain live. Provider refusal bodies and malformed success bodies are not reflected into Workflow errors. |
| External binding, secret, and SSRF boundary | Agent-rule SSRF requirements; security posture Source credential/SSRF rules; spec REQ-034/042/049/050, INV-010/012/017, AC-016/023 | `config.rs:1660-1804`; `workflow/host.rs:333-358`; `wyrd-client/src/workflow/local.rs:113-159`; `skald-workflow/src/route.rs:97-200,266-355,502-542`; `skald-providers/src/endpoint.rs:33-216`; `clients/external.rs`; recorded secret/origin/route evidence | **PASS.** A selected binding must be assigned to the verified tenant before its `SecretRef` values resolve. Secrets are held as `SecretString`, outbound values are marked sensitive, authored headers cannot replace secret headers, reserved transport headers are refused, and errors omit secret values. The shared endpoint owner screens literals and every DNS answer, connects through those screened answers, disables proxies and redirects, preserves TLS verification, and bounds resolution/connect/request work. |
| Revision 14 provider dispatch target | Approved Revision 14 and fixed human decision | `skald-spec/src/{prompt,request}.rs`; `skald-runtime/src/dispatch.rs`; `skald-workflow/src/route.rs:266-355,427-481`; `wyrd-server/components/gateway/workflow.rs:175-243`; `skald-providers/clients/external.rs:95-135`; recorded custom-provider and Vertex evidence | **PASS.** `Prompt.provider` is a dispatch identifier, not a URL or credential. Native dispatch can choose only an installed registry client; server execution rejects Native routes. Governed gateway dispatch converts the identifier to a validated `ModelRef` and then performs the ordinary live model authorization. External routes ignore it and remain pinned to the tenant's configured binding/protocol/origin. A Vertex GenerateContent Prompt reaches the existing Vertex ingress with the same gateway authorization and credential controls as before. |
| Foreign-tenant nondisclosure | Spec AC-009/018/021/025 and fixed harness decision | `pg_workflow_runs.rs` recorded second-tenant assertions for graph visibility, idempotency, get/cancel, Card reads, and Bifrost tables | **PASS.** The authenticated second tenant cannot resolve the fixture tenant's Workflow, observe or cancel its runs, read its Cards, or query its tables; equal idempotency keys remain isolated. The absence of foreign gateway credentials is an accepted harness constraint and does not weaken these directly exercised boundaries. |
| Injection and supply chain | Typed-boundary and dependency rules; standing standard-DRIFT direction | Closed serde inputs, validated identities/header names/URLs, established SQL floor, cumulative `Cargo.toml` and `Cargo.lock` diff | **PASS.** No shell or template execution, path selector, unsafe deserialization, caller-controlled secret file path, token cache, custom crypto, second audit sink, or authorization bypass was introduced. The only manifest additions use existing workspace dependencies (`skald-tool`; test-only `jsonschema`); no new external package entered the lockfile. No bespoke security check, setting, option, or allowlist was added or is required. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. This review does not propose optional mechanisms absent from the
established Wyrd standard or comparable widely used projects.

### Positive Controls

- Verified credentials are the sole source of tenant and principal identity.
- Accepted execution captures bounded, token-free authority and cannot renew,
  refresh, or widen it.
- Create/replay/get/cancel decisions use the canonical fail-closed audit path
  before state disclosure or mutation.
- Run ownership and idempotency are tenant- and effective-principal-qualified
  and return one non-disclosing inaccessible/not-found result.
- Cards, Bifrost, and gateway work re-enters the existing owning service's
  authorization, audit, tenant, admission, and result-validation boundary.
- Built-in tools accept closed inputs and return bounded, complete results or
  redacted non-retryable failures without partial unauthorized data.
- External credentials remain tenant-assigned, secret-indirected, sensitive in
  memory/headers, exact-origin bound, and protected by the existing screened
  no-redirect/no-proxy transport.
- Revision 14 changes dispatch representation without creating a new endpoint,
  credential selector, authorization mechanism, compatibility path, or secret
  surface.

## Verification Evidence and Limits

- The original task and R1-R5 contain recorded focused and broader command
  results for admission, audit failure, accepted authority, second-tenant
  isolation, dynamic Cards/Bifrost denials, gateway/external ownership,
  provider dispatch, secret non-disclosure, and deadline settlement. Those
  results were treated as claims and checked against the named candidate source
  and assertions.
- Revision 14 additionally records passing custom-provider, Vertex, server,
  shared-client, gateway, schema, Python, TypeScript, and Oracle lanes. This
  review checked the changed dispatch paths but did not rerun those commands.
- Per explicit direction, this review did not independently exercise JWT
  verification, Postgres RLS, audit publication, secret stores, production DNS
  behavior, gateway credentials, provider endpoints, or Oracle execution.
- Production deployment removal during an accepted run and production-profile
  external-egress negative cases remain covered by their established owners'
  recorded evidence rather than a new Workflow-specific harness.
- The unchanged foreign-tenant harness directly proves credential-derived
  tenancy and data nondisclosure; it intentionally does not provision a foreign
  model deployment or execute a model step.
- No required authority, source path, prior remediation, or recorded evidence
  was unavailable.

## Material Proposed Findings

None. No exploitable authentication bypass, authorization gap, IDOR, privilege
widening, cross-tenant disclosure, SQL/command/template/path injection,
unsafe deserialization, secret exposure, insecure token retention, SSRF
regression, dependency risk, deployment hazard, or unsupported bespoke
security mechanism was found in the cumulative candidate.

## Overall Result

**PASS**

The cumulative candidate satisfies the approved security, RBAC, tenancy,
secret-handling, audit, and external-egress obligations. Revision 14 changes
provider request representation and dispatch targeting without widening
authority: server Native execution remains refused, governed gateway calls
retain live per-model authorization and audit, and external calls remain bound
to the tenant's preconfigured credential/origin boundary.
