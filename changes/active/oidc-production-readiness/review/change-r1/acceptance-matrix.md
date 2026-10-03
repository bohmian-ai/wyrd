# OIDC production-readiness change acceptance matrix

Subject: `b245056712b1d252bab22b619898a6d9ebd2c095..48f2e2423136942a11949a1c831c69811c4f45eb`

The result column assesses the integrated implementation and journey evidence.
It does not replace the required task-review closure identified in
[`verdict.md`](verdict.md).

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 | Optional connection ownership and OIDC-off recovery path in `wyrd-auth`, server composition, and UI BFF | Unfiltered identity lane: OIDC-off UI plus machine/client paths | PASS |
| REQ-002 | Tenant-owned connection rows, RLS, and one-active lifecycle constraints | Server multi-tenant journeys against Keycloak and Dex | PASS |
| REQ-003 | Typed connection administration routes and settings projection | Administration, real connection-test, rotation, replacement, and UI settings journeys | PASS |
| REQ-004 | Discovery-backed setup, exact callback, supported client-auth validation | Server and UI connection journeys | PASS |
| REQ-005 | Sealed provider secrets, redacted contracts, rotation owner, no recoverable token storage | Rotation, key failure, response-secret, and UI secret-boundary journey coverage recorded in the packet; integrated identity lane passed | PASS |
| REQ-006 | `/t/{tenantKey}/login`, common state-bound callback, deployment-owned public origin | Server callback and production UI journeys | PASS |
| REQ-007 | `openidconnect` code flow with PKCE, state, nonce, issuer/audience/key/time checks, and screened provider transport | Human-login, refusal, issuer-binding, and production UI journeys | PASS |
| REQ-008 | Tenant-bound `(issuer, subject)` user resolution and tenant role mapping | Human-login, provider-switch, and multi-provider journeys | PASS |
| REQ-009 | `wyrd-ui` confidential authorization-code client using `openid-client`; encrypted `jose` cookie | Four production UI/BFF journeys across two BFF and two Wyrd replicas | PASS |
| REQ-010 | Separate API-key operator recovery page using Wyrd exchange authority | OIDC-off credential UI journey | PASS |
| REQ-011 | RFC 8628 CLI login and `webbrowser` launcher | CLI device-login journey in the unfiltered identity lane | PASS |
| REQ-012 | Shared saved-login selection, locked renewal, rotation, local-first logout, and BFF/public-client refresh policy | CLI, shared-client concurrency, Rust, Python, TypeScript, and UI journeys | PASS |
| REQ-013 | Independent API-key and RFC 7523 workload paths | Machine-independence and workload journey evidence; integrated server suite passed | PASS |
| REQ-014 | Tested provider replacement without email linking or inherited authority | Provider-switch and production replacement journeys | PASS |
| REQ-015 | Per-tenant authentication and session revalidation | Multi-provider tenant-switch journey | PASS |
| REQ-016 | Old connection blocks new login/renewal while issued access tokens retain bounded authority | Session-cutoff and production replacement journeys | PASS |
| REQ-017 | Canonical transactional audit for security-significant identity decisions | Audit refusal/rollback evidence in task reviews and integrated server suite | PASS |
| REQ-018 | Self-hosting, identity, CLI/SDK, provider, recovery, and rotation documentation | TASK-005 implementation evidence and recorded docs lane; final independent TASK-005 review is missing | FAIL |
| REQ-021 | Standard form bodies, OAuth success/error envelopes, RFC grants, revocation, and RFC 8414 metadata | Authorization-server, CLI, BFF, and SDK journeys in the integrated lane | PASS |
| INV-001 | Effective tenant/connection comes from verified server-bound state, not path/header/email/token input | Callback refusal, wrong-tenant, and cross-tenant journeys | PASS |
| INV-002 | Stable tenant identity is `(issuer, subject)`; email does not link | Provider-switch and same-email journey evidence | PASS |
| INV-003 | Platform, tenant-user, and workload planes remain distinct | Platform and machine-independence evidence; source/task-review audit | PASS |
| INV-004 | Discovery is metadata only; TLS, SSRF pinning, JWKS, replay, audit, and RLS fail closed | Refusal/rotation journeys and TASK-001 remediation evidence; final independent TASK-001 review is missing | FAIL |
| INV-005 | SDKs and UI project server-owned identity and permissions | All first-class client and UI journeys passed | PASS |
| INV-006 | No hosted signup, social login, licensing, edition detection, or commercial onboarding stub | Complete diff and task non-goal ledgers | PASS |
| INV-007 | One principal, role, credential, revocation, audit, and SQL authority across browser/CLI/SDK paths | Integrated server, UI, CLI, client, and SDK journeys | PASS |
| AC-001 | OIDC-off real UI and SDK behavior | OIDC-off production UI plus saved-login/machine paths | PASS |
| AC-002 | Real-server standard-provider sign-in with allowed and denied calls | Keycloak/Dex server and UI journeys | PASS |
| AC-003 | Two concurrent tenants/providers with isolation and callback refusal | Multi-provider server and UI journeys | PASS |
| AC-004 | CLI-established Rust/Python/TypeScript login, renewal, selection, precedence, concurrency, and refusal | Every saved-login surface ran in the unfiltered lane | PASS |
| AC-005 | API-key and workload independence and exact assertion binding | Machine-independence/workload server journeys | PASS |
| AC-006 | Real-sign-in connection testing and provider replacement without inherited authority | Connection-test and provider-switch journeys | PASS |
| AC-007 | Fault, replay, key/secret rotation, audit, grant refusal, and cross-replica evidence | Server refusal suite and four cross-replica UI journeys; TASK-003 remediation evidence lacks a final independent review | FAIL |
| AC-008 | Provider-agnostic standard OIDC against Keycloak and Dex | Unfiltered identity lane with both providers | PASS |
| AC-009 | Public contracts, CLI help, UI, docs, and generated schemas agree | Recorded codegen/docs checks and integrated journeys; final independent TASK-005 review is missing | FAIL |
| Decision 1 | Tenant is the durable customer security boundary; one active human connection | Connection SQL owner, RLS, and multi-tenant journeys | PASS |
| Decision 2 | Humans receive tenant Wyrd authority; workloads keep independent identities | Human and machine journeys | PASS |
| Decision 3 | Tenant route namespace plus one deployment-controlled callback | Server routes, BFF routes, callback journeys | PASS |
| Decision 4 | No open-source hosted-signup stub | Diff/non-goal inspection | PASS |
| Decision 5 | Provider switch never links by email | Provider-switch journey | PASS |
| Decision 6 | One local login supplies all first-class SDKs | Rust, Python, and TypeScript saved-login journeys | PASS |
| Decision 7 | BFF uses authorization code; CLI uses device grant; tokens mint at redemption | BFF, CLI, and terminal-grant server journeys | PASS |
| Constraint: vetted libraries | `openidconnect`, `oauth2`, `openid-client`, `jose`, and `webbrowser` own their approved protocol/platform boundaries | Dependency/source audit and all integrated journeys | PASS |
| Constraint: conventional RFC 8693 API-key exchange | Standard token-exchange form with `urn:wyrd:oauth:token-type:api_key`; no custom grant | Source/task-review audit and OIDC-off UI/client journeys | PASS |
| Constraint: ingress owns `POST /auth/device` rate limiting | No image-specific rate-limit mechanism added | Diff and architecture inspection | PASS |
| Constraint: stateless API server and BFF-owned encrypted cookie | No Wyrd browser-session store; portable encrypted cookie | Cross-replica UI journey | PASS |
| Constraint: best-effort RFC 7009 logout | One form POST through the redirect-free adapter; local state always clears | UI and client logout journeys | PASS |
| Constraint: client target is a userinfo-free URL origin | Shared parsed origin owner | Rust/Python/TypeScript and focused task evidence | PASS |
| Constraint: self-contained access tokens survive logout until expiry | Bounded cached/bearer behavior retained | UI logout/replay journey evidence | PASS |
| Constraint: audience is `client_id`; migration `20261002000001` remains | Token issuance/validation and migration diff | Integrated migration plus identity lane | PASS |
| Constraint: provider `error_description` is logged without changing public OAuth shape | OAuth refusal logging boundary | Refusal journeys and source audit | PASS |
| Constraint: Windows proof uses `webbrowser`; PLAT-001/WSL stays rejected | Hand launcher deleted; no WSL branch | TASK-012 PASS review and dependency target proof | PASS |
| Non-goal: LDAP/password/SAML/SCIM/social signup and simultaneous tenant providers | Deferred/excluded; no compatibility or commercial scaffolding | Complete diff and task non-goal ledgers | PASS |
| Non-goal: arbitrary IdP tokens as Wyrd authority | Provider tokens terminate at the relying-party boundary | Server/client journey evidence | PASS |
| Non-goal: second principal, role, credential, session, or audit authority | Existing Wyrd owners remain authoritative | Cross-surface source/task-review audit | PASS |
