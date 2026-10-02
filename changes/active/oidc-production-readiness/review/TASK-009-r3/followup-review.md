# TASK-009 round-3 focused follow-up

## Immutable subject and scope

- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Latest remediation delta used for navigation:
  `1ddc10e21054ddc158f461e8c6d8aa862c32a067..04597909203463820b2033c12956f5fe6fcfe1f4`

This pass investigated only the three assigned discovery conflicts. It read all
round-three discovery reports and inspected commit-qualified base and candidate
source, the cumulative and remediation diffs, the task and remediation,
`AGENTS.md`, `architecture/agent-rules.md`, the maintainer and spec-development
guides, the security and deployment authorities, and the task's standards
research. `.codegraph/` is absent, so navigation used Git object reads and
repository search. The candidate commit remained unchanged.

`FIND-TASK-009-5` and `FIND-TASK-009-14` remain withdrawn by binding lead
direction. Nothing below reopens either finding or requires provider-error
redaction, an audience migration preflight, overlap column, or dual write.

## Inspected paths

- `crates/wyrd/wyrd-auth/src/platform_login.rs`
- `crates/shared/wyrd-auth-oidc/src/relying_party.rs`
- `crates/wyrd/wyrd-server/src/components/platform/identity.rs`
- `crates/wyrd/wyrd-server/src/components/admin/routes.rs`
- `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs`
- `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs`
- `crates/wyrd-spec/src/error.rs`
- `crates/wyrd-spec/src/auth/{platform_identity,human_connection}.rs`
- `crates/wyrd/wyrd-auth/src/{connections,pg_resolvers,callback}.rs`
- `crates/shared/wyrd-auth-oidc/src/{jwks,registry}.rs` at the base and candidate
- `crates/shared/wyrd-auth-verify/src/lib.rs` at the base and candidate
- `crates/wyrd/wyrd-server/src/boot/auth.rs` at the base
- `crates/wyrd/wyrd-sql/src/queries/{platform/identity,auth/human_connections}.rs`
- `crates/wyrd/wyrd-sql/src/row_types/auth/human_connections.rs`
- `crates/wyrd/wyrd-sql/migrations/{20260601000023_platform_identity,20260925000000_auth_human_connections}.sql`

## Conflict 1 — `PlatformLogin::relying_party`

### Resolution: `MNT-R3-001` is a required maintainer correction, not preference

Candidate `platform_login.rs:94-102` makes `PlatformLogin` the concrete owner of
the process-local platform `RelyingParty` and its cache. The new public
`PlatformLogin::relying_party` at lines 131-141 has exactly one caller,
`components/platform/identity.rs:235-244`, which immediately invokes
`RelyingParty::discover`. No other candidate caller uses the accessor.

The behavior is correctly routed through the existing cache, but the public
shape violates the repository's struct-centered owner rule: an orchestration
operation using an owner's dependency belongs on that owner, while a public
dependency accessor exposes every lower-level public relying-party operation to
obtain one fresh discovery. The nearby `HumanConnections::relying_party`
pattern does not justify this surface: it is `pub(crate)`, is used by the
separate tenant login and callback owners inside the same crate, and does not
escape the crate's public API. The remediation explicitly required platform
setup to use the existing boot-owned `PlatformLogin`/`RelyingParty` operation;
it did not require exposing the dependency.

The smallest correction is the one proposed by the maintainer review: remove
the accessor and expose one narrowly named fresh-discovery operation on
`PlatformLogin` that delegates to its owned relying party and returns the
provider fact setup needs. This adds no cache, option, trait, wrapper type, or
protocol behavior; it contracts the accidental public surface while preserving
the R2 cache fix. Static repository search plus the existing served platform
configuration/login journey is sufficient closure proof; a new test harness or
new behavioral test is not warranted.

## Conflict 2 — platform configure failure declaration

### Resolution: `MNT-R3-002` is confirmed

The route declaration at candidate
`components/platform/identity.rs:173-192` advertises `200`, `400`, `401`,
`403`, and `500`, but no `503`. Its `400` description includes an
"unreachable" issuer. The implementation at lines 235-243 now performs full
`openidconnect` discovery through `RelyingParty::discover`, whose
`relying_party.rs:417-435` path fetches both the discovery document and its
advertised JWKS. `admin::routes::discovery_error` at lines 765-805 maps blocked
addresses to a catalogued `400`, but maps resolution/client failures,
unreachable or undecodable discovery, unreachable or undecodable JWKS, and
issuer mismatch to `WyrdError::DiscoveryUnavailable`. The catalog at
`wyrd-spec/src/error.rs:1011-1023` fixes that error at HTTP `503` with code
`WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`.

This is therefore a reachable public-contract mismatch, not optional prose.
The adjacent established Wyrd pattern is
`components/admin/routes.rs:250-297`, whose trusted-issuer create route
documents discovery failure in rustdoc and declares the catalogued `503` in
`utoipa`. Platform login routes in the same module likewise declare provider
unavailability as `503`. The configure comment at lines 221-225 is also
imprecise after R2: full discovery is the only network *operation* on this
handler, but it performs the standard metadata and JWKS requests.

The smallest correction is to fix the existing rustdoc/comment, move
unreachable or undecodable provider data out of the `400` description, and add
the existing catalogued `503` response. No error, schema mechanism, document,
or check is needed. Runtime proof should tighten the existing unavailable and
undecodable JWKS loop in
`platform_admin_e2e.rs:1607-1648` to assert HTTP `503` and
`WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`. Contract proof should add the narrow
assertion to the existing served `pg_openapi_contract` target that the PUT
operation advertises that same `503` code. Those existing focused tests and
their owning lanes are sufficient.

## Conflict 3 — human `jwks_ttl_secs`

### Resolution: `PERSIST-TEN-R3-001` is revised but retained as `DRIFT`

The persistence review correctly identifies the candidate's inert public and
durable state, but its diagnosis should not call this a newly introduced cache
regression. The values were already inert at the base:

- At the base, tenant and platform human verification reached
  `ExternalVerifier::verify_id_token_against`, but its `JwksCache` was built
  once in `wyrd-server/src/boot/auth.rs:31-35,91-99` with a fixed 300-second
  TTL. `JwksCache::key` accepted issuer, JWKS URL, and key id, not the stored
  `TrustedIssuer::jwks_ttl` (`wyrd-auth-oidc/src/jwks.rs:238-317`).
- The base tenant callback and platform login passed `IssuerVerification` into
  that verifier; `IssuerVerification` contains no TTL. Although the row
  decoders populated `TrustedIssuer::jwks_ttl`, no human cache consumer read
  it. Thus a platform request value other than 300 and the tenant inherited
  value did not change the base human cache either.
- The candidate replaces that human verifier with `RelyingParty`, whose cache
  is likewise built once with fixed `PROVIDER_TTL = 5 minutes`
  (`relying_party.rs:49-57,343-374`). Tenant begin/callback and platform
  begin/callback never read a row TTL. The candidate therefore preserves the
  old effective runtime policy; it does not create the inertness.

The complete candidate producer/writer/consumer trace still establishes drift:

- Platform callers can set `ConfigurePlatformOidcRequest::jwks_ttl_secs`, the
  handler persists it, SQL stores it, and the view echoes it
  (`platform_identity.rs:37-78`, server identity lines 262-281, platform SQL
  lines 21-124, and the original platform table column).
- Tenant input cannot set a TTL. `HumanConnections::stage` writes the fixed
  `DEFAULT_JWKS_TTL_SECS`, candidate replacement inherits it, the table and SQL
  preserve it, and `HumanConnectionView` reports it
  (`connections.rs:235-300,786-815`; human contract lines 116-155; human SQL
  queries/row type and migration).
- Candidate `human_connection_trusted_issuer` still copies the tenant value to
  `TrustedIssuer::jwks_ttl`, but no human relying-party path consumes that
  object field. Workload trusted issuers remain different: their TTL is still
  public/durable and is part of the workload `ExternalVerifier`/`JwksCache`
  boundary, so it must not be deleted under this finding.

Neither OIDC nor `openidconnect` defines a per-connection JWKS cache-lifetime
setting. The approved standards research says `openidconnect` does no caching
and directs Wyrd to keep one small existing-Moka per-issuer cache; TASK-009
requires that cache and prohibits adding another cache setting. Spec revision
11's REQ-004 names the customer inputs and does not include a TTL. Under the
binding human direction, the two human TTL surfaces are therefore extra
Wyrd-specific options and `DRIFT`, even though they predate this task.

Deleting the human-only fields and columns is required by that standing
direction, not prohibited scope expansion and not restoration of a lost
runtime guarantee. It does change unreleased request/view shapes, but no
approved spec promises those fields and the repository has no release whose
compatibility must be retained. Restoring a per-entry TTL, retaining a
compatibility field, or adding a migration preflight/check would instead add
the nonstandard mechanisms the direction forbids. The correction must leave
the workload trusted-issuer TTL and its consumers intact. Focused contract,
codegen, platform identity SQL, tenant human-connection SQL, and existing
served configure/cache tests are sufficient proof; broad every-language or
full journey sweeps remain change-review evidence under `518026d54`.

## Proposed findings after follow-up

No additional source-local finding is proposed. The focused evidence resolves
the discovery union as follows:

- retain `MNT-R3-001` as a `DRIFT` owner/API finding;
- retain `MNT-R3-002` as an `INCORRECT` public declaration finding; and
- retain `PERSIST-TEN-R3-001` as `DRIFT`, revised to state that the option was
  already inert at the base and is not a candidate-introduced runtime
  regression.

The three corrections are independent: narrowing the platform owner surface,
aligning an existing route declaration with its reachable catalogued failure,
and deleting two nonstandard human-only TTL projections. None requires a new
product, protocol, security, concurrency, compatibility, or persistent-data
decision beyond the approved direct deletion of drift.

## Follow-up result

**RESOLVED**

All three uncertainties were resolved from approved authority and
commit-qualified source. No required reviewer, source, or evidence was
unavailable. The absence of full user-journey and every-language sweeps is not
a task-review gap under the binding verification direction.
