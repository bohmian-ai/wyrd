# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `e86831e5ac784028f8022cc3faeee1c22b12c665`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediation task: `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
- Review mode: static and read-only. No build, test, Cargo, or mise command was run.

The candidate remained at the stated commit throughout this review.

## Reviewed Boundary

This review traced the security- and tenancy-sensitive paths of the cumulative
TASK-004 candidate:

1. authenticated caller extraction and the captured, token-free execution
   authority;
2. fresh create/replay, get, and cancel authorization and canonical audit;
3. idempotency and run lookup keyed by verified tenant and effective principal;
4. exact active graph resolution through the tenant-scoped registry boundary;
5. `cards.get` and `bifrost.query` argument closure, per-call authorization,
   canonical audit, result redaction, and no-partial-query-result behavior;
6. in-process gateway authorization under captured permissions while current
   tenant deployment and credential eligibility remain live;
7. tenant-assigned external gateway bindings and secret handling;
8. authenticated second-tenant Workflow, run, Card, and Bifrost journey
   evidence; and
9. revision 13 provider-tagged request dispatch at the in-process gateway.

The human-approved constraints were treated as fixed: foreign-tenant journeys
do not need a foreign model call because the fixture tenant alone has gateway
credential protection; follower release is grant-stream close without a leader
ack; and the deleted Oracle graph-drain polling and supervisor idle refusal stay
deleted.

## Authority and Source Coverage

| Boundary | Governing authority | Source and evidence inspected | Assessment |
|---|---|---|---|
| Authenticated identity and captured authority | `AGENTS.md` §§2, 9; `architecture/wyrd-design.md:413-454`; `architecture/wyrd-security-posture.md:140-193`; spec REQ-032/032A, INV-022, AC-027 | `components/auth/caller_extractor.rs:11-74`; `components/workflow/host.rs:75-123,230-399`; `components/workflow/tools.rs:42-106`; `components/gateway/workflow.rs:34-131` | PASS. `Caller` is derived from one verified principal, carries tenant, principal permission/Card scopes, credential attribution, and verified delegation, and carries no bearer, API key, refresh token, or renewal handle. The accepted run clones that immutable context and does not refresh it. |
| Fresh HTTP authorization and audit | Agent rules canonical audit requirements; security posture Audit integrity and privacy; spec REQ-032/033, AC-010 | `components/workflow/routes.rs:24-180`; `components/workflow/host.rs:60-203`; `audit/mod.rs:211-276`; recorded Scenario 1 and Scenario 3 evidence | PASS. Every create/replay, get, and cancel request extracts a fresh authenticated caller and calls the typed `Permission::workflow_run()` decision before idempotency or run-table disclosure. The canonical append fails closed. Replay re-audits but returns the original run without replacing its graph or authority. |
| Workflow RBAC | Spec REQ-032; existing typed permission model | `wyrd-runtime/src/permission.rs:80-143,177-239`; `wyrd-runtime/src/builtin_roles.rs:51-94,122-138` | PASS. The added resource/action/scope projection is `workflows:run`; writer and agent receive it, admin covers it through wildcard, and reader/runtime_admin do not. No parallel role, policy, or authorization mechanism was added. |
| Run/idempotency tenancy and non-disclosure | Security posture Tenant and data isolation; spec REQ-030/032/034C, AC-018/021/022 | `components/workflow/runs.rs:41-105,192-203,252-341,367-413,691-713`; `components/workflow/host.rs:93-123,131-203`; recorded Scenarios 2 and 6 | PASS. Idempotency is keyed by `(verified tenant, effective principal, key)`. Get and cancel require the same tenant and principal. Unknown, malformed, foreign-principal, foreign-tenant, expired, evicted, and process-lost runs use the same stable 404 projection. |
| Exact graph and Card tenancy | Spec REQ-029/034, AC-009; repository `TenantConn` rule | `components/workflow/host.rs:290-399`; `components/cards/resolve.rs`; `components/cards/routes.rs:83-113,156-189`; recorded Scenario 1 and Scenario 4 evidence | PASS. Preparation opens the registry connection with `caller.data_tenant_id`, resolves the exact active graph under RLS, and starts no provider/tool work on refusal. The Workflow `cards.get` tool reuses the existing exact-ref authorized/audited Cards boundary and applies an optional UID as an additional equality constraint. |
| Bifrost tool authorization and confidentiality | Bifrost design object authorization/read-audit contract; spec REQ-052, AC-025 | `components/workflow/tools.rs:139-215`; `query/collect.rs:34-318`; query service callers; recorded Scenario 4 evidence | PASS. Tool input exposes no tenant, principal, credential, endpoint, query class, or visibility selector. A fresh request ID is generated for each query call, while tenant and principal remain the captured caller's. SQL validation, object authorization, tenant routing, canonical read audit, and Oracle settlement remain with the existing query service. Stream/protocol/bound failures expose no partial rows, and model-visible errors omit source message/details. |
| Gateway authorization and live eligibility | Security posture accepted-run exception and gateway non-blocking audit rule; spec REQ-032A/033/034, AC-027 | `components/gateway/workflow.rs:34-131,170-315`; `components/gateway/invocation.rs:205-327,400-517,610-672`; recorded Scenarios 3 and 5 | PASS. In-process calls pass `authorized = false`, so the gateway evaluates and audits the captured permission snapshot for the requested and fallback models. Tenant snapshot, deployment capability, credential availability, admission, budget, routing, and fallback remain live gateway-owned decisions. Provider refusal bodies and malformed success bodies are not reflected into workflow-visible diagnostics. |
| External gateway tenant and secret boundary | Security posture Source credentials and SSRF defense; spec REQ-034/042/049/050, AC-016/023 | `config.rs:1668-1804`; `workflow/host.rs:333-357`; `wyrd-client/src/workflow/local.rs:113-159`; `skald-workflow/src/route.rs:97-199,491-540`; `skald-providers/src/endpoint.rs` | PASS. Each server binding names exactly one tenant; preparation resolves only graph-selected bindings whose configured tenant equals the verified caller tenant. Secret values are read only at that boundary, stored in `SecretString`, marked sensitive as headers, omitted from debug output, and not copied into Cards, snapshots, audit, or model-visible errors. The established endpoint owner screens production destinations and pins screened DNS answers. |
| Second-tenant evidence and prior finding closure | R1 `FIND-TASK-004-5`; spec AC-009/018/021/025 | `pg_workflow_runs.rs:601-630,1369-1378,1617-1642,2067-2107,2656-2688`; recorded S1/S2/S4/S6 evidence | PASS. `foreign_admin` provisions and authenticates a real second tenant. Its submission of the fixture tenant's Workflow is refused before provider arrival; equal idempotency keys create distinct tenant-owned runs; cross-tenant get/cancel match the common not-found response; the local tenant cannot read the foreign Card; and the foreign token cannot query the fixture tenant's Bifrost table. The foreign run's model step failing for lack of that tenant's gateway deployment is the approved fixture limitation and does not weaken these assertions. |
| Revision 13 provider-tagged dispatch | Approved spec revision 13; no-compatibility decision | `skald-spec/src/request.rs`; `components/gateway/workflow.rs:170-250`; recorded round-trip/mismatch and Scenario 5 Vertex evidence | PASS. Adjacent `provider`/`body` tagging selects one provider variant instead of shape-ordered fallback. The in-process projection maps `ProviderRequest::Vertex` to `VertexGenerateContent` and its native response decoder. No compatibility reader or untagged fallback was retained. |
| Security mechanism drift and supply chain | Human standing direction; task/remediation non-goals | Complete cumulative diff; `Cargo.toml`; `Cargo.lock` | PASS. No authorization cache, policy engine, tenant simulator, bearer store, renewal task, audit sink, crypto scheme, security allowlist, or repository check was added. `jsonschema` is an already-installed workspace dependency used only for tests; the lockfile adds it to `wyrd-server`'s package dependency list without introducing or upgrading a package. The production `skald-tool` edge moves from dev-only to normal because the server now implements the existing tool contract. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. Optional hardening and controls not required by the approved behavior or
an established repository/platform pattern are intentionally omitted.

### Positive Controls

- Tenant and effective principal originate only from verified authentication;
  request bodies, tool arguments, model output, route bindings, and headers
  cannot select either identity.
- Accepted authority is an immutable `Caller` snapshot rather than retained
  credential material, and later grant changes cannot widen it.
- Fresh create/replay/get/cancel requests require current authentication,
  current `workflows:run`, and a canonical audited decision.
- Run ownership and idempotency are jointly tenant- and principal-qualified.
- Card and Bifrost tools use their established authorization/audit owners and
  expose closed argument objects without an identity or endpoint override.
- Gateway calls intentionally re-enter the gateway authorization owner and
  retain current deployment, credential, budget, capability, and fallback
  eligibility.
- Secret-bearing external headers use redacted storage and sensitive header
  values; diagnostic projections discard provider bodies and internal error
  details.
- Provider-tagged requests remove cross-provider shape inference, including the
  prior Gemini/Vertex ambiguity.

## Evidence Limits

- Per instruction, no build, test, Cargo, or mise command was run. Recorded
  verification results were treated as evidence claims and checked against the
  named source paths and assertions.
- This static review did not independently exercise the live network, Postgres
  RLS, gateway credential store, or Bifrost engine.
- The approved foreign-tenant fixture limitation means the second tenant's run
  cannot prove model execution; the security obligations reviewed here are
  instead proven at authenticated Workflow lookup, run ownership/idempotency,
  Card isolation, and Bifrost isolation boundaries. No harness change is
  required.
- The Oracle follower-release and deleted graph-drain/idle-refusal mechanisms
  were not reopened because they are fixed human decisions and do not alter the
  authenticated tenant or authorization boundaries reviewed here.

## Proposed Finding Ledger

No material security or tenancy findings.

## Overall Result

**PASS**

The cumulative candidate satisfies the reviewed security and tenancy
obligations. The prior missing second-tenant evidence is closed through the
repository's real tenant provisioning and authentication path, captured run
authority does not retain or widen credentials, and Cards, Bifrost, gateway,
audit, secret, and endpoint responsibilities remain with their established
owners. No exploitable injection, authentication/authorization bypass, tenant
data exposure, secret disclosure, insecure token handling, or supply-chain
regression was found in the reviewed range.
