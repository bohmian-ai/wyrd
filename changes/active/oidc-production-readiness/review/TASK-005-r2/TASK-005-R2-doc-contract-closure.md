---
id: TASK-005-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-004, REQ-005, REQ-018, REQ-021, INV-003, AC-009]
depends_on: []
parent_task: TASK-005
remediates: [FIND-TASK-005-3, FIND-TASK-005-4, FIND-TASK-005-7, FIND-TASK-005-8]
---

# Close remaining OAuth and client documentation contracts

## Immutable review inputs

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Review: `changes/active/oidc-production-readiness/review/TASK-005-r2/`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Reviewed candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Validated findings: `FIND-TASK-005-3`, `FIND-TASK-005-4`, `FIND-TASK-005-7`, `FIND-TASK-005-8`

## Outcome

Every task-owned authority, operator guide, generated API page, agent guide,
source rustdoc, and Rust client example describes the already-shipped standard
OAuth/OIDC system exactly: endpoint-specific wire shapes, separate tenant and
platform credential planes, supported public or confidential tenant IdP
clients, and the shared `ClientConfig` API. This remediation changes
documentation and source documentation only.

## Issue diagnoses and required corrections

### FIND-TASK-005-3 — tenant and platform planes remain collapsed in sibling documentation

The cross-plane identity guide says machine-facing issuance and human login end
at tenant `/auth/token`, and it describes only tenant refresh behavior.
Production separately mounts platform API-key exchange at
`/auth/platform/token`; platform OIDC completes through
`/auth/platform/callback` with an access-only response. The OAuth module
rustdoc compounds the same error by saying all four form endpoints identify a
registered client through `OAuthClients`, even though the platform handler has
no such input and exchanges its presented API-key subject through
`PlatformSessions`.

This can send a platform operator to the tenant endpoint, imply nonexistent
platform refresh authority, or lead a maintainer to add tenant client
authentication to the platform exchange. Correct the descriptions at their
current owners. Scope `/auth/token`, grant fan-out, and refresh statements to
the tenant plane; name the existing platform token and OIDC completion paths.
In the shared OAuth rustdoc, distinguish the common form/error/cache wire from
client identification: only the tenant token, device-authorization, and
revocation endpoints identify a registered OAuth client, while the platform
endpoint authenticates its presented platform subject credential directly.

Preserve the two routers, the access-only platform session, and RFC 8693 API-key
exchange with `urn:wyrd:oauth:token-type:api_key`. Do not add an alias,
fallback, registered platform client, refresh path, or shared runtime
abstraction.

### FIND-TASK-005-4 — endpoint-specific OAuth wire is still overgeneralized

The active design authority says the OAuth endpoints use form-plus-JSON without
limiting that claim to the four POST form endpoints. The generated/global API
and agent guidance says every other refusal is Problem Details. The SSO guide
states that form refusals use only `400`, or `401` for `invalid_client`.

The shipped standard contracts are distinct. `GET /auth/authorize` accepts a
query and refuses through an RFC 6749 section 4.1.2.1 redirect after the client
and redirect are trusted, or local HTML before they are trusted. The browser
device-verification and callback interactions likewise have declared
redirect/HTML/Problem outcomes. The four form endpoints use OAuth error JSON;
their successes retain their own standard shapes, including empty revocation
success. `OAuthError::status` also exposes reachable `500 server_error` and
`503 temporarily_unavailable` responses.

Use the existing handlers and served OpenAPI as the sole taxonomy. Narrow the
design authority to the four form endpoints and describe browser authorization
separately. Update `docs/scripts/generate_api_docs.py` first, regenerate its
pages, and align the agent guide so global Problem Details guidance does not
promise one envelope for browser-facing OAuth interactions. Extend the SSO
status description to the already-shipped `500` and `503` cases. Do not add a
common envelope, error identity, status, route, wrapper, logger, or
compatibility mechanism.

### FIND-TASK-005-7 — generic IdP setup incorrectly requires a confidential client

The generic provider procedure requires an OIDC confidential client and then
offers `Public` four lines later. The shipped `HumanClientAuth` accepts
`SecretBasic`, `SecretPost`, or `Public`; validation requires no secret for
`Public`, and the relying-party library redeems that choice with PKCE and no
client secret. An operator can reject a supported registration or create and
seal an unnecessary secret.

Describe the generic registration as an OIDC application using authorization
code with PKCE and the exact Wyrd callback. Distinguish the existing
confidential (`SecretBasic`/`SecretPost`) and public (`Public`, no secret)
token-endpoint authentication choices. Provider examples may keep their
conventional settings. Preserve the typed owner, relying-party behavior, and
sealing rules; add no authentication method or compatibility path.

### FIND-TASK-005-8 — Rust client example uses a nonexistent field

The programmatic credential example assigns `config.api_key`, but the shared
`ClientConfig` exposes `credential: Option<SecretString>` and no `api_key`
field. The Rust SDK re-exports the shared type unchanged, so the documented
example cannot compile.

Change the example to assign the existing `ClientConfig::credential` field.
Preserve explicit-credential precedence and shared-client ownership. Do not add
an alias, helper, field, or compatibility surface.

## Constraints and preserved behavior

- Use standard OAuth 2.0/OIDC terminology and the existing vetted libraries.
- Preserve RFC 8693 API-key exchange with
  `urn:wyrd:oauth:token-type:api_key`.
- Preserve ingress-owned rate limiting for `POST /auth/device`.
- Preserve best-effort RFC 7009 logout revocation.
- Preserve origin-normalized client base URLs and saved-login selection.
- Preserve self-contained access-token validity until expiry after logout.
- Preserve endpoint handlers, status mapping, route ownership, client types,
  platform access-only sessions, credential precedence, and generated-artifact
  ownership.
- Change no runtime behavior, public API, dependency, endpoint, storage,
  configuration, retry, logging mechanism, or compatibility path.
- Do not reopen placement, naming, structure, phrasing, or whitespace-only
  preferences. The prior R1 verdict's extra EOF blank line is not a required
  correction.

## Non-goals

- Redesigning OAuth/OIDC flows or adding a common response envelope.
- Adding platform OAuth client registration or platform refresh tokens.
- Removing the supported public tenant IdP client mode.
- Adding a `ClientConfig::api_key` compatibility alias.
- Changing provider examples beyond what is necessary to avoid contradicting
  the generic supported contract.
- Running user-journey, language, live-provider, browser, or repository-wide
  aggregate suites.

## Acceptance criteria

1. `FIND-TASK-005-3`: the cross-plane concept and OAuth module rustdoc
   distinguish tenant `/auth/token` and registered-client identification from
   platform `/auth/platform/token`, direct platform credential exchange, and
   access-only platform OIDC completion.
2. `FIND-TASK-005-4`: design authority, generated/global API guidance, agent
   guidance, and the SSO status paragraph describe the shipped endpoint-specific
   query/form, redirect/HTML/JSON/Problem, success, and `400`/`401`/`500`/`503`
   contracts without inventing a common envelope.
3. `FIND-TASK-005-7`: generic provider setup permits the shipped confidential
   and public client-authentication choices and states their actual secret
   requirements.
4. `FIND-TASK-005-8`: the Rust example uses `ClientConfig::credential` and no
   nonexistent or compatibility field.
5. Generated API pages are regenerated from `generate_api_docs.py`, and no
   runtime behavior or closed decision changes.

## Focused proof and verification

Before running lanes, statically compare each correction to its existing owner:

- tenant and platform router builders, token handlers, `PlatformSessions`, and
  platform OIDC completion;
- `authorize`, device-page/decision, callback, `OAuthError::status`, and served
  response declarations;
- `HumanClientAuth::validate` and `RelyingParty::redeem`; and
- `wyrd_client::config::ClientConfig` plus the Rust SDK re-export.

Run only the lanes covering this documentation, generator, generated-page, and
Rust-rustdoc write set:

```bash
mise run docs:check
mise run codegen:check
mise run fmt
mise run lints
```

Do not run or require full journey suites, language suites, live-provider
tests, browser suites, or repository aggregates.

## Completion evidence

Record the corrected source locations, the static owner comparison for every
finding, generated-page parity, and the exit result of each focused command.
Route the completed remediation directly to `$wyrd-implement`, then reassess
the complete original base-to-new-candidate range with `$wyrd-task-review`.

## Implementation Evidence

Commit `7ceaf8ece`. Documentation, the API-doc generator with its regenerated pages, and one module rustdoc. No runtime change.

| Finding | Corrected locations | Static owner comparison | Result |
|---|---|---|---|
| FIND-TASK-005-3 | `concepts/identity-and-auth.svx` (dispatch scoped to the tenant plane; platform `/auth/platform/token`, `/auth/platform/login` → `/auth/platform/callback`, access-only); `concepts/authentication.svx` (grant table platform rows, tenant-only refresh); `auth/oauth.rs` module rustdoc (only tenant token, device authorization and revoke use `OAuthClients`) | `platform_auth_router`/`platform_token` exchange the API-key subject via `PlatformSessions`; `platform_session_response` sets `refresh_token: None`; `complete_login` returns `TokenResponse` via Problem Details errors; `oauth_clients` is called only in `components/auth/routes.rs` and `auth/cli_login.rs` | PASS |
| FIND-TASK-005-4 | `wyrd-design.md` (four form endpoints vs browser interactions); `generate_api_docs.py` and regenerated `api/openapi.md` and `api/errors.md`; `for-agents/error-remediation.svx` (JSON API vs browser endpoints; 400/401/500/503); `sso-and-oidc.svx` status paragraph plus browser-endpoint paragraph; `wyrd-security-posture.md` statuses; `concepts/authentication.svx` form vs browser | `OAuthError::status` (401/500/503/400); `authorize` utoipa responses (303 redirect, 400 HTML or Problem Details, 500); device page and `/auth/callback` declarations (HTML, 303, Problem Details); revoke 200 has no body | PASS |
| FIND-TASK-005-7 | `sso-and-oidc.svx` Registering Wyrd steps 1 and 5 | `HumanClientAuth` is `SecretBasic`/`SecretPost`/`Public`; secret methods require `client_secret` and sealing, and `Public` takes none and uses PKCE | PASS |
| FIND-TASK-005-8 | `get-started/client-configuration.svx` Rust example | `wyrd_client::config::ClientConfig::credential: Option<SecretString>`; the SDK re-exports it | PASS |
| Same-class sweep | Same-class claims qualified across the touched files: "only a human login gets a refresh token" (security posture, design, self-hosting authentication), "every request carries a token" (identity-and-auth, self-hosting authentication), "every token endpoint is form/§5.2" (concepts/authentication) | Every file changed since base was grepped for blanket refusal, refresh, route and envelope claims | PASS |

Generated-page parity: `mise run docs:generate`, then `docs:check` passes against the committed pages.
Verification (all exit 0): `mise run docs:check`, `mise run codegen:check`, `mise run fmt`, `mise run lints`, `git diff --check`.
