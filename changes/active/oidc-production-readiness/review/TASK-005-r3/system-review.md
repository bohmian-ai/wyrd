# TASK-005 R3 system-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior remediation tasks:
  - `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`
  - `changes/active/oidc-production-readiness/review/TASK-005-r2/TASK-005-R2-doc-contract-closure.md`

The requested base and candidate resolve to the stated commits. The repository
has no `.codegraph/` directory, so this review used the cumulative diff, the
R2-local `323ce3211..bb1e8e5ad` diff, and direct owner/caller inspection. The
cumulative write set changes architecture and operator documentation,
generated documentation and declarations, documentation generators, CLI help,
and Rust, PyO3, TypeScript, and SQL source documentation. Every changed line in
a compiled source file is a comment, rustdoc, or generated declaration; the
Python generator changes only emitted Markdown. No runtime expression, route,
database query, deployment manifest, timeout, retry, readiness condition,
process-lifecycle branch, or persistent-state transition changes. The
candidate therefore has no runtime or deployment effect of its own.

## Material proposed findings

None. The candidate introduces no behavioral, availability, recovery,
security, tenancy, durability, or public-contract defect within this review's
scope. The corrected operational claims agree with the deployed owners traced
below. No optional resilience mechanism or documentation preference is
proposed.

## Deployed-path evidence

| Deployed path | Runtime owner and topology | Candidate documentation assessment |
| --- | --- | --- |
| Tenant candidate test and activation | `HumanConnections::activate` takes the tenant connection-slot lock, requires a current exact-revision test stamp and an independent recovery credential, and retires/promotes in one PostgreSQL transaction with no provider IO (`crates/wyrd/wyrd-auth/src/connections.rs:509-584`). `active_connection` durably reads the Active row on every login (`connections.rs:642-673`). | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:111-127` accurately states that provider IO belongs to testing and login, not activation; a current 15-minute stamp survives a later outage, lifecycle commits are visible across replicas, and a retired revision cannot renew. This keeps prior `SYS-001` / `FIND-TASK-005-6` closed. |
| Tenant OAuth issuance and browser interactions | The tenant router separately mounts authorization, device authorization and page, callback, token, and revocation (`crates/wyrd/wyrd-server/src/components/auth/routes.rs:43-67`). `authorize` trusts the registered client/redirect before redirecting an RFC error and otherwise returns local HTML or Problem Details (`auth/authorize.rs:60-136`). Callback completion redirects the trusted client, returns static HTML for device/test completion, or returns Problem Details when no client redirect is safe (`components/auth/routes.rs:397-472`). | The design authority, generated API pages, agent guidance, and SSO guide now distinguish form-endpoint JSON from browser redirects, HTML, and declared Problem responses (`architecture/wyrd-design.md:568-582`; `docs/src/content/docs/self-hosting/sso-and-oidc.svx:139-154`). They no longer imply one failure envelope that could make clients retry or parse the wrong response. |
| OAuth form failures during store, audit, or dependency outage | `OAuthError::status` maps `invalid_client` to 401, `server_error` to 500, `temporarily_unavailable` to 503, and other protocol errors to 400; `From<WyrdError>` preserves the 503 retryable class, and `no_store` applies the cache headers (`crates/wyrd/wyrd-server/src/auth/oauth.rs:60-167`). Tenant token issuance commits the grant and audit together and emits no token on failure (`components/auth/routes.rs:70-105`). | The security posture, source rustdoc, SSO guide, and agent remediation guide now expose the reachable 400/401/500/503 protocol statuses and tell agents to back off for `temporarily_unavailable` but not retry `server_error` blindly (`architecture/wyrd-security-posture.md:209-218`; `crates/wyrd/wyrd-server/src/auth/oauth.rs:1-17`; `docs/src/content/docs/for-agents/error-remediation.svx:51-74`). No fallback or retry loop was added. |
| Platform credential exchange and federated login | `/auth/platform/token` directly presents the platform API-key subject to `PlatformSessions`; it has no OAuth client registration and returns an access-only `TokenResponse` (`crates/wyrd/wyrd-server/src/components/platform/routes.rs:54-153`). Federated platform login is independently mounted at `POST /auth/platform/login` and `POST /auth/platform/callback`, and completion also returns an access-only platform session (`components/platform/identity.rs:679-764`). | `docs/src/content/docs/concepts/identity-and-auth.svx:87-124`, `concepts/authentication.svx:44-72`, and `self-hosting/sso-and-oidc.svx:141-152` now keep platform exchange, outage behavior, and recovery separate from tenant `/auth/token` and tenant refresh. This closes the operational portion of `FIND-TASK-005-3` without adding an alias, shared client registry, or refresh authority. |
| BFF session across replica loss or restart | `BrowserSessions` encrypts the refresh token or recovery API key in a client-carried cookie using a key derived from the deployment client secret; access tokens are held in a bounded per-replica cache (`crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:108-187,249-276`). A cache miss or replica replacement renews from the cookie through Wyrd (`browser-sessions.ts:293-333`). | The cumulative docs accurately state that replicas share no server-side browser-session store, a client-secret rotation invalidates old cookies, and the next request repopulates a lost access cache. R2 does not disturb those claims. |
| Logout during revocation outage | BFF logout clears the cookie and local access cache before best-effort RFC 7009 revocation; failure is logged without token material and does not fail logout (`browser-sessions.ts:336-357`). Revocation ends renewal only; a self-contained access token remains valid until its bounded expiry. | `architecture/wyrd-security-posture.md:219-224`, `docs/src/content/docs/concepts/authentication.svx:165-184`, and `self-hosting/sso-and-oidc.svx:137` retain the approved local-logout, no-IdP-logout, and access-token-expiry boundaries. The candidate neither promises instant revocation nor invents retry state. |
| Confidential or public tenant IdP registration | `RelyingParty::redeem` passes a secret only for `SecretBasic`/`SecretPost`; `Public` redeems through the installed OIDC library without a client secret while retaining the recorded PKCE verifier (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:503-554`). Provider discovery, token exchange, and one JWKS refresh remain bounded by that owner. | `docs/src/content/docs/self-hosting/sso-and-oidc.svx:57-70` now documents the shipped confidential and public choices and the public client's lack of a sealing-key dependency. This closes `FIND-TASK-005-7` without adding a credential, fallback, or provider-specific branch. |

## Failure and recovery assessment

| Failure or interruption | What stops | What remains available and durable | Recovery and amplification assessment |
| --- | --- | --- | --- |
| Candidate provider outage before or during testing | The new test and logins through that unavailable provider cannot complete; no new test stamp is written. | The previous Active connection remains committed and can still serve login if it uses an available provider. Machine API-key/workload paths and headless recovery remain independent. A previously successful exact-revision stamp remains usable only until its database-clock expiry. | Restore the provider and test again when needed. Activation performs no probe or retry, so operators must confirm provider health before replacing a working connection; the guide says so explicitly. |
| Candidate provider outage after a successful test but before activation | New tests and logins through that provider fail, but activation itself can still run. | The old Active connection remains until an activation transaction commits; the Candidate and its current stamp survive in PostgreSQL. | Activation may atomically promote the stamped Candidate during the outage. The documentation exposes this tradeoff and preserves the independent recovery credential rather than promising a nonexistent liveness gate. |
| Server process crashes during activation | The request is interrupted. | PostgreSQL atomicity leaves either the prior Active connection or the fully promoted Candidate; the transaction cannot durably retire the old row without promoting the new row. | Retry against a healthy replica after reading current state. No deployment-local lease, probe, or retry loop can amplify the crash. |
| A BFF replica restarts or a request moves to another replica | The local access-token cache entry is absent. | The encrypted browser cookie and server-side refresh authority survive; unrelated tenants and machine paths remain available. | A replica configured with the same deployment client secret decrypts the cookie and renews through Wyrd. If Wyrd is unavailable, the affected request fails upstream; successful discovery is cached, while failed discovery is evicted and retried only on a later request (`browser-sessions.ts:128-163`). |
| Refresh/revocation store or audit outage | The affected token operation refuses with endpoint-native `temporarily_unavailable` or `server_error`; issuance produces no token. Best-effort logout may not revoke remotely. | Existing access tokens keep their bounded snapshot lifetime. Local logout already removes the cookie/cache; other sessions, tenants, machine grants, and non-auth capabilities remain available. | Clients apply the documented endpoint-specific handling. There is no process crash, common-envelope fallback, or background retry storm. |
| Platform store or audit outage during API-key exchange | The platform exchange request refuses with protocol-native `server_error`; it does not mint a session. | Tenant authentication and existing platform sessions remain separate; no tenant refresh or registered-client state is involved. | Restore the platform store/audit dependency and explicitly retry the exchange. The corrected docs no longer direct the caller through tenant `/auth/token` or imply a platform refresh path. |
| Web-app client-secret rotation | Old BFF cookies become undecryptable and affected people must sign in again. | Server token verification, tenant connection state, machine credentials, and operator recovery remain intact. | The deployment guide deliberately discloses reauthentication. A dual-key cookie scheme is neither required by the task nor a standard behavior to invent. |

The documented failure boundaries remain local to the affected request except
for the intentional fail-closed boot boundary when a stored sealed secret
cannot be opened. No corrected passage tells an operator to crash the shared
server for a request-local outage, fail over to another tenant or the platform
connection, or add a nonstandard availability mechanism.

## Affected capabilities and proof assessment

- Human connection testing, activation, replacement, login, refresh cutoff,
  and operator recovery were checked against `HumanConnections`, the callback
  path, and `TenantTokenIssuer`'s exact-revision issuance lock.
- Tenant and platform OAuth availability were checked against their distinct
  router owners, success response constructors, `OAuthError` mapping, and the
  platform access-only response.
- BFF restart, cross-replica session continuity, and interrupted logout were
  checked against `BrowserSessions`' cookie, bounded cache, renewal, and
  revocation order.
- Public-client provider registration was checked against
  `RelyingParty::redeem`; the corrected docs reuse the existing
  `openidconnect`/PKCE behavior and add no custom protocol machinery.

The task and both remediation records report successful narrow verification:
`mise run docs:check`, `mise run codegen:check`, `mise run fmt`, and `mise run
lints`; the original task also records the generated-language checks required
by its broader documentation write set. Those lanes establish rendering,
generator parity, formatting, and lint cleanliness. They do not independently
prove outage semantics; the source and caller traces above provide the
relevant static proof. Per the task and standing direction, no full journey,
live-provider, language, browser, or repository aggregate was run or required.

An independent cumulative `git diff --check` reports one extra blank line at
EOF in the prior review artifact
`changes/active/oidc-production-readiness/review/TASK-005-r1/verdict.md:99`.
That known condition has no shipped, recovery, or task-required documentation
consequence and is non-blocking under the standing direction; it is not a
system-resilience finding.

## Prior-finding closure

| Finding | Status | Evidence |
| --- | --- | --- |
| `SYS-001` / `FIND-TASK-005-6` | CLOSED | Activation is documented as a durable exact-revision stamp check with no provider re-probe, and provider outage effects are correctly split between test/login and activation (`sso-and-oidc.svx:111-127`; `connections.rs:509-584`). |
| `FIND-TASK-005-3` | CLOSED for system resilience | Tenant and platform routes, client identification, access-only platform sessions, and their independent recovery paths now match the router and response owners. |
| `FIND-TASK-005-4` | CLOSED for system resilience | Form endpoints and browser interactions now expose their actual failure envelopes and reachable statuses, so outage handling no longer assumes a false common Problem Details or OAuth-JSON response. |
| `FIND-TASK-005-7` | CLOSED for system resilience | Public IdP client setup now avoids an unnecessary secret/sealing-key dependency and uses the shipped PKCE path. |
| `FIND-TASK-005-8` | CLOSED; no resilience consequence | The Rust example now uses the existing `ClientConfig::credential`; it introduces no runtime path. |

## Overall result

**PASS**

The cumulative candidate has no executable or deployment-topology change, all
material documented outage and recovery claims inspected here match their
runtime owners, and prior system-resilience concerns are closed. The candidate
remained at `bb1e8e5ad4c5f8a8a26c3f0fc0fa527468355c21` through report
preparation.
