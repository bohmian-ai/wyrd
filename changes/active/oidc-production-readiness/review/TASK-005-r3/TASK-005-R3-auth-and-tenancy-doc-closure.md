---
id: TASK-005-R3
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-018, REQ-021, INV-001, INV-003, AC-003, AC-009]
depends_on: []
parent_task: TASK-005
remediates: [FIND-TASK-005-3, FIND-TASK-005-9, FIND-TASK-005-10, FIND-TASK-005-11]
---

# Close grant, bearer, platform, and tenant documentation contracts

## Immutable review inputs

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-005-r3/`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Reviewed candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Validated findings: `FIND-TASK-005-3`, `FIND-TASK-005-9`,
  `FIND-TASK-005-10`, and `FIND-TASK-005-11`

## Outcome

The source rustdoc and operator/concept documentation describe the already
shipped standard authentication and tenancy boundaries exactly: registered
OAuth clients are grant-specific, bearer tokens protect authenticated resource
requests rather than auth bootstrap routes, platform human sessions are
tenantless, and Host is only an untrusted workload-exchange routing hint while
self-hosting may serve one or more tenants. This remediation changes
documentation and source documentation only.

## Issue diagnoses and required corrections

### FIND-TASK-005-3 — tenant token documentation requires a client for clientless machine grants

The OAuth module rustdoc and SSO endpoint guide say the tenant token endpoint
identifies one of Wyrd's registered OAuth clients. The handler does consult
`OAuthClients::identify`, but that operation intentionally returns `None` when
no client is supplied. Only authorization-code, refresh, and device-code grants
require the result. RFC 8693 API-key exchange, delegated exchange, and RFC 7523
JWT-bearer proceed with no Wyrd OAuth client. Device authorization and
revocation separately require a client, while `/auth/platform/token` remains a
clientless direct platform API-key exchange.

The current wording can cause machine integrators to borrow `wyrd-cli`, invent
Basic authentication, or otherwise add a client contract the shipped standard
grant does not have. Correct the two existing owners to state the grant-specific
rule. Reuse the complete `OAuthClients::{identify,require}` and token dispatch
as the authority. Preserve the registered `wyrd-ui`/`wyrd-cli` human flows,
device/revocation client binding, clientless machine grants, and separate
platform route. Add no client, alias, fallback, route, or compatibility path.

### FIND-TASK-005-9 — the operator guide says public/bootstrap requests require a bearer token

The self-hosting authentication guide says every API request, and every
request, presents a signed Wyrd JWT. Production bearer-protects `/v1` and MCP,
but composes authorization, callback, device, metadata, token/revocation,
platform login/token, health, and OpenAPI routes outside those bearer layers.
Those standard routes establish credentials or are intentionally public;
OAuth client Basic authentication is not caller bearer authentication.

An ingress operator following the absolute statement can require a Wyrd bearer
on the routes that mint or establish it, making standard OAuth/OIDC login,
metadata discovery, and machine exchange unreachable. Scope both statements to
protected or authenticated resource/API requests. Preserve the current router,
default-deny layers, handler security declarations, and standard bootstrap
inputs. Add no middleware exception, token, route, or alternative mechanism.

### FIND-TASK-005-10 — the cross-plane glossary gives platform identities tenant-only semantics

The identity-and-auth page introduces both planes and correctly says platform
principals hold no tenant, but its glossary then says every principal/token is
tenant-owned, every `User` authenticates through tenant OIDC, and every access
token carries tenant and role claims. A platform human is a pre-registered
tenantless `PlatformPrincipal` with kind `User`, authenticated through the
deployment-wide platform connection. Its access-only token has no tenant,
roles, permission snapshot, or delegation; the platform extractor verifies the
session and re-reads current grants.

The contradiction can make a reader model a platform administrator as a tenant
identity or expect claims that cannot exist. Qualify the glossary at its
current owner: tenant principals/tokens carry tenant and role/permission data;
platform principals, including human `User`s, are tenantless and their
access-only sessions carry platform scope and principal/session anchors while
current grants are re-read. Preserve separate stores, connections, routes,
session validation, and the absence of implicit cross-plane authority. Add no
identity kind or token field.

### FIND-TASK-005-11 — implicit-tenant guidance makes Host tenant authority and self-hosting single-tenant

The self-hosting configuration guide says every hosted request names its tenant
through `Host` and that a self-hosted deployment serves one tenant. Ordinary
protected tenant requests instead obtain candidate tenant context from the
presented Wyrd token and then verify that token cryptographically; they do not
read `Host`. Only workload JWT-bearer issuance uses the leftmost host label, or
the form `tenant` fallback, as an untrusted candidate before the external
assertion and server-owned binding verify. Boot has no request context and uses
`WYRD_SERVER_TENANT_SLUG` only to identify the tenant that owns configured
workload issuers and bindings. The supported self-hosted topology allows one or
more operator-selected tenants.

The current paragraph can teach operators to trust a spoofable routing header
or mistake the implicit slug for a deployment-wide tenant lock. Correct that
paragraph at its existing owner: explain boot's need for an explicit owner;
state that protected request authority comes from the verified Wyrd token;
describe Host/form tenant only as a pre-issuance workload candidate; and state
that self-hosting supports one or more tenants. Preserve all existing routing,
configuration, verification, and federation behavior. Add no header, router,
tenant lock, or compatibility surface.

## Constraints and preserved behavior

- Use standard OAuth 2.0/OIDC terminology and existing vetted libraries.
- Preserve RFC 8693 API-key exchange with
  `urn:wyrd:oauth:token-type:api_key`.
- Preserve ingress-owned rate limiting for `POST /auth/device`.
- Preserve best-effort RFC 7009 logout revocation.
- Preserve origin-normalized client base URLs and saved-login selection.
- Preserve self-contained access-token validity until expiry after logout.
- Preserve the current token grant dispatch, OAuth client requirements,
  tenant/platform routers, platform access-only sessions, bearer middleware,
  OpenAPI security declarations, tenant verification, workload binding, boot
  seeding, and supported deployment topology.
- Change no runtime behavior, public API, dependency, endpoint, token shape,
  storage, configuration, routing, retry, or compatibility behavior.
- Do not reopen placement, naming, structure, phrasing, optional completeness,
  precision-only preferences, or the prior review-artifact EOF whitespace.

## Non-goals

- Registering a Wyrd OAuth client for API-key, delegation, JWT-bearer, or
  platform exchange.
- Adding bearer middleware exceptions or moving public/bootstrap routes.
- Adding tenant or role claims to platform access tokens.
- Making `Host` tenant authority or adding a self-hosted single-tenant gate.
- Redesigning tenant selection, login, exchange, federation, or deployment
  topology.
- Running journey, language, live-provider, browser, or repository aggregate
  suites.

## Acceptance criteria

1. `FIND-TASK-005-3`: OAuth rustdoc and the SSO endpoint guide require a
   registered client only for authorization-code, refresh, and device-code
   token grants; they preserve clientless RFC 8693/RFC 7523 machine grants,
   client-bound device authorization/revocation, and clientless platform
   exchange.
2. `FIND-TASK-005-9`: the self-hosting authentication guide scopes signed
   Wyrd bearer tokens to protected/authenticated resource requests and does not
   claim that public metadata or authentication/bootstrap routes require one.
3. `FIND-TASK-005-10`: the cross-plane glossary distinguishes tenant
   principals/tokens from tenantless platform human `User`s and access-only
   sessions without inventing platform tenant/role claims.
4. `FIND-TASK-005-11`: the implicit-tenant section describes verified-token
   authority, the narrow untrusted Host/form selector for workload JWT-bearer,
   boot's explicit workload-federation owner, and support for one or more
   self-hosted tenants.
5. No runtime behavior, generated contract, or locked decision changes.

## Focused proof and verification

Before running lanes, statically compare each correction with its existing
owner:

- the complete `OAuthClients::identify`/`require`, tenant `token`, device
  authorization, revocation, and platform token bodies;
- `build_router`, the OpenAPI security addon, auth/platform routers, and the
  affected handlers' security declarations;
- `PlatformPrincipal`, `PlatformAccessTokenClaims`, platform login completion,
  and the platform request extractor; and
- tenant access-token extraction/verification, workload tenant resolution,
  boot implicit-tenant resolution, and the self-hosted topology authority.

Run only the lanes covering the documentation and Rust module-rustdoc write
set:

```bash
mise run docs:check
mise run fmt
mise run lints
```

Run `mise run codegen:check` only if the implementation changes a generated
source owner or generated artifact; the prescribed correction does not require
either. Inspect the remediation diff with `git diff --check`. Do not run or
require full journeys, language suites, live-provider tests, browser suites, or
repository aggregates.

## Completion evidence

Record the corrected locations, the static owner comparison for every finding,
and the exit result of each required focused command. Route the completed
remediation directly to `$wyrd-implement`, then reassess the complete original
base-to-new-candidate range with `$wyrd-task-review`.
