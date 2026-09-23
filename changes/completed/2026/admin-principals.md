# Global and tenant administrative principals

| | |
|---|---|
| Change ID | `SPEC-admin-principals` |
| Completed | 2026-09-23 |
| Reviewed target | `1e21405b70098fdcb87d3c6fee78e82397e00a07` (`main`) |
| Approved specification | revision 14, approved 2026-09-21 |
| Delivery reference | not supplied |
| Completion authority | **human override** — see "Completion authority" |

## Intent and operator value

Wyrd had no way to create a tenant. `platform.tenants` rows existed only because
a migration seeded the system sentinel or a test fixture inserted one directly,
and the sole administrative bootstrap (`wyrd-server bootstrap-key`) minted a
credential for a tenant that had to already exist. Every deployment was
therefore unusable without out-of-band SQL.

This change ships one administrative identity model for SaaS, self-hosted, and
enterprise deployments: fully programmatic and headless, with human OIDC
authentication, credential rotation and recovery, and the foundation for service
accounts and finer-grained authorization.

The operator journey is three steps:

```text
1. start the server            (no administrative state; platform routes refuse)
2. wyrd-server init            -> wyrd_global_xxx, printed once to the terminal
3. wyrd tenant create --name acme  -> wyrd_tenant_xxx, printed once
```

## Shipped externally observable behavior

- `wyrd-server init` creates the first global administrative principal and
  prints its credential exactly once. Server boot never emits credential
  material, so the deployment root credential never enters the log pipeline.
- Global administrative principals create, provision, and administer tenants;
  `platform.tenants.status` carries provisioning and failure states.
- Tenant administrative principals configure their tenant and its OIDC
  connection, manage tenant identities, create service principals, and manage
  credentials.
- Human administrators authenticate through OIDC; refresh-token families back
  human sessions.
- Credentials are issued, rotated, and revoked independently of the principal
  they authenticate. Plaintext is returned only at issuance.
- Tenant access tokens are standard five-minute self-contained JWTs carrying an
  authority snapshot. The privileged platform plane remains database-backed per
  request.
- Administrative surfaces are transactionally audited, superseding the previous
  no-audit stance on admin routes.
- The public HTTP contract is the `utoipa`-generated OpenAPI document served at
  `GET /openapi.json`, with first-class Rust, Python, and TypeScript client
  projections, MCP tools, and CLI commands over the same surface.

## Lasting invariants and constraints

- **Credentials authenticate principals. Roles and permissions authorize
  principals.** Authorization binds to the principal, never to a credential.
- Identity and credentials are separate durable concepts. A credential row
  references a principal and is never itself an identity or a grant owner.
- A principal may hold multiple independently revocable credentials. Rotation
  overlaps credentials rather than mutating principal authority.
- Global principals are tenantless; tenant principals carry typed tenant
  ownership. The authenticated context is one closed two-variant type — platform
  scope or tenant scope — produced by one authentication pipeline, making the
  plane split a type-level guarantee mirroring `OperatorPool` / `TenantConn`.
- Card binding is a property of a machine principal, not a precondition for
  holding a credential.
- Credential verification returns one indistinguishable invalid-credential
  result.
- Tenant request verification is local signature, issuer, audience, and expiry
  verification — no database read, no cache. Revocation and grant changes stop
  subsequent issuance; existing tenant JWTs expire within five minutes. No
  hybrid JWT cache, authorization epoch, revocation checker, or per-request
  tenant-auth introspection is permitted.
- Delegation follows RFC 8693 subject/actor semantics and only ever narrows
  authority.
- Credential issuance and revocation use the canonical transactional audit path.
- Security and availability uncertainty fails closed.

## Material decisions and rationale

- **`PrincipalKind` was amended**, not extended sideways: the doctrine-locked
  set now distinguishes global administration, tenant administration, humans,
  and machines, with tenancy absent for global principals. A parallel identity
  type would have left two competing models.
- **Initialization is an operator-invoked subcommand authorized by database-
  credential possession**, not a server-start side effect, so credential
  material never reaches the log pipeline.
- **Five-minute stateless tenant JWTs over a revocation checker.** Issuance
  resolves current grants once; requests verify locally. The bounded staleness
  window was accepted as the price of removing per-request auth reads from the
  high-volume plane. The low-volume platform plane kept per-request revalidation
  because its cost is affordable and its blast radius is larger.
- **No new RBAC engine.** Existing `Permission` semantics were retained;
  delegation intersection is a narrow operation, not a policy language.
- **`bootstrap-key`, its fabricated Card, and `SYSTEM_OPERATOR_ID` were
  deleted** rather than retained beside the new model, so no second bootstrap
  path survives.
- **PostgreSQL owns coordination time** (TASK-009). Every database coordination,
  eligibility, lease, liveness, retry, and relative-expiry predicate derives its
  instant from `statement_timestamp()`; Rust binds durations and producer-owned
  domain timestamps. The refresh JWT `exp` and its durable row expiry are
  derived from one truncated PostgreSQL instant, so the signed claim and the
  stored expiry cannot disagree. This rule is recorded in `AGENTS.md` §15.

## Approved spec revisions and material deviations

- The specification reached **revision 14**. The material amendment during
  implementation replaced an earlier hybrid revocation/permission design with
  the five-minute stateless tenant JWT architecture described above.
- Two human decisions were recorded and honored rather than treated as findings:
  retaining the audit-publisher `FOR UPDATE NOWAIT` correction, and omitting an
  unshipped compatibility migration.

## Completion authority

This record is written under an **explicit human override**. The final
integrated `$wyrd-change-review` returned **`BLOCKED`** rather than `PASS`,
because the cumulative `$wyrd-task-review` bound to the post-R8 candidate was
never produced. Completion proceeded on the owner's instruction.

That review raised two validated defects. **Both were subsequently remediated**,
and the fixes were verified against the completed target `1e21405b7`:

- **CR1 — credential material accepted through CLI arguments — fixed.** No
  `wyrd-cli` command accepts a secret as a command-line argument. OIDC client
  secrets are read from a file or the environment
  (`crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs`, `client_secret_file`), and
  a refresh token is read only from `WYRD_REFRESH_TOKEN`
  (`crates/wyrd/wyrd-cli/src/auth/refresh.rs`). Secret material is carried in
  `secrecy::SecretString`, so it does not reach argv, shell history, or a
  derived `Debug`.
- **CR2 — delegation audit lost credential attribution — fixed.**
  `TenantGrant::Delegation` carries an audit-only `actor_credential_id`
  (`crates/wyrd/wyrd-auth/src/issuance.rs`), populated from the verified
  caller at `crates/wyrd/wyrd-auth/src/exchange_api_key.rs` and attached to the
  successful exchange event by `exchange_audit_event`. It never enters the
  delegated token. Postgres tests assert the actor credential on both the
  allowed and denied delegated audit rows.

No finding from the review remains open against the shipped tree. The
outstanding item is procedural only: the independent cumulative task review that
the workflow requires was waived, not satisfied.

## Acceptance closure

The final review's acceptance matrix recorded **PASS for every required
behavior (REQ-001 … REQ-052), every invariant, every non-goal, and verification
obligations VER-001 … VER-006**, assessed against the integrated tree. The one
`BLOCKED` row was the workflow gate itself — the absent independent cumulative
task review — not a behavioral obligation.

Verification evidence at completion: `mise run gate` green on the completed
target (exit 0, ~80 tasks, no failures), covering `fmt:check`, workspace lints,
`test:wyrd` (2031), `test:vala` (1335), `test:shared` (659), `test:skald` (410),
`test:bifrost:gate` (111), `test:identity:journey` (20), `codegen:check`,
`docs:check`, the `check:*` boundary family, and the Python, TypeScript, and
storage lanes.

## Links

- Architecture: [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md),
  [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx),
  [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md),
  [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md)
- Repository rules: [`AGENTS.md`](../../../AGENTS.md)
- Code owners: `crates/wyrd/wyrd-auth`, `crates/wyrd/wyrd-sql`,
  `crates/wyrd/wyrd-server`, `crates/wyrd/wyrd-cli`,
  `crates/shared/wyrd-runtime`, `crates/shared/wyrd-auth-issue`,
  `crates/wyrd-spec`, `sdks/wyrd-sdk-rust`, `sdks/wyrd-sdk-python`,
  `sdks/wyrd-sdk-ts`
- Tests: `crates/wyrd/wyrd-sql/tests/pg_admin_principals.rs`,
  `crates/wyrd/wyrd-sql/tests/pg_platform_identity.rs`,
  `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs`,
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs`,
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs`
