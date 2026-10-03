# TASK-005 R3 behavior review

## Review findings

### Important

#### BHV-R3-001 — INCORRECT — tenant token client identification is still overgeneralized

- **Violated obligation:** REQ-021, AC-009, TASK-005 Approach 3, and the
  `FIND-TASK-005-3` remediation require the published OAuth contract to match
  the shipped grant-specific client behavior without adding a client
  requirement to the machine grants.
- **Locations:**
  - `crates/wyrd/wyrd-server/src/auth/oauth.rs:6-8`
  - `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`
- **Evidence:** Both descriptions now say that the tenant token endpoint
  identifies a registered `wyrd-ui` or `wyrd-cli` client. The handler does call
  `OAuthClients::identify`, but that operation intentionally returns
  `Option<OAuthClientId>` (`auth/oauth.rs:317-348`). Only authorization-code,
  refresh, and device-code dispatch consume `required?`
  (`components/auth/routes.rs:132-152`). API-key token exchange, delegated
  token exchange, and JWT-bearer dispatch proceed without a registered client
  (`components/auth/routes.rs:154-184`). The platform correction is accurate:
  `/auth/platform/token` has no registered client and exchanges its presented
  platform API key directly.
- **Observable consequence:** An operator or standard OAuth client author can
  infer that API-key exchange, delegation, or JWT-bearer must impersonate one
  of Wyrd's two human clients or supply client authentication. That is a
  materially false public wire requirement for shipped machine grants and
  conflicts with Wyrd's standard, minimal OAuth integration direction.
- **Required testable correction:** At the two documentation owners, state
  that `/auth/token` consults the registered-client owner but requires a
  registered client only for the authorization-code, refresh, and device-code
  grants; its token-exchange and JWT-bearer grants proceed when no Wyrd OAuth
  client is identified.
  Keep device authorization and revocation client-bound, and preserve the
  platform endpoint's direct credential exchange. Add no client, alias,
  fallback, or runtime branch. Prove closure by comparing the text with the
  complete `token` match and `OAuthClients::{identify,require}` bodies, then run
  the existing narrow docs, codegen, format, and lint lanes.

#### BHV-R3-002 — INCORRECT — the cross-plane glossary assigns platform identities tenant-only token semantics

- **Violated obligation:** INV-003, REQ-018, and AC-009 require platform
  administrators and tenant users to remain distinct security planes and the
  public cross-plane identity map to agree with shipped identity and token
  contracts.
- **Location:** `docs/src/content/docs/concepts/identity-and-auth.svx:38-46`.
- **Evidence:** The page expressly presents both planes, but its glossary says
  every principal, token, role, issuer, and binding belongs to a tenant; says a
  `User` is authenticated only through a tenant's provider; and says every
  access token carries tenant and role claims. Production also stores
  platform-scoped human principals with `PrincipalKindTag::User`
  (`wyrd-sql/tests/pg_platform_identity.rs:130`; the runtime contract is
  `PlatformPrincipal` in `wyrd-runtime/src/principal.rs:459-480`) and resolves
  them through the deployment-wide platform OIDC connection
  (`wyrd-auth/src/platform_login.rs:1-17,277-344`).
  `PlatformAccessTokenClaims` is deliberately unable to carry a tenant, roles,
  permissions, or delegation (`wyrd-auth-verify/src/lib.rs:610-647`), and the
  platform request extractor re-reads current grants
  (`components/auth/platform_extractor.rs:184-207`). The sibling Authentication
  concept correctly lists `User` in either plane, making the public
  contradiction direct.
- **Observable consequence:** A reader following the end-to-end map can model
  a federated platform administrator as a tenant identity or build a platform
  token consumer expecting tenant and role claims that never exist. That
  obscures the load-bearing plane separation rather than expressing a wording
  preference.
- **Required testable correction:** Qualify the glossary at its current owner:
  tenant-scoped principals and tokens carry tenant data, while platform
  principals and their access-only sessions do not; a `User` may be a
  pre-registered platform human or a tenant human, selected through separate
  connections and authorities. Preserve the separate stores, routes, session
  verification, and lack of implicit cross-plane authority. This needs only a
  static comparison to `PlatformPrincipal`, `PlatformAccessTokenClaims`, and
  `PlatformLogin`, followed by `mise run docs:check`; no runtime or new
  abstraction is required.

### Suggestions

None. Wording, placement, naming, structure, and completeness beyond the task
were not treated as findings.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: OIDC is optional; OIDC-off UI, SDK, operator, and machine paths remain available | Architecture and self-hosting docs retain API-key recovery and independent machine identity; the cumulative range changes no executable behavior | Owner journeys are listed from TASK-003/011 evidence; not rerun by this docs review | PASS |
| REQ-005: only stored provider or secret-bearing workload-issuer client secrets require sealing | `cloud-identity.svx`, `configuration.svx`, `authentication.svx`, Kubernetes guidance, and boot rustdoc distinguish secret-bearing and public clients | Prior `FIND-TASK-005-2` source closure plus passing recorded docs/codegen/lint lanes | PASS |
| REQ-018: operator/IdP ownership, callback, one-active connection, role mapping, secret rotation, failures, recovery, CLI login, saved-login selection, and separate human/workload paths are accurately documented | Primary SSO, CLI, client configuration, workload, and recovery guidance cover the required surfaces; the cross-plane glossary remains materially false for platform Users and tokens | Static comparison to platform runtime owners | FAIL — `BHV-R3-002` |
| REQ-021: the approved OAuth endpoints publish their shipped standard wire contracts | Endpoint-specific query/form, redirect/HTML/JSON/Problem, success, and status descriptions now match the handlers; registered-client identification remains overstated for machine grants on `/auth/token` | Static comparison to `token`, `OAuthClients`, `platform_token`, authorize, callback, device, and revoke owners | FAIL — `BHV-R3-001` |
| INV-001: unverified route, host, header, and provider data cannot select effective tenant or connection | Callback, state, exact redirect, issuer, and server-bound selection guidance remains aligned with the runtime | Existing owner tests/evidence listed by TASK-005 | PASS |
| INV-003: platform administrators, tenant users, and workloads remain distinct principal and authorization planes | Runtime and most corrected docs preserve separate routes and stores, but the cross-plane glossary assigns tenant-only identity and claim semantics to the platform plane | `PlatformPrincipal`, `PlatformLogin`, `PlatformAccessTokenClaims`, and platform extractor comparison | FAIL — `BHV-R3-002` |
| INV-004: provider trust remains fail closed | Discovery, SSRF, issuer, JWKS, nonce, state, and failure guidance remains intact | Existing owner evidence listed by TASK-005 | PASS |
| INV-005: UI and SDKs project server-owned identity and permissions | BFF custody, shared-client ownership, source declarations, and generated projections remain server-contract projections | Recorded codegen and N-API evidence | PASS |
| INV-006 and task non-goals | No hosted signup, social login, SAML/SCIM documentation, commercial stub, provider implementation branch, compatibility route, or certified-provider list entered the shipped change | Complete cumulative diff inspection | PASS |
| AC-002 / AC-003: self-hosted and hosted connection setup and tenant isolation are documented | SSO guide covers callback, per-tenant ownership, one active connection, role mapping, same-issuer separation, and provider examples | Owning journeys listed from TASK-002/009/011 evidence; not rerun | PASS |
| AC-004 and carried FIND-TASK-004-8: all first-class clients describe device login, explicit precedence, tenant selection, and newest-login default | Shared client rustdoc, CLI reference, Python source/stubs, TypeScript source/native/declarations, and client guide agree; Rust example uses `ClientConfig::credential` | Recorded `codegen:check` and `ts:napi:check`; prior `FIND-TASK-005-8` closure | PASS |
| AC-005: machine identity remains independent from SSO | API-key and JWT-bearer paths, exact workload binding, and access-only renewal are documented without provider-specific runtime behavior | TASK-002/012 owner evidence listed | PASS |
| AC-006 / AC-007: replacement, real test sign-in, recovery, fault behavior, logout, and bounded token lifetime are accurate | Activation-stamp, BFF cookie, best-effort logout, and access-token-expiry corrections match their owners | Prior `FIND-TASK-005-5` and `FIND-TASK-005-6` source closure | PASS |
| AC-008: provider-agnostic standard OIDC with generic setup and short examples | Generic setup now permits shipped confidential and public clients; Okta, Entra ID, Google, Auth0, and Keycloak remain examples only | Prior `FIND-TASK-005-7` source closure | PASS |
| AC-009: architecture, public docs, CLI/UI/SDK surfaces, generated declarations, and shipped behavior agree | Generated and declaration parity is recorded, but the OAuth client requirement and cross-plane glossary disagree with runtime | Available narrow checks are green but cannot prove semantic prose accuracy | FAIL — `BHV-R3-001`, `BHV-R3-002` |
| Required deletion: no private BFF channel, server browser-session rows, sealed completion, or session-sealing-key model | Complete cumulative diff and search found no reintroduction | Static search | PASS |
| Locked human decisions remain closed | RFC 8693 API-key exchange uses `urn:wyrd:oauth:token-type:api_key`; ingress owns device-page rate limiting; logout revocation is best-effort; client base URL is origin-normalized; access tokens live until expiry | Source and cumulative documentation inspection | PASS |
| Documentation-only scope and no unearned protocol machinery | Candidate changes docs, generated docs/declarations, help/rustdoc, and review records only; no runtime behavior or dependency changed | Base-to-candidate diff inspection | PASS |

## Prior-finding closure

| Finding | Status | Source evidence |
| --- | --- | --- |
| `FIND-TASK-005-1` | CLOSED | Problem Details and Wyrd-code logging are scoped away from extractor-native OAuth form refusals. |
| `FIND-TASK-005-2` | CLOSED | Trusted issuers are workload-only and secret-bearing variants retain the existing sealing requirement. |
| `FIND-TASK-005-3` | **NOT FULLY CLOSED** | Tenant/platform routes, platform access-only behavior, and direct platform exchange are corrected, but the replacement wording now overstates registered-client identification for `/auth/token` machine grants (`BHV-R3-001`). |
| `FIND-TASK-005-4` | CLOSED | Authorities and guides distinguish form endpoints from browser interactions and publish the shipped success/error/status shapes. |
| `FIND-TASK-005-5` | CLOSED | The Wyrd API's stateless request boundary is distinguished from the BFF-owned encrypted cookie. |
| `FIND-TASK-005-6` | CLOSED | Activation is described as a current exact-revision stamp check with no provider liveness re-probe. |
| `FIND-TASK-005-7` | CLOSED | Generic tenant IdP setup permits confidential or public clients with the corresponding secret requirements. |
| `FIND-TASK-005-8` | CLOSED | The Rust example uses the existing `ClientConfig::credential` field. |

`BHV-R3-002` is a new proposed finding in the task-owned cross-plane concept
page. It is not a restatement of a prior endpoint or error-envelope defect.

## Open questions

None. Both proposed corrections are bounded documentation reconciliation with
existing shipped owners and require no new product, API, architecture,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision.

## Verification notes

- Immutable subject reviewed: base
  `134f605367e65b41f1977d6c70ac8ca8b277a69e`, candidate
  `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`, approved specification revision
  11, original TASK-005, both remediation tasks, and prior R1/R2 verdict and
  validation packets.
- The repository has no `.codegraph/` directory, so navigation used the
  complete cumulative diff, the latest remediation diff, `rg`, and direct
  reads of the cited producers, consumers, sibling paths, and tests.
- Available implementation evidence records successful `mise run docs:check`,
  `mise run codegen:check`, `mise run fmt`, `mise run lints`, and generated-page
  parity for the R2 write set. Earlier task evidence also records the required
  TypeScript/Python declaration lanes for their cumulative changes.
- No full journey, live-provider, browser, language, or repository aggregate
  was run or required. The two findings are semantic documentation mismatches
  that the mechanical lanes do not detect.
- The candidate remained at the stated commit throughout this review.

## Overall result

**FAIL**

The latest remediation closes the public-client setup, endpoint response-shape,
platform route, and Rust example defects, but two material documentation
contracts still disagree with the shipped system.
