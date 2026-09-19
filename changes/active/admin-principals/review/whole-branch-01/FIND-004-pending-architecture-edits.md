# `FIND-admin-principals-4` — the two edits that could not be committed

Every other owner this finding names is closed and committed: the
`architecture/v1/00-foundations/` pages and the tenant-OIDC specification in
`5ea453f13`, and the admin route module docs in `a8ecda7e1`, `7039d3fe1`, and
`ad4eb9dd7`.

Two owners remain — `architecture/wyrd-design.md` and
`architecture/wyrd-security-posture.md`. Both carry uncommitted edits from the
concurrent `verified-change-contract` change in this shared worktree, and its
hunks land on the same lines this finding has to replace: it has already
rewritten `wyrd-design.md:149` and `:152` to add a `System` principal kind to
the very closed-set sentence revision 7 reopens. Committing this finding's edit
there would carry that change's work into this branch.

The replacements below are written out so they can be applied verbatim once the
concurrent change lands. Each preserves that change's `System` kind.

## `architecture/wyrd-design.md`

### 1. The `PrincipalKind` closed set (currently around line 144)

Replace:

```
    discrimination lives on `PrincipalKind`. `PrincipalKind` is closed:
    `User`, `Service { card_ref }`, `Agent { card_ref }`, `System`. Service and Agent
    are deployable, card-bound, non-human principals; their JWT projection also
    carries a `card_ref_scope` authorization set derived at mint time. User is
    the marker for human identity. System is an internal tenant machine
    principal with no Card or public credential lifecycle.
```

with:

```
    discrimination lives on `PrincipalKind`. `PrincipalKind` is closed:
    `PlatformAdmin`, `TenantAdmin`, `User`, `Service { card_ref }`,
    `Agent { card_ref }`, `System`. A `PlatformAdmin` principal has no tenant;
    every other kind has exactly one. Card binding is a property of a machine
    principal, not a precondition for being one: Service and Agent principals
    are card-bound and their JWT projection carries a `card_ref_scope`
    authorization set derived at mint time, while a tenant administrative or
    tenant-created automation principal is representable with no Card. User is
    the marker for human identity. System is an internal tenant machine
    principal with no Card or public credential lifecycle. Platform authority
    is a grant held at platform scope, not a property of a kind.
```

### 2. The principal model section (currently around line 442)

Replace:

```
Wyrd principals are UUID-backed runtime identities for `User`, `Service`,
and `Agent` kinds. `Service` and `Agent` principals are card-bound: each
carries a `card_ref` discriminated on `PrincipalKind`, and its JWT carries a
mint-time `card_ref_scope` derived from the transitive card-ref graph rooted at
that card.
```

with:

```
Wyrd principals are UUID-backed runtime identities that exist independently of
any credential: issuing, rotating, revoking, or losing a credential never
creates, destroys, or alters a principal or its role grants. `Service` and
`Agent` principals are card-bound — each carries a `card_ref` discriminated on
`PrincipalKind`, and its JWT carries a mint-time `card_ref_scope` derived from
the transitive card-ref graph rooted at that card — but the administrative and
automation principals a tenant creates for itself hold no Card at all.
```

## `architecture/wyrd-security-posture.md` — APPLIED in `4c9c81f6d`

### 3. Credential lookup (currently around line 58)

Replace:

```
- API-key lookup enters a tenant-scoped transaction before credential
  verification and returns one indistinguishable public error for every
  invalid-key condition.
```

with:

```
- A credential's non-secret prefix resolves its owning principal before
  verification — `wyrd_global_...` at platform scope, `wyrd_sk_<tenant>_...`
  within a tenant, the latter entering a tenant-scoped transaction first. Every
  invalid-credential condition performs exactly one verification and returns
  one indistinguishable public error.
- One principal may hold several simultaneously valid credentials. Revoking one
  retires that credential and advances the principal's authorization epoch; it
  leaves the principal, its grants, and its other credentials intact.
```

### 4. External federation (currently around line 97)

Replace:

```
- External federation accepts tokens only from an explicitly configured
  issuer, audience, algorithm, and claim mapping. OIDC discovery does not make
  an issuer trusted.
```

with:

```
- External federation accepts tokens only from an explicitly configured
  issuer, audience, algorithm, and claim mapping. OIDC discovery does not make
  an issuer trusted.
- A connection is selected by the login entry point, never by a token, header,
  hostname, or post-login chooser. A tenant entry resolves only that tenant's
  connection; the deployment's single platform-scope connection resolves only
  platform principals pre-registered against an expected issuer and claim and
  pinned on `(issuer, subject)` at first login. An unknown platform subject is
  denied and never provisioned just in time, and the platform connection's
  absence or outage never blocks administration through the global
  administrative credential.
```
