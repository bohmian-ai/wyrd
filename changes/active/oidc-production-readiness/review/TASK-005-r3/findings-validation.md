# TASK-005 R3 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Remediation inputs:
  `review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md` and
  `review/TASK-005-r2/TASK-005-R2-doc-contract-closure.md`

The candidate remained at the stated commit throughout validation. The
repository has no `.codegraph/` directory, so I used the complete cumulative
diff, the latest remediation diff, `rg`, and direct reads of the complete
producers, callers, sibling consumers, and existing helpers named below. I read
all seven R3 discovery and follow-up reports. The caller's standing direction
controls materiality: only materially false or misleading shipped-behavior
documentation, or omitted task-required content, is retained. The five locked
OAuth/logout/base-URL/token-lifetime decisions remain closed.

## Proposal validation

| Discovery proposal | Disposition | Independent source validation |
|---|---|---|
| `BHV-R3-001`; registered-client portion of `SEC-R3-001` | **REVISED** → `FIND-TASK-005-3` | `OAuthClients::identify` deliberately returns `Ok(None)` when neither Basic authentication nor `client_id` is present (`auth/oauth.rs:291-347`). `token` requires the resulting client only for authorization code, refresh, and device code; API-key exchange, delegated exchange, and JWT bearer do not consume it (`components/auth/routes.rs:113-188`). Device authorization and revocation separately call `require` (`auth/cli_login.rs:76-90,265-307`). The replacement text therefore keeps the prior client/route contract incomplete even though the platform half is now correct. |
| Every-request bearer portion of `SEC-R3-001` | **CONFIRMED** → `FIND-TASK-005-9` | `/v1` and MCP are bearer-protected, but the auth, platform-token, platform-login, health, and OpenAPI routes are composed outside those bearer layers (`http/router.rs:39-131`). Their operations clear the document-wide bearer requirement where appropriate (`http/openapi.rs:23-36`; handler `security(())` declarations). Saying every API request or every request presents a Wyrd JWT is materially broader than the deployed trust boundary. |
| `BHV-R3-002` | **CONFIRMED** → `FIND-TASK-005-10` | The page introduces both planes and says platform principals hold no tenant, then its glossary assigns every principal/token to a tenant, limits `User` to tenant OIDC, and assigns tenant/role claims to every access token. `PlatformPrincipal` is tenantless and admits `PrincipalKindTag::User` (`wyrd-runtime/src/principal.rs:454-483`); platform OIDC resolves a pre-registered human and issues a platform session (`wyrd-auth/src/platform_login.rs:238-346`); `PlatformAccessTokenClaims` structurally carries no tenant, roles, permissions, or delegation (`wyrd-auth-verify/src/lib.rs:610-648`). |
| `INV-R3-001` | **CONFIRMED** → `FIND-TASK-005-11` | Ordinary tenant request verification obtains the expected tenant from the presented Wyrd token and verifies it (`token_extract.rs:54-149`). Only workload JWT-bearer issuance uses the host label, or the form tenant fallback, as a pre-issuance candidate before the external assertion and server-owned binding verify (`auth/jwt_bearer.rs:15-89`). Boot has no request and uses the configured slug only to own seeded workload federation (`boot/issuer.rs:1-17,61-81`; `boot/mod.rs:2062-2097`). Self-hosting supports one or more operator-selected tenants (`architecture-constraints.md:139-147`; `operations/deployment-and-release.md:6-15`). |

The standards and maintainer reports' no-finding conclusions do not survive
these source traces: both accept the generalized prose without following the
optional client through every token-grant arm, the tenantless platform token
shape through the glossary, or the host reader to its sole workload-exchange
caller. The system report is correct that the candidate changes no deployed
runtime or recovery mechanism, but that does not cure false operator-facing
contracts that TASK-005 explicitly owns. The follow-up report resolves the
conflicts consistently with source. No additional follow-up or specification
decision is required.

## Prior-finding closure

| Finding | Source-backed status |
|---|---|
| `FIND-TASK-005-1` | **CLOSED.** Form-native OAuth errors remain distinct from Problem Details, and only `WyrdError` conversion promises a Wyrd-code log field. |
| `FIND-TASK-005-2` | **CLOSED.** Trusted issuers are documented as workload-only and secret-bearing issuer methods retain the sealing-key requirement. |
| `FIND-TASK-005-3` | **NOT CLOSED; REVISED BELOW.** The separate platform route, clientless platform exchange, and access-only platform session are now accurate. The replacement text still generalizes registered-client identification across all tenant token grants. |
| `FIND-TASK-005-4` | **CLOSED.** The authorities and public guidance distinguish form endpoints from browser interactions and preserve the endpoint-specific success, refusal, and status contracts. |
| `FIND-TASK-005-5` | **CLOSED.** The stateless Wyrd API boundary is distinguished from the BFF-owned encrypted cookie, with locked logout and access-token behavior intact. |
| `FIND-TASK-005-6` | **CLOSED.** Activation is documented as an exact-revision persisted-stamp check with no provider re-probe. |
| `FIND-TASK-005-7` | **CLOSED.** Generic provider setup permits confidential and public clients with their actual secret requirements. |
| `FIND-TASK-005-8` | **CLOSED.** The Rust example uses `ClientConfig::credential`. |

## Final deduplicated finding ledger

### FIND-TASK-005-3 — REVISED — INCORRECT: tenant token documentation requires a registered client for clientless machine grants

- **Discovery sources:** `BHV-R3-001`; registered-client portion of
  `SEC-R3-001`; supported by `followup-review.md` uncertainty 1.
- **Violated obligation:** REQ-021, REQ-018, TASK-005 Approach 3,
  TASK-005-R2 acceptance criterion 1, and AC-009 require the published OAuth
  contract and separate human/workload paths to match the shipped,
  grant-specific standard behavior.
- **Exact locations:** `crates/wyrd/wyrd-server/src/auth/oauth.rs:6-9` and
  `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`.
- **Producer-to-consumer evidence:** `OAuthClients::identify` accepts no client
  as `None`; only authorization-code, refresh, and device-code arms evaluate
  `required?`. The two RFC 8693 token-exchange arms and RFC 7523 JWT-bearer arm
  proceed without a registered client. By contrast, device authorization and
  revocation use `OAuthClients::require`. The platform endpoint correctly has
  no client and directly authenticates its API-key subject. There is no sibling
  runtime path that imposes the documented requirement on machine grants.
- **Observable consequence:** A standard API-key, delegation, or workload
  client can be told to borrow `wyrd-cli` or invent client authentication that
  the shipped grant neither registers nor requires. This materially misstates
  the public wire contract and the independent machine path.
- **Decision-complete minimum correction:** At the two existing owners, state
  that `/auth/token` accepts optional registered-client identification and
  requires it only for authorization code, refresh, and device code. State
  that API-key exchange, delegated exchange, and JWT bearer require no Wyrd
  OAuth client. Keep device authorization and revocation client-bound and keep
  the platform exchange clientless. Reuse `OAuthClients::{identify,require}`
  and the existing grant dispatch as the sole authority; add no client, route,
  fallback, or compatibility behavior.
- **Focused closure proof:** Compare the corrected text with the complete
  `OAuthClients::identify`/`require`, `token`, `device_authorization`, `revoke`,
  and `platform_token` bodies. Run only `mise run docs:check`, `mise run fmt`,
  and `mise run lints`; add `mise run codegen:check` only if a generated source
  owner is changed. No journey or aggregate is required.

### FIND-TASK-005-9 — CONFIRMED — INCORRECT: the operator guide says bootstrap and public requests require a Wyrd bearer token

- **Discovery source:** Every-request bearer portion of `SEC-R3-001`; supported
  by `followup-review.md` uncertainty 1.
- **Violated obligation:** REQ-018, REQ-021, TASK-005's documentation-accuracy
  outcome, and AC-009 require operator trust-boundary guidance to match the
  shipped standard login, bootstrap, metadata, and protected-resource split.
- **Exact location:**
  `docs/src/content/docs/self-hosting/authentication.svx:11,19`.
- **Producer-to-consumer evidence:** `build_router` bearer-protects `/v1` and
  MCP, while composing authorization, callback, device, metadata, tenant and
  platform token exchange, platform login, health, and OpenAPI outside those
  layers. The served OpenAPI defaults to bearer but pre-session operations
  explicitly clear it; OAuth client Basic is client authentication, not a
  caller access token. The sibling concept page already uses the accurate
  phrase “Every authenticated request.”
- **Observable consequence:** An operator following the self-hosting trust
  guide can enforce bearer authentication at ingress on routes that must mint
  or establish that bearer, making ordinary OAuth/OIDC login, metadata, and
  machine exchange unreachable.
- **Decision-complete minimum correction:** Scope both statements to protected
  or authenticated resource/API requests. Preserve each bootstrap endpoint's
  existing standard input, the `/v1` and MCP default-deny layers, and the
  OpenAPI security declarations. Add no middleware exception, token, route, or
  alternate authentication mechanism.
- **Focused closure proof:** Compare the corrected text with the complete
  `build_router`, `SecurityAddon`, auth router, platform auth/login router, and
  relevant handler security declarations. Run only `mise run docs:check` and
  inspect the remediation diff for whitespace errors; no runtime, journey, or
  aggregate proof is warranted.

### FIND-TASK-005-10 — CONFIRMED — INCORRECT: the cross-plane glossary assigns tenant-only identity and claims to the platform plane

- **Discovery source:** `BHV-R3-002`; supported by `followup-review.md`
  uncertainty 2.
- **Violated obligation:** REQ-018, INV-003, and AC-009 require the public
  end-to-end identity map to preserve the shipped platform/tenant separation.
- **Exact location:**
  `docs/src/content/docs/concepts/identity-and-auth.svx:38-46`.
- **Producer-to-consumer evidence:** The page itself says platform principals
  hold no tenant, but the glossary says every principal and token belongs to a
  tenant, every `User` authenticates through tenant OIDC, and every access
  token carries tenant and role claims. `PlatformPrincipal` has no tenant and
  permits a human `User`; platform login pins a pre-registered identity through
  the deployment-wide connection; its access-only token has only platform
  scope and principal/session anchors. `PlatformCaller` verifies the session,
  re-reads the current principal and grant, and constructs the tenantless
  principal. The sibling authentication concept correctly says `User` exists
  in either plane.
- **Observable consequence:** A reader can model a federated platform
  administrator as tenant-owned or build a platform token consumer expecting
  tenant and role claims that cannot exist, obscuring the load-bearing plane
  boundary.
- **Decision-complete minimum correction:** Qualify the glossary at its
  current owner. Tenant principals/tokens own tenant and role/permission
  snapshot data; platform principals, including pre-registered human `User`s,
  are tenantless and their access-only sessions carry platform scope plus
  principal/session anchors while current grants are re-read. Preserve the
  separate stores, connections, routes, and lack of implicit cross-plane
  authority; add no new identity kind or token field.
- **Focused closure proof:** Compare the corrected glossary with
  `PlatformPrincipal`, `PlatformAccessTokenClaims`, `PlatformLogin::complete`,
  and `PlatformCaller::from_request_parts`, then run only
  `mise run docs:check` and inspect the remediation diff for whitespace errors.

### FIND-TASK-005-11 — CONFIRMED — INCORRECT: implicit-tenant guidance makes Host general tenant authority and self-hosting single-tenant

- **Discovery source:** `INV-R3-001`; supported by
  `followup-review.md` uncertainty 3.
- **Violated obligation:** REQ-018, INV-001, AC-003, TASK-005 Approach 1, and
  AC-009 require hosted/self-hosted documentation to match the shipped
  tenant-selection boundary and supported topology.
- **Exact location:**
  `docs/src/content/docs/self-hosting/configuration.svx:96-100`, especially
  line 98.
- **Producer-to-consumer evidence:** Ordinary authenticated tenant requests
  decode the presented token only to select the expected tenant verifier, then
  cryptographically verify the token; they never read `Host`. The only host
  reader in this boundary is `resolve_workload_tenant`, which treats the
  leftmost label, or form `tenant` fallback, as a candidate for workload
  JWT-bearer issuance before assertion and binding verification. Human login
  uses its tenant entry and server-bound state. Boot has no request and uses
  `WYRD_SERVER_TENANT_SLUG` solely to resolve the owner of configured workload
  issuers and bindings. Deployment authority explicitly supports one or more
  operator-selected tenants in self-hosting, and the sibling self-hosting
  overview says a single tenant is usual rather than mandatory.
- **Observable consequence:** An operator can treat a spoofable routing header
  as ordinary tenant authority or treat the implicit slug as a deployment-wide
  tenant lock. Both materially contradict the security boundary and supported
  self-hosted topology.
- **Decision-complete minimum correction:** In the existing implicit-tenant
  section, explain that boot needs an explicit owner for configured workload
  federation because it has no request context in any topology. State that
  protected request authority comes from the verified Wyrd token; only
  workload JWT-bearer issuance may use host/form tenant as an untrusted
  candidate before verification. State that self-hosting supports one or more
  tenants. Preserve existing routing, configuration, and federation behavior;
  add no header, router, tenant lock, or compatibility surface.
- **Focused closure proof:** Compare the corrected paragraph with
  `tenant_from_unverified_access_token`, `verify_access_token`,
  `resolve_workload_tenant`, `resolve_implicit_tenant`, and the supported
  topology authority. Run only `mise run docs:check` and inspect the
  remediation diff for whitespace errors; no journey, live-provider, language,
  or aggregate lane is required.

## Validation completeness and verification limits

Every discovery proposal, including the unique security and invariant claims,
was independently traced to reachable source and all material sibling
consumers. The four retained findings are documentation corrections to
already-shipped behavior. They require no product, public API, architecture,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision, and no custom OAuth/OIDC mechanism.

The task packet records successful `mise run docs:check`, `mise run
codegen:check`, `mise run fmt`, and `mise run lints` for the latest remediation
write set. Those lanes prove rendering, generator parity, formatting, and lint
health; they do not prove the semantic contradictions above. No broad
aggregate, journey suite, live provider, browser, or language suite was run or
required. The prior review-artifact EOF whitespace remains non-material and is
not a finding.
