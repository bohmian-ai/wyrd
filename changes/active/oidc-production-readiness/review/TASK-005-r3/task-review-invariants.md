# TASK-005 R3 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior reviews: `changes/active/oidc-production-readiness/review/TASK-005-r1/` and `TASK-005-r2/`
- Remediation tasks: `TASK-005-R1-doc-contract-accuracy.md` and `TASK-005-R2-doc-contract-closure.md`
- Latest remediation implementation: `323ce32118ec72752a7736b8d42dd957abf6a094..bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`

The candidate was the stated commit before source inspection. The repository
has no `.codegraph/` directory, so I used the complete cumulative and latest
remediation diffs, `rg`, and direct reads of each producer, sibling consumer,
and public documentation owner.

The standing direction controls this review: Wyrd keeps conventional OAuth
2.0/OIDC behavior and its existing vetted libraries. I treated documentation
as blocking only where it materially misstates shipped behavior or a task-owned
contract. I did not reopen the RFC 8693 API-key exchange, ingress-owned device
rate limiting, best-effort RFC 7009 revocation, origin-normalized client base
URL, or validity of self-contained access tokens until expiry.

## Invariant and lifecycle trace

- The tenant OAuth form endpoints still identify `wyrd-ui` or `wyrd-cli`
  through `OAuthClients`. `/auth/platform/token` remains a separate RFC 8693
  API-key exchange with no registered OAuth client, and platform OIDC completion
  returns the same access-only `TokenResponse` through
  `platform_session_response`.
- The four form endpoints still use `OAuthForm`, `OAuthError`, no-store
  responses, and endpoint-specific success shapes. `GET /auth/authorize` uses
  query parameters and a trusted client redirect or local HTML; callback and
  device-browser routes retain their declared redirect, HTML, and Problem
  Details outcomes.
- `HumanClientAuth` still admits `SecretBasic`, `SecretPost`, and `Public`.
  `Public` rejects any supplied secret, and `RelyingParty::redeem` sends no
  secret while retaining PKCE.
- `ClientConfig` still exposes `credential: Option<SecretString>` and no
  `api_key` field. The Rust SDK re-exports this shared client surface unchanged.
  Saved-login selection remains keyed by canonical server origin and tenant;
  without a selector the newest login for that origin wins.
- Normal authenticated tenant requests derive the verification tenant from the
  Wyrd access token and accept authority only after signature and claims
  verification. The `Host` header has one narrower pre-authority role: the
  workload `jwt-bearer` exchange may use its leftmost label as the candidate
  tenant selector, after which the external assertion and server-owned workload
  binding must verify. Boot-time trusted-issuer and binding seeding has no
  request context at all and therefore requires the configured implicit tenant
  slug.

The R2 patch closes the four findings that remained after R1. One separate
task-owned security-contract contradiction remains in the cumulative public
documentation: the implicit-tenant section generalizes the workload-exchange
host selector into a claim that every hosted request obtains its tenant from
`Host`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: OIDC remains optional and operator, UI, SDK, and machine paths remain available | Architecture and self-hosting docs preserve OIDC-off recovery and independent machine credentials; the cumulative range changes no runtime behavior | TASK-005 maps the owning journeys; per direction no broad journey suite was rerun | PASS |
| REQ-005: only stored provider or secret-bearing workload-issuer client secrets require sealing | `cloud-identity.svx:15-19,73-117`, `configuration.svx:72-94`, and `sso-and-oidc.svx:53,59-65` distinguish workload-only issuers and secretless versus secret-bearing client authentication | Static comparison to `HumanClientAuth`, issuer boot/admin persistence, and the sealing owner; recorded focused lanes passed | PASS; `FIND-TASK-005-2` remains closed |
| REQ-018: self-hosted and hosted responsibilities, setup, recovery, client selection, and separate human/workload paths are accurate | The primary SSO, CLI, client, and SDK guidance is aligned, but `configuration.svx:96-100` says every hosted request names its tenant through `Host` and that self-hosting inherently serves one tenant | Direct comparison to `token_extract.rs`, `jwt_bearer.rs`, and `boot/issuer.rs` | FAIL — `INV-R3-001` |
| REQ-021: OAuth request, success, refusal, and metadata contracts remain standard and endpoint-specific | `wyrd-design.md:570-581`, `oauth.rs:1-17`, generated API intros, agent guidance, authentication concepts, and the SSO endpoint table now distinguish query/browser and form endpoints, success bodies, and `400`/`401`/`500`/`503` errors | Static comparison to `authorize`, `device_authorization`, `device_page`, `device_decision`, `callback`, `platform_token`, `revoke`, and `OAuthError::status`; recorded `docs:check` and `codegen:check` passed | PASS; `FIND-TASK-005-1` and `FIND-TASK-005-4` remain closed |
| INV-001 / INV-004: routing context cannot become effective tenant identity and OIDC trust fails closed | Runtime extracts a candidate tenant from an unverified access token or, only for workload exchange, a host/form selector, then verifies the applicable token and binding. The public implicit-tenant explanation incorrectly promotes host routing to the general hosted request identity model | `token_extract.rs:54-149`, `jwt_bearer.rs:15-72`, `postgres.rs:207-216`, and spec revision 11 INV-001 | FAIL — `INV-R3-001` |
| INV-003 / INV-005: platform, tenant-user, and workload planes remain distinct and clients project server authority | `identity-and-auth.svx:87-124`, `authentication.svx:46-70`, and `oauth.rs:3-17` distinguish tenant registered-client grants from direct platform exchange and access-only platform login | Static comparison to both routers, `PlatformSessions`, and `platform_session_response` | PASS; `FIND-TASK-005-3` closed |
| INV-006 and task non-goals | No hosted-signup, social-login, commercial stub, provider-specific runtime path, certified-provider list, SAML, SCIM, compatibility route, or new protocol mechanism entered the cumulative implementation | Complete cumulative diff and scoped searches | PASS |
| AC-002 / AC-003: self-hosted and hosted provider setup, callback, roles, and tenant isolation are documented | The SSO guide accurately covers provider setup, one active connection, shared callback state, role mapping, and independent tenants; the configuration page separately misstates the general hosted tenant-routing boundary | Existing journey mapping plus static source comparison | FAIL only for the documentation-contract consequence in `INV-R3-001` |
| AC-004 and FIND-TASK-004-8: first-class clients document RFC 8628 saved login, tenant selection, newest-login default, and explicit credential precedence | Shared client rustdoc, Python source/stubs, TypeScript source/generated declarations, CLI reference, and `client-configuration.svx:88-125` agree; the Rust example now assigns `ClientConfig::credential` | Recorded `codegen:check` passed; `ClientConfig`, `SavedLogins::select`, and SDK re-export inspected | PASS; `FIND-TASK-005-8` closed |
| AC-005: machine identity remains independent of human SSO | API-key and workload assertion paths remain documented and unchanged | Existing owner evidence and cumulative diff | PASS |
| AC-006 / AC-007: candidate testing, activation, failure, recovery, rotation, logout, and bounded access-token validity are accurate | SSO guidance keeps the exact-revision 15-minute test stamp, no activation probe, BFF cookie custody, best-effort logout, and access-token validity to expiry | Static comparison to activation, BFF, and token-lifecycle owners | PASS; `FIND-TASK-005-5` and `FIND-TASK-005-6` remain closed |
| AC-008: provider-agnostic standard OIDC | Generic setup permits confidential and public clients and keeps the five provider entries as conventional examples; no provider-specific code or certification claim entered | `HumanClientAuth::validate` and `RelyingParty::redeem` inspected | PASS; `FIND-TASK-005-7` closed |
| AC-009: architecture, docs, CLI/UI/SDK surfaces, generated declarations, and shipped behavior agree | Prior OAuth, platform-plane, IdP-client, and SDK-field contradictions are closed, but the public configuration guide still contradicts the shipped tenant-authority boundary | Focused lanes cannot establish semantic agreement; direct source comparison found `INV-R3-001` | FAIL |
| Required deletion and locked decisions | No production passage restores the private BFF channel, server-side browser-session rows, sealed completion, or session-sealing-key model. Closed OAuth/logout/base-URL/token-lifetime decisions remain unchanged | Cumulative search and latest remediation diff | PASS |
| R2 remediation constraint: documentation/source documentation only | Latest remediation changes architecture/docs, the API-doc generator and generated pages, and module rustdoc only | Latest diff inspection | PASS |

## Prior-finding closure

| Finding | Source-backed status |
|---|---|
| `FIND-TASK-005-1` | **CLOSED.** Generated/global error guidance and agent guidance distinguish the four form endpoints' RFC OAuth errors from Problem Details and browser outcomes; only `From<WyrdError>` promises a Wyrd-code log field. |
| `FIND-TASK-005-2` | **CLOSED.** Trusted issuers are documented as workload-only, and `secret_basic`/`secret_post` retain the existing sealing requirement. |
| `FIND-TASK-005-3` | **CLOSED.** The cross-plane concept and OAuth rustdoc distinguish tenant `/auth/token` plus registered clients from direct platform `/auth/platform/token` exchange and access-only platform OIDC completion. |
| `FIND-TASK-005-4` | **CLOSED.** Design authority, generator-owned API pages, agent guidance, security posture, authentication concepts, and the SSO guide describe the shipped endpoint-specific request, response, and status taxonomy without a common envelope. |
| `FIND-TASK-005-5` | **CLOSED.** The stateless API request boundary and BFF-owned encrypted cookie are accurately separated, with best-effort logout and access-token validity through expiry preserved. |
| `FIND-TASK-005-6` | **CLOSED.** Activation is documented as an exact-revision persisted-stamp check with no provider re-probe. |
| `FIND-TASK-005-7` | **CLOSED.** Generic IdP setup permits both shipped confidential modes and the public PKCE mode, with their actual secret and sealing requirements. |
| `FIND-TASK-005-8` | **CLOSED.** The Rust client example assigns the existing `ClientConfig::credential` field; no alias or compatibility surface was added. |

## Proposed findings

### INV-R3-001 — INCORRECT — the configuration guide promotes a narrow host routing hint into general tenant identity

- **Violated obligation:** REQ-018, INV-001, AC-003, AC-009, and TASK-005
  Approach 1 require hosted/self-hosted identity documentation to match the
  shipped tenant-selection and authority boundary.
- **Location:** `docs/src/content/docs/self-hosting/configuration.svx:96-100`,
  especially line 98.
- **Producer-to-consumer evidence:** The page says that in hosted Wyrd “each
  request names its tenant through the Host” and contrasts that with a
  self-hosted deployment serving one tenant. Normal authenticated request
  extraction does not use `Host`: `token_extract.rs:54-149` reads the tenant
  claim from the presented Wyrd access token and cryptographically verifies
  that token for the expected tenant. `Host` is read only by the workload
  `jwt-bearer` exchange (`jwt_bearer.rs:53-72`) as one untrusted pre-issuance
  selector, with the form `tenant` as fallback; the external assertion and
  server-owned binding must then verify before any Wyrd authority is minted.
  Boot seeding has no request in either deployment model and binds configured
  issuers/bindings through the explicit tenant slug (`boot/issuer.rs:1-17,61-81`).
  The same changed documentation set correctly says authenticated request
  identity is token-derived (`identity-and-auth.svx:55-85`), so the public
  accounts contradict each other.
- **Observable consequence:** A hosted operator can treat a spoofable routing
  header as the tenant security authority for ordinary API calls, or infer that
  the implicit slug is a single-tenant/self-hosting mechanism rather than the
  owner selector for boot-seeded workload federation. That is a material
  misdescription of the shipped tenancy boundary, not a wording preference.
- **Required testable correction:** At the existing implicit-tenant section,
  state that the configured slug selects the owning tenant for boot-seeded
  trusted issuers and workload bindings because boot has no request context in
  either deployment model. Distinguish that durable binding from runtime
  authority: authenticated API requests derive tenant identity from the
  verified Wyrd token; only the workload JWT-bearer issuance path may use the
  host label (or its form fallback) as an untrusted candidate selector before
  assertion and binding verification. Do not change runtime routing, add a
  tenant header, or remove the existing hosted workload selector.
- **Focused closure proof:** Compare the corrected paragraph with
  `tenant_from_unverified_access_token`, `verify_access_token`,
  `resolve_workload_tenant`, and `resolve_implicit_tenant`; run only
  `mise run docs:check` and `git diff --check`. No journey, live provider,
  language suite, or aggregate is warranted for this documentation-only gap.

## Non-blocking notes

- `docs/src/content/docs/get-started/client-configuration.svx:101-103` contains
  a pre-existing incomplete sentence contrasting global-aware and
  environment-only constructors. It is unrelated to the task's identity,
  saved-login, and OAuth contract obligations and is not retained as a finding
  under the standing direction.
- The cumulative range still makes `git diff --check` exit `2` for an extra
  blank line at EOF in the prior R1 verdict. The R2 validator already rejected
  this whitespace-only discrepancy as non-material; it remains non-blocking.

## Verification assessment

The supplied R2 implementation evidence records exit 0 for `mise run
docs:check`, `mise run codegen:check`, `mise run fmt`, and `mise run lints`.
Those are the narrowest lanes covering its documentation, generator,
generated-page, and Rust-rustdoc write set. No full journey, browser,
live-provider, language, or repository aggregate was run or required. The
current cumulative `git diff --check` result is limited only by the prior
review-artifact whitespace noted above. Mechanical lanes do not prove the
semantic tenant-authority contradiction in `INV-R3-001`.

## Overall result

**FAIL**

All eight prior findings are source-closed without reopening a locked product
decision. The cumulative task is not acceptance-complete while a task-owned
public configuration guide materially misstates `Host` as the general hosted
tenant identity boundary.
