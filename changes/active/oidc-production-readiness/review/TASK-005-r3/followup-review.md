# TASK-005 R3 focused follow-up review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Remediation inputs:
  `review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md` and
  `review/TASK-005-r2/TASK-005-R2-doc-contract-closure.md`

I read all six R3 discovery reports and investigated only the three disputed
documentation-contract claims assigned to this follow-up. The repository has
no `.codegraph/` directory, so navigation used the cumulative diff, `rg`, and
direct reads of the complete relevant owners and sibling consumers. The five
caller-locked decisions were not reopened.

## Uncertainty 1: registered-client and bearer-authentication generalizations

### Source path inspected

- Public and source documentation:
  - `crates/wyrd/wyrd-server/src/auth/oauth.rs:1-17,271-362`
  - `docs/src/content/docs/self-hosting/sso-and-oidc.svx:130-154`
  - `docs/src/content/docs/self-hosting/authentication.svx:9-33`
- Token and client dispatch:
  - `crates/wyrd/wyrd-server/src/components/auth/routes.rs:43-189`
  - `crates/wyrd/wyrd-server/src/auth/cli_login.rs:1-90,265-307`
  - `crates/wyrd/wyrd-server/src/components/platform/routes.rs:42-153`
- Anonymous routing and public contract declarations:
  - `crates/wyrd/wyrd-server/src/http/router.rs:39-80,97-131`
  - `crates/wyrd/wyrd-server/src/http/openapi.rs:15-75`
  - `crates/wyrd/wyrd-server/src/auth/authorize.rs:55-188`
  - `crates/wyrd/wyrd-server/src/components/platform/identity.rs:84-93,679-760`

### Resolution

**The disputed claim is materially false and misleading; this is not a
precision-only preference.**

`OAuthClients::identify` deliberately returns `Ok(None)` when a request carries
neither Basic client authentication nor `client_id`
(`auth/oauth.rs:302-346`). `token` calls that parser once, but consumes the
derived `required` error only for `authorization_code`, `refresh_token`, and
`device_code` (`components/auth/routes.rs:119-153`). API-key token exchange,
delegated token exchange, and `jwt-bearer` proceed without a registered client
(`routes.rs:154-186`). In contrast, `device_authorization` and `revoke` call
`OAuthClients::require` and therefore are client-bound
(`cli_login.rs:76-90,295-307`). The separate platform exchange accepts no OAuth
client at all and authenticates its presented API-key subject directly
(`components/platform/routes.rs:93-143`).

The statements that the tenant token endpoint "identify[ies] one of" the two
registered clients (`sso-and-oidc.svx:152`) and that the tenant token endpoint
identifies a registered client (`auth/oauth.rs:6-8`) are therefore not saved by
the fact that the handler invokes a function named `identify`. In context they
positively contrast the tenant endpoint with the clientless platform endpoint,
and a conventional machine-grant caller is allowed to identify no client. A
reader can reasonably infer a wire requirement that does not exist and submit a
borrowed `wyrd-cli` identity or Basic credential for an API-key, delegation, or
workload exchange. That contradicts REQ-021 and AC-009's shipped standard wire
contract and the task's required separate workload path.

The same source trace confirms the security reviewer's bearer-authentication
half. `self-hosting/authentication.svx:11,19` says every API request, and every
request, presents a signed Wyrd JWT. The auth router, platform token router, and
platform-login router are merged outside the `/v1` bearer middleware
(`http/router.rs:47-80,103-112`). Authorization, metadata, device authorization,
device browser interactions, callback, tenant/platform token exchange,
revocation, platform login/callback, health, and served OpenAPI are reachable
without a pre-existing Wyrd access token. Their OpenAPI operations clear the
document-wide bearer requirement with `security(())`; the client-bound form
endpoints optionally declare OAuth Basic client authentication, which is not
caller bearer authentication (`http/openapi.rs:23-36,44-74`). The concept page
already uses the accurate narrower phrase "Every authenticated request"
(`concepts/identity-and-auth.svx:11-16`).

This is operationally material: the self-hosting page is an operator trust-
boundary guide, and its absolute statement can lead an ingress operator to
require a bearer token on bootstrap/login/metadata routes, making standard
OAuth/OIDC login and machine exchange unreachable. The bounded correction is
documentation-only: require a registered client for the three human token
grants, device authorization, and revocation; permit no identified client for
the three machine grants; and scope the signed-token statement to protected or
authenticated resource requests. Preserve all handlers and add no client,
middleware exception, alias, or compatibility path.

### Proposed finding disposition

- `BHV-R3-001` / registered-client portion of `SEC-R3-001`: **SUPPORTED**.
- Bearer-bootstrap portion of `SEC-R3-001`: **SUPPORTED** and belongs in the
  same public authentication-boundary correction sweep, with its own cited
  locations and consequence.

## Uncertainty 2: cross-plane glossary assigns tenant semantics to platform users and tokens

### Source path inspected

- Disputed and sibling documentation:
  - `docs/src/content/docs/concepts/identity-and-auth.svx:9-85,87-127`
  - `docs/src/content/docs/concepts/authentication.svx:9-51,53-84`
  - `docs/src/content/docs/self-hosting/authentication.svx:13-23`
  - `architecture/wyrd-security-posture.md:45-65,145-186`
- Runtime and token contracts:
  - `crates/shared/wyrd-runtime/src/principal.rs:454-527`
  - `crates/shared/wyrd-auth-verify/src/lib.rs:610-648`
  - `crates/wyrd/wyrd-auth/src/platform_login.rs:1-18,143-181,240-346`
  - `crates/wyrd/wyrd-auth/src/platform_sessions.rs:1-11,42-68,187-310,313-390`
  - `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:1-19,37-64,126-202`
  - `crates/wyrd/wyrd-server/src/components/platform/identity.rs:679-760`
  - `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:1-57`
  - `crates/wyrd/wyrd-sql/tests/pg_platform_identity.rs:100-139`

### Resolution

**The disputed glossary is materially false about the platform plane; this is
not a request for additional completeness.**

The page explicitly introduces both planes and correctly says platform
principals hold no tenant (`identity-and-auth.svx:18-21`), then its glossary
says every principal and token belongs to exactly one tenant, every `User` is
authenticated through a tenant provider, and every access token carries
tenant and role claims (`identity-and-auth.svx:38-45`). Those statements cannot
all be true.

Production represents a platform human as `PlatformPrincipal` with
`PrincipalKindTag::User`; the type structurally has no tenant
(`wyrd-runtime/principal.rs:454-483`). Platform OIDC uses the deployment-wide
platform connection, resolves or pins a pre-registered identity, and issues a
platform session (`platform_login.rs:240-346`, `platform_sessions.rs:187-260`).
The stored platform registration is explicitly a `User`
(`pg_platform_identity.rs:119-139`). Its `PlatformAccessTokenClaims` has no
tenant, roles, permissions snapshot, or delegation chain
(`wyrd-auth-verify/lib.rs:610-645`); the platform extractor verifies that token,
confirms current session/principal state, and re-reads the current platform
grant before constructing a tenantless `PlatformPrincipal`
(`platform_extractor.rs:126-202`). The sibling Authentication concept already
states that `User` exists in either plane (`concepts/authentication.svx:23-39`).

Because this page is the advertised end-to-end cross-plane identity map, the
contradiction obscures INV-003's load-bearing plane separation: a platform
consumer can expect nonexistent tenant/role claims, while an operator can
model a federated platform administrator as a tenant identity. The smallest
correction is to qualify the glossary itself: tenant principals and tokens own
tenant/role data; platform principals, including pre-registered human `User`s,
are tenantless and their access-only tokens carry the platform scope and
principal/session anchors. Preserve the separate connections, stores, routes,
and current-grant revalidation.

### Proposed finding disposition

- `BHV-R3-002`: **SUPPORTED** as a distinct material public-contract finding.

## Uncertainty 3: implicit-tenant guidance generalizes host routing and single tenancy

### Source path inspected

- Disputed and sibling documentation/authority:
  - `docs/src/content/docs/self-hosting/configuration.svx:72-100`
  - `docs/src/content/docs/self-hosting/index.svx:15-31`
  - `docs/src/content/docs/concepts/identity-and-auth.svx:55-85`
  - `docs/src/content/docs/concepts/authentication.svx:73-84,236-248`
  - `architecture/references/doctrine/architecture-constraints.md:139-151`
  - `architecture/operations/deployment-and-release.md:6-24`
  - `architecture/wyrd-security-posture.md:240-255`
- Normal tenant request verification:
  - `crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:21-149`
  - `crates/wyrd/wyrd-server/src/http/router.rs:49-80`
- Workload pre-issuance selection:
  - `crates/wyrd/wyrd-server/src/auth/mod.rs:15-43`
  - `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:15-98`
- Boot ownership and deployment topology:
  - `crates/wyrd/wyrd-server/src/boot/issuer.rs:1-23,61-111`
  - `crates/wyrd/wyrd-server/src/boot/mod.rs:1421-1453,2062-2097`
  - `crates/wyrd/wyrd-server/src/config.rs:1726-1732,2117-2129`
  - `crates/wyrd/wyrd-sql/src/postgres.rs:180-228`

### Resolution

**The disputed paragraph materially misstates both the tenant-authority
boundary and the supported self-hosted topology.**

`configuration.svx:98` says every hosted request names its tenant through
`Host`. Normal protected tenant requests do not: the default-deny `/v1` edge
extracts `X-Wyrd-Access-Token`, decodes its tenant claim only to select the
expected verifier, and then cryptographically verifies the token for that
tenant (`token_extract.rs:100-149`, `http/router.rs:49-80`). The `Host` header
has one narrower pre-issuance role. For `jwt-bearer`, the leftmost host label,
or the form's `tenant` fallback, selects a candidate tenant; only after the
external assertion and server-owned workload binding verify does Wyrd mint
tenant authority (`auth/mod.rs:25-37`, `jwt_bearer.rs:15-72`). Tenant human
login uses its explicit tenant entry parameter and server-bound state, not a
host-derived effective identity. This distinction is required by INV-001,
which expressly forbids hosts from selecting effective tenant identity.

The same paragraph says a self-hosted deployment serves one tenant. Current
deployment authority supports one or more operator-selected tenants in a
self-hosted deployment (`architecture-constraints.md:139-147`,
`deployment-and-release.md:6-15`), and the sibling self-hosting overview says a
self-hosted deployment *usually* runs one while retaining multi-tenant
isolation (`self-hosting/index.svx:25-27`). The configured implicit slug does
not technically restrict the server to one tenant; it selects the one owner of
the TOML-seeded trusted issuers and workload bindings because boot has no
request context (`boot/issuer.rs:1-17,61-81`, `boot/mod.rs:2062-2097`). Runtime
tenant slug resolution is the same directory lookup, not evidence that every
request is host-authoritative (`wyrd-sql/postgres.rs:180-205`). Some internal
configuration/boot comments use "explicitly single-tenant" as an operational
profile distinction for key-source validation, but no request gate restricts
all serving to that slug; they do not override the architecture's supported
one-or-more-tenant self-hosted topology.

The current wording can cause an operator to treat a spoofable routing header
as ordinary request authority or misunderstand `WYRD_SERVER_TENANT_SLUG` as a
deployment-wide tenant lock rather than the owner selector for boot-seeded
workload federation. That is a security and deployment contract consequence
under REQ-018, INV-001, AC-003, and AC-009, not a phrasing preference. The
bounded correction belongs in the existing implicit-tenant paragraph: explain
boot's lack of request context; state that authenticated requests derive tenant
authority from the verified Wyrd token; describe `Host`/form tenant only as an
untrusted candidate selector for workload JWT-bearer issuance; and describe
self-hosting as supporting one or more tenants. Do not add runtime routing,
headers, or configuration.

### Proposed finding disposition

- `INV-R3-001`: **SUPPORTED** as a distinct material public tenancy-contract
  finding.

## New proposals and follow-up result

No defect outside the three assigned uncertainties was investigated or added.
The bearer-bootstrap locations in `self-hosting/authentication.svx` are retained
as the already raised security reviewer's sibling evidence, not a new unrelated
proposal. The supported proposal union is therefore:

1. grant-specific registered-client requirements plus the protected-resource
   scope of bearer authentication (`BHV-R3-001` / `SEC-R3-001`);
2. tenantless platform `User` and token semantics (`BHV-R3-002`); and
3. token-derived tenant authority, narrow workload-host selection, and the
   one-or-more-tenant self-hosted topology (`INV-R3-001`).

Each is reachable, task-owned documentation accuracy under REQ-018 or REQ-021
and AC-009, has an observable operator/client/security consequence, and closes
through existing documentation owners without runtime behavior or invented
OAuth/OIDC machinery. No source conflict remains for independent validation.

## Verification notes

- This follow-up was source validation only. I ran no tests, broad aggregate,
  journey suite, live-provider flow, browser suite, or language suite.
- The already recorded narrow documentation/codegen/format/lint evidence does
  not prove semantic prose agreement; the producer-to-consumer traces above are
  the relevant evidence for these disputes.
- The candidate remained
  `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21` through report preparation.

## Overall result

**RESOLVED**

All three discovery conflicts are resolved from the approved authority and
current source. The disputed passages are materially false or misleading about
shipped authentication, security-plane, tenancy, or deployment behavior; none
is merely a placement, naming, structure, wording, or optional-precision
preference.
