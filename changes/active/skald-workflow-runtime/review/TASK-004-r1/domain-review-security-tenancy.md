# Security and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Review mode: static and read-only. No build, test, Cargo, or mise command was run.

The candidate remained at the stated commit while this review was performed.

## Reviewed Boundary

This review traced the security-sensitive paths introduced or materially changed by TASK-004:

1. verified HTTP caller extraction into `Caller`, and the absence of retained bearer material;
2. `workflows:run` permission construction and built-in role grants;
3. create, replay, get, and cancel authorization/audit ordering;
4. process-local run ownership and idempotency scoping by verified tenant and effective principal;
5. captured authority used by in-process gateway, Cards, and Bifrost calls;
6. exact-reference Card reads through the existing authorization/audit owner;
7. query-tool argument closure, captured tenant/principal, query authorization/audit, and no-partial-row error behavior;
8. tenant-qualified external gateway bindings, secret-reference resolution, endpoint binding, and redacted failures;
9. indistinguishable run-not-found behavior; and
10. the recorded journey evidence for cross-tenant isolation.

The related Oracle cleanup changes were inspected for security-boundary drift. They change resource-release and settlement mechanics, not peer identity, verified tenant context, object authorization, or audit ownership.

## Authority and Source Coverage

| Boundary | Governing authority | Source and evidence inspected | Assessment |
|---|---|---|---|
| Identity and captured execution authority | `AGENTS.md` §§2, 9; `architecture/wyrd-security-posture.md` §§Security principles, Access and refresh tokens, Authorization and policy; spec REQ-032/032A, INV-022, AC-027 | `components/auth/caller_extractor.rs:11-52`; `components/workflow/host.rs:75-123,230-388`; `components/workflow/tools.rs:42-107`; `components/gateway/workflow.rs:34-63` | Source passes. `Caller` contains verified tenant, principal, request id, and verified delegation only; no bearer or refresh secret is retained. Fresh HTTP requests re-authenticate, while accepted work uses the immutable permission snapshot. |
| Workflow RBAC and audit | Security posture §§Authorization and policy, Audit integrity and privacy; spec REQ-032/033, AC-010 | `wyrd-runtime/src/permission.rs` (`Resource::Workflows`, `Permission::workflow_run`); `builtin_roles.rs` writer/agent grants and negative role test; `components/workflow/host.rs:81-123,125-204`; `audit/mod.rs:211-276`; recorded `admission_is_audited_and_side_effect_free_on_refusal` evidence | Pass. Every run lookup occurs after the canonical fail-closed audited permission decision. Gateway calls explicitly use `authorized = false` at `components/gateway/workflow.rs:111-113`, preserving gateway-owned admission and audit. |
| Run and idempotency tenancy | Security posture §§Security principles, Tenant and data isolation; spec REQ-030/032/034C, AC-018/021/022 | `components/workflow/runs.rs:38-48,80-106,189-200,249-329,340-386,616-621`; `components/workflow/host.rs:93-112,125-204`; recorded lifecycle/idempotency evidence | Source passes. Keys are `(tenant, principal, IdempotencyKey)` and lookup/cancel require the same verified tenant and principal. Unknown, malformed, evicted, and non-owner paths share one error projection. Required cross-tenant runtime proof is incomplete; see SEC-TEN-1. |
| Exact graph and Card tenancy | Security posture §§Tenant and data isolation; spec REQ-029/034/052, AC-009/025 | `components/workflow/host.rs:299-388`; `components/cards/resolve.rs:548-710`; `components/cards/routes.rs:156-189`; `components/cards/service.rs:120-143`; recorded tool and admission evidence | Source passes. Graph reads open a tenant-scoped registry transaction from `caller.data_tenant_id`; tool Card reads authorize/audit through the shared exact-ref boundary, then read via the caller's tenant connection. Cross-tenant journey evidence is incomplete; see SEC-TEN-1. |
| Bifrost tool authorization and result confidentiality | Security posture §§Authorization and policy, Tenant and data isolation, Audit integrity and privacy; spec REQ-052, AC-025 | `components/workflow/tools.rs:139-207`; `query/collect.rs:31-198,200-257,305-329`; `query/service.rs:244-280`; recorded `declared_tools_use_captured_scopes_and_owned_services` and forwarded-settlement evidence | Source passes. Tool arguments expose no tenant/principal/endpoint selector, the captured caller reaches `stream_query`, Oracle owns object authorization/audit, and collector failures return no partial rows. Second-tenant proof is missing; see SEC-TEN-1. |
| External gateway secret and tenant boundary | Security posture §§Source credentials and SSRF defense, Cryptography and secret handling; spec REQ-034/042/049/050, AC-016/023 | `config.rs:1660-1804`; `workflow/host.rs:324-348`; `wyrd-client/src/workflow/local.rs:115-157`; `skald-workflow/src/route.rs` binding validation, origin equality, endpoint policy, sensitive headers; recorded `server_routes_keep_gateway_and_external_ownership` evidence | Pass. Only a selected binding assigned to the verified tenant is resolved; absent/foreign assignment becomes unavailable. Secret values use redacted types and sensitive headers, stay out of run values, and endpoint screening remains with the existing provider transport owner. |
| Security-mechanism drift | Human standing direction; task non-goals; spec INV-001/005/009/010/012 and non-goals | Complete diff, especially new Workflow owner, Cards/query reuse, gateway adapter, config, test probes, and Bifrost cleanup remediation | Pass. No new policy engine, authorization cache, audit writer, token-renewal path, tenant selector, security allowlist, custom crypto, or standalone security check was added. The test-only preparation gate and existing fault-injection patterns are scoped proof seams, not production controls. No unsupported bespoke security mechanism is required by the correction below. |

## Security Audit

### Critical

None.

### High

None.

### Medium

- **SEC-TEN-1 — MISSING: required second-tenant security journey is absent.**
  - Violated obligations: AC-009 requires cross-tenant Workflow-reference refusal without provider/tool execution; AC-018 requires another tenant and another principal to receive the same not-found result for an owned run; AC-025 requires a second tenant to produce no unauthorized tool data. REQ-032 independently requires foreign-tenant run lookup and cancellation to be indistinguishable from nonexistent state.
  - Exact evidence: `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:884-918` proves under-privileged roles only in the fixture's tenant; `:964-1006` uses an unknown version but does not submit an existing Workflow from another tenant; `:1406-1442` uses one same-tenant principal lacking both read grants; `:1527-1598` proves a binding assigned to a random foreign tenant ID is unavailable but authenticates no second tenant; and `:1695-1722` compares unknown/malformed/evicted runs with a different principal created by the same fixture, not a foreign tenant. The file contains no second-tenant fixture or credential. The implementation evidence table nevertheless marks S1, S4, and S6 `PASS`, and the material limit claims the common 404 is journeyed.
  - Observable consequence: the static source strongly indicates correct isolation, but the required real client → server proof never crosses the credential-derived tenant boundary. A regression in token-to-tenant routing, tenant connection selection, run-table qualification, or tool-call context could therefore leave the recorded TASK-004 evidence green while violating the explicit cross-tenant acceptance contract.
  - Smallest testable correction: extend the existing `pg_workflow_runs` journey surface using the repository's normal second-tenant provisioning and authenticated client path. Prove (a) a second tenant cannot submit the first tenant's exact active Workflow and causes no upstream/tool call, (b) its get and cancel of the first tenant's run produce the same stable 404 body as an unknown run, and (c) its built-in Card/query calls cannot observe the first tenant's seeded objects or rows. Reuse the current auth, tenant provisioning, registry, Cards, Bifrost, and Workflow clients; do not add a new tenant simulator, security gate, static check, configuration option, or bespoke harness.

### Low / Defense In Depth

None. Optional hardening and speculative controls are intentionally omitted.

### Positive Controls

- `Caller::from_authenticated` derives tenant, effective principal, credential attribution, permissions, and delegation from one verified principal and stores no bearer token.
- Workflow create/get/cancel use the typed `Permission::workflow_run()` and the canonical fail-closed audit owner before run-table disclosure.
- Writer and agent receive the exact grant; admin uses the existing wildcard; reader and runtime_admin remain excluded.
- Run ownership and idempotency are both qualified by verified tenant and effective principal.
- In-process gateway calls pass `authorized = false`, retaining current gateway authorization, deployment, credential, budget, and audit ownership.
- Built-in tools have closed argument shapes with no tenant, principal, credential, endpoint, path, or visibility selector.
- Cards reads reuse the existing exact-ref authorized/audited boundary; Bifrost queries reuse the existing query authorization, object decision, tenant context, audit, terminal, and result-bound owners.
- External binding selection requires exact tenant assignment; secret values are resolved only for selected bindings, use redacted wrappers/sensitive headers, and are excluded from model-visible failures and run snapshots.
- Model-visible tool errors retain stable catalog metadata but discard source messages/details that could contain sensitive data.

## Verification Limits

- Per instruction, this review ran no builds, tests, Cargo, or mise tasks.
- Recorded command results in the task were treated as claims and checked against the named source. They were not re-executed.
- Live gateway deployment removal mid-run, production-profile external egress negatives, and fallback/dialect combinations remain the task's recorded limits and rely on existing owner evidence. No defect was found in the TASK-004 composition for those limits.
- The missing second-tenant journey is a failed acceptance obligation, not merely a verification limit, because AC-009, AC-018, and AC-025 explicitly require that proof.

## Proposed Finding Ledger

| Source ID | Classification | Location | Violated obligation | Result |
|---|---|---|---|---|
| SEC-TEN-1 | MISSING | `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:855-1083,1302-1489,1604-1722` | AC-009, AC-018, AC-025; REQ-032 foreign-tenant proof | Retain for independent validation |

## Overall Result

**FAIL**

The implementation's inspected security and tenancy boundaries are coherent and no exploitable authorization, secret, injection, or tenant-selection defect was found. TASK-004 does not yet satisfy its exact approved acceptance contract because its recorded production-shaped evidence never crosses a real second-tenant credential boundary.
