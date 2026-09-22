# Final integrated change review — admin principals

## Review Findings

### Critical

None.

### Important

- **CR1 — VIOLATION** — [`crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs:49`](../../../../../crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs), [`auth/workload_binding.rs:43`](../../../../../crates/wyrd/wyrd-cli/src/auth/workload_binding.rs), [`auth/refresh.rs:17`](../../../../../crates/wyrd/wyrd-cli/src/auth/refresh.rs), [`auth/issue_key.rs:40`](../../../../../crates/wyrd/wyrd-cli/src/auth/issue_key.rs), and [`principal/revoke.rs:40`](../../../../../crates/wyrd/wyrd-cli/src/principal/revoke.rs) still accept bearer, refresh, or OIDC client credentials through command-line arguments, and most bearer/refresh values are plain `String` fields on `Debug` argument structs. This exposes credentials through shell history and process listings and can expose them through diagnostics, contrary to `architecture/wyrd-security-posture.md:342-345` and the no-recoverable/no-diagnostic-secret obligation in `INV-002`. Remove secret-valued CLI options from the complete in-scope CLI surface, use the existing ambient client configuration for access tokens, accept refresh and OIDC secrets only through non-argv sources, carry all secret material in redacting types, and test both argument rejection and redacted debug behavior. The R8 evidence's classification of three commands as out-of-scope is not supported by the repository security authority; the same pattern is also present in `eval/run.rs:54`, `query/mod.rs:28`, and other changed administration commands.
- **CR2 — INCORRECT** — [`crates/wyrd/wyrd-auth/src/issuance.rs:128`](../../../../../crates/wyrd/wyrd-auth/src/issuance.rs) deliberately returns no credential id for `TenantGrant::Delegation`, and the successful delegation event at [`issuance.rs:533`](../../../../../crates/wyrd/wyrd-auth/src/issuance.rs) therefore omits the credential that authenticated the delegating caller even though the refusal path preserves it at [`exchange_api_key.rs:325`](../../../../../crates/wyrd/wyrd-auth/src/exchange_api_key.rs). A successful delegation authorized by an API-key- or refresh-attributed caller produces an allowed canonical audit row with `credential_id = NULL`, so audit cannot answer which credential performed the operation as required by `REQ-037` and `AC-009`. Carry the verified caller's optional credential id through the private delegation grant only for audit attribution, attach it to the successful exchange event, preserve `None` for federated/already-delegated callers, and add a Postgres test whose source token has a non-null credential id while the resulting delegated JWT remains non-credential-attributed.

### Suggestions

None.

## Verdict

**BLOCKED**

The integrated candidate cannot pass the final change gate because the mandatory independent cumulative task review of the R8 implementation does not exist. The R8 task explicitly requires the next `$wyrd-task-review` to reassess the complete candidate (`whole-branch-08/TASK-001-008-R8-close-validated-findings.md:5-6`), `$wyrd-task-review` defines `PASS` as the task completion gate, and `$wyrd-change-review` requires a credible PASS review bound to every task implementation. The latest cumulative verdict is still `FIX_REQUIRED` and is bound to `eb9b2f69cb883fa508ed168f21cb868451e61b82`, before the R8 code; its implementation evidence at `84b7f4ad6` is not an independent two-wave review. The only PASS verdict in the packet is the older, narrower `task-008-r6` review.

This final integrated review is not a substitute for the required `$wyrd-task-review`, because that workflow mandates its own two independent review waves, validation ledger, and completion gate. Route the immutable cumulative candidate through `$wyrd-task-review`; that review must also validate CR1 and CR2 above. No remediation task is emitted while the required review gate is absent.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Integrated target: `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 13, SHA-256 `57f91317e68b06e7b4d34ea94b964e4a1dd99678275a2ee67d1d51f9b4b46332`
- Reviewed range: `c5c20754a167e8f4d74a555a720bd51df6179a6f..84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Final code candidate identified by the R8 evidence: `20e5becad`; the target adds the evidence record only.

## Acceptance Matrix

Status meanings: **PASS** is supported by inspected source plus the recorded final-tree evidence; **FAIL** is a validated material defect; **BLOCKED** is a missing mandatory workflow gate.

### Required behavior

| Obligation | Status | Integrated evidence / assessment |
|---|---:|---|
| REQ-001 | PASS | Durable principal/credential separation is enforced by the platform and tenant schemas and domain types. |
| REQ-002 | PASS | The closed principal-kind model covers platform, human, tenant-admin, service, and agent identities with plane-validating conversions. |
| REQ-003 | PASS | Tenant principals retain typed tenant ownership and Card binding only where the kind requires it. |
| REQ-004 | PASS | Platform principals are tenantless and constrained to platform-valid kinds. |
| REQ-005 | PASS | Credential rows reference principals rather than acting as identities or grant owners. |
| REQ-006 | PASS | Multiple independently revocable credentials per principal are represented and covered by rotation/revocation tests. |
| REQ-007 | PASS | Plaintext is returned only at issuance while storage keeps verifier and lookup metadata. |
| REQ-008 | PASS | Credential lookup and verification preserve the single indistinguishable invalid-credential result. |
| REQ-009 | PASS | Credential metadata, expiry, revocation, and use tracking are durable and typed. |
| REQ-010 | PASS | Rotation overlaps credentials rather than mutating principal authority. |
| REQ-011 | PASS | Credential issuance and revocation use the canonical transactional audit path. |
| REQ-012 | PASS | The shared `TenantTokenIssuer` owns all five tenant issuance paths and mints five-minute JWT authority snapshots. |
| REQ-012a | PASS | Tenant request verification is local signature/issuer/audience/expiry verification without a database or cache. |
| REQ-012b | PASS | Revocation and grant/principal changes stop new issuance while existing tenant JWTs expire within five minutes. |
| REQ-012c | PASS | Delegation intersects verified caller authority with current target grants and retains the existing `act` representation. |
| REQ-013 | PASS | One typed authenticated context distinguishes platform and tenant callers. |
| REQ-014 | PASS | Authentication establishes principal identity and credential attribution independently of authorization grants. |
| REQ-015 | PASS | Tenant authorization consumes signed effective permissions rather than credential records. |
| REQ-016 | PASS | Platform authorization revalidates current principal, credential, and grants through `OperatorPool`. |
| REQ-017 | PASS | Typed permissions and resources drive authorization checks. |
| REQ-018 | PASS | Platform-to-tenant administration remains an explicit, separately authorized and audited capability. |
| REQ-019 | PASS | Tenant SQL enters through `TenantConn` and RLS; affected boundary checks and isolation tests pass. |
| REQ-020 | PASS | `wyrd-server init` creates the singleton root principal and first credential transactionally. |
| REQ-021 | PASS | Initialization is idempotent/refusing after completion and concurrency-tested. |
| REQ-022 | PASS | Initialization prints the credential once and persists no plaintext. |
| REQ-023 | PASS | Platform routes refuse before initialization rather than fabricating authority. |
| REQ-024 | PASS | Initialization failure/rollback and retry behavior is covered by the platform journey. |
| REQ-025 | PASS | Authorized tenant creation provisions tenant, built-in grants, tenant admin, credential, and audit atomically/resumably. |
| REQ-026 | PASS | Provisioning state and retry/concurrency convergence are represented and journey-tested. |
| REQ-027 | PASS | Tenant credentials are returned once with verifier-only persistence. |
| REQ-028 | PASS | Tenant lifecycle authorization and audited no-effect decisions are implemented. |
| REQ-029 | PASS | Tenant administration exposes principal lifecycle, roles, and credentials through typed contracts. |
| REQ-030 | PASS | Service principals may be Card-free for administration while Agent and deployed workload constraints remain enforced. |
| REQ-031 | PASS | Tenant administrative operations use tenant-scoped transactions and RLS. |
| REQ-032 | PASS | Global credential recovery remains an operator/database action, not an HTTP route. |
| REQ-033 | PASS | Tenant-admin recovery is an explicit platform-plane operation preserving separation and audit. |
| REQ-034 | PASS | Verified tenant OIDC identity resolves into the same tenant principal/context model. |
| REQ-035 | PASS | Unknown or invalid tenant OIDC identities fail closed; identity journeys pass. |
| REQ-036 | PASS | HTTP, OpenAPI, CLI, SDK, MCP, and documentation project the server-owned identity contract without a second durable authority. |
| REQ-037 | **FAIL** | CR2: successful delegation loses the authenticating caller credential id in its allowed audit row. |
| REQ-038 | PASS | The bootstrap-key command, fabricated bootstrap Card, operator identity, and task are removed. |
| REQ-039 | PASS | Obsolete platform user/role/key schema and query slots are removed by the branch migrations/code. |
| REQ-040 | PASS | Stale bootstrap and hybrid-auth architecture is removed from live code/docs; classified historical review text is inert. |
| REQ-041 | PASS | Platform principals hold durable grants and authorize through current-state platform reads. |
| REQ-042 | PASS | Platform OIDC subjects map only to pre-authorized platform principals; no just-in-time platform admin creation. |
| REQ-043 | PASS | The optional platform OIDC connection is deployment-owned and separate from tenant OIDC. |
| REQ-044 | PASS | Platform login remains available alongside credential administration and fails closed on invalid federation. |
| REQ-045 | PASS | Platform authorization observes suspension/revocation/grant changes on the next request. |
| REQ-046 | PASS | Platform identity and grant administration uses the platform pool and canonical audit path. |
| REQ-047 | PASS | Changed CLI route calls use the shared `wyrd-client` transport/header path rather than private HTTP stacks. |
| REQ-048 | PASS | Client renewal differentiates API-key re-exchange and rotating human refresh behavior, including replay revocation. |
| REQ-049 | PASS | Runtime OpenAPI remains generated from mounted `utoipa` routes and contract-tested without a checked-in snapshot authority. |

### Invariants

| Obligation | Status | Integrated evidence / assessment |
|---|---:|---|
| INV-001 | PASS | Credentials authenticate principals; grants and authorization attach to principals. |
| INV-002 | **FAIL** | CR1: live CLI flags place raw credentials in argv/process and shell-history surfaces; several are plain debug-visible strings. |
| INV-003 | PASS | Verified credentials bind exactly one principal and tenant; caller input does not select authority. |
| INV-004 | PASS | Platform and tenant control planes remain distinct in types, pools, routes, and checks. |
| INV-004a | PASS | Platform authority does not implicitly grant tenant data access. |
| INV-004b | PASS | Tenant identities and grants cannot create or confer platform authority. |
| INV-005 | PASS | Initialization uniqueness is enforced and concurrency-tested. |
| INV-006 | PASS | Tenant creation converges without duplicate usable tenants/admins. |
| INV-007 | PASS | Tenant principal/credential/grant data remains RLS-confined. |
| INV-008 | PASS | Principal suspension/revocation semantics match the five-minute tenant snapshot/current platform-state split. |
| INV-009 | PASS | Credential rotation and revocation do not mutate principal identity or unrelated credentials. |
| INV-010 | PASS | Unknown/invalid credentials retain indistinguishable public failures. |
| INV-011 | PASS | Audit failure closes the covered operation rather than returning an unaudited authorization result. |
| INV-012 | PASS | Platform recovery and tenant-admin recovery preserve their separate authority boundaries. |
| INV-013 | PASS | Delegated tokens retain the approved chain/depth representation. |
| INV-013a | PASS | Delegated effective authority is a semantic intersection and cannot amplify the caller. |
| INV-014 | PASS | No new durable identity Card kind or parallel identity authority is introduced. |
| INV-015 | PASS | Runtime OpenAPI has one generator/served document rather than a parallel checked-in contract. |

### Acceptance criteria

| Obligation | Status | Integrated evidence / assessment |
|---|---:|---|
| AC-001 | PASS | Principal-kind, plane, tenancy, and Card-binding cases are covered by focused and SQL tests. |
| AC-002 | PASS | Multiple credentials, one-time plaintext, verifier-only storage, rotation, and revocation evidence is present. |
| AC-003 | PASS | Invalid-key timing/error-shape behavior and single verification are covered. |
| AC-004 | PASS | One authenticated context and the platform/tenant authorization split are exercised. |
| AC-005 | PASS | Initialization happy, refusal, rollback, retry, and concurrency paths pass the platform journey. |
| AC-006 | PASS | Tenant provisioning happy, failure, retry, and concurrency behavior passes. |
| AC-007 | PASS | Administration and recovery flows preserve plane separation and transactional effects. |
| AC-008 | PASS | Suspension blocks new issuance/current platform sessions while tenant snapshots remain expiry-bounded. |
| AC-009 | **FAIL** | CR2: a successful delegation decision cannot name the credential that authenticated its direct caller. |
| AC-010 | PASS | Persistence, error uniformity, local JWT verification, removed hybrid-auth machinery, current platform reads, and RLS checks are evidenced. |
| AC-011 | PASS | Human and machine identity paths converge on the same authenticated/authorized model. |
| AC-012 | PASS | Platform human administration and credential fallback behavior pass identity/platform journeys. |
| AC-013 | PASS | HTTP/OpenAPI/CLI/SDK/MCP surfaces project the principal model and stable errors. |
| AC-014 | PASS | Bifrost verifies JWT authority locally and authorizes resolved table identity, including gRPC journey coverage. |
| AC-015 | PASS | Tenant service-principal creation/grant/credential paths and restrictions are covered. |
| AC-016 | PASS | Platform and tenant recovery paths do not create a second identity or cross plane implicitly. |
| AC-017 | PASS | Client/header consolidation and removal of duplicate route clients are verified. |
| AC-018 | PASS | Five issuance paths, automatic re-exchange, refresh rotation, and replay-family revocation are evidenced. |
| AC-019 | PASS | Served OpenAPI route/auth/body/problem coverage is runtime-tested and the checked-in snapshot was removed. |
| AC-020 | PASS | Delegation scope combinations, exactly-one allowed/denied decision, no-effect decisions, and fail-closed audit behavior are tested; CR2 is attribution, not the count/outcome invariant stated here. |

### Material constraints

| Constraint | Status | Integrated evidence / assessment |
|---|---:|---|
| Server is the sole durable identity/credential/tenancy/authorization authority | PASS | Clients project server contracts and do not persist competing authority. |
| Full tenant separation across state and artifacts | PASS | RLS, `TenantConn`, pool-boundary checks, and tenant-isolation lanes pass. |
| Existing contracts remain authoritative except explicit spec amendments | PASS | Hybrid revocation/permission contracts are removed and the standard JWT snapshot architecture is consistently applied. |
| Security/availability uncertainty fails closed | PASS | Authentication, authorization, audit, OIDC, and store failures refuse without fallback. |
| No compatibility route, alias, legacy name, or migration shim | PASS | No unshipped compatibility layer was added; the human rejection of R8-4 is honored. |
| RBAC is not overbuilt | PASS | The implementation extends typed permissions without introducing a parallel policy engine or customizable-role redesign. |
| Narrow verification authority is respected | PASS | Evidence uses the specified focused and owning lanes; no prohibited aggregate is claimed as acceptance proof. |

### Non-goals

| Non-goal | Status | Integrated evidence / assessment |
|---|---:|---|
| Redefine tenant OIDC mechanics | PASS | Existing verification mechanics are reused; this change owns only the identity seam and platform connection. |
| Build a new RBAC engine or scope language | PASS | Existing `Permission` semantics are retained; intersection is a narrow delegation operation. |
| Add Principal/Credential/Tenant Card kinds | PASS | Administrative identity remains server state. |
| Add application-level global credential recovery | PASS | Recovery remains operator/database-only. |
| Add billing, quotas, organization, deletion, or migration | PASS | None is introduced. |
| Change apply provisioning, delegation representation/depth, or emit scope | PASS | Those contracts remain; only the explicitly amended attenuation/audit behavior changes. |
| Add a customer signup UI | PASS | No UI signup authority is introduced. |

### Verification obligations and workflow gates

| Obligation | Status | Integrated evidence / assessment |
|---|---:|---|
| VER-001 | PASS | Evidence covers the named changed surfaces, including pre-existing CLI route callers for header consolidation. |
| VER-002 | PASS | The R8 record lists 16 exact focused nextest expressions, each selecting and passing one test. |
| VER-003 | PASS | Required lints and narrow lanes are recorded; prohibited broad aggregates are not used as acceptance evidence. |
| VER-004 | PASS | Compilation/testing remains scoped to touched crates and direct surfaces. |
| VER-005 | PASS | No unrelated failure is used to block or weaken the change. |
| VER-006 | PASS | `codegen:check` and runtime OpenAPI contract tests pass; generated declarations show no drift. |
| Original TASK-001 through TASK-008 implementation/review chain | **BLOCKED** | Only `task-008-r6` has a PASS verdict; all cumulative verdicts through `whole-branch-08` are FIX_REQUIRED, and no review verdict is bound to the post-R8 code candidate/target. |

## Cross-task and contract assessment

- The principal/credential schema, typed identity model, shared tenant issuer, local verifier, platform current-state authorization, client renewal, Bifrost admission, OpenAPI routing, and canonical audit staging compose coherently on the integrated tree.
- The revised five-minute stateless tenant JWT architecture is consistently implemented: issuance resolves current grants; requests verify claims locally; principal, credential, tenant, and grant changes stop subsequent issuance; the lower-volume platform plane continues to revalidate current state.
- The explicit human decisions to retain the audit-publisher `FOR UPDATE NOWAIT` correction (`FIND-admin-principals-R8-1`) and omit an unshipped compatibility migration (`FIND-admin-principals-R8-4`) are honored and are not findings.
- CR1 is a cross-surface secret-handling defect, not a request to redesign client authentication. CR2 is a narrow loss of existing audit context across the delegation-to-issuer seam, not a request to alter JWT structure or delegation semantics.

## Open Questions

None. Both implementation defects have bounded fixes, and the blocking workflow route is explicit.

## Verification Notes

- Inspected the complete `c5c20754a167e8f4d74a555a720bd51df6179a6f..84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6` diff and relevant callers using CodeGraph-first exploration followed by line-level source inspection.
- Read the approved revision-13 specification, original TASK-001..008 packets, all review/remediation artifacts, R8 evidence, repository rules, security posture, design/doctrine, Bifrost design, testing rules, and spec-driven workflow authority from the immutable target.
- The R8 record reports: `fmt:check`, workspace lints, client-tier/unwrap/clippy-allow/tenant-isolation/from-pools checks, principal unit/integration tests, shared and SQL tests, two consecutive platform journeys, identity/CLI/Bifrost MCP/server journeys, Bifrost server integration, codegen, docs, strict rustdoc on affected crates, and diff whitespace all passing. Those results were assessed as evidence; this review did not claim to rerun them.
- The recorded Python-command substitution is environmental and does not change the scripts or their asserted results.
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f..84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6` is clean.
- The final identity recheck found `HEAD == 84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`; only this review artifact is uncommitted.

