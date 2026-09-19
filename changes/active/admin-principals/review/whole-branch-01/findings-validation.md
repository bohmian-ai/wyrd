# Whole-branch structured Ponytail validation — admin principals

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate | `072cf8b30c7135e8cf15f92da3e371a9c999703c` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 7 |
| Reviewed inputs | Complete base-to-candidate diff; TASK-001 through TASK-008; all prior remediation and verdict artifacts; `task-review.md`, `standards-review.md`, and the security, data, and contract domain reports in this directory |

The candidate remained `072cf8b30c7135e8cf15f92da3e371a9c999703c`
before and after validation. Source was kept immutable. This report is the only
file written by the Wave 2 reviewer.

## Overall validation result

**FIX_REQUIRED**

No retained correction changes approved product behavior or an
expensive-to-reverse decision. The findings can be closed by reusing current
owners: the canonical Vala audit append/publisher, `OperatorPool`,
`WyrdPostgres`/`ServerPostgres::tenant_conn`, the existing platform and
principal services, `wyrd-client`, the current MCP adapter, the shared error
catalog, and the repository test harness. The same-name-across-space Card key
does require a separate persistent-identity decision, but it predates this
candidate and the approved spec explicitly excludes changing Card-bound
provisioning, so it is a handoff rather than remediation here.

## Wave 1 finding validation

| Wave 1 finding | Result | Final disposition |
|---|---|---|
| TREV-001 / DATA-3 | **CONFIRMED** | `FIND-004-2` |
| TREV-002 / DATA-4 | **CONFIRMED** | `FIND-004-3`; prior `FIND-004-7` is subsumed by the same state-machine correction |
| TREV-003 | **CONFIRMED** | `FIND-006-3` |
| TREV-004 | **CONFIRMED** | `FIND-004-5`; prior `FIND-005-3` remains folded into it |
| TREV-005 / RS-WB-4 / CONTRACT-1 | **CONFIRMED** | `FIND-admin-principals-4` |
| TREV-006 / RS-WB-2 / DATA-5 | **CONFIRMED** | `FIND-admin-principals-2` |
| TREV-007 | **REVISED** | Split into the already-stable `FIND-003-2`, `FIND-004-4`, `FIND-005-2`, and `FIND-006-4`; implementation defects are not duplicated as test-only findings |
| TREV-008 | **REVISED** | `FIND-003-3` for dead/legacy implementation residue; the operator prose is in `FIND-004-5` |
| TREV-009 / RS-WB-8 | **CONFIRMED** | `FIND-TASK-001-10` |
| RS-WB-1 / DATA-1 / SEC-04 | **CONFIRMED** | `FIND-admin-principals-1` |
| DATA-2 / SEC-04 transaction portion | **REVISED** | Folded into `FIND-admin-principals-1` for single-plane effects. Cross-plane provisioning/recovery must not pretend to have impossible cross-role atomicity; their audit describes the permission decision, while each durable boundary retains its existing resumable/fail-closed contract. |
| RS-WB-3 | **CONFIRMED** | `FIND-admin-principals-3` |
| RS-WB-5 | **CONFIRMED** | `FIND-admin-principals-5` |
| RS-WB-6 / RS-WB-7 / CONTRACT-5 | **CONFIRMED** | `FIND-admin-principals-6`; CI lane widening rejected |
| SEC-01 | **CONFIRMED** | `FIND-admin-principals-7` |
| SEC-02 | **CONFIRMED** | `FIND-admin-principals-8` |
| SEC-03 | **CONFIRMED** | `FIND-admin-principals-9` |
| SEC-05 | **CONFIRMED** | `FIND-admin-principals-10` |
| SEC-06 | **CONFIRMED** | `FIND-admin-principals-11` |
| CONTRACT-2 | **REVISED** | CLI gap retained in `FIND-004-5`; the task-authorized MCP subset remains list/revoke only, with its missing successful act/observe proof retained as `FIND-admin-principals-12` |
| CONTRACT-3 | **CONFIRMED** | `FIND-admin-principals-13` |
| CONTRACT-4 | **CONFIRMED** | `FIND-004-5` and `FIND-admin-principals-4` |
| Cross-space Card uniqueness handoff | **REJECTED as this task's finding** | Preserved under non-findings below |
| Base-red `auth_e2e` test | **REJECTED as candidate regression** | Preserved as a whole-repository verification limit |
| Post-validation MCP journey failures | **CONFIRMED** | `FIND-admin-principals-14`; distinct from the missing successful revoke proof in `FIND-admin-principals-12` |

## Final deduplicated finding ledger

### FIND-admin-principals-1 — CONFIRMED — VIOLATION — platform authorization bypasses the canonical audit authority

- **Wave 1 IDs:** RS-WB-1, DATA-1, DATA-2, SEC-04.
- **Violated obligation:** AGENTS.md and `architecture/agent-rules.md` mandate
  one append into `vala.audit_staging`, one `AuditPublisher`, and transactional
  coupling of a decision to a same-plane effect; REQ-037 and AC-009 require the
  same behavior for these operations.
- **Location:** `20260601000021_platform_authz_audit.sql:1-40`,
  `queries/platform/audit_authz.rs:39-74`, `platform_authz.rs:90-143`, and
  `components/platform/identity.rs:111-133,154-302,418-517`.
- **Reachability and caller trace:** `insert_platform_authz_audit` has exactly
  one production caller, `PlatformAuthorization::authorize`. Its allowance is
  consumed by registration, provisioning, recovery, and every platform
  identity route. The identity helper commits at line 127 before its OIDC/list/
  status effects; registration alone keeps its effect in the returned
  transaction. No publisher or supported reader consumes `platform.audit_authz`.
- **Observable consequence:** privileged platform decisions never enter the
  supported retained audit history, and single-plane failures can leave a
  committed allowance for an effect that did not commit.
- **Decision-complete minimum correction:** delete the alternate migration,
  query slot, and direct tests. Extend the existing canonical audit admission
  only as needed for the already-public platform principal kinds, and stage
  platform decisions under the existing `DataTenantId::SYSTEM_OWNER` partition
  through the canonical Vala append/publisher. Keep each single-plane operator
  mutation and its allowance in one owner-controlled transaction; keep denials
  durable and fail closed. For provisioning and recovery, record the truthful
  named cross-plane permission decision at the platform boundary and preserve
  their existing separate tenant-boundary/resumption semantics; do not add a
  second coordinator, audit store, or false whole-workflow atomicity claim.
- **Focused closure proof:** one allowed and one denied platform operation land
  in `vala.audit_staging` and publish once into `vala.system.audit_log`; an
  injected append failure refuses before mutation; an injected single-plane
  mutation failure leaves neither mutation nor allowance; schema inspection
  proves `platform.audit_authz` is absent.

### FIND-admin-principals-2 — CONFIRMED — VIOLATION — new services export raw SQL capabilities

- **Wave 1 IDs:** TREV-006, RS-WB-2, DATA-5.
- **Violated obligation:** `architecture/agent-rules.md` permits only
  `TenantConn` and `OperatorPool` in library connection fields/signatures and
  expressly bans raw `PgPool` and caller-handed SQLx transactions.
- **Location:** `components/platform/provisioning.rs:85-103`,
  `components/platform/recovery.rs:33-53`, `platform_authz.rs:90-97`, and the
  `*_tx` entry points in platform audit, credentials, identity, grants,
  principals, and provisioning queries.
- **Reachability and caller trace:** the only production constructors of
  `TenantProvisioning` and `TenantRecovery` receive `state.postgres.app_pool()`
  in `platform/routes.rs:133,177`. Every changed `*_tx` function has a live
  initialization, registration, or authorization caller; these are not dormant
  helpers.
- **Observable consequence:** tenant acquisition bypasses the repository owner,
  and a raw privileged transaction becomes a portable library capability.
- **Decision-complete minimum correction:** acquire tenant transactions through
  `ServerPostgres`/`WyrdPostgres::tenant_conn` at the existing server boundary
  and pass `&mut TenantConn` to tenant work. Move the bounded multi-statement
  operator workflows behind focused `OperatorPool`-owning SQL/service methods so
  raw `Transaction` does not cross public module signatures. Preserve the
  current one-commit operations; add no third connection wrapper.
- **Focused closure proof:** a static source check covers raw pool fields and raw
  transaction parameters in addition to the existing construction allowlist,
  and the focused initialization, registration, authorization, provisioning,
  and recovery Postgres tests pass.

### FIND-admin-principals-3 — CONFIRMED — VIOLATION — public errors disclose internal source strings

- **Wave 1 IDs:** RS-WB-3; SEC-04 noted the same boundary as defense in depth.
- **Violated obligation:** the shared error guidance requires stable safe public
  details and server-side structured diagnostics for SQL, cryptography,
  provider, and serialization failures.
- **Location:** `components/platform/routes.rs:100-103,201-205`,
  `components/auth/platform_extractor.rs:105-120,162-169`,
  `components/platform/identity.rs:127-131,626-688`, and
  `components/principals/routes.rs:544-549`.
- **Reachability and caller trace:** each mapper feeds a served Axum handler or
  extractor and `WyrdErrorResponse` serializes `as_problem_json()` unchanged.
  The source strings therefore reach HTTP rather than only tracing.
- **Observable consequence:** clients can receive schema, constraint, provider,
  parser, key-store, or cryptographic details, and supposedly stable errors vary
  with internal libraries.
- **Decision-complete minimum correction:** at each existing conversion
  boundary, trace the source error with scrubbed structured fields and return
  the existing stable catalog variant with empty or deliberately typed safe
  details. Do not add error variants or a second mapper layer.
- **Focused closure proof:** inject representative SQL, sealing-key/session, and
  serialization failures and assert the problem body omits the source string
  while server diagnostics retain it.

### FIND-admin-principals-4 — CONFIRMED — MISSING — required architecture amendments never landed

- **Wave 1 IDs:** TREV-005, RS-WB-4, CONTRACT-1, CONTRACT-4.
- **Violated obligation:** the spec's Required architecture amendments and
  AGENTS.md's authority hierarchy.
- **Location:** `architecture/wyrd-design.md`,
  `architecture/wyrd-security-posture.md`, the affected
  `architecture/v1/00-foundations/` pages, `components/admin/routes.rs` module
  documentation, and `changes/active/tenant-oidc-federation/spec.md`.
- **Evidence:** none of those authorities changed in the complete range. They
  still close principal kinds to User/Service/Agent, require machine principals
  to be Card-bound and tenant-owned, retain the old no-audit stance, and state
  every OIDC connection is tenant-owned.
- **Observable consequence:** the repository's governing design rejects the
  identity and control-plane model the branch ships.
- **Decision-complete minimum correction:** amend only the named owners so they
  describe revision 7's two planes, platform and tenant-admin kinds, optional
  machine Card binding, grant-held platform authority, deployment OIDC
  exception, credential/revocation behavior, and canonical audit requirement.
  Replace stale statements; do not add parallel architecture prose.
- **Focused closure proof:** `mise run docs:check`, the applicable design-sync
  checks, and a targeted stale-model search.

### FIND-admin-principals-5 — CONFIRMED — VIOLATION — candidate verification tasks cannot run safely

- **Wave 1 IDs:** RS-WB-5.
- **Violated obligation:** AGENTS.md testing workflow requires repository-owned
  environment setup and exact nextest selection for named tests.
- **Location:** `mise.toml:116-139`.
- **Evidence:** both candidate Postgres tasks depend on nonexistent
  `setup:postgres`; `mise tasks ls` confirms no such task. The unit/integration
  commands use positional `cargo test` substring filters that can select zero
  tests after a rename.
- **Observable consequence:** the capability gates fail before testing or can
  pass without the named proof.
- **Decision-complete minimum correction:** use the existing
  `scripts/postgres/with-test-postgres.sh` plus `db:migrate:all:inner` pattern and
  exact nextest selectors for named slices, or whole explicit targets where the
  task owns the target. Add no new setup task or harness.
- **Focused closure proof:** list the exact tests, then execute each corrected
  `mise` task from a clean environment and record a nonzero test count.

### FIND-admin-principals-6 — CONFIRMED — DRIFT — changed source documentation contradicts the code or rustdoc

- **Wave 1 IDs:** RS-WB-6, RS-WB-7, CONTRACT-5.
- **Violated obligation:** repository Rust documentation must describe current
  behavior and render validly.
- **Location:** `wyrd-testing/src/server.rs:2626,2671` and
  `wyrd-sql/src/queries/auth/service_accounts.rs:174-179`.
- **Evidence:** the candidate changed the lookup to JSONB containment while the
  fixture still says exact equality. The changed public rustdoc links to the
  private `SERVICE_ACCOUNT_BY_CARD_REF_SQL` and `cargo doc -p wyrd-sql --no-deps`
  emits `rustdoc::private_intra_doc_links`.
- **Observable consequence:** maintainers are told the opposite predicate and
  public docs contain a non-public link.
- **Decision-complete minimum correction:** rewrite the fixture comment to state
  that the UID-less ref is the client-expressible containment selector, and use
  plain code formatting/self-contained wording for the private constant. No
  logic or new test is needed.
- **Focused closure proof:** source inspection plus warning-free
  `mise exec -- cargo doc --locked -p wyrd-sql --no-deps` for this candidate
  warning. Do not widen the repository rustdoc lane.

### FIND-admin-principals-7 — CONFIRMED — MISSING — registered human platform administrators receive no grant

- **Wave 1 IDs:** SEC-01.
- **Violated obligation:** REQ-041, REQ-046, AC-015, and TASK-007 require the
  pre-registered human to become an independently usable administrator.
- **Location:** `components/platform/identity.rs:333-389`,
  `boot/init.rs:55-69,126-129`, and
  `queries/platform/principal_grants.rs:25-79`.
- **Reachability and caller trace:** `register_admin` inserts a principal and
  identity only. The sole production caller of `set_platform_grant_tx` is root
  initialization; `set_platform_grant` is test-only. The extractor converts an
  absent grant to an empty permission set, so the successful federated login is
  denied by every protected platform route.
- **Observable consequence:** the promised human administration path cannot
  administer anything.
- **Decision-complete minimum correction:** define the fixed platform-admin
  permission set once in its existing owner and install it in the already-
  authorized `register_admin` transaction through the existing grant write.
  Do not add editable roles, a separate grant endpoint, or type-implied bypass.
- **Focused closure proof:** the registered human completes verified first login
  and performs a protected platform operation; a tenant principal cannot create
  or grant a platform principal.

### FIND-admin-principals-8 — CONFIRMED — INCORRECT — platform suspension can remove the last usable administrator

- **Wave 1 IDs:** SEC-02.
- **Violated obligation:** TASK-007's served revocation path, REQ-033's recovery
  boundary, and INV-011 fail-closed behavior.
- **Location:** `components/platform/identity.rs:471-517` and
  `queries/platform/principals.rs:169-209`.
- **Reachability and caller trace:** `set_admin_status` is a served route and a
  `wyrd-client::Platform` method. Its only guard counts all active rows in one
  statement and updates in a later statement. It does not require a grant or an
  authentication anchor and takes no lock.
- **Observable consequence:** an unpinned/grantless human permits root
  suspension, and concurrent suspensions can both consume the last usable path.
- **Decision-complete minimum correction:** make status change and final-path
  protection one operator-owned transaction. Serialize competing suspensions
  and count only active principals that hold the fixed platform authority and
  have a usable credential or pinned federated identity. Reuse existing
  principal, grant, credential, and identity tables; add no new lock service.
- **Focused closure proof:** an unpinned or ungranted row does not permit root
  suspension, and two concurrent suspensions leave one independently
  authenticating, authorized administrator.

### FIND-admin-principals-9 — CONFIRMED — MISSING — platform credential lifecycle has no served surface

- **Wave 1 IDs:** SEC-03.
- **Violated obligation:** REQ-006 through REQ-010, REQ-036, AC-005, TASK-006.
- **Location:** `platform/routes.rs:37-53`,
  `platform_credentials.rs:137-211`, and
  `queries/platform/credentials.rs:197-240`.
- **Reachability and caller trace:** `PlatformCredentials::issue` has no
  production caller. Platform list/revoke query functions are test-only; served
  routes only exchange a credential and manage tenant/OIDC/admin state.
- **Observable consequence:** the root credential cannot be rotated, listed, or
  revoked without direct SQL.
- **Decision-complete minimum correction:** expose the existing platform
  credential owner through separately authorized and canonically audited issue,
  metadata-list, and revoke HTTP operations; project them through the existing
  `wyrd-client::Platform` and the operator CLI. Preserve one-time plaintext and
  per-request revocation. MCP credential issuance remains excluded by its
  approved transcript-secret boundary.
- **Focused closure proof:** issue B, authenticate B, revoke A, prove A's live
  session fails on its next request, prove B still works, and prove listing
  contains metadata but neither plaintext.

### FIND-admin-principals-10 — CONFIRMED — VIOLATION — runtime platform OIDC fetches bypass SSRF screening

- **Wave 1 IDs:** SEC-05.
- **Violated obligation:** security posture SSRF policy, `agent-rules.md`,
  TASK-007, and INV-011.
- **Location:** `platform_login.rs:145-175,207-246`, `login.rs:145-163`,
  `callback.rs:216-234`, and `wyrd-auth-verify/src/lib.rs:679-706`.
- **Reachability and caller trace:** configuration alone calls the screened
  `discover_jwks_uri`. Anonymous platform login then calls
  `discover_authorization_endpoint`, callback calls `discover_provider`, and
  token verification's JWKS cache fetches the stored URL; each runtime path can
  re-resolve through an ordinary client.
- **Observable consequence:** DNS rebinding after configuration can turn
  anonymous login/callback/JWKS refresh into requests to blocked internal or
  metadata addresses.
- **Decision-complete minimum correction:** reuse the existing deployment-
  profile address policy, bounded DNS resolution, redirect refusal, and pinned
  client for every platform discovery, token, and JWKS fetch. Pass that existing
  capability into `PlatformLogin`/verification; do not invent a second URL
  policy or trust a prior resolution.
- **Focused closure proof:** change DNS resolution after configuration and prove
  begin-login, callback token exchange, and JWKS refresh each reject the newly
  blocked address without an internal request.

### FIND-admin-principals-11 — CONFIRMED — INCORRECT — invalid platform credentials have distinguishable verifier work

- **Wave 1 IDs:** SEC-06.
- **Violated obligation:** INV-012 and AC-010 require invalid credential cases
  to be publicly indistinguishable and resistant to enumeration inference.
- **Location:** `platform_credentials.rs:190-210,346-421` and anonymous
  `platform/routes.rs:75-104`.
- **Reachability and caller trace:** every platform credential exchange reaches
  `authenticate_for_session`; malformed, unknown, revoked, expired, and
  suspended cases return before Argon2, while a known live prefix with a wrong
  tail performs Argon2.
- **Observable consequence:** the public endpoint provides a live-prefix timing
  oracle even though its error body is identical.
- **Decision-complete minimum correction:** keep one process-owned dummy Argon2
  verifier and perform exactly one verifier call for every invalid shape,
  selecting the real verifier only for a usable row. Reuse the current hash/
  verify implementation and public error; do not use wall-clock padding.
- **Focused closure proof:** instrument verifier invocation and prove malformed,
  unknown, unusable, and known-wrong inputs each invoke it exactly once and
  return the same stable error.

### FIND-admin-principals-12 — CONFIRMED — MISSING — MCP never proves its authorized write

- **Wave 1 IDs:** revised from CONTRACT-2.
- **Violated obligation:** AC-014 and TASK-008 require the approved MCP
  administrative operation to be exercised against a real server as discover,
  act, and observe.
- **Location:** `wyrd-mcp/tests/bifrost/mcp/principals.rs:25-154`.
- **Evidence:** the journey proves catalog visibility, an under-scoped refusal,
  and an admin read. It never invokes `principals.revoke_credential` as the
  authorized admin or observes that credential retired.
- **Observable consequence:** the only advertised MCP write has no product-path
  proof.
- **Decision-complete minimum correction:** extend this existing journey to
  revoke a real non-current credential as the admin and then list/attempt use to
  observe retirement. Keep the approved MCP surface to metadata listing and
  revocation; do not add platform tools or secret-returning issuance.
- **Focused closure proof:** the existing real-server MCP test covers successful
  revoke and observed retirement while retaining its catalog and denied-write
  assertions.

### FIND-admin-principals-13 — CONFIRMED — INCORRECT — the generated administrative HTTP contract is incomplete and self-contradictory

- **Wave 1 IDs:** CONTRACT-3.
- **Violated obligation:** REQ-036 and AC-014 require typed bodies and stable
  error contracts for every administrative path.
- **Location:** administrative `#[utoipa::path]` response lists,
  `auth/revoke.rs:29-69`, `wyrd-spec/src/auth/revoke.rs:10-47`,
  `wyrd-client/src/principals/handle.rs:121-145`, and generated `openapi.yaml`.
- **Reachability and caller trace:** every administrative route is registered in
  OpenAPI and served. Their error responses declare descriptions without
  `WyrdProblem` bodies. The CLI calls `Principals::revoke_principal`, which sends
  `RevokePrincipalRequest`; the handler has no `Json` extractor, ignores kind
  and reason, and returns unit while a detailed response schema also exists.
- **Observable consequence:** generated clients cannot type failures and are
  instructed to send/receive a revocation shape the server does not honor.
- **Decision-complete minimum correction:** apply the existing `WyrdProblem`
  annotation pattern to administrative failures. Make principal revocation
  honor the already-shipped request body, require its declared kind to match the
  resolved principal kind, and use its reason in audit. Keep the existing empty
  success response used by the server, client, and CLI; delete the unused
  `RevokePrincipalResponse` schema and regenerate. The request is already sent
  by the CLI, so accepting it and deleting the zero-caller response is the
  smaller compatibility-preserving path.
- **Focused closure proof:** one OpenAPI source test enumerates administrative
  paths and asserts problem bodies; missing/invalid revoke bodies fail; a valid
  body reaches the audit reason; `mise run codegen:check` passes.

### FIND-admin-principals-14 — CONFIRMED — REGRESSION — MCP catalog journeys still assert the pre-principal catalog

- **Source:** deterministic post-validation execution of
  `mise run test:bifrost:journey:mcp` (2 failures out of 9).
- **Violated obligation:** TASK-008 and AC-014 require the principal tools to be
  part of the shipped MCP catalog and the real-server MCP lane to pass; AGENTS.md
  requires verification for the changed agent-facing surface.
- **Location:** `wyrd-mcp/tests/bifrost/mcp/connectivity.rs:173-184` and
  `wyrd-mcp/tests/bifrost/mcp/discovery.rs:22-30,78-87`, plus the stale catalog
  rustdoc at `wyrd-server/src/mcp/mod.rs:128-136`.
- **Reachability and caller trace:** `WyrdMcpHandler::catalog` now always appends
  `principals.list_credentials`, while `list_tools` additionally appends
  `principals.revoke_credential` when the verified caller has
  `service_accounts:write`. Both failed journeys use an administrative caller,
  so the actual catalogs correctly contain both candidate-added tools. The two
  tests were unchanged in the branch and still require only the prior three
  Bifrost tools (plus the opted-in probe in connectivity).
- **Observable consequence:** the canonical MCP journey gate is red even though
  the newly advertised catalog is intended behavior, so the repository cannot
  satisfy the requested green state.
- **Decision-complete minimum correction:** update the two existing exact
  catalog expectations and their prose to include
  `principals.list_credentials` and `principals.revoke_credential` in the
  current catalog order, retaining the connectivity probe as the final
  test-only tool. Correct `WyrdMcpHandler::catalog`'s adjacent “exactly three”
  rustdoc in the same edit. Do not loosen the checks to unordered containment,
  duplicate catalog construction in a helper, remove the principal tools, or
  add another test target: exact order and absence of unapproved tools are
  useful contracts these journeys already own.
- **Focused closure proof:** rerun the two exact named nextest expressions, then
  `mise run test:bifrost:journey:mcp`; all 9 tests pass. This does not close
  `FIND-admin-principals-12`, which still requires an authorized revoke followed
  by observed retirement.

### FIND-TASK-001-10 — CONFIRMED — VIOLATION — branch history carries prohibited AI identities and trailers

- **Wave 1 IDs:** TREV-009, RS-WB-8; stable prior ID preserved.
- **Violated obligation:** AGENTS.md §13.
- **Location:** commit objects in the immutable base-to-candidate range.
- **Evidence:** the complete log contains 58 commits using
  `Claude <noreply@anthropic.com>` as author/committer or an AI co-author trailer.
- **Observable consequence:** the delivered history cannot pass the repository
  provenance rule even when its tree is corrected.
- **Decision-complete minimum correction:** the branch owner, not the
  implementation agent, rewrites only the unmerged offending commits to the
  already-configured contributor identity and removes AI co-author trailers,
  without changing their cumulative tree. No `git config`, identity environment
  override, or additional trailer is permitted.
- **Focused closure proof:** inspect every commit in the new immutable
  base-to-candidate range for exact author/committer identity and absence of AI
  co-author trailers, then prove the final tree equals the remediated tree.

### FIND-003-2 — CONFIRMED — MISSING — initialization acceptance proof remains absent

- **Wave 1 IDs:** TREV-007; stable prior ID preserved.
- **Violated obligation:** REQ-022 through REQ-024 and AC-001.
- **Evidence:** `platform_admin_e2e.rs:86-170` covers success and sequential
  replay only. It does not exercise concurrent invocations, injected failure at
  each write, captured output/logs, or an uninitialized running server that
  serves ordinary traffic and stably refuses the platform plane.
- **Decision-complete minimum correction:** add only those scenarios to the
  existing real-server/init harness; do not create a new fixture.
- **Focused closure proof:** exact named nextest expressions prove one winner
  under concurrency, clean retry after each failure, no credential in captured
  diagnostics, and the uninitialized-server split behavior.

### FIND-003-3 — CONFIRMED — DRIFT — dead and replaced initialization vocabulary remains

- **Wave 1 IDs:** TREV-008; stable prior ID preserved.
- **Violated obligation:** REQ-038 and the Ponytail deletion rung.
- **Location:** `boot/init.rs:35-40`, `wyrd-auth/src/audit.rs:189-195`, and
  `docs/self-hosting/local-development.svx:30-39`.
- **Evidence:** `InitError::NotConfigured` has no constructor. The old command
  name remains as an audit unit-test label, and the page still calls `init` a
  bootstrap command. The prefix shown there is tenant-shaped rather than
  `wyrd_global_...`.
- **Decision-complete minimum correction:** delete the unused variant, use a
  neutral non-UUID audit test label, and replace the stale prose/prefix in the
  existing page. Add no compatibility alias.
- **Focused closure proof:** compiler/lints plus a tree search outside the
  change packet finds no removed command vocabulary.

### FIND-004-2 — CONFIRMED — MISSING — tenant directory lifecycle operations do not exist

- **Wave 1 IDs:** TREV-001, DATA-3; stable prior ID preserved.
- **Violated obligation:** REQ-028, REQ-036, AC-008, TASK-004.
- **Location:** `platform/routes.rs:37-44`, `queries/platform/provisioning.rs:150-180`,
  and `wyrd-client/src/platform/handle.rs:102-136`.
- **Reachability and caller trace:** the platform router and client expose only
  create and recover. `set_tenant_suspended` has zero callers; no list or
  inspect query exists. The current suspension test updates SQL directly.
- **Observable consequence:** a platform administrator cannot list, inspect,
  suspend, or resume tenants through Wyrd.
- **Decision-complete minimum correction:** add the four typed operations to the
  existing platform directory/router and `wyrd-client::Platform`, using current
  permissions and canonical audit owner. Project them into the CLI; add no new
  lifecycle service.
- **Focused closure proof:** through the real client, suspend an active tenant,
  prove fresh credentials and a pre-existing live token fail, resume it, and
  prove the same principal, grant, and state work again.

### FIND-004-3 — CONFIRMED — INCORRECT — provisioning can leave an unreclaimable tenant

- **Wave 1 IDs:** TREV-002, DATA-4; stable prior ID preserved; prior
  `FIND-004-7` is the same root cause.
- **Violated obligation:** REQ-026 through REQ-027, INV-006, AC-007.
- **Location:** `queries/platform/provisioning.rs:27-64,99-147` and
  `components/platform/provisioning.rs:140-181`.
- **Reachability and caller trace:** `insert_provisioning_tenant` has one live
  caller and adopts only `failed`. Cancellation or `mark_tenant_active` failure
  bypasses the error arm; the error arm discards `mark_tenant_failed` failure.
  A surviving `provisioning` row makes every retry conflict.
- **Observable consequence:** a transient failure can permanently burn the slug
  after tenant principal/credential state has committed.
- **Decision-complete minimum correction:** make the existing claim owner
  serialize and adopt failed or stale/incomplete provisioning rows under the
  original tenant id; surface transition failures; reuse the idempotent role
  seed and existing tenant-admin principal. Do not add a second coordinator.
- **Focused closure proof:** inject failure/cancellation at each post-claim
  stage and race two creates; retry converges on one active tenant, one admin
  principal, and one usable returned credential with no orphan or duplicate.

### FIND-004-4 — CONFIRMED — MISSING — provisioning failure and concurrency proof is absent

- **Wave 1 IDs:** TREV-007; stable prior ID preserved.
- **Violated obligation:** AC-007 and TASK-004 verification.
- **Evidence:** `platform_admin_e2e.rs:1364-1438` manually changes an already
  successful tenant to failed. It injects no real stage failure or cancellation
  and has no concurrent-create case.
- **Decision-complete minimum correction:** extend the existing real-server
  journey with the failure seams required to prove `FIND-004-3`; do not build a
  separate harness or duplicate state-machine tests at every tier.
- **Focused closure proof:** exact nextest selectors for the stage-failure,
  cancellation/retry, and concurrent-create journeys.

### FIND-004-5 — CONFIRMED — MISSING — the CLI and operator documentation do not deliver the approved workflow

- **Wave 1 IDs:** TREV-004, CONTRACT-2, CONTRACT-4; stable prior ID preserved;
  prior `FIND-005-3` remains folded into it and `FIND-008-7` is reopened for the
  incomplete documentation claim.
- **Violated obligation:** REQ-036, REQ-040, AC-002, AC-013, AC-014, and
  TASK-004 through TASK-006.
- **Location:** `wyrd-cli/src/cli.rs:55-85` and the existing self-hosting pages.
- **Evidence:** there is no tenant/platform command, and `Principal` exposes only
  revoke. The CLI cannot create a tenant, create a restricted principal, manage
  credentials, recover administration, or perform lifecycle operations. Docs
  omit the executable journey, SaaS custody, overlap rotation, and distinct
  tenant/global recovery, and retain the old principal model/prefix prose.
- **Observable consequence:** operators must hand-write HTTP and cannot follow
  the promised three-command path.
- **Decision-complete minimum correction:** project only the approved operations
  through the existing `wyrd-client::Platform` and `Principals` handles, then
  replace stale content in the existing self-hosting pages with commands that
  actually ship. Do not add another transport, page, SDK binding, or UI.
- **Focused closure proof:** a real-binary journey runs init, tenant create,
  tenant configure/restricted-principal creation and later rotation/recovery
  without SQL; `docs:check` and stale-model search pass.

### FIND-005-2 — CONFIRMED — MISSING — required tenant-isolation journey is absent

- **Wave 1 IDs:** TREV-007; stable prior ID preserved.
- **Violated obligation:** AC-004 and TASK-005.
- **Evidence:** RLS/composite keys are structurally present, but no real-client
  journey drives tenant A against tenant B's principal and credential routes or
  verifies a non-enumerating refusal.
- **Decision-complete minimum correction:** add that one negative journey to the
  existing platform/principal real-server target using two provisioned tenants;
  do not add manual tenant filters or another fixture.
- **Focused closure proof:** tenant A cannot list, issue, revoke, authenticate,
  or recover tenant B's identities, and the response does not reveal existence.

### FIND-006-3 — CONFIRMED — INCORRECT — recovery issues credentials for non-active tenants

- **Wave 1 IDs:** TREV-003; stable prior ID preserved.
- **Violated obligation:** REQ-026, INV-011, and the approved remediation's
  explicit recovery guard.
- **Location:** `components/platform/recovery.rs:69-124`.
- **Reachability and caller trace:** the sole served recovery path authorizes,
  opens the caller-supplied tenant, and inserts a credential without reading the
  directory state through the `OperatorPool` it already owns.
- **Observable consequence:** provisioning, failed, or suspended tenants acquire
  new durable secret state and the operation falsely reports success.
- **Decision-complete minimum correction:** use the existing platform tenant
  status lookup before tenant acquisition and return one non-enumerating refusal
  for every non-active state. Do not create another principal or status cache.
- **Focused closure proof:** each non-active state is refused and credential row
  count is unchanged; active recovery retains the same principal/grants.

### FIND-006-4 — CONFIRMED — MISSING — tenant credential audit failure is not proved fail-closed

- **Wave 1 IDs:** TREV-007; stable prior ID preserved.
- **Violated obligation:** REQ-037, AC-009, TASK-005 and TASK-006.
- **Evidence:** tenant principal routes now append canonically in their mutation
  transaction, but no test makes that append fail and proves the principal or
  credential mutation refuses.
- **Decision-complete minimum correction:** use the existing Postgres fault
  mechanism against `vala.audit_staging` in the current route tests; add no new
  injection framework.
- **Focused closure proof:** one representative principal/credential mutation
  fails with the stable audit-unavailable error and leaves both domain rows and
  audit rows unchanged.

## Prior-finding closure

- `FIND-003-1` remains closed: initialization hashes before opening one
  operator transaction and commits principal, grant, and credential together.
- `FIND-004-1` remains closed for credential exchange: tenant lifecycle is
  checked at the shared authentication seam. Lifecycle administration itself is
  still missing under `FIND-004-2`.
- `FIND-005-1`, `FIND-006-1`, `FIND-006-2`, `FIND-006-5`, and `FIND-006-6`
  remain closed by the tenant canonical audit and revocation work already in the
  candidate.
- `FIND-007-3`, `FIND-007-5`, `FIND-007-9`, and the configuration-time portion
  of `FIND-007-4` remain closed. `FIND-admin-principals-10` is a distinct runtime
  re-resolution path, not a relabeling of the fixed configuration defect.
- `FIND-008-3` remains closed as to MCP tool existence and scope gating;
  `FIND-admin-principals-12` is the narrower missing successful journey.
- `FIND-008-5` remains closed as to path registration and authentication scheme;
  `FIND-admin-principals-13` covers the independently missing error bodies and
  contradictory revocation body.
- Findings explicitly retained above were either never closed in the cumulative
  candidate (`FIND-003-2`, `FIND-003-3`, `FIND-004-2` through `-5`,
  `FIND-005-2`, `FIND-006-3`, `FIND-006-4`, `FIND-TASK-001-10`) or are reopened
  because the claimed closeout is contradicted by current source (`FIND-008-7`,
  folded into `FIND-004-5`).

## Validated non-findings and handoffs

### Base-red auth journey

`auth_e2e::cache_ttl_path_also_flips_verdict` fails with
`WYRD_AUTH_503_VERIFY_UNAVAILABLE`, tracing through
`DelegateError::Database(_)` at `exchange_api_key.rs:791`, and reproduces at the
immutable base. It is not a candidate regression and is not included in the
remediation ledger. It does prevent any current claim that the entire repository
is green; after the baseline defect/environment cause is resolved, the aggregate
must be rerun.

### Same-named Cards across spaces

`UNIQUE (data_tenant_id, name)` on `wyrd.auth_service_accounts` is reachable and
does block same-named Card-bound principals in different spaces. It predates the
candidate, the current containment lookup relies on it for single-row behavior,
and revision 7 makes changing Card-bound `wyrd apply` provisioning a non-goal.
Changing the key requires a separate Card identity/persistent-data decision and
a space-qualified lookup. Do not delete or relax the constraint in this
remediation.

### Rustdoc CI lane widening

The candidate-attributable private-link warning is a source defect retained in
`FIND-admin-principals-6`. Adding `wyrd-sql` to a permanent rustdoc lane or
changing repository lint policy is unnecessary to correct the line and is
rejected as unrelated gate cost.

### Cross-plane audit atomicity

The approved rules audit an authorization decision at its own commit boundary
and prohibit false claims of whole-workflow atomicity. Platform-only effects can
and must share the authorization transaction. Provisioning and recovery cross
the platform/operator and tenant/RLS boundaries; remediation must keep their
decisions canonically audited and their existing failure/retry semantics, not
introduce a new distributed transaction or claim the two roles commit together.

## Verification limits

- Static validation read the complete diff and the complete bodies/callers of
  every changed correction owner named above. Caller searches confirmed the
  zero-caller and single-caller claims in the ledger.
- `git diff --check` and several prior focused lanes are green, but they do not
  exercise the retained gaps.
- The orchestrator's broad gate encountered a concurrent-build missing-rlib
  failure in a Bifrost integration lane after earlier format, all-feature
  Clippy, skills-sync, migrations, and early Bifrost work passed. That failure
  is not source evidence against this candidate, but the aggregate was not a
  credible clean completion run.
- A later isolated `mise run test:bifrost:journey:mcp` deterministically ran 9
  tests and failed the two stale exact-catalog assertions retained as
  `FIND-admin-principals-14`; this is candidate-attributable verification
  evidence, not concurrent-build noise.
- The base-reproduced auth failure independently means the user's requested
  all-repository-green state is not yet demonstrated.
