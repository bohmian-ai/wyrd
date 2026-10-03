# TASK-012 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Candidate observed at the requested commit before and after review.

This review followed the complete base-to-candidate diff and the resulting
caller paths through `TokenExchange`, `AuthMiddleware`, saved-login renewal,
CLI login/refresh/logout, the shared identity harness, and the CLI journey. It
also checked the existing owners that satisfy the task's inherited TASK-004
findings: `HttpConfig`, `canonical_origin`, and `CredentialsFile`.

The review applies the standing direction that Wyrd uses standard OAuth 2.0
and OIDC conventions and vetted libraries. It does not reopen the lead-decided
RFC 7009 loopback implementation or the accepted Windows proof.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011 / AC-004: CLI device login uses the standard RFC 8628 client flow | `crates/shared/wyrd-client/src/auth.rs:118-121,241-247,305-348` configures `oauth2::BasicClient`, sends the standard device request with the tenant extension, and delegates polling cadence, `authorization_pending`, `slow_down`, denial, and expiry to `oauth2` 5.0. `crates/wyrd/wyrd-cli/src/auth/login.rs:99-127` consumes that API and saves only the returned Wyrd credential. | Recorded focused `cli_device_login_journey` and Rust/Python/TypeScript saved-login journeys all exited 0. Source inspection confirms the CLI journey covers pending, wrong, denied, expired, replayed, and successful codes without token output. | PASS |
| REQ-012 / AC-004: routine saved-login renewal uses the standard refresh grant and preserves lock-and-reread single-flight across processes | `crates/shared/wyrd-client/src/auth.rs:350-362` uses `exchange_refresh_token`; `crates/shared/wyrd-client/src/saved_login.rs:276-335` still takes the credential-file lock, rereads, reuses a token another process saved, then refreshes and atomically stores the rotated pair while holding the lock. | Recorded `client` / `concurrent_saved_renewal` journey exited 0; all three SDK saved-user journeys exited 0. | PASS |
| REQ-012: logout removes the selected local record first, then revokes that login best-effort and warns on failure | `crates/wyrd/wyrd-cli/src/auth/login.rs:155-203` preserves local-first removal and warning behavior. `crates/shared/wyrd-client/src/auth.rs:364-392` sends the lead-approved single RFC 7009 form POST, including `token_type_hint=refresh_token`, through the redirect-free client and accepts any 2xx. | Recorded CLI journey exited 0 and source inspection confirms it proves one-chain revocation, continued renewal of the other login, and offline local deletion plus warning. | PASS |
| Lead-decided RFC 7009 loopback behavior is preserved without invented machinery | `crates/shared/wyrd-client/src/auth.rs:364-392,441-459` implements exactly one conventional form POST through the existing adapter because `oauth2` 5.0 rejects all non-HTTPS revocation URLs. It adds no setting, fallback protocol, redirect exception, or second client. | Covered by `token_exchange_never_follows_a_redirect` and the recorded CLI logout journey. | PASS |
| REQ-021: device and refresh requests use OAuth form wire semantics and the public `wyrd-cli` client identity | `crates/shared/wyrd-client/src/auth.rs:241-247,313-362` uses `oauth2` with `AuthType::RequestBody`; the crate emits the standard client id, grant type, device code, and refresh token form fields. | Recorded client and CLI lanes exited 0; real-server journeys exercised both flows. | PASS |
| REQ-013 / AC-005: API-key, workload JWT, delegation, and platform exchanges remain on the shared client path | `crates/shared/wyrd-client/src/auth.rs:261-303,394-438,957-1027` retains one form POST only for RFC 8693 and RFC 7523, which `oauth2` does not model, and leaves the existing `AuthMiddleware` cache/single-flight owner intact. `platform_session` uses the same adapter. | Recorded `test:shared` passed, including the fixture-backed API-key auth test; recorded workload jwt-bearer identity journey exited 0. | PASS |
| INV-005: no first-class SDK grows language-specific token logic | The cumulative diff changes only the shared Rust owner, CLI, test harnesses, manifests, and Rust journeys; no `sdks/wyrd-sdk-{python,ts,rust}` implementation changed. All SDKs continue through `wyrd-client`. | `git diff --name-status` plus recorded client-tier checks. | PASS |
| FIND-TASK-004-5: every shared secret-bearing caller refuses malformed/unsupported and remote-cleartext targets, including mixed-case spelling, while allowing HTTPS and real loopback HTTP | `crates/shared/wyrd-client/src/transport/config.rs:201-256` parses the URL and decides from normalized scheme/host. `TokenExchange::new` calls that owner before building either HTTP client (`auth.rs:197-253`). | Existing focused config tests and `auth::tests::token_exchange_refuses_remote_cleartext`; recorded `test:shared` exited 0. CLI journey also proves login and refresh refuse a remote HTTP server. | PASS |
| FIND-TASK-004-13: 307/308 never replay any token, assertion, device, refresh, or revocation body | `crates/shared/wyrd-client/src/auth.rs:123-158,227-230` has one `AuthHttp` backed by `reqwest::redirect::Policy::none()`. Both `oauth2` operations and the necessary RFC 8693/7523/7009 form POSTs use it. | `auth::tests::token_exchange_never_follows_a_redirect` exercises both 307 and 308 for refresh, JWT bearer, and revocation, and proves zero requests reached the redirect target; recorded `test:shared` exited 0. | PASS |
| FIND-TASK-004-9: saved server identity is one canonical origin and userinfo is refused before persistence or output | `crates/shared/wyrd-client/src/saved_login.rs:113-133` uses the existing `HttpsOrigin::of_url` owner; login and logout call it before saved-record operations (`crates/wyrd/wyrd-cli/src/auth/login.rs:72-80,155-163`). | `saved_login::tests::canonical_origin_is_the_url_origin` and the CLI journey cover case/default port/path/query/fragment equivalence and redacted userinfo refusal; recorded lanes exited 0. | PASS |
| FIND-TASK-004-10: group/world-writable credential directories fail closed while owner-controlled modes retain the full lifecycle | `crates/shared/wyrd-client/src/credentials_file.rs:182-212,307-331` applies the conventional `0o022` forbidden mask at the existing credential-file owner. | `saved_login::tests::unsafe_and_corrupt_stores_fail_closed` covers `0777`, `0775`, `0757`, safe `0755`/`0700`, and save/select/remove; recorded `test:shared` exited 0. | PASS |
| FIND-TASK-004-14: browser opening uses the vetted platform library, never the CLI's hand launcher or a configurable command | `crates/wyrd/wyrd-cli/src/auth/login.rs:85-124` passes the verification URL as one string to `webbrowser::open`, preserves printed fallback, and skips the call under `--no-browser`. The old `open_in_browser`/`browser_command`, `cmd`, `rundll32`, and `xdg-open` code is deleted. | Accepted lead proof: recorded `cargo check -p webbrowser --target x86_64-pc-windows-msvc` exited 0. Recorded CLI journey drives `--no-browser`. The host's lack of a Windows C cross-compiler is not used to weaken or replace this decision. | PASS |
| AC-007 client fault coverage: wrong, missing, expired, denied, or replayed device authority yields no credential; malformed/unsafe targets and redirects fail closed | The `oauth2` device poll returns only after a valid token response, and `LoginFlow::run` does not call `save` on any refusal (`login.rs:99-127`). Parsed-target validation and the one no-redirect adapter precede all secret-bearing calls. | Recorded CLI journey covers pending, wrong, denied, expired, replayed, and successful device codes; focused unit tests cover parsed-target and 307/308 failures. | PASS |
| Dependency constraints: exact vetted versions, `oauth2` default features off, and no extra transport stack | Workspace `Cargo.toml` pins `oauth2 = 5.0.0` with `default-features = false` and `webbrowser = 1.2.4`; owner manifests consume workspace dependencies. The adapter intentionally reuses workspace `reqwest` 0.13. | Recorded `check:workspace-hack`, client-tier checks, `ts:napi:check`, and `codegen:check` exited 0. Manifest and lockfile diff inspected. | PASS |
| Delete replaced custom mechanics without deleting standards-required unsupported grants | The custom CLI poll and per-OS launcher are gone. Device and refresh POST/decode now belong to `oauth2`. The remaining private `grant`/`post_form` path is limited to RFC 8693, RFC 7523, and the explicitly approved RFC 7009 loopback POST; there is no JSON compatibility path. | Complete source/diff inspection and recorded format/lint/shared/CLI lanes. | PASS |
| Preserve explicit credential precedence, tenant selection, newest-login default, cache/single-flight, and existing first-class SDK behavior | No resolution or cache owner was replaced. `SavedLogins::select` and `AuthMiddleware` remain the shared authorities, and renewal enters through `SavedLoginSource`. | Recorded Rust/Python/TypeScript saved-login journeys and `test:shared` exited 0. | PASS |
| Prohibited changes and non-goals remain excluded | No second store, browser command setting, shell escaping layer, language-specific token implementation, redirect allowlist, compatibility route, or new OAuth option appears in the cumulative diff. The fixture `expires_at` to `expires_in` correction aligns the mock with RFC 6749 §5.1 and the real server contract. | Complete base-to-candidate diff inspected; `git diff --check` was clean. | PASS |

## Review findings

### Critical

None.

### Important

None.

### Suggestions

None. The reviewed task is an acceptance audit; optional hardening and
unrequested refactors were excluded.

## Caller and failure-path coverage

- `AuthHttp` is the sole redirect-free adapter for every changed grant and the
  retained unsupported-by-`oauth2` RFC forms.
- `TokenExchange` callers were traced through `AuthMiddleware`, platform
  sessions, CLI login, CLI refresh, CLI logout, `SavedLoginSource`, and the
  shared human-login harness.
- Device terminal errors are produced by the server in RFC form, consumed by
  `oauth2`, and projected back through the existing Wyrd error catalog. The
  CLI saves nothing until polling returns a successful token response.
- Refresh refusal leaves the on-disk login unchanged and returns the existing
  log-in-again error. A successful rotation is stored under the existing file
  lock before it is returned.
- Redirect, malformed response, transport loss, browser-launch failure,
  Ctrl-C, and offline logout paths retain bounded, caller-visible behavior.
- API-key/workload/delegation sibling consumers still use the same form owner,
  auth cache, single-flight gate, timeout, and error projection.

## Verification notes

The implementation record reports these narrow write-set lanes green:
`mise run test:shared` (732 tests), `mise exec -- cargo nextest run --locked -p
wyrd-cli --lib` (60 tests), the five focused identity journey commands, the
workload jwt-bearer journey, `codegen:check`, `ts:napi:check`, the client-tier
checks, `check:workspace-hack`, `check:unwrap-audit`, `fmt`, `lints`, and
`git diff --check`. No Python or TypeScript source changed, so their format,
lint, and typecheck lanes were not applicable.

Per task-review coordination, this reviewer did not start competing Cargo or
mise processes while the independent reviews ran. The recorded commands are
credible for the inspected source and directly cover the changed behavior.
No broad aggregate or unfiltered journey suite is required at this task gate;
those remain for change review.

## Overall result

**PASS**

The resulting repository satisfies TASK-012 exactly. The candidate replaces
the client-owned protocol mechanics with the specified vetted libraries,
retains only standards-required form POSTs that `oauth2` does not support (plus
the lead-approved RFC 7009 loopback path), closes the routed client findings,
and adds no nonstandard mechanism or remediation-worthy drift.
