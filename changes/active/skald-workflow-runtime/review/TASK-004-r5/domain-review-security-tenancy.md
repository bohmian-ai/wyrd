# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1, R2, R3, and R4 in their preceding review directories
- Review mode: static, cumulative base-to-candidate review. No build, test,
  Cargo, or mise command was run.

The candidate resolved to the requested commit at the start of review and
immediately before this report was written. `.codegraph/` is absent, so the
review used immutable Git diff inspection, repository search, and direct source
reading.

## Reviewed Boundary

The cumulative change was traced across the security-sensitive Workflow paths:

1. default-deny HTTP authentication and construction of `Caller` from one
   verified tenant principal;
2. typed `workflows:run` grants and create/replay/get/cancel authorization and
   canonical audit ordering;
3. accepted capture of principal attribution, permission/Card scopes,
   credential attribution, and delegation without retaining bearer material;
4. tenant- and effective-principal-qualified idempotency, run ownership,
   cancellation, retention, and non-disclosing not-found behavior;
5. exact active graph resolution and `cards.get` through the tenant-scoped,
   authorized, audited Cards boundary;
6. `bifrost.query` closed arguments, captured tenant/principal context,
   per-call authorization/audit, complete-result handling, and redacted errors;
7. in-process gateway calls under captured scopes while current gateway
   deployment, credential, capability, fallback, admission, and audit remain
   gateway-owned;
8. tenant-assigned external bindings, secret indirection, origin and reserved-
   header enforcement, endpoint screening, and error non-disclosure;
9. authenticated second-tenant Workflow, run, Card, and Bifrost journey
   evidence; and
10. R4's one-time projection of the prepared run deadline into every shared
    built-in query-tool clone.

The standing human decisions were applied as authority and not reopened. In
particular, the foreign tenant is not required to execute a model step because
the harness seeds gateway credentials only for the fixture tenant. Deleted
Oracle polling/refusal behavior, follower release semantics, published step
attempt semantics, and the one-time prepared-deadline bind were likewise
treated as fixed.

## Authority and Source Coverage

| Boundary | Governing authority | Source and recorded evidence inspected | Assessment |
|---|---|---|---|
| Authentication and tenant derivation | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; security posture security principles; spec REQ-032/032A and INV-022 | `http/router.rs:49-80`; `components/auth/caller_extractor.rs:11-74`; recorded Scenarios 1 and 3 | **PASS.** Workflow routes are inside the default-deny authenticated `/v1` group. `Caller` derives tenant, effective principal, credential attribution, permissions/Card scope, and verified delegation from one verifier result. It contains no bearer, refresh token, API key, or renewal handle. |
| Workflow RBAC and audit ordering | Canonical audit rules; spec REQ-032/033/034C, AC-010 | `wyrd-runtime/src/permission.rs:69-175`; `builtin_roles.rs:51-94,122-139`; `workflow/routes.rs:24-180`; `workflow/host.rs:60-204`; recorded admission/audit-fault evidence | **PASS.** `Permission::workflow_run()` uses the established typed permission model. Writer and agent receive it, admin covers it by wildcard, and reader/runtime_admin do not. Every create/replay/get/cancel evaluates and transactionally audits current authority before idempotency or run-state disclosure. |
| Accepted token-free authority | `wyrd-design.md:445-454`; `wyrd-security-posture.md:181-190`; spec REQ-032A, INV-022, AC-027 | `workflow/host.rs:230-400`; `workflow/tools.rs:43-124`; `gateway/workflow.rs:34-131`; recorded accepted-authority journey | **PASS.** Preparation clones the verified `Caller`, not a credential. The immutable captured scopes cannot widen after acceptance; later HTTP requests authenticate independently; gateway, Cards, and query owners still make their own per-call decisions and audits. |
| Idempotency, ownership, and IDOR resistance | Security posture tenant isolation; spec REQ-030/032/034C, AC-018/021/022 | `workflow/host.rs:91-203`; `workflow/runs.rs:47-57,198-209,258-415`; recorded Scenarios 2 and 6 | **PASS.** Idempotency is keyed by verified `(tenant, effective principal, IdempotencyKey)`. Get/cancel require both values. Malformed, unknown, foreign-tenant, foreign-principal, expired, evicted, lost, and non-owning-replica run IDs share the stable not-found projection. Replay cannot replace run authority. |
| Exact graph and Card boundary | `TenantConn`/RLS rules; spec REQ-029/032/052, AC-009/025 | `workflow/host.rs:290-400`; `components/cards/resolve.rs`; `components/cards/routes.rs:156-189`; recorded Scenarios 1 and 4 | **PASS.** Graph preparation opens the registry connection from the verified caller tenant and pins the exact active graph. `cards.get` accepts one typed exact reference, inherits only the executing Agent space when omitted, and reuses the canonical Cards authorization/audit owner. |
| Query-tool tenancy and confidentiality | Security posture object authorization and error privacy; spec REQ-052, AC-020/025 | `workflow/tools.rs:156-233`; `query/collect.rs`; existing query service; recorded tool and forwarded-query evidence | **PASS.** Tool input cannot select tenant, principal, credential, endpoint, source, path, query class, or visibility. The captured caller enters the existing Bifrost owner, which retains SQL admission, table authorization, tenant routing, Oracle audit, terminal validation, and result ceilings. Failed or incomplete streams expose no partial rows, and model-visible errors omit source messages/details. |
| Governed gateway calls | Security posture gateway decision/audit contract; spec REQ-029/032A/033/034, AC-014/015/027 | `gateway/workflow.rs:34-131,170-315`; existing `GatewayInvocation`; recorded Scenarios 3 and 5 | **PASS.** Calls pass `authorized = false`, preserving current per-model/fallback gateway authorization and audit. Current tenant deployment, credential availability, capability, budget, admission, capture, and cancellation remain live gateway-owned controls. Provider refusal bodies and malformed success bodies are not reflected into Workflow diagnostics. |
| External endpoint and secret boundary | Agent-rule SSRF requirements; security posture secret/network rules; spec REQ-034/049, INV-010/012, AC-016/023 | `config.rs:1660-1804`; `workflow/host.rs:333-358`; `wyrd-client/src/workflow/local.rs:113-159`; `skald-workflow/src/route.rs`; existing provider endpoint owner | **PASS.** A server binding is assigned to exactly one tenant and resolved only when that authenticated tenant's pinned graph selects it. Secrets remain behind `SecretRef`/`SecretString`, outbound secret headers are sensitive, authored and reserved collisions fail closed, and the existing resolve/screen/pin owner handles production destinations and redirects. Values do not enter Cards, run snapshots, audit, logs, or model-visible errors. |
| Foreign-tenant proof | Spec AC-009/018/021/025 and closed `FIND-TASK-004-5` | `pg_workflow_runs.rs` second-tenant fixture and assertions cited by R1-R4; recorded Scenario 1/2/4/6 evidence | **PASS.** A real authenticated second tenant cannot resolve the fixture tenant's Workflow, cannot read/cancel its run, cannot read its Card, and cannot query its Bifrost table; equal idempotency keys stay tenant-isolated. The accepted lack of foreign gateway credentials does not weaken those boundaries. |
| R4 prepared-deadline bind | R4 remediation; spec bounded-tool and total-deadline requirements | `skald-workflow/src/workflow.rs:140-209`; `workflow_surface.rs:690-713`; `workflow/host.rs:374-400`; `workflow/tools.rs:43-96,188-225`; recorded focused and Oracle-journey evidence | **PASS.** `WorkflowExecutor` remains the sole deadline sampler. Per-run `RunTools` shares one ordinary `Arc<OnceLock<Instant>>`, and preparation binds the exact prepared deadline before acceptance or execution. This changes no identity, scope, authorization, tenant, credential, or audit data. |
| Injection, dependency, and mechanism drift | Security posture fail-closed input rules; standing standard-DRIFT direction; task/remediation non-goals | Typed route bodies/paths, closed tool inputs, gateway projection, binding/endpoint owners, cumulative manifests and lockfile, R4 diff | **PASS.** No shell/template execution, path traversal, unsafe deserialization, caller-controlled filesystem path, custom crypto, authorization cache, audit sink, token store, tenant selector, security allowlist/check, setting, or option was introduced. SQL remains intentionally model-supplied but enters the established SELECT-only object-authorized query owner. `skald-tool` and test-only `jsonschema` were already-installed workspace dependencies; R4 adds no package or dependency. The standard `OnceLock` one-time bind is established native machinery, not bespoke enforcement drift. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. Optional controls or bespoke checks not required by approved authority,
established Wyrd behavior, or comparable standard practice are intentionally
not proposed.

### Positive Controls

- Tenant and effective-principal identity originate only from verified
  credentials; bodies, paths, model output, tool arguments, and binding names
  cannot choose tenancy.
- Accepted authority is a bounded, token-free copy of verified caller context;
  replay cannot replace or widen it.
- Create/replay/get/cancel each perform a fresh typed permission decision and
  canonical fail-closed audit before state disclosure or mutation.
- Run ownership and idempotency are tenant- and principal-qualified, with one
  non-disclosing inaccessible/not-found response.
- Cards, Bifrost, and gateway calls re-enter their established per-call
  authorization, audit, tenant, and admission owners.
- Built-in tools expose closed inputs, complete bounded results, and redacted
  failures with no partial unauthorized data.
- External credentials use tenant assignment, indirection, redacted secret
  types, sensitive headers, exact-origin enforcement, and the existing
  screened/pinned transport.
- R4 reuses the standard-library one-time cell and the existing prepared-run
  deadline owner without adding a security mechanism or widening authority.

## Verification Evidence and Limits

- The original task and R1-R4 record focused and broader command results for
  Workflow admission, accepted authority, second-tenant isolation, Cards and
  query denials, gateway/external ownership, lifecycle, R3 attempt settlement,
  and R4 deadline identity. They were treated as claims and checked against the
  candidate source and named assertions.
- Per explicit direction, this review ran no build, test, Cargo, or mise
  command. It did not independently exercise JWT verification, Postgres RLS,
  audit publication, secret stores, production DNS screening, gateway
  credentials, or Oracle execution.
- The foreign tenant's absent model step is the approved harness constraint;
  the recorded journey directly covers credential-derived tenant selection,
  registry visibility, idempotency, run ownership, Card isolation, and Bifrost
  table isolation.
- No required authority, source path, remediation input, or recorded evidence
  was unavailable.

## Material Proposed Findings

None. No exploitable authentication bypass, authorization gap, IDOR, privilege
widening, cross-tenant disclosure, injection path, secret exposure, insecure
token retention, dependency regression, deployment hazard, or unsupported
bespoke security mechanism was found in the cumulative candidate.

## Overall Result

**PASS**

The cumulative candidate satisfies the approved security, RBAC, and tenancy
obligations. R4's prepared-deadline correction is security-neutral: it binds
existing per-run tools to the already-owned deadline before acceptance without
changing captured authority or any resource owner's live authorization,
credential, tenant, audit, and disclosure controls.
