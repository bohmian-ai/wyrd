---
task: TASK-003-006-R1
title: Initialization atomicity, tenant lifecycle enforcement, tenant-plane audit, and revocation epoch
spec: SPEC-admin-principals
spec_revision: 6
remediates: [TASK-003, TASK-004, TASK-005, TASK-006]
review: changes/active/admin-principals/review/task-003-006/verdict.md
obligations: [REQ-021, REQ-023, REQ-026, REQ-027, REQ-028, REQ-036, REQ-037, REQ-040, INV-005, INV-006, INV-013, AC-001, AC-002, AC-004, AC-007, AC-008, AC-009, AC-017]
---

## Subject

- Approved spec: `changes/active/admin-principals/spec.md`, revision 6.
- Original tasks: `changes/active/admin-principals/tasks/TASK-003-deployment-initialization.md`,
  `TASK-004-tenant-provisioning.md`, `TASK-005-tenant-administration.md`,
  `TASK-006-credential-lifecycle-and-recovery.md`.
- Reviewed candidate: `9bc53a6` on `claude/admin-principals-spec-qfsmjc`,
  base `40a73817d415e9a1626e6ec7a91edda083e344d3`.
- Verdict and full diagnosis: `verdict.md` in this directory. Standards audit:
  `standards-review.md`.

Verification scope remains `VER-001` … `VER-006`. Do not run broad aggregates.
Do not weaken, disable, `#[ignore]`, or delete any test, including the ones this
task says prove less than they claim — correct the behavior, then strengthen the
assertion.

## Issue diagnoses

### 1. Initialization is not atomic and can permanently brick a deployment (FIND-003-1)

**Violated obligation**: REQ-021, REQ-023, INV-005, TASK-003 constraint
"Initialization is one transaction".

**Current behavior**: `crates/wyrd/wyrd-server/src/boot/init.rs:76-101` issues
three independent statements against `&OperatorPool` — `insert_platform_principal`
(line 78), `set_platform_grant` (line 95), `PlatformCredentials::issue` (line 97).
`crates/wyrd/wyrd-sql/src/queries/platform/principals.rs:44-47` documents this
explicitly.

**Consequence**: a failure or cancellation after the principal row commits leaves
a `platform-admin` principal with no credential and no grant. The `UNIQUE(name)`
guard then makes every retry return `InitError::AlreadyInitialized`
(`init.rs:87-89`). There is no application-level path to a credential — the only
platform credential issuer is `initialize_platform_root` itself — so the
deployment is permanently unusable and can only be repaired with the out-of-band
SQL this change exists to abolish.

**Why the existing proof falls short**: `initialization_happens_at_most_once`
(`platform_admin_e2e.rs:144`) exercises two *successful* sequential runs. No test
injects a failure between the three statements, and no test runs them
concurrently, so neither the atomicity nor the race property is exercised.

### 2. Tenant lifecycle status is never enforced (FIND-004-1, FIND-006-3)

**Violated obligation**: REQ-026, REQ-028, INV-006, TASK-004 acceptance criterion
"A non-ready tenant … refuses authenticated tenant operations", TASK-005
acceptance criterion "A suspended tenant or principal is refused on every
operation on this surface".

**Current behavior**: `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:222-230`
checks the *principal's* status and the credential's lifecycle. It never reads
`platform.tenants.status`. `TenantConn::acquire`
(`crates/wyrd/wyrd-sql/src/tenant_conn.rs`) binds `app.current_tenant` and
performs no directory lookup. `TenantRecovery::recover`
(`components/platform/recovery.rs:69-101`) mints a credential for any
caller-supplied `tenant_id` without consulting the directory.

**Consequence**: the three lifecycle states this change added are advisory
decoration. A `suspended` tenant's principals keep authenticating and
authorizing; a `failed` tenant whose tenant-scope transaction committed is fully
usable; recovery works against both.

**Why the existing proof falls short**: the migration comment at
`migrations/20260601000022_tenant_lifecycle.sql:52-54` reasons that the live-tenant
index makes non-active tenants invisible "without any consumer change". That
holds for `list_active_tenant_ids` and `platform.resolve_tenant_by_slug` only.
Neither is on the credential-exchange path, because a Wyrd key carries its tenant
id in its prefix and never resolves by slug. No test covers a non-active tenant's
credential.

### 3. A failed provisioning burns its slug; retry never converges (FIND-004-3, FIND-004-7)

**Violated obligation**: REQ-027, AC-007, TASK-004 acceptance criterion "a retry
converges on exactly one correct tenant".

**Current behavior**: `insert_provisioning_tenant`
(`crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:27-44`) is a bare
`INSERT` with no `ON CONFLICT`, against `platform.tenants.slug TEXT UNIQUE`.
`TenantProvisioning::provision` (`components/platform/provisioning.rs:139-177`)
has no branch that adopts an existing `provisioning` or `failed` row. The failure
arm at line 172 discards `mark_tenant_failed`'s result and is an ordinary
`.await` inside the handler future, so a cancelled request never reaches it.

**Consequence**: after any failed attempt, retrying the identical request returns
`WyrdError::Conflict {"field":"slug"}` forever; that tenant can never be created
on this deployment. A cancelled attempt leaves the row stuck in `provisioning`
with no reason recorded. The rustdoc at `queries/platform/provisioning.rs:73-75`
claims "a retry knows it is resuming rather than starting fresh"; nothing resumes.

**Why the existing proof falls short**: there is no provisioning failure, retry,
or concurrency test at any tier (FIND-004-4). The highest-risk property of a
two-boundary operation is entirely unproven.

### 4. The tenant principal and credential surface writes no audit (FIND-005-1, FIND-006-4)

**Violated obligation**: REQ-037, AC-009, INV-011, AGENTS.md §2,
`architecture/agent-rules.md`, TASK-005 approach step 3, TASK-005 and TASK-006
constraints and acceptance criteria.

**Current behavior**: `components/principals/routes.rs` makes four authorization
decisions through `require_principal_admin` (lines 57-69, called at 137, 199, 217,
241) — a pure in-memory `PermissionSet::contains` with an early return. No
transaction is opened for the decision, no row is appended for an allowance or a
denial, and nothing fails closed on an unrecordable audit. The decisions govern
creating durable principals, minting credentials, reading credential metadata,
and revoking. `components/admin/routes.rs:15-17` still records the deliberate
no-audit stance the spec lists under *Required architecture amendments*.

**Consequence**: AC-009's requirement that audit state which principal, using
which credential, performed which operation, against which tenant, at what time
cannot be satisfied for the operations that establish tenant authority. Denials —
including the escalation attempt at `platform_admin_e2e.rs:391-406` — leave no
record.

**Why the existing proof falls short**: no test asserts an audit row for either
outcome on this surface, and the injected-audit-failure refusal that TASK-005 and
TASK-006 both require has nothing to exercise.

### 5. Credential revocation does not stop the tokens it minted (FIND-006-1)

**Violated obligation**: INV-013, AC-005, AC-010, TASK-006 acceptance criterion "A
revoked credential's live tokens stop verifying at the epoch".

**Current behavior**: `REVOKE_API_KEY_SQL`
(`crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:11-15`) sets
`wyrd.auth_api_keys.revoked_at` and nothing else. The epoch the verifier consults
is `wyrd.auth_service_accounts.tokens_not_before`, read by
`service_account_revocation_epoch`
(`crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:44-60`) through
`SqlRevocationCheck` (`wyrd-auth/src/revocation_resolver.rs:113-133`). Nothing on
the credential-revocation path writes it.

**Consequence**: an access token minted from a leaked credential keeps
authorizing until its natural expiry after the operator revokes that credential.

**Why the existing proof falls short**: `platform_admin_e2e.rs:485-489` asserts
that the revoked credential can no longer *mint a new token*, under the message
"the retired credential stops working". A token minted from `first` before the
revocation is never constructed, so the property the acceptance criterion states
is never tested — and it is false.

### 6. Missing surfaces: suspend/list/inspect, CLI, documentation, OpenAPI (FIND-004-2, FIND-004-5, FIND-004-6, FIND-006-2, FIND-005-4)

**Violated obligation**: REQ-028, REQ-036, REQ-040, AC-002, AC-008, TASK-004
approach steps 4, 5 and 7, TASK-005 approach step 5, TASK-006 approach steps 5
and 6.

**Current behavior**:
- `components/platform/routes.rs:34-41` exposes two routes. There is no tenant
  listing, no single-tenant read, no suspend and no resume.
  `set_tenant_suspended` (`queries/platform/provisioning.rs:105-127`) is written,
  correct, and has zero callers. `Permission::tenant_read()` and
  `Permission::tenant_suspend()` are granted at `boot/init.rs:54-55` and asserted
  at `boot/init.rs:110-133`, but no route requires either — the test proves a
  grant, not a capability.
- `crates/wyrd/wyrd-cli/` and `docs/src/content/docs/self-hosting/` are untouched
  by the whole diff. The spec's headline three-command journey has no
  `wyrd tenant create`.
- No new handler carries `#[utoipa::path]` and `http/openapi.rs:14-41` registers
  none of them, so `openapi.yaml` contains no `/platform` or `/v1/principals`
  path and `codegen:check` passes vacuously.
- `components/principals/routes.rs:239-245` discards the `principal_id` its own
  route path names, so a credential belonging to a different principal in the
  same tenant is revoked and reported as success.
- `components/principals/routes.rs:149-159` renders a `UNIQUE (data_tenant_id,
  name)` violation as `WyrdError::Internal` (HTTP 500) with the raw database
  message, where the platform plane's `slug_or_store`
  (`provisioning.rs:256-260`) correctly produces `WyrdError::Conflict`.

## Intended correction outcome

A deployment survives a crashed initialization and retries cleanly. A tenant that
is not `active` is unusable through every authenticated path. A failed
provisioning is resumable to one correct tenant. Every authorization decision on
the tenant principal and credential surface is recorded in the transaction that
acts on it and refuses when it cannot be. Revoking a credential immediately stops
the tokens it minted. The platform plane can list, inspect, suspend and resume
tenants, and the whole administrative contract is reachable from the CLI, the
generated OpenAPI document, and the self-hosting documentation.

## Decision-complete recommendation

Each correction reuses an owner the repository already has. None requires a new
product, API, architecture, security, compatibility, cross-service,
concurrency-semantics, or persistent-data decision, so all of it stays inside
spec revision 6.

**1. Make initialization one transaction.** Move the three writes onto one
`Transaction<'_, Postgres>` begun from `OperatorPool`, in `wyrd-sql` where the
durable layer belongs (AGENTS.md §15), following the shape
`PlatformAuthorization::authorize` already uses to hand an open transaction
across a boundary (`wyrd-auth/src/platform_authz.rs:110-137`). The `UNIQUE(name)`
guard stays exactly as it is — it is the right single-initialization mechanism
and the reason the concurrent case converges; the defect is only that the
remaining writes are outside its transaction. Argon2 hashing must stay on
`spawn_blocking` and must happen *before* the transaction opens, so no connection
is held across the hash. Delete `InitError::NotConfigured` or construct it at the
one site in `main.rs:75-79` that currently duplicates its message (FIND-003-3).

**2. Gate the tenant lifecycle at the authentication seam, once.** The one place
every tenant-plane entry converges is credential exchange, so put the gate there
rather than in each handler — a root-cause fix at the shared owner, not repeated
symptom guards. `exchange_api_key` already resolves the tenant id from the key
prefix before any tenant-scoped read; add the directory status check on that
path. `platform.tenants` is outside RLS, so read it the way the existing
`platform.resolve_tenant_by_slug` security-definer function already does for
`wyrd_app` — extend or mirror that established mechanism rather than handing
`wyrd-auth` an `OperatorPool`, which would be a new connection path on the
per-request surface and is not permitted. A non-active tenant must produce the
single indistinguishable invalid-credential error (INV-012), never a
state-revealing one. Apply the same refusal in `TenantRecovery::recover` before
minting, on the operator boundary it already holds.

**3. Make provisioning resumable rather than conflict-on-retry.** Change
`insert_provisioning_tenant` so a request for a slug whose row is `provisioning`
or `failed` adopts that row and continues, while a slug whose row is `active` or
`suspended` remains `SlugTaken`. Adoption must reuse the existing
`data_tenant_id`, so the resumed attempt converges on one tenant. The tenant-scope
work is already idempotent-friendly — `seed_builtin_roles_for_tenant` is
idempotent by contract and `tenant_admin_principal_id`
(`queries/auth/service_accounts.rs`) already resolves an existing administrator —
so resumption should reuse an existing tenant administrative principal and issue
its credential rather than creating a second. This subsumes FIND-004-7: a stale
`provisioning` row left by a cancelled request is no longer a stuck state, so no
separate cancellation guard is needed. Stop discarding `mark_tenant_failed`'s
result. Resolve the rustdoc at `queries/platform/provisioning.rs:73-75` so it
describes what the code does.

**4. Give the tenant plane the audit shape the platform plane already has.**
`PlatformAuthorization` is the pattern: begin the transaction, append the
decision, roll back and refuse on an append failure, commit denials durably, hand
the open transaction to the caller so decision and effect commit together. It is
bound to `OperatorPool`, so the tenant-plane equivalent is that same shape over
`TenantConn` against the tenant-scope authorization audit table. Do not duplicate
`platform.audit_authz`; use the tenant-scope audit owner that
`components/auth/audit_writer.rs` and `queries/auth/` already provide, and add a
column only if the existing row cannot carry the authenticating credential id.
Every decision in `components/principals/routes.rs` routes through it — allowed
and denied alike — replacing `require_principal_admin`'s bare early return.
Retire the no-audit note at `components/admin/routes.rs:15-17`.

**5. Couple credential revocation to the epoch.** Advance the owning principal's
`wyrd.auth_service_accounts.tokens_not_before` in the same transaction as the
`revoked_at` write. The epoch machinery, its resolver and its listener all exist
and are correct for principal revocation; reuse them and add no second revocation
mechanism. In the same change, resolve the credential by both tenant and the
`principal_id` the route path names, and return not-found on a mismatch
(FIND-006-2). Note the trade-off and accept it: advancing the principal epoch
invalidates tokens minted from that principal's *other* live credentials too.
That is what INV-013's "applicable authorization epoch" means at the granularity
the schema offers, and it does not break overlapping rotation — the surviving
credential re-exchanges immediately. Do not introduce per-credential epoch state
to avoid it; that is a persistent-data decision outside this task.

**6. Complete the surfaces.** Add tenant listing, single-tenant inspection, and
suspend/resume to `platform_router`, authorized on `Permission::tenant_read()`
and `Permission::tenant_suspend()` — already granted — and audited through
`PlatformAuthorization` like the two existing routes; `set_tenant_suspended` is
already written and correct, so this is wiring, not new persistence. Classify the
duplicate-name unique violation the way `slug_or_store` already does
(FIND-005-4). Annotate all handlers with `#[utoipa::path]` and register their
paths and component schemas in `http/openapi.rs`, then regenerate. Add the CLI
commands for tenant creation and tenant principal/credential administration under
the existing `wyrd-cli` command structure. Rewrite
`docs/src/content/docs/self-hosting/` to the three-command journey, covering
rotation and credential-loss recovery including the deployment-level path for the
global credential.

## Constraints and preserved behavior

- Everything the review found correct stays correct: the two-plane type-level
  separation, `PlatformAuthorization`'s decision/audit coupling, the durable
  `UNIQUE(name)` single-initialization guard, the migrations, the
  identity/credential separation, recovery against the same principal id, and the
  removal of `bootstrap-key`, its fabricated CardRef and `SYSTEM_OPERATOR_ID`.
- Exactly two connection abstractions: `&mut TenantConn<'_>` under RLS and
  `&OperatorPool`. No third path, and no `OperatorPool` on the per-request tenant
  surface.
- No hand-written tenant filter added to a tenant-scope query; RLS remains the
  tenant boundary.
- INV-002: no plaintext in any durable, log, trace, error or audit payload.
  INV-012: one indistinguishable invalid-credential error.
- No compatibility route, alias, legacy name or migration shim.
- Migrations are forward-only and must validate existing rows.
- The five existing journeys keep passing; strengthen the rotation journey's
  assertion rather than replacing it.
- Non-goals unchanged: human identity and OIDC (TASK-007); SDK and MCP projection
  (TASK-008); billing, plans, quotas, `Organization`, tenant deletion, signup UI;
  a credential-management UI; per-credential epoch granularity.

## Acceptance criteria

1. An injected failure at the grant write and at the credential write during
   initialization leaves no `platform.principals` row, and a subsequent
   invocation initializes cleanly. (FIND-003-1)
2. Concurrent invocations of initialization yield exactly one principal and one
   credential; the loser refuses. (FIND-003-2)
3. Server start, before and after initialization, emits no credential material to
   captured stdout, logs or traces. (FIND-003-2)
4. An uninitialized server serves ordinary routes and refuses platform routes
   with a stable error. (FIND-003-2)
5. `InitError::NotConfigured` is either constructed or gone, with no duplicated
   message literal. (FIND-003-3)
6. A credential belonging to a `provisioning`, `failed`, or `suspended` tenant is
   refused with the single indistinguishable invalid-credential error, and yields
   no authenticated tenant context. (FIND-004-1)
7. Suspending a tenant stops its principals authenticating and stops their live
   tokens at the epoch; resuming restores access with state and grants intact.
   (FIND-004-1, FIND-004-2)
8. Recovery against a non-active tenant is refused without revealing the tenant's
   state. (FIND-006-3)
9. An injected failure at each provisioning stage leaves no usable tenant, is
   observable as failed or still provisioning, and an identical retry converges on
   exactly one tenant with exactly one administrative principal and no orphaned
   credential. (FIND-004-3, FIND-004-4, FIND-004-7)
10. Concurrent creation for one slug yields exactly one tenant and one
    administrative principal. (FIND-004-4)
11. A cancelled provisioning request does not leave a permanently unusable slug.
    (FIND-004-7)
12. Every authorization decision on `components/principals/routes.rs` appends its
    row in the transaction that performs the operation, for allowed and denied
    alike, naming principal, authenticating credential, permission, resource,
    tenant and outcome. (FIND-005-1)
13. An injected audit-append failure on that surface refuses the operation and
    commits nothing. (FIND-005-1, FIND-006-4)
14. `components/admin/routes.rs` no longer records a deliberate no-audit stance.
    (FIND-005-1)
15. A tenant A credential is refused against tenant B's principals and
    credentials, and the denial reveals nothing about tenant B. (FIND-005-2)
16. No tenant-plane operation — principal creation or role grant — can create a
    platform principal or confer platform authority. (FIND-005-2)
17. A duplicate principal name returns a caller-correctable conflict, not an
    internal error carrying a database message. (FIND-005-4)
18. A token minted from credential A before A is revoked is refused immediately
    after A is revoked, while the principal's other credentials re-exchange
    successfully. (FIND-006-1)
19. Revoking a credential under a principal id that does not own it returns
    not-found and revokes nothing. (FIND-006-2)
20. A platform principal can list tenants, inspect one tenant's state, and suspend
    and resume a tenant, each decision audited. (FIND-004-2)
21. `openapi.yaml` describes every platform and tenant administrative route, and
    `codegen:check` is meaningful for this surface. (FIND-004-6)
22. The three-command operator journey — initialize, create tenant, configure and
    create a restricted machine principal — is drivable through the shipped CLI
    with no database access after initialization. (FIND-004-5, FIND-003-2)
23. Self-hosting documentation describes install → initialize → create tenant →
    configure, plus rotation and credential-loss recovery. (FIND-004-5)
24. No reference to `bootstrap-key` remains outside `changes/`. (FIND-004-8)

## Proof

Focused proof that directly exercises each gap:

- Initialization atomicity, concurrency, and staged failure: Postgres-backed, in
  the initialization suite, through `scripts/postgres/with-test-postgres.sh`.
- Non-active-tenant refusal at exchange, and suspend/resume round trip: a
  real-server journey, since it spans client → server → client and the epoch.
- Provisioning failure, retry convergence, cancellation, and concurrency:
  Postgres-backed integration proof against the real two-boundary path.
- Tenant-plane audit for allowed and denied outcomes, and the injected
  audit-append refusal: Postgres-backed integration proof, mirroring the existing
  `platform_authz.rs` pg_tests shape.
- Cross-tenant and platform-escalation denial on the principal surface: a
  real-server journey.
- Revocation epoch: strengthen
  `a_tenant_rotates_an_automation_credential_without_an_outage` so it mints a
  token from the first credential *before* revoking it and asserts that token is
  refused afterwards. Do not remove any existing assertion.
- CLI journey covering the three-command path.

Broader verification for the touched surfaces:

```bash
mise run fmt
mise run lints
mise exec -- cargo clippy --locked -p wyrd-auth -p wyrd-sql -p wyrd-server -p wyrd-cli -p wyrd-testing --all-targets
mise run codegen:check
mise run docs:check
mise run check:unwrap-audit
mise run test:principals:unit
mise run test:principals:integration
mise run test:platform:journey
```

Every specifically named Rust test must also be run through its exact focused
expression, `mise exec -- cargo nextest run --locked -p <crate> <target> -E
'test(=…)'`, with the repository-managed Postgres wrapper where required
(`VER-002`). Do not run broad aggregates (`VER-003`).
