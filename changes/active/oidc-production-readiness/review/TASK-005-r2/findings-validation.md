# TASK-005 R2 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-005-r1/`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`

The candidate remained checked out at the stated commit before and after source
inspection. The repository has no `.codegraph/` directory, so validation used
the complete cumulative and remediation diffs, `rg`, and direct reads of each
cited producer, caller, sibling consumer, and documentation owner. All required
R2 discovery reports and the focused follow-up report were present and read.

The standing direction controls this ledger: corrections use the existing
standard OAuth 2.0/OIDC behavior and installed libraries, without a new
protocol, endpoint, client type, compatibility path, or runtime mechanism.
Documentation blocks only where it is false, misleading, or omits a task-owned
contract. Placement, naming, structure, phrasing, and whitespace preferences
are excluded. The locked RFC 8693 API-key exchange, ingress-owned device-page
rate limiting, best-effort RFC 7009 revocation, origin-normalized base URL, and
access-token lifetime through expiry remain closed.

## Source and reachability trace

| Boundary | Producer and sibling writers | Reachable consumers | Existing owner to preserve |
| --- | --- | --- | --- |
| Endpoint-specific OAuth wire | `authorize` accepts a query and returns a client redirect or local HTML/Problem response; the device verification page and common callback likewise have browser HTML/redirect outcomes; `token`, `platform_token`, `device_authorization`, and `revoke` accept `OAuthForm`; `OAuthError::status` selects `400`, `401`, `500`, or `503` (`auth/authorize.rs:31-137`, `auth/cli_login.rs:125-243,265-307`, `components/auth/routes.rs:70-189,396-488`, `components/platform/routes.rs:63-143`, `auth/oauth.rs:57-76`) | The active design authority, API-doc generator and generated pages, agent remediation guide, and SSO operator guide publish the wire contract | Preserve the handlers, `OAuthForm`, `OAuthError`, served OpenAPI, and standard endpoint-specific response shapes; correct only their documentation |
| Tenant/platform plane split | Tenant grants dispatch through `/auth/token` after `OAuthClients` identification; platform API-key exchange is mounted separately at `/auth/platform/token` and directly uses `PlatformSessions`; platform OIDC completes at `/auth/platform/callback` with an access-only session (`components/auth/routes.rs:55-188`, `components/platform/routes.rs:54-153`, `components/platform/identity.rs:714-764`) | The cross-plane identity concept page and shared OAuth module rustdoc currently generalize the tenant route/client model across the platform plane | Preserve both route owners, the RFC 8693 API-key subject token, and the access-only platform session; add no alias or registered-client requirement |
| Tenant IdP client authentication | `HumanClientAuth` accepts `SecretBasic`, `SecretPost`, or `Public`; validation requires no secret for `Public`; `RelyingParty::redeem` passes no secret for that variant and still uses PKCE (`wyrd-spec/src/auth/human_connection.rs:70-113,232-264`, `wyrd-auth-oidc/src/relying_party.rs:503-535`) | The generic IdP setup procedure is used by operators before creating and testing a tenant connection | Preserve the typed client-auth choices and relying-party implementation; describe the choice instead of requiring a confidential client |
| Shared Rust client configuration | `ClientConfig` exposes `credential: Option<SecretString>` and no `api_key` field; `wyrd-sdk-rust` re-exports the shared client unchanged (`wyrd-client/src/config.rs:32-61`, `wyrd-sdk-rust/src/lib.rs:27`) | The programmatic override example is intended to compile for Rust SDK users | Preserve the existing public field and credential precedence; correct the example rather than adding an alias |

## Validation of proposed findings

| Proposal | Result | Independent validation |
| --- | --- | --- |
| `BHV-R2-001` | **REVISED** → `FIND-TASK-005-3` | The concept page is explicitly cross-plane yet routes all machine and human issuance through the tenant endpoint and describes only tenant refresh behavior. This is the same platform/tenant route contract as the prior finding, still false in a sibling public consumer. The correction also covers the platform OIDC access-only completion already shipped. |
| `SEC-R2-001` | **REVISED**, duplicate of `BHV-R2-001` → `FIND-TASK-005-3` | The OAuth module rustdoc adds the same collapsed-plane error: `/auth/platform/token` uses `OAuthForm` but does not identify a registered client through `OAuthClients`. Both consumers belong to one correction boundary. |
| `MAINT-R2-001` | **REVISED** → `FIND-TASK-005-4` | `wyrd-design.md` incorrectly assigns form-plus-JSON behavior to the whole OAuth surface, including the query/redirect authorization endpoint and empty revocation success. The earlier finding's named locations were repaired, but this sibling authority keeps the endpoint-specific contract incomplete. |
| `STD-TASK-005-R2-1` | **REVISED**, consolidated into `FIND-TASK-005-4` | The global Problem Details statements exclude only the four form endpoints, while reachable `/auth/authorize` refusals can be RFC 6749 redirects or local HTML. Sibling browser interactions at `/auth/device` and `/auth/callback` also publish HTML/redirect outcomes. The generator is the source owner for both generated pages. |
| `STD-TASK-005-R2-2` | **REVISED**, consolidated into `FIND-TASK-005-4` | The SSO guide positively limits form-endpoint refusals to `400` and `401`, while `OAuthError::status` and the served route declarations expose reachable `500 server_error` and `503 temporarily_unavailable`. This is another consumer of the same endpoint-shape taxonomy, not a separate runtime defect. |
| `BHV-R2-002` | **CONFIRMED** → `FIND-TASK-005-7` | The generic setup requires a confidential client and then offers the shipped `Public` choice four lines later. `HumanClientAuth::Public` is accepted, requires no secret, and is redeemed through the existing relying-party library with PKCE. The contradiction can cause an operator to reject a supported registration or create an unnecessary secret. |
| `INV-R2-001` | **CONFIRMED** → `FIND-TASK-005-8` | The Rust example assigns `config.api_key`, but the sole shared public type exposes `credential`; the Rust SDK blanket-re-exports that exact type. The example cannot compile and therefore misstates the shipped API. |
| `STD-TASK-005-R2-3` | **REJECTED** | `git diff --check` does report one extra blank line at EOF in the prior R1 verdict, so the recorded result is literally inaccurate. The underlying condition is whitespace in a review artifact, changes no shipped behavior or required task documentation, and is expressly non-blocking under the standing direction. It is not retained as a finding or remediation requirement. |

The follow-up consolidation is accepted with stable prior IDs retained: the
endpoint-shape proposals share one correction boundary under
`FIND-TASK-005-4`, and the tenant/platform proposals share one boundary under
`FIND-TASK-005-3`. No additional proposed finding was needed.

## Prior-finding closure

| Prior finding | Source-backed status |
| --- | --- |
| `FIND-TASK-005-1` | **CLOSED.** The four form endpoints are now distinguished from Problem Details, extractor-native errors are not promised a Wyrd identity, and only `WyrdError` conversion promises a Wyrd-code log field. The newly validated authorization-endpoint omission is retained under the broader endpoint-shape `FIND-TASK-005-4`, not used to reopen the corrected form-error identity finding. |
| `FIND-TASK-005-2` | **CLOSED.** Trusted issuers are documented as workload-only, Human setup is routed to tenant connections, and secret-bearing workload issuers retain the sealing-key requirement. |
| `FIND-TASK-005-3` | **NOT CLOSED; REVISED BELOW.** The operator authentication page is corrected, but the cross-plane concept page and OAuth rustdoc still collapse the separate route and client-identification contracts. |
| `FIND-TASK-005-4` | **NOT CLOSED; REVISED BELOW.** The security posture, SSO success-shape text, and OAuth rustdoc now distinguish token, device-authorization, and revocation successes, but the active design authority and sibling public error/status guidance still generalize endpoint-specific wire behavior. |
| `FIND-TASK-005-5` | **CLOSED.** The operator guide now distinguishes the stateless Wyrd API request boundary from the encrypted HttpOnly cookie owned by the BFF and preserves the locked logout/access-token behavior. |
| `FIND-TASK-005-6` | **CLOSED.** Activation guidance now describes the exact-revision 15-minute test stamp and correctly states that activation performs no provider IO. |

## Final deduplicated finding ledger

### FIND-TASK-005-3 — REVISED — INCORRECT: sibling docs still collapse tenant and platform authentication planes

- **Discovery sources:** `BHV-R2-001`, `SEC-R2-001`.
- **Violated obligation:** REQ-018, REQ-021, INV-003, TASK-005 Approach 3,
  remediation acceptance criterion 3, and AC-009 require tenant and platform
  routes, credentials, and session behavior to remain distinct and accurately
  documented.
- **Locations:**
  - `docs/src/content/docs/concepts/identity-and-auth.svx:3,17-20,86-115`
  - `crates/wyrd/wyrd-server/src/auth/oauth.rs:3-10`
- **Evidence:** The concept page presents an end-to-end map across both planes
  but says all machine-facing issuance and all human login end at
  `/auth/token`. Production separately mounts the platform API-key exchange at
  `/auth/platform/token`; platform OIDC completes at `/auth/platform/callback`
  with an access-only `TokenResponse`. The shared rustdoc says all four form
  endpoints identify a client through `OAuthClients`, but `platform_token`
  accepts no such dependency and directly exchanges the presented platform
  API-key subject through `PlatformSessions`.
- **Observable consequence:** A platform operator can send a platform
  credential to the tenant endpoint, expect tenant refresh behavior from a
  platform federated login, or infer that the platform exchange requires a
  `wyrd-ui`/`wyrd-cli` registration. Each conflicts with the shipped security
  plane and fails or misrepresents credential custody.
- **Decision-complete minimum correction:** At the existing concept-page
  owner, scope `/auth/token`, its grant fan-out, and refresh statements to the
  tenant plane; name `/auth/platform/token` for platform API-key exchange and
  the existing access-only platform OIDC completion. In the OAuth module
  rustdoc, state that the four endpoints share form/error/cache wire behavior,
  while only the tenant token, device-authorization, and revocation endpoints
  identify a registered OAuth client; platform token exchange authenticates
  its presented platform subject credential directly. Preserve both routers,
  RFC 8693 with `urn:wyrd:oauth:token-type:api_key`, and the existing platform
  session. Add no alias, fallback, client registration, refresh path, or shared
  runtime abstraction.
- **Focused closure proof:** Compare the corrected descriptions to
  `auth_router`, `token`, `platform_auth_router`, `platform_token`, and
  `complete_login`. Run `mise run docs:check`, `mise run fmt`, and
  `mise run lints`. No journey or aggregate is required.

### FIND-TASK-005-4 — REVISED — INCORRECT: public authorities still generalize endpoint-specific OAuth wire behavior

- **Discovery sources:** `MAINT-R2-001`, `STD-TASK-005-R2-1`,
  `STD-TASK-005-R2-2`.
- **Violated obligation:** REQ-021, TASK-005 Outcome and Approach 3,
  remediation acceptance criterion 4, and AC-009 require the documented OAuth
  surface to match each shipped standard request, success, refusal, and status
  contract.
- **Locations:**
  - `architecture/wyrd-design.md:570-573`
  - `docs/scripts/generate_api_docs.py:60-66,123-125` and generated
    `docs/src/content/docs/api/openapi.md:26-32`,
    `docs/src/content/docs/api/errors.md:11-13`
  - `docs/src/content/docs/for-agents/error-remediation.svx:13-17`
  - `docs/src/content/docs/self-hosting/sso-and-oidc.svx:152`
- **Evidence:** The design authority says the OAuth endpoints use a form/JSON
  wire without limiting that claim to the four POST form endpoints.
  `GET /auth/authorize` instead accepts query parameters and refuses through a
  validated client's RFC 6749 section 4.1.2.1 redirect or through local HTML
  before the client/redirect is trusted; its served OpenAPI also exposes
  Problem responses for configuration failures. The sibling device
  verification and callback routes likewise expose browser HTML and redirect
  outcomes alongside their Problem responses. The generator and agent guide
  claim every other error is Problem Details, omitting those reachable browser
  outcomes. Separately, the SSO guide says form refusals use only `400` or
  `401`, while `OAuthError::status` and route declarations expose reachable
  `500 server_error` and `503 temporarily_unavailable`.
- **Observable consequence:** A client or operator can select the wrong parser
  for an authorization refusal, miss the redirect-carried error, expect JSON
  from a browser page or empty revocation success, or treat a shipped
  dependency/audit failure status as outside the documented OAuth contract.
  Because `wyrd-design.md` is active authority, the same generalization can
  also steer later maintenance back toward a nonstandard common envelope.
- **Decision-complete minimum correction:** Use the endpoint taxonomy already
  owned by the handlers. Narrow the design authority to the four form
  endpoints, retain each success's existing standard shape, and describe the
  browser authorization endpoint separately as query plus standard redirect or
  local HTML before trust. Update `generate_api_docs.py` first and regenerate
  its pages so global Problem Details guidance directs browser-facing OAuth
  authorization, device-verification, and callback interactions to their
  declared redirect/HTML/Problem outcomes instead of promising one envelope;
  align the agent guide with that same split. Extend the SSO form-endpoint
  status sentence to the existing `500 server_error` and
  `503 temporarily_unavailable` cases. Do not add an envelope, error identity,
  status, route, wrapper, or compatibility mechanism.
- **Focused closure proof:** Statically compare the corrected text with
  `authorize`, `device_page`, `device_decision`, `callback`,
  `OAuthError::status`, and the existing OpenAPI response declarations for
  token, platform token, device authorization, and revocation.
  Run `mise run docs:check` and `mise run codegen:check`. This correction does
  not require Rust source changes, a journey, or an aggregate.

### FIND-TASK-005-7 — CONFIRMED — INCORRECT: generic IdP setup requires a confidential client although Public is supported

- **Discovery source:** `BHV-R2-002`.
- **Violated obligation:** REQ-004, REQ-005, REQ-018, TASK-005 Approach 2,
  and AC-009 require setup inputs and sealing needs to match the selected
  supported client-authentication method.
- **Location:** `docs/src/content/docs/self-hosting/sso-and-oidc.svx:57-65`.
- **Evidence:** The generic procedure says every provider needs an OIDC
  confidential client, then instructs the operator to choose `Public` for a
  client without a secret. `HumanClientAuth::Public` is an accepted variant,
  validation requires its secret to be absent, and `RelyingParty::redeem` uses
  the installed OIDC library with no client secret while retaining PKCE.
- **Observable consequence:** An operator can conclude that a supported public
  registration is invalid or create and seal an unnecessary shared secret.
- **Decision-complete minimum correction:** Describe the provider registration
  as an OIDC application using authorization code with PKCE and the exact Wyrd
  callback, then distinguish the existing confidential
  (`SecretBasic`/`SecretPost`) and public (`Public`, no secret) token-endpoint
  authentication choices. Keep provider-specific examples limited to their
  conventional settings. Preserve `HumanClientAuth`, the relying-party owner,
  and current sealing behavior; add no auth method or compatibility path.
- **Focused closure proof:** Compare the corrected generic procedure to
  `HumanClientAuth::validate` and `RelyingParty::redeem`, then run
  `mise run docs:check`. No runtime or journey lane is required.

### FIND-TASK-005-8 — CONFIRMED — INCORRECT: the Rust client example assigns a nonexistent public field

- **Discovery source:** `INV-R2-001`.
- **Violated obligation:** REQ-018, TASK-005's documentation-accuracy outcome,
  FIND-TASK-004-8 closure, and AC-009 require SDK configuration documentation
  to use the shipped shared-client API.
- **Location:** `docs/src/content/docs/get-started/client-configuration.svx:88-99`,
  specifically line 97.
- **Evidence:** The example assigns `config.api_key`. The sole shared
  `ClientConfig` exposes `credential: Option<SecretString>` and no `api_key`
  field, the Rust SDK re-exports that type unchanged, and the same page's
  precedence list correctly names `ClientConfig::credential`.
- **Observable consequence:** A Rust SDK user following the documented
  programmatic override receives a compile error before constructing a client.
- **Decision-complete minimum correction:** Change the example to assign the
  existing `ClientConfig::credential` field. Preserve its credential
  precedence and shared-client ownership; do not add an alias, helper, field,
  or compatibility surface.
- **Focused closure proof:** Check the corrected example field against
  `wyrd_client::config::ClientConfig` and the Rust SDK re-export, then run
  `mise run docs:check`. No new test harness, journey, or aggregate is required.

## Non-blocking note

The immutable range's `git diff --check` exits `2` for one extra blank line at
EOF in `changes/active/oidc-production-readiness/review/TASK-005-r1/verdict.md`,
although the task evidence records exit `0`. This is a factual evidence
discrepancy, but its sole underlying condition is whitespace in a prior review
artifact. It has no behavioral, security, tenancy, durability, or public
contract consequence and is excluded from the material ledger by the standing
direction. It may be cleaned up with the next artifact edit, but it is not a
required remediation outcome.

## Validation completeness and verification limits

Every discovery proposal, including unique claims, was checked against the
candidate source and its reachable owners. The final ledger contains four
material correction boundaries: two continuing prior findings and two new
findings. No retained correction requires a product, public API, architecture,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision; all are bounded reconciliation of documentation with already-shipped
standard behavior.

The review packet records successful `mise run docs:check`, `mise run
codegen:check`, `mise run fmt`, and `mise run lints` results for the remediation
write set. Those lanes establish rendering, generated-page parity, and Rust
source-doc formatting/linting, but they cannot prove the semantic mismatches in
this ledger. The focused closure proofs above are the narrowest lanes for the
specified documentation owners. No full journey suite, language suite, broad
aggregate, live IdP, database, or browser run was used or required.
