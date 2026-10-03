# TASK-012 Round 2 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Prior maintainer report: `changes/active/oidc-production-readiness/review/TASK-012-r1/maintainer-review.md`

The candidate identity was checked before source inspection and again before
this report was written; it remained unchanged.

## Changed-surface coverage

| Changed surface | Owners, callers, and relevant proof | Maintainer assessment |
|---|---|---|
| Workspace, `wyrd-client`, CLI, workspace-hack manifests, and lockfile | TASK-012's exact `oauth2 = 5.0.0` and `webbrowser = 1.2.4` decisions; `TokenExchange`, `LoginFlow`, and their compiled consumers | Dependencies stay at the narrow owners, `oauth2` keeps default features disabled, and the remediation adds no dependency, feature, setting, or compatibility path. |
| `wyrd-client/src/auth.rs`: `AuthHttp`, `TokenExchange`, retained form operations, and auth middleware integration | `LoginFlow`, CLI refresh/logout, `SavedLoginSource`, `HumanSso`, `Platform`, `AuthMiddleware`, delegation, workload assertion, and auth tests | `TokenExchange` remains the cohesive dependency-owning auth owner. Device and refresh have one `oauth2` path. The only hand form operations left are RFC 8693, RFC 7523, and the locked RFC 7009 exception, all through the same redirect-free adapter. The public arbitrary-grant operation is deleted. |
| `wyrd-client/src/transport/config.rs`: `HttpConfig::validate` and `TransportConfig::validate` | `TokenExchange::new`, `HttpTransport::new`, `canonical_origin`, CLI client validation, direct client/platform construction, transport tests | The existing `HttpsOrigin::of_url` authority now produces the normalized, userinfo-free origin. The owner uses the standard URL parser and deletes the local loopback/origin parsing duplication. No new parser, redaction layer, allowlist, or option was introduced. |
| `wyrd-client/src/transport/http.rs`: stored origin, constructor, same-origin request resolution | `WyrdClient`, `Platform`, MCP authenticated replay, cards/storage/Bifrost callers, transport integration tests | The transport stores the domain value returned by `HttpConfig`, joins relative paths to it, and reuses the same origin authority for absolute URLs. The former `same_origin`/`split_origin` string parser is deleted. The owner and method shape remain direct and discoverable. |
| `wyrd-client/src/saved_login.rs`: `canonical_origin` and locked renewal | `ClientConfig::resolve_credential`, CLI login/logout, all three SDKs through shared auth, saved-login tests | Canonicalization delegates to `HttpConfig` instead of maintaining a second URL decision. Lock, reread, rotation, atomic replacement, selection, and local-first logout ownership are unchanged. |
| `wyrd-cli/src/auth/login.rs`, `auth/refresh.rs`, and CLI journey | `LoginFlow`, status/logout/refresh commands, compiled `wyrd` binary | `LoginFlow` still owns the interactive workflow. `webbrowser::open`, the printed fallback, `--no-browser`, cancellation, and saved-login behavior remain visible at the call site. The journey no longer preserves a production raw device-poll bypass; replay, denial, and expiry use the standard client path, while server-only single-poll outcomes remain with the server journey. |
| `wyrd-testing/src/human_login.rs` and shared-client transport/auth fixtures | Real identity journeys and focused owner tests | The helper reuses production OAuth operations rather than implementing another poller. The token fixtures use the RFC 6749 response shape. Changed helpers and async operations now document their role and uncertain-completion behavior. |
| Original task and round-1 artifacts | Seven exact selectors plus the remediation's focused CLI and static/boundary evidence | Evidence is recorded at the owning task instead of adding a script, lane, check, or evidence mechanism. The accepted Windows proof remains the `webbrowser` target check; no WSL branch or Wyrd launcher entered the implementation. |
| Rust/Python/TypeScript consumer and declaration parity | `wyrd-client` remains the shared implementation used by all three SDKs; no binding source or generated declaration changed | No language-specific OAuth logic or divergent generated declaration was introduced. The Rust-only removal of `TokenExchange::exchange` closes an unintended broad client surface rather than changing a documented cross-language contract. |

## Changed-symbol and caller trace

- `AuthHttp::send` is the one redirect-free transport boundary used by both
  `oauth2::AsyncHttpClient` and the three retained standard form operations.
  Its field, associated types, call operation, and cancellation boundary are
  now documented.
- `TokenExchange::{device_authorization, device_access_token, refresh}` are the
  only device and refresh operations. Their callers are `LoginFlow`,
  `SavedLoginSource`, CLI refresh, `HumanSso`, and the CLI journey.
- `TokenExchange::revoke_refresh_token` remains one form POST used by CLI
  logout and `HumanSso::revoke`, matching the locked loopback decision. Its
  idempotency and uncertain-response behavior are explicit.
- Private `TokenExchange::grant` is reachable from `platform_session` and
  `AuthMiddleware::post_token_request`. Those producers construct only RFC
  8693 token exchange and RFC 7523 JWT bearer requests. No public caller or
  test can feed it a device or refresh variant.
- `HttpConfig::validate` supplies `HttpsOrigin` directly to both
  `TokenExchange::new` and `HttpTransport::new`; `canonical_origin` delegates
  to that same owner. The former independent string parsing is gone.
- `HttpTransport::authenticated_url` compares parsed `HttpsOrigin` values
  before attaching credentials. Every cards, storage, platform, and Bifrost
  HTTP method reaches that method through the existing transport owner.

## Prior maintainer-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `MNT-TASK-012-1` — duplicate hand device/refresh surface | Public `TokenExchange::exchange` is deleted; private `grant` has only the RFC 8693/RFC 7523 producers; the CLI journey uses `device_access_token` for replay and deletes its raw `DeviceCode` form requests. | CLOSED |
| `MNT-TASK-012-2` — incomplete Rust maintenance contracts | `AuthHttp`'s field and associated types, adapter send/call, device authorization/redemption, refresh, revocation, retained grants/form send, normalized-origin owners, and `token_body` now carry substantive rustdoc. The docs state the relevant cancellation, partial-progress, retry, and idempotency consequences without adding code or a check. | CLOSED |

## Material findings

None.

## Calibration notes

- RFC 7009 remains the lead-approved single form POST because `oauth2` 5.0
  refuses loopback HTTP revocation URLs. It is not duplicate grant machinery
  and is not reopened here.
- The native Windows `webbrowser` target check is the accepted platform proof.
  PLAT-001 remains rejected; requiring a WSL branch, launcher wrapper, browser
  option, or extra harness would be nonstandard drift.
- Returning the existing `HttpsOrigin` value from `HttpConfig::validate` is
  earned by the two real consumers that must construct identical root
  endpoints. It replaces local parsing rather than creating another URL type
  or abstraction.
- Minor prose wrapping and import-group alternatives are equally clear and
  have no concrete maintenance consequence; they are not findings.

## Verification assessment

The task and remediation record the exact seven one-selected/one-passed Rust
tests, the filtered CLI device-login journey, focused `wyrd-client` transport
targets, the two crate Clippy command, format, client/CLI boundary checks,
unwrap audit, and `git diff --check`, all passing. That evidence covers the
changed owners and the removal of the broad form surface. No full journey
suite or repository aggregate is needed at this remediation review; those are
reserved for the change review. This maintainer pass started no competing
Cargo or `mise` work.

## Result

**PASS**

The cumulative candidate is maintainable at the changed boundaries: it uses
one standard OAuth path per supported grant, one existing origin authority,
cohesive owners, and complete maintenance contracts, with no independently
material maintainer finding remaining.
