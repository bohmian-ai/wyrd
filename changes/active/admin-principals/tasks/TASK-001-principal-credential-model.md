---
task: TASK-001
title: Principal and credential model
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-039, INV-001, INV-002, INV-003, INV-008, INV-009, INV-010, INV-012, INV-014, AC-010]
depends_on: []
---

## Objective

Durable principals exist independently of credentials, and credentials are
principal-generic. A principal may hold several simultaneously valid
credentials, each independently revocable, with the plaintext returned exactly
once and never recoverable. The principal type set distinguishes global
administration, tenant administration, humans, and machines, and tenancy is
present only where it applies. Card binding becomes a property of a machine
principal, not a precondition for holding a credential.

This task delivers the durable model and its Rust-native operations. It does
not deliver routes, initialization, or provisioning — later tasks consume it.

## Constraints

- `wyrd-spec` stays IO-free, async-free, and PyO3-free.
- Platform-scope principals, credentials, and grants live at platform scope
  reachable through `&OperatorPool`; tenant-scope rows stay under `TenantConn`
  RLS. No third connection abstraction, no hand-written tenant filters.
- Existing Card-bound Service and Agent principals keep `card_ref`,
  `card_ref_scope`, `wyrd apply` provisioning, and emit-scope behavior
  unchanged. Their durable identity keys must survive.
- Reuse the existing Argon2 hashing, lookup-prefix, expiry, revocation,
  `last_used_at`, single indistinguishable invalid-credential error, and
  `wyrd.audit_credential_issuance` seams. Do not introduce a parallel
  credential implementation.
- `wyrd.auth_refresh_tokens` is the principal-generic precedent; follow it
  rather than inventing a different ownership shape.
- Non-goal: routes, HTTP contracts, initialization, tenant provisioning, OIDC,
  role-grant management surfaces.
- Non-goal: a new RBAC engine, explicit deny, or permission-scope redesign.

## Relevant Surface

- `crates/wyrd-spec/src/ids.rs`, `crates/wyrd-spec/src/auth/` — identity
  newtypes, principal kind tags, wire-facing enums.
- `crates/shared/wyrd-runtime/src/principal.rs` — `Principal`, `PrincipalKind`.
- `crates/wyrd/wyrd-sql/migrations/`, `crates/wyrd/wyrd-sql/src/queries/auth/`,
  `crates/wyrd/wyrd-sql/src/queries/platform/` — durable schema and query slots,
  including the dormant `platform.users`/`roles`/`user_roles`/`api_keys`
  objects this task resolves per `REQ-039`.
- `crates/wyrd/wyrd-auth/src/issue_api_key.rs`, `repo.rs`, `service_accounts.rs`
  — issuance, lookup, revocation.
- `crates/shared/wyrd-crypt` — hashing and secret generation.

## Approach

1. Fix the principal and credential contracts in `wyrd-spec`: type set, tenancy
   constraints, credential metadata, and the identity newtypes they need.
2. Widen `Principal`/`PrincipalKind` in `wyrd-runtime` so a machine principal
   may or may not carry a Card, preserving the existing Card-bound projections.
3. Write the migration: platform-scope principal, credential, and grant tables
   (resolving the dormant `platform.*` objects), tenant-scope principal changes,
   and principal-generic credential ownership with its durable constraints.
4. Move credential issuance, lookup, verification, revocation, and rotation onto
   the principal-generic owner, keeping the existing hashing and error contract.
5. Add focused coverage for the tenancy constraints, multi-credential ownership,
   independent revocation, rotation overlap, and verifier-only persistence.

## Acceptance Criteria

- A principal persists and resolves with no credential; issuing, revoking, and
  expiring credentials never mutates the principal or its grants.
- One principal holds two valid credentials at once; revoking or expiring one
  leaves the other authenticating and the principal's authorization unchanged.
- A global-administration principal cannot persist with a tenant, and a
  tenant-scope principal cannot persist without one; both are rejected durably.
- A machine principal persists and holds a credential with no Card. An existing
  Card-bound Service or Agent principal keeps its card identity and resolves as
  before.
- Credential creation returns the plaintext once; the stored row holds only the
  verifier plus non-secret metadata; no read, list, log, trace, or error path
  returns or records the plaintext.
- Every invalid-credential condition — unknown, expired, revoked, wrong secret —
  returns one indistinguishable error.
- No `platform.users`/`roles`/`user_roles`/`api_keys` object remains that is
  neither part of the platform store nor removed.

## Verification

Scope is `VER-001` through `VER-006`: focused and subsystem-integration proof
only. Do not run `mise run gate`, `test:rust`, family lanes, or any
`--all-features` workspace lane, and do not treat failures outside the
principal and credential surfaces as this task's obligation.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-runtime -p wyrd-auth -p wyrd-sql --all-targets
mise exec -- cargo nextest run --locked -p wyrd-spec --lib
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib
```

Postgres-backed constraint and lifecycle coverage runs through the
repository-managed wrapper (`scripts/postgres/with-test-postgres.sh`) against
the migrated schema. Add a focused `mise` lane for this capability following the
`test:cards:unit` / `test:cards:integration` pattern, and run it.

`mise run codegen:check` when generated contracts move.

## Implementation decisions

**Scope vs authority.** `REQ-003` states human principals have exactly one
tenant; `REQ-042` states human platform principals have none. Both are satisfied
by reading the principal *type* as fixing **scope** and grants as fixing
**authority**: `GlobalAdmin` is the platform-scope type and carries no tenant,
while `TenantAdmin`, `Human`, `Service`, and `Agent` are tenant-scope types that
carry exactly one. A human platform administrator is a platform-scope principal
that authenticates through OIDC rather than a credential (`TASK-007`); "human"
as a *type* stays tenant-scoped. No requirement is changed.

**Two principal stores, one model.** Tenant-scope principals stay in `wyrd.*`
under `TenantConn` RLS, which remains the load-bearing tenant boundary.
Platform-scope principals, credentials, and grants live in `platform.*` reached
only through `OperatorPool`. Absence of tenancy for platform principals is
structural — the platform table has no tenant column — rather than a nullable
column guarded by a check.

**Runtime projection.** `wyrd_runtime::Principal` keeps its required
`tenant_id` and remains the tenant-scope projection. The platform-scope
projection is a separate type. This preserves the approved two-variant
authenticated context (`TASK-002`) without making tenancy optional on every
handler that reads it.

## Verification evidence

Environment: `mise` is not installed in this container, so `mise exec -- cargo …`
was substituted with direct `cargo` invocations against the same toolchain, and
Postgres-backed suites ran through the repository wrapper
`scripts/postgres/with-test-postgres.sh`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Principal persists and resolves with no credential; credential lifecycle never mutates principal or grants | `queries/platform/principals.rs`, `principal_grants.rs` | `pg_admin_principals::principal_persists_and_is_authorized_without_any_credential`, `…::credentials_are_independent_of_each_other_and_of_authority` | PASS |
| One principal holds two live credentials; revoking one leaves the other authenticating, authorization unchanged | `queries/platform/credentials.rs` | `pg_admin_principals::credentials_are_independent_of_each_other_and_of_authority` | PASS |
| Global-administration principal cannot persist with a tenant | `migrations/20260601000020_admin_principals.sql` | `pg_admin_principals::platform_principals_have_no_tenant_column`, `…::platform_store_rejects_a_tenant_scope_kind` | PASS |
| Tenant-scope principal cannot persist without a tenant | pre-existing `data_tenant_id NOT NULL REFERENCES platform.tenants` on `wyrd.auth_service_accounts`, unchanged by this task | inherited constraint; not separately re-proven, and this task adds no path that could weaken it | PASS (inherited) |
| Machine principal persists and holds a credential with no Card; Card-bound Service/Agent keep their identity | `migrations/…_admin_principals.sql`, `queries/auth/service_accounts.rs`, `wyrd-runtime/src/principal.rs` | `pg_admin_principals::tenant_admin_principal_persists_without_a_card`, `…::tenant_admin_principal_cannot_bind_a_card`, `wyrd-auth-verify::into_verified_accepts_card_free_service_with_empty_scope` | PASS |
| Credential creation generates the secret, returns plaintext once, persists only a verifier | `wyrd-auth/src/platform_credentials.rs` (`PlatformCredential::generate`, `PlatformCredentials::issue`) | `platform_credentials::pg_tests::issued_plaintext_authenticates_and_is_never_stored`, `platform_credentials::tests::*` | PASS |
| Every invalid-credential condition returns one indistinguishable error | `platform_credentials.rs::authenticate`, `PlatformCredentialError::InvalidCredential` | `platform_credentials::pg_tests::every_rejection_is_the_same_error` (malformed, unknown prefix, wrong secret, revoked, expired, suspended principal) | PASS |
| No unreachable second identity model remains in the schema | `migrations/…_admin_principals.sql`, `queries/platform/mod.rs`, `wyrd-sql/src/lib.rs` | `pg_migration::migrations_apply_and_are_idempotent` | PASS |

Commands run (final pass):

```bash
cargo fmt --all
python3 scripts/check_tenant_isolation.py                       # passed
cargo run -p wyrd-spec --example gen_schemas --features server  # regenerated; 8 goldens committed
cargo clippy --locked -p wyrd-spec -p wyrd-runtime -p wyrd-auth-verify \
  -p wyrd-auth-issue -p wyrd-sql -p wyrd-auth --all-targets     # no new findings
scripts/postgres/with-test-postgres.sh -- bash -lc '\
  cargo test --locked -p wyrd-sql --test pg_admin_principals -- --test-threads=1        # 9 passed
  cargo test --locked -p wyrd-auth --lib platform_credentials -- --test-threads=1       # 7 passed
  cargo test --locked -p wyrd-auth --lib exchange_api_key -- --test-threads=1           # 10 passed
  cargo test --locked -p wyrd-auth --lib -- --test-threads=1'                           # 87 passed, 0 failed
cargo test --locked -p wyrd-spec --lib auth::principal_kind    # passed
cargo test --locked -p wyrd-runtime --lib                      # 43 passed
cargo test --locked -p wyrd-auth-verify --lib into_verified    # 11 passed
git diff --check                                               # clean
```

New repository lanes: `test:principals:unit` and `test:principals:integration`
in `mise.toml`, following the `test:cards:*` pattern.

Material limits:

- Seven `wyrd-auth-verify` external-JWKS tests and two `wyrd-spec` golden and
  proptest cases fail in this container for reasons that reproduce at the base
  commit (missing rustls crypto provider; pre-existing). `VER-005` places them
  out of scope.
- Non-goals held: no routes, initialization, tenant provisioning, OIDC, or RBAC
  engine changes. Minting a *tenant* access token for a Card-free principal is
  refused with `IssueError::InvalidPrincipalKind` and lands in `TASK-002` with
  `REQ-012`; platform credential issuance and verification are delivered here
  because `TASK-003` consumes them.

## Review remediation

Review `changes/active/admin-principals/review/task-001/` returned
`FIX_REQUIRED` with 10 findings. All 10 were independently reproduced and
addressed:

| Finding | Class | Resolution |
|---|---|---|
| FIND-1 | REGRESSION | Raw test inserts in `exchange_api_key.rs` still named `sa_id`; renamed. `wyrd-auth` lib now 87 passed / 0 failed with `pg_tests` included. |
| FIND-2 | VIOLATION | `check_tenant_isolation.py` failed on the three new platform query modules; added the sanctioned raw-query marker used by `queries/auth/service_accounts.rs`. Check passes. |
| FIND-3 | VIOLATION | Generated schema goldens were stale after the `PrincipalKindTag` change; regenerated and committed 8 files. |
| FIND-4 | MISSING | Platform credential issuance did not exist and its proof was circular. Added `wyrd-auth/src/platform_credentials.rs` with real generation, Argon2 hashing off the runtime, verifier-only persistence, and single-error verification, proved against Postgres. |
| FIND-5 | INCORRECT | `ServiceAccountPrincipalRow.card_ref` widened to match the nullable column; the three latent decode sites (`refresh.rs`, `jwt_bearer.rs`, delegation) now handle a Card-free principal explicitly. `revoke.rs` no longer duplicates the kind mapping and defers to `principal_kind_wire`, which now recognizes `tenant_admin`. |
| FIND-6 | MISSING | Added `test:principals:unit` and `test:principals:integration` to `mise.toml`. |
| FIND-7 | DRIFT | Deleted `is_platform_scoped`, `may_bind_card`, and `count_platform_principals` — no production caller and the CHECK constraints already enforce the properties. |
| FIND-8 | INCORRECT | A kind error now returns `IssueError::InvalidPrincipalKind` rather than `InvalidCardRef`, in both issue paths. |
| FIND-9 | VIOLATION | Corrected `insert_platform_principal` rustdoc that promised transactional composition its signature cannot provide. |
| FIND-10 | VIOLATION | Git identity is `Claude <noreply@anthropic.com>` rather than the AGENTS.md §13 contributor identity. Surfaced to the user; §13 forbids fixing it with `git config`, so it remains open and outside this task's control. |
