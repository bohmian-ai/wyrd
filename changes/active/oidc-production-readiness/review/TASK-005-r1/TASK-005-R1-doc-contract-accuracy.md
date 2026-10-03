---
id: TASK-005-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-005, REQ-018, REQ-021, INV-003, AC-007, AC-009]
depends_on: []
parent_task: TASK-005
remediates: [FIND-TASK-005-1, FIND-TASK-005-2, FIND-TASK-005-3, FIND-TASK-005-4, FIND-TASK-005-5, FIND-TASK-005-6]
---

# Reconcile OAuth, issuer, session, and activation documentation

## Immutable review inputs

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-005-r1/`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Reviewed candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Validated findings: `FIND-TASK-005-1` through `FIND-TASK-005-6`

## Outcome

Operators, agents, and client authors receive one accurate account of the
already-shipped standard OAuth/OIDC system: its standard endpoint-specific
wire contracts, workload-only trusted issuers, separate platform and tenant
token routes, BFF-owned encrypted cookie, and test-stamp-based connection
activation. This remediation changes documentation and source documentation;
it does not change runtime behavior.

## Issue diagnoses and required corrections

### FIND-TASK-005-1 — OAuth error identity and logging

The API-doc generator, generated OpenAPI page, agent error guide, generated
error catalog text, security posture, SSO guide, and OAuth module rustdoc make
blanket claims that every error is Problem Details or every OAuth refusal has
a Wyrd catalog code in logs. The shipped `OAuthForm` and `OAuthClients`
boundaries directly construct RFC OAuth errors for malformed form input and
client identification; only errors converted from `WyrdError` have a Wyrd
catalog code and the corresponding structured log field. An agent can branch
on a nonexistent `code`, and an operator can search for a correlator that was
never emitted.

Correct the documentation at its existing source owners. Scope Problem Details
and Wyrd catalog-code instructions to non-OAuth operations, direct the four
OAuth form endpoints to their existing RFC 6749 `error` and optional
`error_description` response, and qualify logging to match only the existing
`WyrdError` conversion path. Regenerate pages from the generator; do not
hand-edit generated output as an independent authority. Do not add another
response field, error catalog, logger, wrapper, or compatibility mechanism.

### FIND-TASK-005-2 — trusted workload issuer scope and sealing

The cloud-identity field reference advertises Human/default-Human trusted
issuers and human-only mapping behavior even though both production entry
paths refuse Human issuers in favor of tenant OIDC connections. The
configuration guide also says workload federation carries no client secret
despite the shipped `secret_basic` and `secret_post` variants, whose secrets
require the existing sealing key. An operator can therefore configure an
issuer that is deterministically refused or omit the key required for a
supported secret-bearing issuer.

Keep the current tenant OIDC connection and trusted workload issuer owners.
Describe the trusted-issuer surface as workload-only, route human federation
to the tenant connection guide, and stop presenting Human-only fields as
usable on this path. Distinguish public/private-key workload issuers, which
carry no shared client secret, from `secret_basic` and `secret_post` workload
issuers, which require the existing sealing key. Do not change the enum,
schema, compatibility behavior, persistence, or issuer implementation.

### FIND-TASK-005-3 — platform and tenant token routes

The operator authentication guide says every grant ends at `/auth/token`, even
though it introduces both administration planes and the platform RFC 8693
exchange is mounted only at `/auth/platform/token`. An operator can send a
platform credential to the tenant route and receive a refusal.

Qualify the current grant table and `/auth/token` statement as tenant-plane
issuance, and name `/auth/platform/token` as the existing platform
credential-exchange route. Preserve the separate route owners; add no alias,
fallback, or shared route abstraction.

### FIND-TASK-005-4 — endpoint-specific OAuth success responses

The security posture, SSO guide, and OAuth module rustdoc collapse token,
device-authorization, and revocation success into RFC 6749 section 5.1. The
shipped token handlers return the section 5.1 token response, device
authorization returns the RFC 8628 section 3.2 response, and revocation returns
an empty `200` under RFC 7009 section 2.2. A conventional client can otherwise
choose the wrong decoder.

Describe those three existing standard success contracts separately. Preserve
form encoding, RFC OAuth error JSON, `Cache-Control: no-store`, and the current
handlers. Do not create a common success envelope.

### FIND-TASK-005-5 — BFF cookie custody

The operator authentication page categorically says there is no session
cookie, while the shipped BFF stores the refresh token or recovery API key in
an encrypted Secure, HttpOnly, SameSite=Lax cookie and presents a Wyrd bearer
token to the API server. This gives operators the wrong credential-custody and
client-secret-rotation model.

State the existing boundary: the Wyrd API server has no browser-session store
or ambient request identity, while the web-app BFF owns the encrypted HttpOnly
cookie. Preserve best-effort RFC 7009 logout, the absence of a server-side
browser-session table, and the bounded validity of already-issued access
tokens. Add no server-side session mechanism.

### FIND-TASK-005-6 — activation and provider liveness

The SSO lifecycle guide says a provider outage makes activation fail with
`WYRD_AUTH_409_CONNECTION_NOT_TESTED`. The shipped activation path performs no
provider IO: it verifies the exact candidate's persisted 15-minute test stamp
and the recovery authority, then promotes the candidate. An outage after a
successful test does not invalidate that current stamp. The existing wording
can lead an operator to retire a working connection while believing activation
freshly proved provider availability.

State that a missing, stale, or failed test stamp blocks activation; a provider
outage blocks a new test and later login but is not re-probed during the
15-minute activation window. Preserve the exact-revision stamp and recovery-key
guarantees. Do not add an activation probe, retry, or new availability
mechanism.

## Constraints and preserved behavior

- Use standard OAuth 2.0/OIDC terminology and the current well-vetted library
  owners. Do not invent a Wyrd-specific protocol or error identity.
- Preserve RFC 8693 API-key exchange with
  `urn:wyrd:oauth:token-type:api_key`.
- Preserve ingress-owned rate limiting for `POST /auth/device`.
- Preserve best-effort RFC 7009 logout revocation.
- Preserve origin-normalized client base URLs and saved-login selection.
- Preserve self-contained access-token validity until expiry after logout,
  revocation, or connection replacement.
- Preserve the current platform/tenant route split, provider test stamp,
  sealing-key behavior, encrypted BFF cookie, and generated-artifact ownership.
- Do not add runtime behavior, tests, dependencies, endpoints, aliases,
  storage, configuration, retries, logging mechanisms, or compatibility paths.
- Do not reopen placement, naming, structure, or wording preferences; only
  correct the validated false or misleading claims.

## Non-goals

- Changing OAuth/OIDC implementation, grant support, or client registration.
- Adding provider-specific code or a certified-provider matrix.
- Adding instant access-token revocation, server-side browser sessions, or IdP
  logout.
- Changing Human trusted-issuer compatibility representation or migration
  behavior.
- Re-running user-journey suites or a broad repository aggregate.

## Acceptance criteria

1. `FIND-TASK-005-1`: general API and agent guidance clearly exempts the four
   OAuth form endpoints from Problem Details/Wyrd response-code handling, and
   logging claims match the existing split between protocol-native errors and
   `WyrdError` conversions. Generated pages match their source generators.
2. `FIND-TASK-005-2`: trusted-issuer docs describe only the accepted workload
   path, route human federation to tenant OIDC connections, and accurately
   distinguish secretless and secret-bearing workload issuer sealing needs.
3. `FIND-TASK-005-3`: operator docs distinguish tenant `/auth/token` from
   platform `/auth/platform/token` without adding an alias or fallback.
4. `FIND-TASK-005-4`: token, device-authorization, and revocation success
   responses are documented under their actual RFC-owned shapes.
5. `FIND-TASK-005-5`: operator docs distinguish the stateless Wyrd API server
   from the BFF's encrypted HttpOnly cookie and preserve the approved logout
   and access-token lifetime semantics.
6. `FIND-TASK-005-6`: activation guidance describes the exact-revision test
   stamp and 15-minute window without promising a provider re-probe.
7. No closed decision, non-goal, unrelated documentation, or executable
   behavior changes.

## Focused proof and verification

Before running lanes, statically compare each corrected statement with its
existing owner named in `findings-validation.md`: OAuth extractors and handler
return sites, both trusted-issuer entry points and sealing owner, tenant and
platform routers, `BrowserSessions`, and the connection test/activation path.

Run only the lanes covering the remediation write set:

```bash
mise run docs:check
mise run codegen:check
mise run fmt
mise run lints
git diff --check
```

`codegen:check` is required because API documentation generators and generated
pages are corrected. Rust formatting and lints are required because the OAuth
module rustdoc is corrected. Do not run full journey suites, language test
suites, or repository aggregates; this remediation changes documentation and
source documentation only.

## Completion evidence

Record the corrected source locations, the static owner comparison for each
finding, and the exit result of every focused command above. Route the
completed remediation directly to `$wyrd-implement`, then reassess the full
original base-to-new-candidate range with `$wyrd-task-review`.
