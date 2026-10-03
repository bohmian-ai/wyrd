# TASK-012 round-2 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Prior candidate used only to locate remediation: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`

The candidate was the named commit before and after this review. I inspected the
complete base-to-candidate diff and used the remediation diff only to focus the
closure audit. Caller paths were traced through `TokenExchange`,
`AuthMiddleware`, `HttpTransport`, saved-login selection and renewal, CLI
login/refresh/logout, the shared human-login harness, and the CLI journey.

This review applies the standing direction that Wyrd uses conventional OAuth
2.0/OIDC behavior and vetted libraries without Wyrd-only mechanisms. It does
not reopen the lead-decided RFC 7009 form POST, the accepted `webbrowser`
Windows-target proof, or rejected WSL proposal `PLAT-001`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011 / AC-004: CLI device login uses the standard RFC 8628 client flow | `crates/shared/wyrd-client/src/auth.rs:119-122,265-271,316-370` configures `oauth2::BasicClient`, uses the standard device authorization request, and delegates polling, `authorization_pending`, `slow_down`, denial, expiry, and redemption to `oauth2` 5.0. `crates/wyrd/wyrd-cli/src/auth/login.rs:84-145` consumes that flow and saves only a successful response. | The implementation record reports the filtered `cli_device_login_journey` passing. Its current source covers successful login, denial, expiry, replay, cross-origin/unknown approval refusal, no token output, and `--no-browser`; the server journey owns the single-poll pending and unknown-code answers as the remediation permits. | PASS |
| REQ-012 / AC-004: routine saved-login renewal uses the standard refresh grant and preserves cross-process lock-and-reread | `auth.rs:372-393` uses `exchange_refresh_token`; `saved_login.rs:274-333` holds the credential-file lock, rereads the record, reuses another process's fresh token, and stores a rotated pair before releasing the lock. | The cumulative record reports the Rust/Python/TypeScript saved-login journeys and `concurrent_saved_renewal` passing; remediation did not change this owner except to consume the normalized origin. | PASS |
| REQ-012: logout deletes locally first, then revokes best-effort and warns on failure | `wyrd-cli/src/auth/login.rs:158-198` preserves local-first removal. `auth.rs:395-427` sends the lead-approved RFC 7009 `token` plus `token_type_hint=refresh_token` form through the redirect-free client, treats any 2xx as success, and leaves failure as an unconfirmed warning. | The recorded CLI journey covers one-chain revocation, survival of a sibling login, and offline local deletion. `auth::tests::token_exchange_never_follows_a_redirect` covers the revocation redirect boundary. | PASS |
| REQ-021: device and refresh requests use standard OAuth form semantics and the public `wyrd-cli` client identity | `auth.rs:265-271,328-393` uses `oauth2` with `AuthType::RequestBody`; the library owns the standard device and refresh forms. | Recorded client and CLI evidence passes, including the exact redirect test selector. | PASS |
| REQ-013 / AC-005: API-key, workload JWT, delegation, and platform exchanges remain on the shared client path | `auth.rs:286-314,429-484,998-1074` retains the one conventional form route for RFC 8693 and RFC 7523, which `oauth2` does not model. Production producers build only those two grant variants; device and refresh callers use their `oauth2` methods. | The record reports the exact fixture-backed API-key selector and workload jwt-bearer journey passing. | PASS |
| INV-005: no first-class SDK grows language-specific durable identity or token logic | The cumulative diff changes the shared Rust owner and its CLI/test consumers; no Python, TypeScript, or Rust SDK implementation adds a grant path. | Diff inspection and recorded client-tier checks. | PASS |
| FIND-TASK-012-1: the public arbitrary `TokenRequest` exchange is gone and device/refresh have no reachable hand implementation | The public `TokenExchange::exchange` was deleted. The remaining private form tail is reached in production only by `platform_session` and `AuthMiddleware::post_token_request`, whose producers construct RFC 8693 `TokenExchange` or RFC 7523 `JwtBearer`. Device and refresh enter only `device_access_token` and `refresh`. The CLI journey's former manual `DeviceCode` form calls are deleted. | Current caller search and cumulative source inspection; recorded focused CLI journey and auth tests. | PASS |
| FIND-TASK-012-2 / FIND-TASK-004-5 and -9: shared clients use one normalized, userinfo-free deployment origin | `transport/config.rs:202-244` parses with `reqwest::Url` and delegates normalization to existing `HttpsOrigin::of_url`. `TokenExchange::new` (`auth.rs:220-277`), `HttpTransport::new` (`transport/http.rs:153-173`), and `canonical_origin` (`saved_login.rs:112-131`) consume that same authority. Endpoints join the returned root origin; debug output contains only the normalized origin. | Exact config and saved-login selectors are recorded one-selected/one-passed. The config tests cover mixed-case/default-port/path/query/fragment normalization, HTTPS and real loopback HTTP, remote cleartext refusal, userinfo refusal, and non-disclosure. | PASS |
| FIND-TASK-004-10: unsafe credential directories fail closed | The existing `CredentialsFile` permission owner and saved-login lifecycle remain unchanged by remediation. | The exact `saved_login::tests::unsafe_and_corrupt_stores_fail_closed` selector is recorded one-selected/one-passed. | PASS |
| FIND-TASK-004-13: secret-bearing redirects never replay a body | `AuthHttp` is backed by `reqwest::redirect::Policy::none()` and is the one adapter for `oauth2` grants plus RFC 8693/7523/7009 forms (`auth.rs:124-180,252-275,458-483`). | The exact 307/308 redirect selector is recorded one-selected/one-passed and proves zero requests reach the redirect target for refresh, JWT bearer, and revocation; device requests use the identical adapter. | PASS |
| FIND-TASK-004-14: browser opening uses the vetted platform library without a Wyrd launcher or setting | `wyrd-cli/src/auth/login.rs:84-123` passes the verification URL as one value to `webbrowser::open`, always prints it, and skips launch under `--no-browser`. The hand per-OS launcher is deleted. | Lead-accepted `cargo check -p webbrowser --target x86_64-pc-windows-msvc` passed; the CLI journey covers `--no-browser`. No WSL mechanism is required or present. | PASS |
| FIND-TASK-012-3: changed OAuth boundaries document cancellation and uncertain completion | `auth.rs:124-180,286-484` documents the adapter field and associated types, request cancellation, device redemption, refresh rotation/reuse, idempotent revocation, retained form-grant retry behavior, and platform exchange. `auth.rs:1189-1201` documents the RFC 6749 test response. Modified transport/origin owners also describe their normalization contract. | The remediation record reports focused all-target/all-feature Clippy passing. Runtime tests are not needed to prove documentation content. | PASS |
| FIND-TASK-012-4: every named Rust test has exact nonzero-selector evidence | TASK-012 now records six exact `wyrd-client --lib` selectors and the exact Postgres-backed integration selector through the repository wrapper. | Each is recorded as `1 test run: 1 passed`. | PASS |
| Dependency and ownership constraints | Workspace `oauth2` is exactly 5.0.0 with default features disabled; `webbrowser` is exactly 1.2.4. Only `wyrd-client` and `wyrd-cli` consume them, respectively, and workspace-hack entries are regenerated. | Manifest/lock diff plus recorded workspace-hack and tier checks. | PASS |
| Preserve cache/single-flight, explicit credential precedence, tenant selection, newest-login default, and SDK projection | `AuthMiddleware`, `SavedLogins::select`, `SavedLoginSource`, and the credential chain remain the owners; remediation changes only the target origin they consume. | Cumulative journey evidence recorded in TASK-012; source and caller inspection found no changed bypass. | PASS |
| Prohibited changes and non-goals remain excluded | No second credential store, browser command, shell escaping layer, redirect exception, language-specific grant logic, compatibility path, new OAuth option, parser, permanent check, or recovery protocol entered the cumulative diff. | Complete base-to-candidate name-status and source diff inspection; `git diff --check` is clean. | PASS |

## Review findings

### Critical

None.

### Important

None.

### Suggestions

None. Optional hardening and unrequested refactors are outside this acceptance
audit.

## Prior-finding closure

- `FIND-TASK-012-1`: closed. The externally reachable arbitrary form exchange
  is deleted, current production producers of the private form tail are only
  RFC 8693 and RFC 7523, and device/refresh consumers use `oauth2`.
- `FIND-TASK-012-2`: closed. All three affected owners delegate to the existing
  `HttpConfig`/`HttpsOrigin` normalization authority, and userinfo cannot enter
  endpoints or diagnostics.
- `FIND-TASK-012-3`: closed. The materially changed OAuth and origin boundaries
  now document the requested maintenance and uncertain-completion contracts.
- `FIND-TASK-012-4`: closed. The original task contains all seven exact
  commands and their one-selected/one-passed results.

## Caller and failure-path coverage

- `TokenExchange` callers were traced through `AuthMiddleware`, platform
  sessions, CLI login/refresh/logout, `SavedLoginSource`, and the shared
  human-login harness.
- Device terminal failures return no token, and `LoginFlow` saves nothing until
  `oauth2` returns a successful token response. A lost successful redemption
  is documented as requiring a new login.
- Refresh stays under the existing file lock. Refusal leaves the stored login
  unchanged; a successful rotation is persisted before it is returned. Lost
  rotation and reuse consequences match REQ-012.
- Redirect, malformed response, transport loss, browser-launch failure,
  Ctrl-C, and offline logout retain bounded caller-visible behavior.
- API-key, workload, delegation, and platform siblings use the same normalized
  origin, redirect-free adapter, timeout, error projection, and auth
  cache/single-flight owner.

## Verification notes

The implementation records the required narrow remediation proof green: the
seven exact named tests, the filtered CLI journey, focused `wyrd-client` and
`wyrd-cli` Clippy, `fmt`, client/CLI tier checks, unwrap audit, and
`git diff --check`. The cumulative TASK-012 record also contains the earlier
focused saved-login and machine-identity evidence. I did not run another Cargo
or `mise` process during the parallel review. Per the human direction and the
remediation task, full identity sweeps, full journey suites, `test:rust`, and
`gate` are change-review evidence and are neither required nor recommended for
this task review.

## Overall result

**PASS**

The resulting repository satisfies the original task and closes all four
round-1 findings. It uses `oauth2` and `webbrowser` for the behavior those
libraries own, retains only the locked/conventional form requests the library
cannot serve, and adds no nonstandard mechanism or remediation-worthy drift.
