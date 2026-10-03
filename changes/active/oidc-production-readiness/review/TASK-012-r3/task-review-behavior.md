# TASK-012 round-3 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Prior candidate used only to locate the latest remediation:
  `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved authority:
  `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Round-1 remediation:
  `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Round-2 remediation:
  `changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`

The candidate was the named commit before and after this review. I inspected the
complete base-to-candidate diff and used `5d9a3ddf..dc67bf1c` only to focus the
latest closure check. CodeGraph was not present. Caller paths were traced
through `TokenExchange`, `AuthMiddleware`, `HttpTransport`, saved-login
selection and renewal, CLI login/refresh/logout, the shared human-login
harness, and the CLI journey.

This review applies the standing direction that Wyrd uses conventional OAuth
2.0/OIDC behavior and vetted libraries without Wyrd-only machinery. It does
not reopen the lead-decided RFC 7009 form POST, the accepted `webbrowser`
Windows-target proof, or rejected WSL proposal `PLAT-001`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011 / AC-004: CLI device login uses the standard RFC 8628 client flow and preserves `--no-browser` | `crates/shared/wyrd-client/src/auth.rs:119-122,266-272,317-370` configures `oauth2::BasicClient`, delegates device authorization and polling to `oauth2`, and uses the shared adapter. `crates/wyrd/wyrd-cli/src/auth/login.rs:84-145` prints the code and URL, uses `webbrowser::open` unless disabled, and saves only a successful response. | TASK-012 records the filtered `cli_device_login_journey` passing. Current source covers success, denial, expiry, replay, cross-origin/unknown approval refusal, no token output, and `--no-browser`; the server journey owns single-poll pending and unknown-code behavior as the remediation permits. | PASS |
| REQ-012 / AC-004: saved-login renewal uses the standard refresh grant under the existing cross-process lock and reread | `auth.rs:373-393` uses `exchange_refresh_token`; `saved_login.rs:274-333` locks, rereads, reuses another process's fresh token, rotates only when needed, and atomically stores the replacement before releasing the lock. | The cumulative record reports the Rust, Python, and TypeScript saved-login journeys and `concurrent_saved_renewal` passing. The two remediation rounds preserve this path. | PASS |
| REQ-012: logout removes locally first, then revokes best-effort and warns on failure | `wyrd-cli/src/auth/login.rs:158-198` removes the selected record before network work. `auth.rs:396-428` sends the locked RFC 7009 `token` plus `token_type_hint=refresh_token` form through the redirect-free client, accepts any 2xx, and documents idempotent retry semantics. | The recorded CLI journey covers one-chain revocation, continued renewal of a sibling login, and offline local deletion with a warning. The exact redirect test covers revocation redirect refusal. | PASS |
| REQ-021: device and refresh use standard OAuth form semantics and the public `wyrd-cli` client identity | `auth.rs:266-272,329-393` uses `oauth2` with `AuthType::RequestBody`; the library owns the RFC 8628 device and RFC 6749 refresh forms. | Recorded focused client and CLI evidence passes. | PASS |
| REQ-013 / AC-005: API-key, workload, delegation, and platform exchanges remain on one shared Rust client path | `auth.rs:287-315,430-485,1004-1080` retains the conventional form owner only for RFC 8693 and RFC 7523, which `oauth2` does not model. Production producers build only those two grant variants; device and refresh callers use their `oauth2` methods. | TASK-012 records the exact fixture-backed API-key selector and the workload jwt-bearer journey passing. | PASS |
| INV-005: no first-class SDK adds language-specific durable identity or token logic | The cumulative diff changes the shared Rust owner and its CLI/test consumers; no Python, TypeScript, or Rust SDK implementation adds a grant or credential path. | Complete name-status and source diff plus recorded client-tier checks. | PASS |
| FIND-TASK-012-1: no callable custom form path accepts device or refresh grants | The former public arbitrary `TokenExchange::exchange` is absent. Private `grant` is reached in production only from `platform_session` and `AuthMiddleware::post_token_request`, whose producers construct RFC 8693 `TokenExchange` or RFC 7523 `JwtBearer`. `device_access_token` and `refresh` are the only device/refresh methods. | Caller search, cumulative source inspection, exact auth tests, and the filtered CLI journey recorded in TASK-012. | PASS |
| FIND-TASK-012-2 / FIND-TASK-004-5 and -9: every affected shared client consumes one parsed, normalized, userinfo-free origin | `transport/config.rs:204-246` parses once and delegates normalization to `HttpsOrigin::of_url`; `TokenExchange::new` (`auth.rs:221-278`), `HttpTransport::new` (`transport/http.rs:143-177`), and `canonical_origin` (`saved_login.rs:108-128`) consume that authority. Endpoints are built from the returned root origin and diagnostics expose only that origin. | Exact config, token-exchange, and saved-login selectors are recorded one-selected/one-passed. They cover mixed case, default ports, path/query/fragment removal, HTTPS and real loopback HTTP, remote cleartext and userinfo refusal, and non-disclosure. | PASS |
| FIND-TASK-004-10: unsafe credential directories fail closed | The existing `CredentialsFile` owner retains its `0o022` group/world-write refusal and private-file lifecycle. | Exact `saved_login::tests::unsafe_and_corrupt_stores_fail_closed` selector recorded one-selected/one-passed. | PASS |
| FIND-TASK-004-13: no secret-bearing 307/308 response replays a body | `AuthHttp` uses `reqwest::redirect::Policy::none()` and is the one adapter for `oauth2` device/refresh plus the retained RFC 8693, RFC 7523, and RFC 7009 forms (`auth.rs:124-180,253-276,448-485`). | Exact `auth::tests::token_exchange_never_follows_a_redirect` selector recorded one-selected/one-passed; it proves zero requests reach the target for refresh, JWT bearer, and revocation, while device requests use the same adapter. | PASS |
| FIND-TASK-004-14: browser opening uses the vetted platform library without a Wyrd launcher or setting | `wyrd-cli/src/auth/login.rs:84-123` passes one URL to `webbrowser::open`, always prints it, and skips launch under `--no-browser`; the hand per-OS launcher is deleted. | Lead-accepted `cargo check -p webbrowser --target x86_64-pc-windows-msvc` passed; the CLI journey covers `--no-browser`. | PASS |
| FIND-TASK-012-3: changed OAuth and normalized-origin boundaries state their actual maintenance and uncertain-completion contracts | `auth.rs:124-180,182-219,287-485,637-650,1195-1207,1815-1826` documents adapter ownership, normalized diagnostics, cancellation, device redemption, refresh rotation/reuse, idempotent revocation, retained form retry behavior, the token fixture, and the redirect test. `transport/config.rs:146-166,204-246,354-425` documents origin-only public behavior, helper errors, and test panics. `transport/http.rs:131-140` documents its diagnostic boundary. `Cargo.toml:101-106` accurately separates `oauth2` device/refresh ownership from the retained adapter forms. Generated schemas project the corrected public `HttpConfig` descriptions. | The R2 evidence records `fmt`, focused all-target/all-feature `wyrd-client` Clippy, `cargo doc`, four schema-drift tests, and `git diff --check` passing. The latest diff changes documentation and generated schema descriptions only. | PASS |
| FIND-TASK-012-4: every named Rust test has exact nonzero-selector evidence | TASK-012 records six exact `wyrd-client --lib` selectors and the setup-wrapped Postgres integration selector. | Each is recorded as `1 test run: 1 passed`. | PASS |
| Dependency and ownership constraints | Workspace `oauth2` remains exactly 5.0.0 with default features disabled; `webbrowser` remains exactly 1.2.4. The dependencies stay with `wyrd-client` and `wyrd-cli`, and workspace-hack is regenerated. | Manifest/lock inspection plus recorded workspace-hack and tier checks. | PASS |
| Preserve cache/single-flight, explicit credential precedence, tenant selection, newest-login default, local-first logout, and shared SDK projection | `AuthMiddleware`, `ClientConfig::resolve_credential`, `SavedLogins::select`, `SavedLoginSource`, and `CredentialsFile` remain the owners. Neither remediation adds a bypass. | Cumulative task journey evidence and source/caller inspection. | PASS |
| Prohibited changes and non-goals remain excluded | No second credential store, browser command, shell escaping layer, redirect exception, language-specific grant logic, compatibility path, OAuth option, parser, permanent check, recovery protocol, WSL branch, or broad test harness entered the cumulative implementation. | Complete cumulative diff and clean `git diff --check`; latest remediation is bounded to existing documentation and regenerated schema descriptions. | PASS |

## Review findings

### Critical

None.

### Important

None.

### Suggestions

None. Placement, naming, structure, and wording preferences without a
behavioral, security, tenancy, durability, or public-contract consequence are
non-blocking under the standing direction and were not promoted into findings.

## Prior-finding closure

- `FIND-TASK-012-1`: **CLOSED**. Device and refresh have no custom form entry;
  the remaining form owner serves only RFC 8693, RFC 7523, and the locked RFC
  7009 exception.
- `FIND-TASK-012-2`: **CLOSED**. `TokenExchange`, `HttpTransport`, and saved
  login identity consume the same parsed normalized origin, and userinfo cannot
  enter an endpoint or diagnostic.
- `FIND-TASK-012-3`: **CLOSED**. Round 3 source states the normalized-origin,
  diagnostic, cancellation, partial-progress, retry, idempotency, error, and
  panic contracts required by the two remediation tasks; the generated schema
  descriptions match the source.
- `FIND-TASK-012-4`: **CLOSED**. The original task records all seven exact
  selectors and their one-selected/one-passed results.

## Caller and failure-path coverage

- `TokenExchange` callers were traced through `AuthMiddleware`, platform
  sessions, CLI login/refresh/logout, `SavedLoginSource`, and the shared
  human-login harness.
- Device terminal failures return no token, and `LoginFlow` saves nothing
  until `oauth2` returns a successful token response. A lost successful
  redemption is explicitly documented as requiring a new login.
- Refresh remains under the file lock. Refusal leaves the stored login
  unchanged; a successful rotation is saved before it is returned. Lost
  rotation and reuse consequences match REQ-012.
- Redirect, malformed response, transport loss, browser-launch failure,
  Ctrl-C, and offline logout retain bounded caller-visible behavior.
- API-key, workload, delegation, and platform siblings share the normalized
  origin, redirect-free adapter, timeout, error projection, and auth
  cache/single-flight owner.

## Verification notes

The implementation records the required narrow proof green: seven exact named
tests, the filtered CLI journey, focused client/CLI Clippy, format, client-tier
checks, unwrap audit, schema drift, documentation build, and `git diff
--check`. I independently inspected the cumulative and latest diffs and ran
`git diff --check`; I did not start a competing Cargo or `mise` process during
the parallel review. Per the human direction and both remediation tasks, full
identity sweeps, full journey suites, `test:rust`, `gate`, and other aggregates
are neither required nor a verification limit for this task review.

## Overall result

**PASS**

The resulting repository satisfies the original TASK-012 behavior and closes
all prior findings. It uses `oauth2` and `webbrowser` for the behavior those
libraries own, retains only the conventional form requests the library cannot
serve plus the locked RFC 7009 exception, and adds no nonstandard mechanism or
remediation-worthy drift.
