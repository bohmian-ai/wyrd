---
id: TASK-020
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 5
requirements: [REQ-001, REQ-003, REQ-016, REQ-027]
invariants: [INV-003, INV-004]
acceptance: [AC-020]
depends_on: []
---

# TASK-020 — Zero-configuration image login

## Outcome and Value

A person who pulls the official image and runs it with only its database and storage settings can sign in through the UI, whether they reach it at `http://localhost:8080` or at an `https://` host behind a proxy. Nobody generates a client secret, works out a public origin or sets a UI `ORIGIN`. This unblocks TASK-009's `authenticated_analytical_bff` image case.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| OAuth client identification | `wyrd-server/src/auth/oauth.rs::OAuthClients` | `cli_login.rs` device and revoke handlers, `components/auth/routes.rs` token handler, `oauth.rs` unit tests, `identity_e2e.rs` | `wyrd-ui` requires a configured secret | `wyrd-ui` identifies by `client_id`, like `wyrd-cli`; the secret, its config and the Basic path are deleted | None: deletion. `OAuthClients` holds no state any more, so it becomes two free functions. |
| Public origin | `wyrd-auth/src/connections.rs::HumanConnections::require_callback`, `config.rs` `public_origin` | `authorize.rs`, `cli_login.rs::device_decision`, `cli_logins.rs::authorize`, `login.rs::begin_bound`, admin identity routes | Required configuration; a missing value is a runtime refusal | The callback comes from the request origin unless `WYRD_PUBLIC_ORIGIN` pins it | One request-origin extractor in the server's auth module; no new config. |
| BFF session | `wyrd-ui/src/lib/server/auth/browser-sessions.ts::BrowserSessions` | `hooks.server.ts`, the login routes, `browser-sessions.test.ts`, `production-auth.integration.test.ts` | Discovery at the public origin, a client secret, and a cookie key derived from it | A locally built `openid-client` configuration over `WYRD_SERVER_URL` with `client.None()`; the cookie holds the server-validated credential | None: the existing library and cookie API. |
| Image | `docker/official/extras/entrypoint.sh`, `nginx.conf.template` | `scripts/server/test-startup-prod.sh` | The BFF assumes `https`; a TLS proxy's forwarded scheme is overwritten | `PROTOCOL_HEADER=x-forwarded-proto`; nginx forwards an incoming `X-Forwarded-Proto`, otherwise its own scheme | None. |

Prohibited: a generated or persisted UI key, an image-generated secret, another session store, compatibility for the removed secret settings, and server-side browser sessions.

## Ordered Implementation Scenarios

### Scenario 1 — `wyrd-ui` is a public client

**Behavior.** `client_id=wyrd-ui` identifies the UI on token, device and revoke requests; an `Authorization: Basic` request is `invalid_client`. Metadata advertises auth method `none`.
**RED.** `oauth.rs` unit test `clients_identify_by_client_id`.
**GREEN.** Delete the secret registration; identify both clients by `client_id`.
**REFACTOR.** Remove the config, boot, test-builder and OpenAPI secret surfaces.

### Scenario 2 — The request supplies the public origin

**Behavior.** Without a pin, `/auth/authorize` accepts only `{scheme}://{Host}/login/callback` and sends the provider `{origin}/auth/callback`; another origin's redirect URI is refused; a pinned origin wins.
**RED.** `authorize.rs` pg tests `authorize_derives_the_origin_from_the_request` and `a_pinned_origin_wins`.
**GREEN.** A request-origin extractor; `HumanConnections` takes the request origin where it needs the callback.
**REFACTOR.** Delete `public_origin_missing`.

### Scenario 3 — The BFF signs in without a secret or discovery

**Behavior.** API-key and SSO sign-in, renewal and logout work through `WYRD_SERVER_URL` with no `WYRD_UI_CLIENT_SECRET`; the session cookie is `Secure` only over `https`.
**RED.** `browser-sessions.test.ts` without the secret env.
**GREEN.** Rebuild `BrowserSessions` on a local configuration and plain cookies.
**REFACTOR.** Remove the secret from the journey hosts.

### Scenario 4 — The official image signs in with no settings

**Behavior.** `test-startup-prod.sh` case `authenticated_analytical_bff` signs in over plain `http` with no public origin or secret.
**RED.** The case as it stands fails sign-in.
**GREEN.** Entrypoint `PROTOCOL_HEADER`; nginx forwarded-scheme pass-through.

## Verification and Evidence

- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=auth::oauth::tests::clients_identify_by_client_id)'`
- `mise run test:identity:journey` with `WYRD_IDENTITY_TARGET=ui` (SSO and API-key browser login without a secret)
- `WYRD_STARTUP_FILTER=authenticated_analytical_bff scripts/server/test-startup-prod.sh`
- `pnpm run check` and `vitest run src/lib/server/auth/browser-sessions.test.ts`

## Authority Links

- `changes/active/wyrd-ui-live-data/spec.md` revision 5 (REQ-027, AC-020, decision 9)
- `changes/completed/2026/oidc-production-readiness.md` (superseded confidential-client decision)
