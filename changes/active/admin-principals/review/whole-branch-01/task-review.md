# Whole-branch task implementation review — admin principals

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `072cf8b30c7135e8cf15f92da3e371a9c999703c` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 7 |
| Original tasks | `TASK-001` through `TASK-008` and every non-superseded remediation task under `changes/active/admin-principals/review/` |

The candidate remained at `072cf8b30c7135e8cf15f92da3e371a9c999703c`
during this review. The complete base-to-candidate diff was reviewed. Prior
implementation summaries were not used as acceptance evidence.

## Overall result

**FAIL**

The principal, credential, two-plane authentication, platform-human identity,
shared-client consolidation, and most credential-lifecycle behavior are present.
The cumulative candidate nevertheless does not satisfy the approved change. It
still lacks the required tenant-directory lifecycle surface and shipped operator
CLI, leaves cancelled provisioning permanently unresumable, permits recovery
against non-active tenants, omits the explicitly required architecture
amendments, crosses the mandated SQL boundary with raw pools, lacks several
required journey proofs, and violates the repository's commit-identity rule.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001..011, INV-001..003, INV-008..009, INV-012 — durable principals independent from principal-generic, verifier-only credentials; optional Card binding; multiple independently revocable credentials | `20260601000020_admin_principals.sql`; `wyrd-runtime/src/principal.rs`; `wyrd-auth/{issue_api_key,platform_credentials}.rs`; tenant and platform credential query owners | `pg_admin_principals.rs`; credential and rotation journeys in `platform_admin_e2e.rs`; prior TASK-001 remediation closure | PASS |
| REQ-012..019, INV-004..004b, INV-013, INV-015 — two entry paths, verified token request path, closed platform/tenant contexts, grant-based authorization, canonical `X-Wyrd-Access-Token` | `AuthContext`; `PlatformSessions`; `PlatformCaller`; tenant `Caller`; `PlatformAuthorization`; canonical client transport | Cross-plane journey, platform session tests, auth-check/runtime tests, TASK-002 and TASK-008 remediation evidence | PASS |
| REQ-020..023, INV-005 — explicit, one-transaction, once-only initialization | `boot/init.rs:94-140` hashes before opening one operator transaction and commits principal, grant, and credential together; the unique root name serializes competing inserts | `platform_admin_e2e.rs:150-170` proves sequential refusal only | PASS for implementation; FAIL for required concurrent, injected-failure, and output-capture proof — TREV-007 |
| REQ-024 — uninitialized server serves ordinary traffic and platform operations refuse stably | platform handlers reject absent platform configuration/session | No real-server test starts an uninitialized deployment and proves both halves | FAIL — TREV-007 |
| REQ-025..027, INV-006 — provision a complete non-usable-then-active tenant and make failures/cancellation/retries/concurrency converge | `TenantProvisioning` orders directory creation, tenant transaction, then activation; `insert_provisioning_tenant` adopts only `failed` rows | Happy path and a synthetic failed-row retry exist | FAIL — a cancelled/stale `provisioning` row cannot be adopted and failure marking is discarded (TREV-002); required fault/concurrency proof is absent (TREV-007) |
| REQ-028, AC-008 — list, inspect, suspend, and resume tenants through the platform plane; suspend blocks credentials and live tokens | `set_tenant_suspended` exists but has no production caller; `platform_router` exposes only create and recovery | `a_suspended_tenant_admits_no_credential` mutates `platform.tenants` directly; no lifecycle route or live-token epoch journey exists | FAIL — TREV-001 |
| REQ-029..031, REQ-046, AC-004, AC-012, INV-007 — tenant administration, restricted machines, RLS isolation, no platform escalation | `components/principals/routes.rs` and tenant `TenantConn` queries; platform types cannot be created there | restricted-machine and escalation journeys exist; cross-tenant behavior follows forced RLS, but the remediation's named principals/credentials negative journey is absent | PASS for implementation; FAIL for required proof — TREV-007 |
| REQ-032 — recovery reuses the same tenant-admin principal without changing grants or giving the platform ordinary tenant access | `components/platform/recovery.rs:69-130` resolves `tenant_admin_principal_id` then inserts only a credential | `tenant_administration_survives_losing_every_credential`; cross-plane journey | PASS for active tenants; FAIL for the non-active-tenant remediation obligation — TREV-003 |
| REQ-033 — global credential loss remains deployment-level recovery | no application self-recovery route for the platform root | source inspection | PASS |
| REQ-034..035 — federated tenant humans resolve to independent principals and the same context shape | existing tenant OIDC resolver remains; platform identity is separate | identity journeys and prior TASK-007 review evidence | PASS |
| REQ-041..046, AC-015..017 — platform grants, pre-registration, one optional platform OIDC connection, identity pinning, plane separation | platform identity migration/query/auth/server owners | platform administrator journeys in `platform_admin_e2e.rs`; prior TASK-007/008 closure | PASS |
| REQ-047 — every Wyrd-owned HTTP caller uses `wyrd-client` | CLI call sites route through `wyrd-cli/src/client.rs`; shared handles own status/header behavior | CLI journey evidence and TASK-008 final PASS | PASS |
| REQ-036, AC-014 — typed HTTP/OpenAPI plus CLI, SDK, and MCP projections of the administrative operations this change builds | OpenAPI and shared-client principal/platform handles exist; MCP credential tools exist | codegen and CLI issue/revoke journeys | FAIL — the required `wyrd tenant create` operator command does not exist, and missing lifecycle routes cannot be projected (TREV-001, TREV-004) |
| REQ-037, AC-009 — allowed/denied decisions are transactionally audited and audit failure refuses | platform and tenant authorization owners append before effect; credential routes reuse those seams | allowance/denial coverage exists | FAIL for evidence: the cumulative remediation explicitly requires injected audit-append failure proof on the tenant principal/credential surface and none exists — TREV-007 |
| REQ-038 — remove `bootstrap-key`, fabricated bootstrap Card, synthetic operator, and its task | command, route, fixture, and task are removed | repository search finds no production path, but `wyrd-auth/src/audit.rs:194` still contains `bootstrap-key` despite remediation criterion 24 requiring no reference outside `changes/` | FAIL — TREV-008 |
| REQ-039 — remove/repurpose unreachable `platform.users`, roles, user_roles, api_keys | migration replaces them with platform principal/credential/grant state; obsolete query modules deleted | migration tests | PASS |
| REQ-040, AC-002 — document and prove the three-command operator journey, rotation, and both recovery paths | self-hosting pages mention `init` and HTTP administration | no shipped `wyrd tenant create`; `running-the-server.svx` gives no create/configure commands; `local-development.svx:30-39` calls init a bootstrap command and shows the wrong `wyrd_sk_acme_…` global prefix | FAIL — TREV-004 |
| Required architecture amendments | none of the named architecture files or `tenant-oidc-federation/spec.md` changed | current authority still declares the old closed kind set and Card-only machine model | FAIL — TREV-005 |
| SQL ownership constraint — only `TenantConn` and `OperatorPool`, never raw `PgPool`, in library fields/signatures | new provisioning and recovery handles store and accept `sqlx::PgPool` | source inspection; the current `check:from-pools-allowlist` does not check this rule and therefore passes vacuously | FAIL — TREV-006 |
| Non-goals — no new RBAC engine, Card kind, organization noun, UI, explicit deny, or compatibility surface | diff adds none | complete diff inspection | PASS |
| VER-006 — generated schemas/OpenAPI remain synchronized | generated schema and OpenAPI changes are present | prior `codegen:check` evidence; no drift observed in the reviewed diff | PASS |
| Current user requirement: entire repository green | available focused evidence is substantially green | `auth_e2e::cache_ttl_path_also_flips_verdict` is red, but the same `DelegateError::Database(_)` failure at `exchange_api_key.rs:791` reproduces at the branch base | NOT A CANDIDATE REGRESSION; repository-green closure remains a verification limit, not a task finding |
| AGENTS.md §13 — local contributor identity, never AI identity or AI co-author trailers | branch history contains commits authored/committed as `Claude <noreply@anthropic.com>` and many `Co-Authored-By: Claude…` trailers | complete `git log` audit found 58 offending commits | FAIL — TREV-009 |

## Proposed findings

### TREV-001 — MISSING — tenant lifecycle administration was never exposed

- **Violated obligation:** REQ-028, REQ-036, AC-008, TASK-004 criteria, and
  remediation R1 criterion 20.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/platform/routes.rs:37-44`;
  `crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:150-180`;
  `crates/shared/wyrd-client/src/platform/handle.rs:91-137`.
- **Evidence:** `platform_router` registers only tenant creation and tenant-admin
  credential recovery. `set_tenant_suspended` has no caller. The shared platform
  handle likewise offers only create and recovery. There are no list, inspect,
  suspend, or resume contracts.
- **Observable consequence:** a platform administrator cannot perform four
  explicitly required tenant-directory operations through any shipped surface.
  The suspension journey changes the database directly and therefore does not
  prove the product capability or its authorization/audit behavior.
- **Required testable correction:** wire the existing tenant-directory owner and
  `set_tenant_suspended` into typed, audited list/inspect/suspend/resume HTTP
  operations; project those exact operations through `wyrd-client` and the
  operator CLI; regenerate OpenAPI; prove suspend and resume through the real
  client/server route, including a token minted before suspension.

### TREV-002 — INCORRECT — cancellation can strand a tenant forever in `provisioning`

- **Violated obligation:** REQ-027, AC-007, INV-006, remediation R1 criteria
  9-11, and its decision-complete instruction to adopt stale `provisioning` or
  `failed` rows.
- **Exact location:** `crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:33-49`;
  `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:140-180`.
- **Evidence:** the durable claim adopts only `status = 'failed'`. Once the first
  transaction commits the new `provisioning` row, cancellation or process death
  before the error arm leaves that row forever. A retry gets the synthetic unique
  violation. Even on an ordinary error, `mark_tenant_failed` is assigned to
  `let _`, so a failed status write produces the same stuck state.
- **Observable consequence:** one interrupted tenant creation permanently burns
  its slug and cannot converge through retry, exactly the failure the approved
  remediation required the shared claim owner to close.
- **Required testable correction:** implement the already-decided resumable claim
  in `insert_provisioning_tenant` for stale/incomplete `provisioning` as well as
  `failed`, preserving the existing tenant id and serializing concurrent claimants;
  stop discarding failure-state write errors. Add a cancellation/failure journey
  that retries the same slug and a concurrent-create proof yielding one tenant,
  one administrator, and no orphan credentials.

### TREV-003 — INCORRECT — tenant-admin recovery ignores tenant lifecycle state

- **Violated obligation:** remediation R1 criterion 8 and its approved decision
  to apply the tenant lifecycle refusal in `TenantRecovery::recover`; REQ-026's
  non-usability rule and INV-011 fail-closed behavior.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/platform/recovery.rs:69-124`.
- **Evidence:** recovery authorizes the platform permission, opens a tenant
  connection, resolves the admin, and inserts a credential without ever reading
  `platform.tenants.status` through the `OperatorPool` it already owns.
- **Observable consequence:** recovery against `provisioning`, `failed`, or
  `suspended` state creates fresh credential material for a tenant the lifecycle
  contract says is not usable. Authentication later refuses it, but the recovery
  operation itself falsely reports success and creates unnecessary durable secret
  state.
- **Required testable correction:** reuse the platform tenant-status lookup at the
  start of the existing recovery owner and refuse every non-active state without
  revealing tenant inventory; prove no credential row is inserted for each
  refused state.

### TREV-004 — MISSING — the shipped CLI and documentation do not deliver the operator journey

- **Violated obligation:** AC-002, REQ-036, REQ-040, TASK-004/TASK-005/TASK-006
  CLI criteria, and remediation R1 criteria 22-23.
- **Exact location:** `crates/wyrd/wyrd-cli/src/cli.rs:55-85`;
  `docs/src/content/docs/self-hosting/running-the-server.svx:31-46`;
  `docs/src/content/docs/self-hosting/local-development.svx:30-48`.
- **Evidence:** the top-level CLI has no `Tenant` command, so the specified
  `wyrd tenant create --name acme` command cannot run. The real-server operator
  test calls `initialize_platform_root` and raw HTTP helpers rather than the
  shipped server subcommand and CLI. The docs say tenant creation is possible but
  show no create/configure/rotation/recovery commands; local-development calls
  init a bootstrap command and prints the tenant-shaped `wyrd_sk_acme_…` example
  instead of the global `wyrd_global_…` credential.
- **Observable consequence:** the promised three-command headless operator path
  is not a product surface a user can follow, and the documented credential shape
  contradicts the implementation.
- **Required testable correction:** add only the missing operator commands on the
  existing `wyrd-client` handles (including tenant creation and the administrative
  operations required by the tasks), correct the self-hosting instructions, and
  drive init → tenant create → tenant configure/restricted principal through the
  actual binaries against a real server.

### TREV-005 — MISSING — the change did not amend its governing architecture

- **Violated obligation:** the specification's **Required architecture
  amendments** section and AGENTS.md's requirement that current architecture
  remain authoritative.
- **Exact location:** `architecture/wyrd-design.md:145-156`;
  `architecture/wyrd-security-posture.md:40-48`;
  `architecture/v1/00-foundations/service-identity.md:1-6`;
  `changes/active/tenant-oidc-federation/spec.md:77-79,95-97`.
- **Evidence:** none of the required authority files changed in the complete
  diff. They still define `PrincipalKind` as only User/Service/Agent, require
  Service and Agent principals to be Card-bound, and state that every human OIDC
  connection belongs to exactly one tenant. The candidate implements global and
  tenant admin kinds, Card-free machine principals, and a deployment-owned
  platform OIDC connection.
- **Observable consequence:** the repository has two contradictory sources of
  truth. A future conforming change can legitimately remove or reject the new
  behavior by following the still-current architecture.
- **Required testable correction:** amend only the four architecture/spec owners
  explicitly named by revision 7 so their principal set, optional Card binding,
  two control planes, administrative permissions, credential lifecycle, and
  platform OIDC exception match the shipped contract; run the applicable docs
  checks.

### TREV-006 — VIOLATION — new services retain raw tenant database pools

- **Violated obligation:** AGENTS.md/`architecture/agent-rules.md`: raw
  `sqlx::PgPool` is banned from library struct fields and signatures; the only
  permitted tenant-scoped boundary is `TenantConn`.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:85-103`;
  `crates/wyrd/wyrd-server/src/components/platform/recovery.rs:33-53`.
- **Evidence:** both new owning structs store `app: sqlx::PgPool` and accept it in
  public constructors, then manufacture `TenantConn` internally. The existing
  `check:from-pools-allowlist` only greps `from_pools` calls and cannot prove this
  rule; its green result is not contrary evidence.
- **Observable consequence:** these workflows bypass the repository's enforced
  connection-capability shape, so tenant context is not present in the type of
  the dependency and future methods can issue raw app-pool work accidentally.
- **Required testable correction:** move tenant connection acquisition to the
  existing boundary owner and pass `&mut TenantConn<'_>` into the tenant-scoped
  operation, while keeping the platform directory operation on `OperatorPool`.
  Do not introduce a third connection abstraction. Add a static check only if an
  existing sanctioned boundary check cannot cover the live invariant.

### TREV-007 — MISSING — required failure, concurrency, isolation, and audit proofs remain absent

- **Violated obligation:** AC-001, AC-004, AC-007, AC-008, AC-009; TASK-003
  through TASK-006 verification; non-superseded remediation R1 criteria 1-4,
  9-11, 13, 15, and 20.
- **Exact location:** `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:86-170,1299-1479`
  and absence of the named scenarios from the remainder of that target.
- **Evidence:** initialization covers happy path and sequential replay, not
  concurrent invocation, stage failure, captured output, or uninitialized-server
  refusal. Provisioning retry is manufactured by updating a successfully created
  tenant to `failed`, not by injecting failures at each stage; there is no
  cancellation or concurrent-create test. Suspension is performed with direct
  SQL and does not exercise a route or an already-minted token. No injected
  audit-append failure exists on the tenant principal/credential surface. No
  tenant-A-against-tenant-B principals/credentials journey exists.
- **Observable consequence:** the highest-risk contractual branches are asserted
  in prose but are not protected by the user journeys the approved spec makes the
  acceptance evidence. At least one untested branch is demonstrably wrong
  (TREV-002).
- **Required testable correction:** add the smallest real-server scenarios named
  by the approved tasks, reusing `WyrdTestServer` and existing fault seams; do not
  create another harness. Each focused command must select exactly the named test
  and run through `mise exec -- cargo nextest` with the repository-managed
  Postgres wrapper.

### TREV-008 — DRIFT — removed bootstrap vocabulary and a dead initialization variant remain

- **Violated obligation:** REQ-038; remediation R1 criteria 5 and 24; Ponytail
  deletion rung.
- **Exact location:** `crates/wyrd/wyrd-server/src/boot/init.rs:37-39`;
  `crates/wyrd/wyrd-auth/src/audit.rs:189-195`;
  `docs/src/content/docs/self-hosting/local-development.svx:30-39`.
- **Evidence:** `InitError::NotConfigured` has no construction site. The only
  surviving literal `bootstrap-key` outside `changes/` is an unrelated audit-id
  unit-test input. Local-development still describes init as a bootstrap command.
- **Observable consequence:** dead and replaced concepts remain in the permanent
  implementation and user documentation, contrary to the explicit no-legacy
  outcome; the error enum advertises an impossible state.
- **Required testable correction:** delete the unused variant, replace the test
  label with a neutral non-UUID label, and describe initialization with current
  vocabulary. No compatibility alias or replacement abstraction is needed.

### TREV-009 — VIOLATION — branch history uses AI author/committer identities and trailers

- **Violated obligation:** AGENTS.md §13: use the local contributor identity,
  never sign as anyone else, and never add AI co-author trailers.
- **Exact location:** git objects in
  `c5c20754a167e8f4d74a555a720bd51df6179a6f..072cf8b30c7135e8cf15f92da3e371a9c999703c`.
- **Evidence:** the complete log audit found 58 commits with either
  `Claude <noreply@anthropic.com>` as author/committer or a
  `Co-Authored-By: Claude…` trailer. Later clean commits do not repair earlier
  objects in the reviewed range.
- **Observable consequence:** the cumulative candidate cannot satisfy the
  repository's provenance gate even if its working tree becomes correct.
- **Required testable correction:** the change owner must rewrite the offending
  branch commits using the already configured contributor identity and remove AI
  trailers, without changing tree content; verify every commit in the resulting
  immutable range. This history rewrite requires explicit owner coordination and
  must not be performed opportunistically by an implementation agent.

## Ponytail validation of supplied additional issues

| Supplied issue | Validation | Task-review treatment |
|---|---|---|
| `auth_e2e::cache_ttl_path_also_flips_verdict` fails with `WYRD_AUTH_503_VERIFY_UNAVAILABLE` from `DelegateError::Database(_)` at `exchange_api_key.rs:791` | Reproduction at the immutable base establishes that it is not introduced by this candidate. It is an auth-surface repository red, so it must be disclosed under the user's all-green requirement, but the task-review rules prohibit relabeling unrelated pre-existing debt as an implementation finding. | REJECTED as a candidate finding; retained as a verification limit for the final verdict/remediation coordinator. |
| `UNIQUE (data_tenant_id, name)` blocks same-named Cards across spaces | Confirmed at `20260601000001_auth.sql:85`. It predates the change and changing it changes Card identity/write semantics outside this approved specification. The current containment lookup relies on that uniqueness, so a local constraint tweak would create an ambiguous credential-issuance path. | REJECTED as an admin-principals finding; preserve as a spec-owner handoff. |
| stale uid-less/exact-lookup comment | Confirmed at `wyrd-testing/src/server.rs:2670-2672`; the production lookup now uses JSONB containment. The lines predate and are not changed by this branch, and correcting test-harness prose does not close an admin-principals acceptance gap. | REJECTED as a material finding; safe spec-owner/test-maintainer handoff. |
| `cargo doc -p wyrd-sql` emits `rustdoc::private_intra_doc_links`, while `wyrd-sql` is absent from the rustdoc lane | Confirmed as an ungated warning around the private `queries::cards::auth_projection` link. No approved task requires expanding the rustdoc lane, and a warning outside every required gate is not evidence that a required behavior is broken. | REJECTED as a task finding. Widening CI is a separate permanent-cost decision; do not bundle it into remediation. |

## Verification notes and limits

- Read: `AGENTS.md`, `architecture/agent-rules.md`, the spec-driven-development
  reference, revision 7, all eight original tasks, all prior verdicts and
  remediation tasks, the complete diff, and source/callers for every retained
  finding.
- Re-ran `mise run check:from-pools-allowlist`; it passed, but its script only
  constrains `from_pools` call sites and does not inspect raw `PgPool` fields or
  signatures, so it does not falsify TREV-006.
- This reviewer did not rerun the Postgres-backed suites. Their prior focused
  results are useful evidence for covered paths but cannot substitute for the
  absent scenarios in TREV-007.
- The user-requested repository-wide green state is not demonstrated. The known
  `auth_e2e` failure is pre-existing at the base, not a candidate regression.
- The candidate's absence of tenant lifecycle routes, stale provisioning claim,
  non-active recovery behavior, raw-pool fields, architecture drift, CLI omission,
  and history metadata are source-proven and do not depend on test execution.

## Review findings summary

### Critical

- None classified critical: the confirmed security boundaries fail closed, but
  required lifecycle/availability/product surfaces remain incomplete.

### Important

- TREV-001 — missing tenant list/inspect/suspend/resume surface.
- TREV-002 — cancellation strands tenant provisioning permanently.
- TREV-003 — recovery issues credentials for non-active tenants.
- TREV-004 — missing operator CLI journey and contradictory documentation.
- TREV-005 — required governing architecture amendments absent.
- TREV-006 — raw tenant pool violates the connection boundary.
- TREV-007 — required high-risk journey evidence absent.
- TREV-009 — branch history violates contributor identity rules.

### Suggestions

- TREV-008 — delete the remaining dead/legacy bootstrap vocabulary while closing
  the bounded remediation.
