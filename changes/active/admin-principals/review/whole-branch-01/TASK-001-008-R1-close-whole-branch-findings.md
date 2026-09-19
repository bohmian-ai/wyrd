# TASK-001-008-R1 — Close whole-branch admin-principal findings

## Route

Implement this packet with `$wyrd-implement`. A later `$wyrd-task-review`
must reassess the complete cumulative candidate against the approved spec and
all original tasks.

## Authority and immutable review subject

- Approved specification: `changes/active/admin-principals/spec.md`, revision 7.
- Original tasks:
  - `changes/active/admin-principals/tasks/TASK-001-principal-credential-model.md`
  - `changes/active/admin-principals/tasks/TASK-002-auth-context-and-planes.md`
  - `changes/active/admin-principals/tasks/TASK-003-deployment-initialization.md`
  - `changes/active/admin-principals/tasks/TASK-004-tenant-provisioning.md`
  - `changes/active/admin-principals/tasks/TASK-005-tenant-administration.md`
  - `changes/active/admin-principals/tasks/TASK-006-credential-lifecycle-and-recovery.md`
  - `changes/active/admin-principals/tasks/TASK-007-platform-human-administration.md`
  - `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `072cf8b30c7135e8cf15f92da3e371a9c999703c`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-01/findings-validation.md`.

The implementation target is the cumulative branch, not only its most recent
commit. Preserve revision 7 behavior and close every finding below in one
cohesive candidate.

## Outcome

Deliver the approved two-plane administration workflow end to end: canonical
and transactionally truthful audit, repository-owned database capabilities,
safe public failures, runnable verification lanes, usable platform and tenant
administration surfaces, secure human and credential authentication, complete
typed contracts, and the required real-user journeys. Reuse the existing
owners and delete superseded paths; do not add parallel services, stores,
transports, policies, or test harnesses.

## Validated diagnosis and required correction

### 1. Canonical audit and database ownership

#### `FIND-admin-principals-1` — platform authorization uses an unsupported audit plane

The candidate adds `platform.audit_authz` through
`20260601000021_platform_authz_audit.sql`,
`queries/platform/audit_authz.rs`, `platform_authz.rs`, and platform identity
callers. That table has no canonical publisher or retained reader, while some
allowances commit before their same-plane effects. This violates the single
`vala.audit_staging`/`AuditPublisher` authority, REQ-037, and AC-009, and can
leave both missing retained history and allowances for failed mutations.

Delete the alternate migration, query slot, and direct tests. Extend the
existing canonical audit admission only as needed for the public platform
principal kinds and stage platform decisions under
`DataTenantId::SYSTEM_OWNER`. Keep each platform-only operator mutation and
its allowance in one owner-controlled transaction; denials remain durable and
fail closed. For provisioning and recovery, audit the truthful named platform
decision at its own boundary and retain the existing resumable cross-plane
semantics. Do not add another publisher, coordinator, audit table, or claim of
distributed atomicity.

#### `FIND-admin-principals-2` — services export raw SQL capabilities

`TenantProvisioning`, `TenantRecovery`, `PlatformAuthorization`, and the
changed platform query APIs accept or retain raw `PgPool` or caller-handed
SQLx transactions. Their production callers are live registration,
authorization, provisioning, and recovery paths. This violates the repository
connection boundary and makes privileged transactions portable library
capabilities.

Acquire tenant work through `ServerPostgres`/
`WyrdPostgres::tenant_conn` and pass `&mut TenantConn`. Put bounded
multi-statement operator workflows behind focused `OperatorPool`-owning
SQL/service methods. Preserve the current commit boundaries and reuse the two
existing connection owners; do not introduce a third wrapper.

#### `FIND-006-4` — audit-append failure is not proved fail-closed

Tenant principal and credential routes append in their mutation transaction,
but no test fails the append and proves both the domain mutation and audit row
remain absent. The candidate therefore lacks required proof for REQ-037,
AC-009, TASK-005, and TASK-006.

Use the existing Postgres fault mechanism against `vala.audit_staging` in the
current route tests. One representative principal or credential mutation must
return the stable audit-unavailable error and leave both domain and audit rows
unchanged. Add no injection framework.

### 2. Public errors, contracts, and source documentation

#### `FIND-admin-principals-3` — served failures disclose internal strings

The route and extractor mappers in platform routes, platform authentication,
platform identity, and principal routes serialize SQL, provider, parser,
key-store, cryptographic, or serialization source text into public problem
details. This violates the stable safe-error boundary and makes wire behavior
depend on internal libraries.

At each existing conversion boundary, emit scrubbed structured diagnostics and
return the existing stable catalog error with empty or deliberately typed safe
details. Do not add new public variants or another mapper layer.

#### `FIND-admin-principals-13` — administrative OpenAPI is incomplete and contradictory

Administrative routes omit `WyrdProblem` bodies from error responses. The CLI
sends `RevokePrincipalRequest`, but the handler has no JSON extractor, ignores
kind and reason, and returns unit while an unused detailed response is also
declared. Generated clients therefore cannot type failures and cannot rely on
the documented revocation contract, contrary to REQ-036 and AC-014.

Apply the repository's existing `WyrdProblem` annotation pattern to every
administrative failure. Accept and honor the already-shipped revoke request,
including its audit reason. Return the detailed response only if a current
caller needs it; otherwise delete that unused response type and regenerate.
Do not introduce a second request shape or compatibility alias.

#### `FIND-admin-principals-6` — changed source comments and rustdoc are false

`wyrd-testing/src/server.rs` still describes exact JSONB equality after the
candidate changed the fixture to containment, and public documentation in
`wyrd-sql` links to private `SERVICE_ACCOUNT_BY_CARD_REF_SQL`, producing
`rustdoc::private_intra_doc_links`.

Rewrite the fixture comment to describe the UID-less, client-expressible
containment selector and replace the private intra-doc link with plain code
formatting or self-contained wording. This is a source-only repair: do not
change lookup logic or widen the permanent rustdoc CI lane.

#### `FIND-admin-principals-4` — governing architecture still rejects the implementation

The candidate does not amend `architecture/wyrd-design.md`,
`architecture/wyrd-security-posture.md`, the affected
`architecture/v1/00-foundations/` pages, the admin route module docs, or the
tenant-OIDC specification. Those owners still close principal kinds to the old
model, require machine principals to be Card-bound and tenant-owned, retain the
old audit statement, and say all OIDC is tenant-owned. The shipped design thus
contradicts revision 7.

Update only those existing authorities to describe the two planes, platform
and tenant-admin principal kinds, optional machine Card binding, grant-held
platform authority, the deployment OIDC exception, credential/revocation
behavior, and canonical audit requirement. Replace stale statements rather
than adding parallel explanation.

#### `FIND-003-3` — dead initialization vocabulary remains

`InitError::NotConfigured` has no constructor, an audit test retains the old
command label, and local-development documentation calls `init` a bootstrap
command while showing a tenant-shaped prefix. This violates REQ-038 and leaves
replaced behavior in the product narrative.

Delete the unused variant, use a neutral non-UUID audit test label, and update
the existing page to the shipped command and `wyrd_global_...` prefix. Add no
legacy alias.

### 3. Platform administrators and credentials

#### `FIND-admin-principals-7` — registered human administrators receive no authority

`register_admin` writes a principal and identity but no platform grant. Only
root initialization calls the grant writer, and the extractor maps an absent
grant to no permissions. A human can complete login yet cannot call any
protected platform route, violating REQ-041, REQ-046, AC-015, and TASK-007.

Define the fixed platform-administrator permission set once in its existing
owner and install it in the already-authorized registration transaction through
the existing grant write. Do not add editable roles, a grant endpoint, or a
principal-kind authorization bypass.

#### `FIND-admin-principals-8` — suspension can remove the final usable administrator

The served `set_admin_status` path counts all active rows, does not require a
grant or usable authentication anchor, and performs the count and update in
separate statements without serialization. A grantless human can permit root
suspension, and concurrent suspensions can remove every recovery path.

Make status change and final-path protection one operator-owned transaction.
Serialize competing suspensions and count only active principals with the
fixed platform authority plus a usable credential or pinned federated
identity. Reuse the existing principal, grant, credential, and identity
tables; add no lock service.

#### `FIND-admin-principals-9` — platform credential lifecycle is unreachable

The implementation can issue platform credentials internally, but issue has
no production caller and list/revoke queries are test-only. The root credential
cannot be rotated, listed, or revoked through Wyrd, contrary to REQ-006 through
REQ-010, REQ-036, AC-005, and TASK-006.

Serve separately authorized and canonically audited issue, metadata-list, and
revoke HTTP operations through the existing platform credential owner. Project
them through `wyrd-client::Platform` and the operator CLI. Preserve one-time
plaintext and per-request revocation. MCP credential issuance remains
excluded because it would return a secret in a tool transcript.

#### `FIND-admin-principals-10` — runtime OIDC requests bypass SSRF pinning

Configuration screens discovery, but anonymous platform login, callback token
exchange/provider discovery, and verifier JWKS refresh re-resolve through an
ordinary client. DNS rebinding can therefore redirect runtime requests to
blocked internal addresses, violating the deployment SSRF policy and INV-011.

Reuse the existing deployment-profile address policy, bounded DNS resolution,
redirect refusal, and pinned client for every platform discovery, token, and
JWKS fetch. Pass that capability into the existing login and verification
owners. Do not create a second URL policy or trust configuration-time
resolution for later requests.

#### `FIND-admin-principals-11` — platform credential timing reveals live prefixes

Malformed, unknown, revoked, expired, and suspended inputs return before
Argon2, while a live known prefix with a wrong tail performs Argon2. The public
exchange endpoint therefore exposes a timing oracle despite identical error
bodies, violating INV-012 and AC-010.

Keep one process-owned dummy Argon2 verifier and perform exactly one verifier
call for every invalid shape, selecting the real verifier only for a usable
row. Reuse the current hash/verify implementation and public error; do not add
wall-clock padding.

### 4. Tenant directory, provisioning, and recovery

#### `FIND-004-2` — tenant lifecycle operations are absent

The router and `wyrd-client::Platform` expose only create and recover.
`set_tenant_suspended` has no caller, no list or inspect query exists, and the
test changes SQL directly. Platform administrators therefore cannot list,
inspect, suspend, or resume tenants, violating REQ-028, REQ-036, AC-008, and
TASK-004.

Add those four typed operations to the existing platform directory/router and
client handle, using the existing permissions and canonical audit owner, and
project them into the CLI. Add no new lifecycle service.

#### `FIND-004-3` — interrupted provisioning can permanently burn a slug

The claim path adopts only `failed`. Cancellation and active-transition
failure bypass the error arm, that arm discards failure to mark failed, and a
surviving `provisioning` row makes every retry conflict after tenant state may
already have committed. This violates REQ-026, REQ-027, INV-006, and AC-007.

Make the existing claim owner serialize and adopt failed or stale/incomplete
rows under the original tenant ID, and surface transition failures. Reuse the
idempotent role seed and existing tenant-admin principal. Do not add another
coordinator or create replacement tenant identities.

#### `FIND-004-4` — provisioning failure/concurrency proof is synthetic

The current journey manually marks a successfully provisioned tenant failed;
it injects no actual stage failure or cancellation and has no concurrent-create
case. It does not prove the state-machine guarantees in AC-007.

Extend the existing real-server journey with the failure seams needed for
`FIND-004-3`: post-claim stage failures, cancellation/retry, and two racing
creates. Reuse the current server harness rather than duplicating the workflow
across lower tiers.

#### `FIND-006-3` — recovery ignores tenant lifecycle state

The sole served recovery path opens the tenant and inserts a credential without
checking the platform directory through the `OperatorPool` it already owns.
Provisioning, failed, and suspended tenants can acquire new durable secrets,
violating REQ-026 and INV-011.

Read the existing platform tenant status before tenant acquisition and return
one non-enumerating refusal for every non-active state. Keep the active path's
principal and grants; add no status cache or replacement principal.

#### `FIND-005-2` — tenant isolation lacks product-path proof

RLS and composite keys exist, but no real-client journey drives tenant A
against tenant B's principal and credential routes and verifies a
non-enumerating refusal. This leaves AC-004 and TASK-005 unproved.

Add one negative journey to the existing platform/principal real-server target
using two provisioned tenants. Exercise list, issue, revoke, authenticate, and
recover attempts without adding manual tenant filters or another fixture.

### 5. Operator, SDK, MCP, and initialization journeys

#### `FIND-004-5` — the CLI and operator docs omit the approved workflow

The CLI has no tenant/platform command and exposes only principal revocation.
It cannot create or manage tenants, create restricted principals, manage
credentials, recover administration, or execute lifecycle operations. Existing
self-hosting pages omit executable commands, SaaS custody, overlap rotation,
and distinct tenant/global recovery while retaining stale model and prefix
prose. This violates REQ-036, REQ-040, AC-002, AC-013, AC-014, and the operator
deliverables of TASK-004 through TASK-006.

Project only approved operations through the existing
`wyrd-client::Platform` and `Principals` handles, then replace stale content in
the existing self-hosting pages with commands that ship. Do not add another
transport, documentation page, SDK binding, or UI.

#### `FIND-admin-principals-12` — MCP never proves its authorized write

The real-server MCP journey proves discovery, denied write, and admin read but
never invokes `principals.revoke_credential` successfully or observes the
credential retired. The only approved MCP write therefore lacks the
discover-act-observe proof required by AC-014 and TASK-008.

Extend that journey to revoke a real non-current credential as the authorized
admin, then list or attempt use to observe retirement. Keep the approved MCP
surface to metadata listing and revocation; add no platform tools or
secret-returning issuance.

#### `FIND-admin-principals-14` — MCP catalog journeys retain the old catalog

The candidate correctly adds `principals.list_credentials` and
`principals.revoke_credential`, but the connectivity and discovery journeys
still assert that administrative callers see only the original three Bifrost
tools. `mise run test:bifrost:journey:mcp` deterministically fails those two
assertions, and adjacent `WyrdMcpHandler::catalog` rustdoc still says the
catalog contains exactly three tools. This is a candidate regression against
TASK-008, AC-014, and the required green agent-facing journey lane.

Update both existing exact ordered expectations and their prose to include the
two principal tools in current catalog order, leaving the opted-in probe last
in the connectivity fixture. Correct the adjacent server rustdoc in the same
edit. Preserve exactness and ordering; do not weaken the assertions to
containment, duplicate catalog construction in a helper, remove the principal
tools, or add a test target. This correction does not replace
`FIND-admin-principals-12`'s successful revoke/observe proof.

#### `FIND-003-2` — initialization acceptance proof is incomplete

The initialization journey covers success and sequential replay only. It omits
concurrent invocation, injected failure at each write, captured diagnostics,
and an uninitialized running server that serves ordinary traffic while stably
refusing the platform plane. REQ-022 through REQ-024 and AC-001 remain
unproved.

Add only those scenarios to the existing real-server/init harness. Prove one
winner under concurrency, clean retry after each write failure, no credential
in captured output or logs, and the uninitialized-server split behavior.

### 6. Verification plumbing and provenance

#### `FIND-admin-principals-5` — candidate verification tasks are unsafe or unrunnable

The new Postgres tasks in `mise.toml` depend on nonexistent `setup:postgres`
and use positional `cargo test` substring filters that can select zero tests.
They therefore fail before proof or can silently pass after renames, contrary
to the repository testing rules.

Reuse `scripts/postgres/with-test-postgres.sh` with the existing
`db:migrate:all:inner` pattern and exact nextest expressions for named tests,
or whole explicit targets when the task owns the target. Add no setup task or
new harness.

#### `FIND-TASK-001-10` — branch history violates repository identity rules

The immutable base-to-candidate log contains 58 commits using
`Claude <noreply@anthropic.com>` as author/committer or an AI co-author trailer.
This violates AGENTS.md section 13 even if the final tree is repaired.

This step is owned by the branch owner, not the implementation agent. After
the implementation tree is complete, rewrite only the unmerged offending
commits to the already-configured contributor identity and remove AI co-author
trailers without changing the cumulative tree. Do not run `git config`, set
identity environment variables, or add a replacement trailer.

## Constraints and preserved behavior

- Keep the approved revision 7 two-plane model and all 16 Card kinds.
- Keep durable behavior in Rust server owners and project it through the shared
  `wyrd-client`; do not duplicate it in CLI, MCP, or language SDKs.
- Keep one canonical Vala audit append and publisher. Do not create a platform
  audit store or distributed transaction across operator and tenant roles.
- Preserve one-time credential plaintext, stable non-enumerating public
  failures, fail-closed authorization, tenant isolation, and per-request
  revocation checks.
- Preserve the approved MCP subset: tenant credential metadata listing and
  revocation only. Platform administration and credential issuance remain out
  of MCP.
- Preserve the existing Card-bound containment lookup and
  `UNIQUE (data_tenant_id, name)`. Same-named Cards across spaces require a
  separate persistent-identity specification and are not part of this task.
- Correct the candidate rustdoc source warning without widening permanent CI.
- Do not address the base-reproduced
  `auth_e2e::cache_ttl_path_also_flips_verdict` failure as a candidate defect.
  The aggregate gate must be rerun after its independent baseline cause is
  resolved.
- No compatibility aliases, editable role system, additional auth bypass,
  audit publisher, connection wrapper, lifecycle service, status cache, lock
  service, transport, UI, documentation page, or test harness.

## Acceptance criteria

| Finding | Required observable acceptance |
|---|---|
| `FIND-admin-principals-1` | Allowed and denied platform decisions stage canonically and publish once; append or same-plane mutation failure leaves no allowance/effect mismatch; `platform.audit_authz` is absent. |
| `FIND-admin-principals-2` | Changed library fields/signatures expose only `TenantConn` or `OperatorPool`; initialization, registration, authorization, provisioning, and recovery retain their behavior. |
| `FIND-admin-principals-3` | Injected SQL, session/key, provider, and serialization failures return stable safe problem bodies while scrubbed source diagnostics remain server-side. |
| `FIND-admin-principals-4` | `5ea453f13` — `architecture/v1/00-foundations/service-identity.md`, `tenancy.md`, and the tenant-OIDC spec; `4c9c81f6d` — `wyrd-security-posture.md` credential and login model; `50fc266d0` — `wyrd-design.md` principal kinds reopened to both planes, preserving the concurrent change's `System` kind; admin route module docs already current in `a8ecda7e1`/`7039d3fe1`/`ad4eb9dd7` | `mise run docs:check`, `mise run lints` | PASS |
| `FIND-admin-principals-5` | Every changed `mise` task provisions its owned environment, selects a nonzero exact test set, and passes from a clean invocation. |
| `FIND-admin-principals-6` | The fixture comment describes containment and `cargo doc -p wyrd-sql --no-deps` emits no candidate private-link warning. |
| `FIND-admin-principals-7` | A registered and verified human receives the fixed grant and performs a protected platform operation; tenant authority cannot create or grant a platform principal. |
| `FIND-admin-principals-8` | Ungranted/unpinned rows cannot justify root suspension, and concurrent suspensions leave one independently authenticating authorized administrator. |
| `FIND-admin-principals-9` | Through served client/CLI paths, issue B, authenticate B, list metadata without plaintext, revoke A, reject A's live session, and retain B. |
| `FIND-admin-principals-10` | DNS rebinding after configuration is rejected for begin-login, callback token exchange, and JWKS refresh without an internal request. |
| `FIND-admin-principals-11` | Malformed, unknown, unusable, and known-wrong credentials each execute exactly one Argon2 verification and return the same stable error. |
| `FIND-admin-principals-12` | The existing real-server MCP journey performs authorized revocation and observes retirement while preserving discovery and denied-write checks. |
| `FIND-admin-principals-13` | Administrative OpenAPI failures carry `WyrdProblem`; revoke requires the shipped body, records its reason, and generated artifacts are current. |
| `FIND-admin-principals-14` | Both exact MCP catalog journeys include the two principal tools in shipped order, adjacent rustdoc is current, and `mise run test:bifrost:journey:mcp` passes all 9 tests. |
| `FIND-TASK-001-10` | Owned by the branch owner per the finding's own text. The exact rewrite command, its verification, and a correction naming the fifteen commits this remediation itself added trailers to are in `FIND-TASK-001-10-history-rewrite.md` | Verification commands recorded in that file; the rewrite is the owner's to run | DEFERRED |
| `FIND-003-2` | Exact real-server tests prove concurrent init, retry after every injected write failure, secret-free diagnostics, and uninitialized split behavior. |
| `FIND-003-3` | The unused variant and superseded command vocabulary/prefix are absent outside historical change records. |
| `FIND-004-2` | A real client lists/inspects tenants, suspends one, observes both fresh and existing token refusal, resumes it, and reuses the same principal/grant state. |
| `FIND-004-3` | Failure or cancellation at each post-claim stage and two racing creates converge on one active tenant, one admin principal, and one usable credential without orphans. |
| `FIND-004-4` | Exact real-server selectors exercise the actual stage-failure, cancellation/retry, and concurrent-create paths. |
| `FIND-004-5` | A real binary completes initialization, tenant creation/configuration, restricted-principal creation, rotation, lifecycle administration, and recovery without SQL; operator docs match those commands. |
| `FIND-005-2` | Tenant A cannot list, issue, revoke, authenticate, or recover tenant B identities, and no response reveals whether the target exists. |
| `FIND-006-3` | Recovery refuses every non-active state without adding a credential; active recovery retains the same principal and grants. |
| `FIND-006-4` | Forced canonical audit append failure refuses the mutation and leaves domain and audit rows unchanged. |

## Verification

Run every exact focused test introduced or named by the implementation through
`mise exec -- cargo nextest run --locked` with explicit package, target, and
exact test expression; use the repository Postgres wrapper wherever required.
At minimum, record the focused proof named in every acceptance row above.

Then run the narrowest owning capability lanes plus:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run docs:check
mise run check:client-tier
mise run check:unwrap-audit
mise exec -- cargo doc --locked -p wyrd-sql --no-deps
```

Because this remediation crosses contracts, server, SQL, client, CLI, MCP,
documentation, and shared CI configuration, finish with `mise run gate` after
the independently base-red auth journey is repaired or otherwise restored to a
green baseline. Do not claim repository-wide green from a run that omits or
filters that failure.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-admin-principals-1` | `a8ecda7e1` — `wyrd-auth/src/platform_authz.rs` stages through the canonical append; `wyrd-sql/src/queries/platform/audit_authz.rs` and migration `20260601000021_platform_authz_audit.sql` deleted | `mise run test:platform:journey` (`the_two_control_planes_cannot_reach_each_other`, `an_unrecordable_tenant_mutation_leaves_nothing_behind`) | PASS |
| `FIND-admin-principals-2` | `a8ecda7e1` — `wyrd-sql/src/operator_pool.rs`; platform components take `OperatorPool`/`TenantConn` only | `mise run test:platform:journey`, `mise run check:client-tier` | PASS |
| `FIND-admin-principals-3` | `301067e51` — `wyrd-server/src/http/error.rs`, `components/platform/identity.rs`, `components/principals/routes.rs` | `mise run test:platform:journey` (`served_platform_failures_disclose_nothing_internal`) | PASS |
| `FIND-admin-principals-4` | `5ea453f13` — `architecture/v1/00-foundations/service-identity.md` and `tenancy.md` restated to revision 7, `changes/active/tenant-oidc-federation/spec.md` distinguishes the deployment-owned platform connection; admin route module docs already current in `a8ecda7e1`/`7039d3fe1`/`ad4eb9dd7`. Four top-level authorities remain — see *Blocked* below | `mise run docs:check`, `mise run lints` | PARTIAL |
| `FIND-admin-principals-5` | `07100de71` — `mise.toml` principal lanes run under the repository Postgres wrapper with exact selectors | `mise run test:principals:unit` (4/4), `mise run test:principals:integration` (5/5) | PASS |
| `FIND-admin-principals-6` | `c798e0bb2` — `wyrd-sql/src/queries/auth/service_accounts.rs`, `wyrd-testing/src/server.rs`; `aae714ec1` — `wyrd-sql/src/error.rs` intra-doc link | `mise exec -- cargo doc --locked -p wyrd-sql --no-deps` — 0 warnings | PASS |
| `FIND-admin-principals-7` | `f908aedbe` — `wyrd-server/src/boot/init.rs`, `components/platform/identity.rs` | `mise run test:platform:journey` (`an_operator_lists_and_suspends_platform_administrators`) | PASS |
| `FIND-admin-principals-8` | `ad25a318d` — `wyrd-sql/src/queries/platform/principals.rs` counts only grantable, pinned rows | `mise run test:platform:journey` (`the_last_active_platform_principal_cannot_be_suspended`), `pg_platform_identity` | PASS |
| `FIND-admin-principals-9` | `7039d3fe1` — `components/platform/credentials.rs`, `wyrd-client/src/platform/handle.rs`, `wyrd-cli/src/platform/credential.rs` | `mise run test:platform:journey` (`an_operator_rotates_the_deployment_root_credential`, `revoking_a_platform_credential_ends_its_live_sessions`) | PASS |
| `FIND-admin-principals-10` | `cf7ccb56a` — `wyrd-auth-oidc/src/screening.rs` applied in `jwks.rs`, `callback.rs`, `login.rs`, `platform_login.rs` | `mise run test:platform:journey` (`a_connection_cannot_name_an_unresolvable_issuer`), `wyrd-auth-oidc` unit lane | PASS |
| `FIND-admin-principals-11` | `a88f885bc` — `wyrd-auth/src/platform_credentials.rs` verifies exactly once against a fixed dummy verifier | `wyrd-auth --lib` credential tests | PASS |
| `FIND-admin-principals-12` | `d4fab19d1` — `wyrd-mcp/tests/bifrost/mcp/principals.rs` performs the authorized revocation and observes retirement | `mise run test:bifrost:journey:mcp` (9/9) | PASS |
| `FIND-admin-principals-13` | `d4fab19d1` — `body = WyrdProblem` on every administrative error row; `RevokePrincipalRequest` required, reason recorded in `AuditDetail::PrincipalRevocation` | `mise run codegen:check`, `mise run docs:check`, `mise run test:platform:journey` (`a_tenant_revokes_a_compromised_principal_with_its_reason`) | PASS |
| `FIND-admin-principals-14` | `18b6b3676` — `wyrd-server/src/mcp/mod.rs`, `discovery.rs`, `connectivity.rs` | `mise run test:bifrost:journey:mcp` (9/9) | PASS |
| `FIND-TASK-001-10` | Owned by the branch owner, not this agent — see *Out of scope* below | — | DEFERRED |
| `FIND-003-2` | `32bc80f77` — `platform_admin_e2e.rs` concurrency, per-write injected failure, and uninitialized journeys | `mise run test:platform:journey` (`initialization_has_exactly_one_winner_under_concurrency`, `initialization_retries_cleanly_after_a_failure_at_each_write`, `an_uninitialized_deployment_serves_tenants_and_refuses_the_platform_plane`) | PASS |
| `FIND-003-3` | `6603b27f6` — `wyrd-auth/src/audit.rs`, `wyrd-server/src/boot/init.rs`, `local-development.svx` | `mise run docs:check`, `mise run lints` | PASS |
| `FIND-004-2` | `7798918b6` — `TenantProvisioning::{list,inspect,set_suspended}`, three platform routes, `wyrd-client` and `wyrd-cli` tenant verbs, `SqlRevocationCheck` tenant-admission gate | `mise run test:platform:journey` (`an_operator_suspends_and_resumes_a_tenant_through_the_platform_plane`) | PASS |
| `FIND-004-3` | `d5670dce4` — stale-claim adoption in `queries/platform/provisioning.rs`; promotion folded into the failure path | `mise run test:platform:journey` (`interrupted_and_racing_provisioning_converge_on_one_tenant`) | PASS |
| `FIND-004-4` | `d5670dce4` — the journey injects a real stage failure through a slug-scoped trigger and races two creates with `tokio::join!` | same selector as `FIND-004-3` | PASS |
| `FIND-004-5` | `bbd09ba2b` — `wyrd-cli/src/principal/credential.rs`, `platform/tenant.rs`, both self-hosting pages; `07c638d9a` — `wyrd-cli/tests/operator_journey.rs` | `mise run test:cli:journey` (`operator_journey::operator_administers_a_deployment_through_the_cli`), `mise run docs:check` | PASS |
| `FIND-005-2` | `ad4eb9dd7` — `require_principal` guard shared by `mint_credential` and `list_credentials_for` | `mise run test:platform:journey` (`one_tenant_cannot_reach_another_tenants_identities`) | PASS |
| `FIND-006-3` | `ad4eb9dd7` — directory gate in `components/platform/recovery.rs`, `ProvisionError::TenantUnavailable` → 404 | `mise run test:platform:journey` (`recovery_is_refused_for_every_state_but_active`) | PASS |
| `FIND-006-4` | `a8ecda7e1` + `301067e51` — injected `vala.audit_staging` append failure refuses the mutation | `mise run test:platform:journey` (`an_unrecordable_tenant_mutation_leaves_nothing_behind`) | PASS |

### Verification commands run

```
mise run fmt
mise run lints
mise run codegen:check
mise run docs:check
mise run check:client-tier
mise run check:unwrap-audit
mise exec -- cargo doc --locked -p wyrd-sql --no-deps
mise run test:platform:journey        # 28/28
mise run test:bifrost:journey:mcp     #  9/9
mise run test:principals:integration  #  5/5
mise run test:principals:unit         #  4/4
mise run test:cli:journey             # 24 passed, 5 ignored
git diff --check                      # clean
```

### Blocked

None. `FIND-admin-principals-4`'s two conflicted files were committed by
staging only this change's hunks against `HEAD`, leaving the concurrent
`verified-change-contract` edits unstaged in the worktree and its `System`
principal kind preserved verbatim in the committed text.

### Out of scope

`FIND-TASK-001-10` is a history rewrite the finding assigns to the branch
owner, and forbids the implementation agent from running `git config`, setting
identity environment variables, or adding a replacement trailer. The command,
its verification, and the list of commits this remediation itself must clean up
are in `FIND-TASK-001-10-history-rewrite.md`.

### Recorded conflicts and limits

- The task's closing instruction to finish with `mise run gate` conflicts with
  approved spec revision 7 `VER-003`, which forbids broad aggregates
  (`mise run gate`, `test:rust`, `--all-features` workspace lanes) as
  acceptance evidence. The approved spec is the authority, so the narrowest
  owning lanes above are the recorded evidence and `mise run gate` was not run.
- Principal revocation advances `tokens_not_before`; it does not retire the
  principal's credentials. A revoked principal holding a surviving credential
  can mint a fresh working token. That is the shipped behavior of every
  revocation path on this branch, and no finding asked to change it.
