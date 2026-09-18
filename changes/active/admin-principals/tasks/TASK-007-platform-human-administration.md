---
task: TASK-007
title: Platform human administration and human identity
spec: SPEC-admin-principals
spec_revision: 6
obligations: [REQ-034, REQ-035, REQ-041, REQ-042, REQ-043, REQ-044, REQ-045, REQ-046, REQ-040, INV-004a, INV-004b, AC-011, AC-015, AC-016, AC-017]
depends_on: [TASK-002, TASK-003, TASK-005]
---

## Objective

Humans become principals in the same model. A verified federated identity
resolves through its durable `(issuer, subject)` to a principal and mints a
token producing the same authenticated context as a machine credential. Platform
authority becomes a grant that both the bootstrap global principal and human
platform principals can hold, and a deployment may configure one platform-scope
OIDC connection so platform administrators sign in individually and are
individually audited.

## Constraints

- Federated identity is unique per tenant on `(tenant, issuer, subject)`,
  preserving the existing `wyrd.auth_user_identities` key. One human may hold
  independent principals in multiple tenants.
- Human platform principals live at platform scope with no tenant. A human who
  is also a tenant user holds a separate, independent tenant-scoped principal;
  the two are never merged.
- Platform principals are pre-registered against an expected issuer and a
  matching claim. First successful login pins `(issuer, subject)` durably;
  later logins match on that alone. An unknown subject at the platform plane is
  denied — no just-in-time provisioning of platform principals.
- The platform-scope OIDC connection is deployment-owned, optional, and distinct
  from every tenant-owned connection. Its absence, misconfiguration, or provider
  outage never prevents platform administration through the global credential.
- The entry point selects the connection, the connection selects the principal,
  the principal carries the scope. No scope is inferred from a token, header,
  hostname, or post-login chooser, and a platform session carries platform scope
  only.
- Only a principal already holding platform authority may create a platform
  principal or grant platform authority.
- Platform authority never confers tenant data access.
- Reuse the existing OIDC verification mechanics — discovery, JWKS, PKCE, nonce,
  state, SSRF screening, secret protection. Do not define a second verification
  implementation.
- Non-goal: tenant OIDC configuration, claim mapping, group-to-role mapping, and
  tenant login routing — owned by `SPEC-tenant-oidc-federation`.
- Non-goal: a platform admin UI. This delivers the contract a UI would project.

## Relevant Surface

- `crates/shared/wyrd-auth-oidc/` — provider, trusted issuer, claims, registry.
- `crates/wyrd/wyrd-auth/src/login.rs`, `callback.rs`, `pg_resolvers.rs`.
- `crates/wyrd/wyrd-server/src/components/auth/routes.rs` — login and callback
  entries.
- `crates/wyrd/wyrd-sql/` — `wyrd.auth_user_identities`, and the platform
  principal, credential, and grant store from `TASK-001`.
- `crates/wyrd/wyrd-testing/src/oidc_fixture.rs`,
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs`, and the
  `test:identity:journey` lane (Keycloak and Dex, gated by `WYRD_IDENTITY_E2E`).
- `changes/active/tenant-oidc-federation/spec.md` — amend its assumption that
  every connection is tenant-owned.
- `docs/src/content/docs/` — platform administration and SaaS operator model.

## Approach

1. Make platform authority a grant resolved from the platform store, held by
   both the bootstrap principal and human platform principals.
2. Add the platform-scope OIDC connection: configure, replace, remove, on the
   platform plane, with secrets protected and redacted.
3. Add platform principal pre-registration, listing, and revocation, authorized
   only from the platform plane.
4. Add the platform login entry that resolves only the platform connection, and
   implement first-login identity pinning with unknown-subject denial.
5. Confirm the tenant human path resolves per-tenant identity and produces the
   same authenticated context shape, adding coverage where it does not.
6. Amend the federation specification and document the platform administration
   and SaaS operator model.

## Acceptance Criteria

- The global administrative credential configures the platform OIDC connection
  and pre-registers the first human platform administrator.
- That administrator's first login succeeds and pins `(issuer, subject)`; a
  later login with a changed matching claim but the same pinned subject still
  succeeds, and a changed subject does not.
- An authenticated but unregistered subject is denied at the platform plane, and
  no principal is created.
- With the platform connection absent, removed, or its provider failing, the
  global credential still administers the platform.
- A platform session carries platform scope only. The same human authenticating
  through a tenant connection receives an independent tenant-scoped principal
  and session, and neither session reaches the other's plane.
- No tenant-plane operation — tenant principal creation, role grant, or OIDC
  group mapping — creates a platform principal or confers platform authority.
- A human tenant administrator coexists with the tenant administrative
  principal; neither displaces nor requires removal of the other.
- A federated human and a machine credential produce the same authenticated
  context shape, and no downstream handler branches on the entry path.
- Provider secrets appear in no read, list, log, trace, error, or audit payload.

## Verification

Scope is `VER-001` through `VER-006`.

```bash
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-auth-oidc -p wyrd-auth -p wyrd-server --all-targets
mise run codegen:check
mise run docs:check
```

The primary proof is a real-provider identity journey extending the existing
`test:identity:journey` lane (Keycloak and Dex under `WYRD_IDENTITY_E2E`) to
cover platform-connection configuration, pre-registration, first-login pinning,
unknown-subject denial, scope separation across a platform and a tenant session
for the same human, and credential administration while the provider is
unavailable. Escalation-resistance coverage runs as Postgres-backed integration
proof. Run the exact nextest expressions for the tests you add.
