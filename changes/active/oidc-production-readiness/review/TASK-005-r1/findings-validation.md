# TASK-005 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`

The candidate was still checked out at the stated commit before this report was
written. The repository has no `.codegraph/` directory, so validation used the
complete Git diff, `rg`, and direct reads of each cited owner, caller, sibling
consumer, and documentation generator. There was no follow-up report to
validate; the orchestrator recorded that the discovery claims were additive or
overlapping rather than materially conflicting.

The binding standing direction was applied throughout: the correction is the
smallest documentation reconciliation with the shipped OAuth 2.0/OIDC
standards and existing libraries. None of the closed decisions about RFC 8693
API-key exchange, ingress-owned device-page rate limiting, best-effort RFC 7009
revocation, origin-normalized client URLs, or access-token validity through
expiry is reopened.

## Source and reachability trace

| Boundary | Producer and writers | Reachable consumers and sibling paths | Existing owner to preserve |
| --- | --- | --- | --- |
| OAuth refusal wire | `OAuthForm` and `OAuthClients` construct protocol-native `OAuthError` values directly; `From<WyrdError>` is the only path that logs `wyrd_code` (`crates/wyrd/wyrd-server/src/auth/oauth.rs:100-123,162-352`) | `/auth/token`, `/auth/platform/token`, `/auth/device_authorization`, and `/auth/revoke` all use those extractors/helpers; general OpenAPI and agent pages are public consumers of the same contract | `OAuthError`, `OAuthForm`, `OAuthClients`, and the live handler-generated OpenAPI remain the one wire implementation; documentation must describe them rather than add another error identity |
| OAuth success wire | Tenant and platform token handlers return `TokenResponse`; device authorization returns `DeviceAuthorization`; revocation returns empty `200` (`components/auth/routes.rs:70-189`, `components/platform/routes.rs:63-142`, `auth/cli_login.rs:43-90,265-307`) | Standard OAuth clients and the public SSO/security documentation consume these distinct endpoint contracts | Preserve the RFC-owned response shapes already emitted by the handlers |
| Trusted workload issuer | Both boot seeding and admin create refuse `IssuerTokenPolicy::Human` before persistence; secret-bearing workload issuers are sealed and accepted (`boot/issuer.rs:84-156`, `components/admin/routes.rs:270-325`, `wyrd-auth/src/pg_resolvers.rs:673-705`) | Boot TOML, admin CRUD, workload assertion verification, sealing rewrap, and the cloud-identity/configuration guides share the stored issuer authority | Preserve `refuse_human_trusted_issuer`, boot's Human refusal, and `issuer_write_from_trusted`; correct only the advertised configuration |
| Administration planes | Tenant grants dispatch through `/auth/token`; platform credential exchange is separately mounted at `/auth/platform/token` (`components/auth/routes.rs:43-68,70-189`, `components/platform/routes.rs:54-142`) | Operators use the self-hosting authentication guide for both planes; shared client auth also has separate tenant and platform exchange methods | Preserve the two existing routes and qualify the guide; do not add an alias |
| Browser session | `BrowserSessions` seals a refresh token or recovery API key into a Secure, HttpOnly, SameSite=Lax cookie and produces bearer-authenticated server calls (`wyrd-ui/src/lib/server/auth/browser-sessions.ts:23-34,98-177,189-387`) | Login callback, hooks, tenant layouts, API-key recovery, settings, and logout use the singleton `browserSessions` owner | Preserve the BFF cookie and the stateless Wyrd API request boundary |
| Connection activation | Test completion stamps the exact candidate revision for 15 minutes; activation reads only the current stamp and recovery-key authority under the slot lock, then promotes in one transaction (`wyrd-auth/src/connections.rs:428-576`; `wyrd-sql/src/queries/auth/human_connections.rs:304-335`) | Tenant admin activation is the sole runtime caller; provider discovery/JWKS/token IO occurs during candidate testing and later login, not activation | Preserve the persisted test stamp and recovery-key checks; correct only the runbook's liveness claim |

## Validation of discovery proposals

| Proposal | Result | Independent validation |
| --- | --- | --- |
| `BHV-001` | **REVISED** → `FIND-TASK-005-1` | The public OpenAPI and agent pages do erase the approved OAuth error exception. The correction must also update `docs/scripts/generate_api_docs.py`, which owns the generated OpenAPI page, and must be consolidated with `SEC-003`: both arise from claiming that every refusal has a Wyrd catalog identity. |
| `BHV-002` | **REVISED** → `FIND-TASK-005-2` | The workload page advertises Human/default-Human configuration that both production entry paths refuse. It shares the trusted-issuer documentation boundary with `INV-REV-001`, including the false claim that workload federation carries no client secret. |
| `INV-REV-001` | **REVISED** → `FIND-TASK-005-2` | Both halves are reachable and material: omitted/default-Human TOML fails at boot, explicit Human admin input is refused, and `secret_basic`/`secret_post` workload issuers require sealing. One documentation correction can reconcile that single trusted-issuer boundary. |
| `INV-REV-002` | **CONFIRMED** → `FIND-TASK-005-3` | The unqualified “Every grant” statement includes the platform plane introduced on the same page, but production mounts platform RFC 8693 exchange only at `/auth/platform/token`. |
| `STD-TASK-005-1` | **CONFIRMED** → `FIND-TASK-005-4` | The authority and guide collapse three different standard success contracts into RFC 6749 section 5.1. Handler and OpenAPI source prove the device response and empty revocation response are different by design. |
| `MAINT-001` | **CONFIRMED** → `FIND-TASK-005-5` | The categorical “There is no session cookie” contradicts the shipped BFF owner and the candidate's sibling documentation. It is a credential-custody error, not a wording preference. |
| `SYS-001` | **CONFIRMED** → `FIND-TASK-005-6` | Activation performs no provider IO. A provider outage after a successful test does not invalidate the still-current stamp, so the activation runbook promises a liveness refusal the deployed service does not perform. |
| `SEC-001` | **CONFIRMED**, duplicate of `MAINT-001` → `FIND-TASK-005-5` | The BFF cookie is reachable from every web login and recovery path and holds renewal authority; the operator-facing denial of that cookie is materially misleading. |
| `SEC-002` | **CONFIRMED**, duplicate of `STD-TASK-005-1` → `FIND-TASK-005-4` | RFC 8628 device authorization returns its own response object and RFC 7009 revocation returns an empty success; neither is a token response. |
| `SEC-003` | **REVISED**, consolidated with `BHV-001` → `FIND-TASK-005-1` | Only `OAuthError::from(WyrdError)` logs `wyrd_code`; malformed form and client-identification refusals directly construct `OAuthError` and carry no Wyrd catalog code. The smallest correction is to remove or scope the promises, not invent a second error/logging mechanism. |

No discovery proposal was rejected. The revisions above consolidate common
causes and move generated-file corrections to their existing source owners.

## Final deduplicated finding ledger

### FIND-TASK-005-1 — REVISED — INCORRECT: OAuth errors are still documented as universally carrying a Wyrd catalog identity

- **Discovery sources:** `BHV-001`, `SEC-003`.
- **Violated obligation:** REQ-021, TASK-005 Approach 3, and AC-009 require the
  four OAuth form endpoints to be the explicit RFC 6749 error-wire exception
  and require public and source documentation to match the shipped contract.
- **Locations:**
  - `docs/scripts/generate_api_docs.py:60-62` and generated
    `docs/src/content/docs/api/openapi.md:26-28`
  - `docs/src/content/docs/for-agents/error-remediation.svx:13-53,90-116`
  - `docs/scripts/generate_api_docs.py:119-123` and generated
    `docs/src/content/docs/api/errors.md:11-15`
  - `architecture/wyrd-security-posture.md:208-212`
  - `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`
  - `crates/wyrd/wyrd-server/src/auth/oauth.rs:3-9`
- **Evidence:** `OAuthForm` rejects wrong content types, unreadable bodies,
  repeated parameters, and decode failures by directly constructing
  `OAuthError` (`oauth.rs:162-259`). `OAuthClients` directly constructs
  `invalid_client` and `invalid_request` for malformed credentials, unknown
  clients, and contradictory identifiers (`oauth.rs:263-352`). Only
  `From<WyrdError>` emits the `tracing::info!` field `wyrd_code`
  (`oauth.rs:100-123`). The response contains only OAuth `error` and optional
  `error_description` (`oauth.rs:127-145`). These paths are reached before or
  inside every one of the four form handlers.
- **Observable consequence:** An agent can branch on a nonexistent response
  `code`, and an operator can search for a promised Wyrd-code log correlator
  that is absent for malformed and client-authentication refusals.
- **Decision-complete minimum correction:** Reuse the existing OAuth exception
  already stated by REQ-021. Update the API-doc generator (then regenerate its
  pages) and the agent guide so Problem Details/Wyrd-code instructions apply to
  non-OAuth operations, with the four form endpoints directed to their existing
  RFC 6749 error contract. Remove or narrowly qualify every blanket promise
  that the server logs each OAuth refusal under a Wyrd code, including the
  OAuth module rustdoc; it may state only what the existing
  `From<WyrdError>` path actually does. Do not add another response field,
  error catalog, logger, wrapper, or compatibility path.
- **Focused closure proof:** Statically exercise one extractor-native refusal
  (for example repeated form parameters) and one `WyrdError`-mapped refusal
  against the corrected descriptions; the former must not be promised a Wyrd
  code and both must retain the RFC body. Run `mise run docs:check`,
  `mise run codegen:check`, `mise run fmt`, and `mise run lints` for the
  generator/generated-doc/Rust-rustdoc write set. No journey or aggregate is
  required.

### FIND-TASK-005-2 — REVISED — INCORRECT: trusted-issuer docs advertise refused Human issuers and deny supported secret-bearing workload issuers

- **Discovery sources:** `BHV-002`, `INV-REV-001`.
- **Violated obligation:** REQ-005, REQ-018, TASK-005 Approach 2, and AC-009
  require the human/workload boundary and sealing-key requirement to match the
  shipped configuration.
- **Locations:** `docs/src/content/docs/concepts/cloud-identity.svx:73-117` and
  `docs/src/content/docs/self-hosting/configuration.svx:72-80`.
- **Evidence:** The cloud-identity introduction correctly says trusted issuers
  are workload-only, but its field table advertises `human`, default-Human,
  human email/group/default-role behavior. Admin creation calls
  `refuse_human_trusted_issuer` before discovery or persistence
  (`components/admin/routes.rs:297-319`), while boot rejects a Human entry
  before IO (`boot/issuer.rs:106-122`). Conversely, the config DTO accepts
  `secret_basic` and `secret_post` (`config.rs:3352-3382`), and
  `issuer_write_from_trusted` requires a sealing key for them
  (`pg_resolvers.rs:673-705`). The configuration guide nevertheless says
  workload federation carries no client secret immediately after acknowledging
  the supported secret-bearing case.
- **Observable consequence:** An operator can author a Human trusted issuer
  that deterministically fails, or omit the sealing key for a secret-bearing
  workload issuer and make boot/admin creation fail closed.
- **Decision-complete minimum correction:** Keep the existing workload issuer
  and tenant OIDC connection owners. Describe `principal_kind` as workload-only
  on this surface (and explain that omitted/default Human is refused where that
  detail matters), stop presenting Human-only mappings as usable trusted-issuer
  behavior, and route human setup to the tenant connection guide. State that
  public/private-key workload issuers carry no shared secret while
  `secret_basic`/`secret_post` workload issuers require the existing sealing
  key. Do not change the enum, persistence schema, migration compatibility, or
  issuer implementation in this docs task.
- **Focused closure proof:** Compare the corrected field/configuration text to
  both production entry points and to `issuer_write_from_trusted`, then run
  `mise run docs:check`. No runtime or journey lane is required.

### FIND-TASK-005-3 — CONFIRMED — INCORRECT: the operator guide sends the platform grant to the tenant token endpoint

- **Discovery source:** `INV-REV-002`.
- **Violated obligation:** REQ-018, REQ-021, INV-003, and AC-009 require the
  platform and tenant planes and their standard routes to remain distinct.
- **Location:** `docs/src/content/docs/self-hosting/authentication.svx:21-33`.
- **Evidence:** The page says every grant ends at `/auth/token`. The tenant
  router mounts that endpoint and its five tenant grants
  (`components/auth/routes.rs:43-68,70-189`), while the separately mounted
  platform router accepts the platform API-key RFC 8693 exchange only at
  `/auth/platform/token` (`components/platform/routes.rs:54-142`). There is no
  alias between them.
- **Observable consequence:** An operator can send a platform credential to
  the tenant plane and receive a refusal, obscuring the deliberate
  administration-plane boundary.
- **Decision-complete minimum correction:** Qualify the existing grant table as
  tenant-plane issuance and name `/auth/platform/token` as the existing
  platform credential-exchange route. Preserve both route owners; add no alias,
  fallback, or new abstraction.
- **Focused closure proof:** Compare the corrected sentence/table to both
  router builders and run `mise run docs:check`.

### FIND-TASK-005-4 — CONFIRMED — INCORRECT: three OAuth success contracts are collapsed into RFC 6749 section 5.1

- **Discovery sources:** `STD-TASK-005-1`, `SEC-002`.
- **Violated obligation:** REQ-021 and AC-009 require each public endpoint to
  describe its shipped standard wire contract.
- **Locations:** `architecture/wyrd-security-posture.md:208-212`,
  `docs/src/content/docs/self-hosting/sso-and-oidc.svx:139-152`, and
  `crates/wyrd/wyrd-server/src/auth/oauth.rs:3-9`.
- **Evidence:** The tenant and platform token handlers return `TokenResponse`
  (`components/auth/routes.rs:188-189`; `components/platform/routes.rs:124-131`).
  Device authorization returns the RFC 8628 `DeviceAuthorization` object
  (`auth/cli_login.rs:43-90`). Revocation returns `no_store(StatusCode::OK, ())`
  with no body (`auth/cli_login.rs:265-307`). The approved spec itself says
  only token success responses use RFC 6749 section 5.1.
- **Observable consequence:** A conventional OAuth client or operator can
  require a token-shaped JSON decoder for device authorization or revocation
  and reject the standard response Wyrd actually ships.
- **Decision-complete minimum correction:** Describe the already-shipped
  standard shapes separately: `/auth/token` and `/auth/platform/token` use the
  RFC 6749 section 5.1 token response; `/auth/device_authorization` uses the
  RFC 8628 section 3.2 response; `/auth/revoke` returns empty `200` per RFC 7009
  section 2.2. Preserve form encoding, RFC error JSON, no-store headers, and the
  current handlers. Correct the shared OAuth module rustdoc in the same edit;
  do not introduce a common envelope.
- **Focused closure proof:** Statically compare the corrected authority, guide,
  and rustdoc with the three handler return sites and served OpenAPI
  declarations. Run `mise run docs:check`, `mise run fmt`, and
  `mise run lints`. No journey or aggregate is required.

### FIND-TASK-005-5 — CONFIRMED — INCORRECT: the operator authentication guide denies the shipped BFF cookie

- **Discovery sources:** `MAINT-001`, `SEC-001`.
- **Violated obligation:** REQ-009, REQ-018, TASK-005's outcome, and AC-009
  require the BFF's encrypted cookie session and credential ownership to be
  documented accurately.
- **Location:** `docs/src/content/docs/self-hosting/authentication.svx:9-11`.
- **Evidence:** `BrowserSessions` writes an encrypted Secure, HttpOnly,
  SameSite=Lax cookie containing a refresh token or recovery API key and uses
  the resulting access token on Wyrd API requests
  (`browser-sessions.ts:23-34,98-177,189-387`). Login callback, hooks, layouts,
  recovery, and settings all consume this owner. The categorical statement
  “There is no session cookie” therefore contradicts both shipped behavior and
  the candidate's security and SSO pages.
- **Observable consequence:** Operators receive the wrong credential-custody
  and client-secret-rotation model and can miss that rotating the BFF secret
  invalidates active encrypted cookies.
- **Decision-complete minimum correction:** State the existing boundary: the
  Wyrd API server keeps no browser-session store or ambient request identity,
  while the web-app BFF owns the encrypted HttpOnly cookie and presents a Wyrd
  bearer token per server request. Preserve best-effort logout and access-token
  validity through expiry; add no server-side session mechanism.
- **Focused closure proof:** Compare the corrected operator statement to
  `BrowserSessions` and the existing sibling SSO/security text, then run
  `mise run docs:check`.

### FIND-TASK-005-6 — CONFIRMED — INCORRECT: activation documentation promises a provider-liveness check that activation does not perform

- **Discovery source:** `SYS-001`.
- **Violated obligation:** REQ-018 and AC-009 require connection lifecycle,
  failure, and recovery guidance to describe the deployed behavior.
- **Location:** `docs/src/content/docs/self-hosting/sso-and-oidc.svx:111-119`.
- **Evidence:** A completed test stamps one exact revision for 15 minutes
  (`connections.rs:428-506`). `HumanConnections::activate` takes the slot lock,
  checks that persisted stamp and the recovery API key, and promotes the
  candidate without discovery, JWKS, or token-endpoint IO
  (`connections.rs:509-584`; `human_connections.rs:304-335`). Thus an outage
  after testing does not cause activation to return
  `WYRD_AUTH_409_CONNECTION_NOT_TESTED` while the stamp remains current.
- **Observable consequence:** An operator can believe activation freshly
  proves provider availability, retire a working Active connection during a
  post-test outage, and make routine human login unavailable until recovery.
- **Decision-complete minimum correction:** Say that a missing, stale, or
  failed test stamp blocks activation; a provider outage blocks a new test and
  later login but is not re-probed during the 15-minute activation window.
  Preserve the exact-revision stamp and recovery-key guarantees; do not add a
  new activation probe, retry, or availability mechanism.
- **Focused closure proof:** Compare the corrected lifecycle text to
  `stamp_test_sign_in`, `activate`, and `human_candidate_test_is_current`, then
  run `mise run docs:check`.

## Validation limits

The fresh orchestrator verification supplied to this reviewer passed:
`mise run docs:check`, `mise run codegen:check`, `mise run ts:napi:check`,
`mise run fmt:check`, `mise run lints`, `mise run py:format:check`,
`mise run py:lints`, and `mise run py:typecheck`. Those lanes establish
rendering, generation, declaration, formatting, lint, and typing consistency;
they do not detect the semantic contradictions validated above. In accordance
with the task and standing direction, no journey suite or broad aggregate was
run or required. The proposed closure proofs likewise use only the narrowest
lanes covering each correction's write set.
