---
id: TASK-012
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-011, REQ-012, REQ-013, REQ-021, INV-005, AC-004, AC-005, AC-007]
depends_on: [TASK-010]
---

# Shared client and CLI on `oauth2` and `webbrowser`

## Outcome and Value

`wyrd auth login`, saved-login renewal, and logout run on the vetted `oauth2`
crate and the `webbrowser` launcher, so the Rust, Python, and TypeScript SDKs
share standard OAuth client code and the hand-written exchange and browser
launcher are gone. This is report item T4 in
[`research/auth-standards-recommendation.md`](../research/auth-standards-recommendation.md),
and it closes the client-side TASK-004 round-2 findings routed here.

## Owners, Scope, Consumers, and Prohibited Changes

Owners: `crates/shared/wyrd-client` (`auth.rs`, `HttpConfig`,
`saved_login.rs`, `credentials_file.rs`) and `wyrd-cli` (`auth/login.rs`).
Consumers: the Rust, Python, and TypeScript SDKs through `wyrd-client`, and
their journeys.

Libraries and standards, exactly:

- `oauth2 = 5.0.0` with `default-features = false`, over a redirect-disabled
  `AsyncHttpClient` adapter on workspace `reqwest 0.13`
  (`reqwest::redirect::Policy::none()`). Use the crate for the device flow
  (RFC 8628 §3.1–3.5, `exchange_device_access_token`), refresh (RFC 6749 §6),
  and revocation (RFC 7009 §2). RFC 8693 token exchange and RFC 7523
  jwt-bearer keep one form POST through the same client (RFC 8693 §2.1;
  RFC 7523 §2.1).
- `webbrowser = 1.2.4` opens the verification URL.
- Keep `AuthMiddleware` caching and single-flight, `credentials.toml` with std
  `File::lock` and atomic replace, and `canonical_origin` on
  `url::Url::origin()`.

Delete: `TokenExchange::{post, decode, device_authorization,
revoke_refresh_token}` hand POST/decode code, and the `open_browser` launcher
in `wyrd-cli/src/auth/login.rs` (around lines 299–327) including its
`cmd /C start`, `rundll32`, and `xdg-open` paths.

Prohibited: language-specific token logic in `sdks/*`; following redirects on
any secret-bearing call; a second credential store; shell escaping or a
configurable browser command.

## Approach

1. Add the redirect-disabled adapter and move device, refresh, and revoke to
   `oauth2`; keep RFC 8693/7523 as one form POST through it.
2. Replace the CLI launcher with `webbrowser`, keeping the printed URL and
   `--no-browser`.
3. Fix `HttpConfig` target validation, saved-login origin, and credential
   directory permissions at their existing owners.
4. Delete the replaced code and rerun the three-language journeys.

## Ordered Implementation Scenarios

### Scenario 1 — CLI and SDK journeys on `oauth2`

**Behavior.** CLI device login, then Rust, Python, and TypeScript saved-login
journeys: allowed and denied calls, renewal without the IdP, explicit
override, `tenant` selection, newest-login default, and revoked or expired
credential refusal (REQ-011, REQ-012, AC-004).

**RED.** After TASK-010's form-body endpoints, `cli_device_login_journey`,
`saved_user_auth_journey`, `test_saved_user_auth_journey`, and
`saved user auth journey` fail on the hand JSON exchange.

**GREEN.** Move device, refresh, and revoke to `oauth2`; keep RFC 8693/7523 as
form POSTs. Rerun all four journeys.

**REFACTOR.** Delete the hand `TokenExchange` code.

### Scenario 2 — Concurrent renewal under the file lock

**Behavior.** Separate processes sharing one saved login refresh under the
file lock and reuse the token another process saved; no family revocation
(REQ-012, AC-004).

**RED.** `concurrent_saved_renewal` fails if the `oauth2` refresh bypasses the
lock-and-reread path.

**GREEN.** Run the `oauth2` refresh inside the existing lock. Rerun
Scenario 1.

**REFACTOR.** None beyond removing dead helpers.

### Scenario 3 — Transport, origin, and directory boundaries

**Behavior.**
- FIND-TASK-004-5: remote cleartext and malformed targets are refused before
  any request, including mixed-case schemes; HTTPS and real loopback HTTP
  (`localhost`, `127.0.0.1`, `::1`) work; every shared caller uses one parsed
  decision.
- FIND-TASK-004-13: 307/308 responses never replay token or revocation bodies.
  The redirect target observes zero requests.
- FIND-TASK-004-9: case, default-port, path, query, and fragment variants map
  to one origin record. Userinfo is refused and never persisted or printed.
- FIND-TASK-004-10: a group- or world-writable (`0o022`) credential directory
  fails closed; owner-only modes support the full lifecycle.

**RED.** Extend the existing transport, saved-login, and Unix permission
tests with those cases; each fails on the current code.

**GREEN.** Fix `HttpConfig` with the URL parser, use `Url::origin()` and
refuse userinfo, reject `0o022` directory bits, and set `Policy::none()` on
the adapter. Rerun Scenarios 1–2.

**REFACTOR.** One parsed target decision shared by transport and token calls.

### Scenario 4 — Browser launch without a command interpreter

**Behavior.** The verification URL, including `&`, `|`, `^`, quotes, or
whitespace, opens through `webbrowser` as one URL item on every platform with
no command interpreter; the printed URL and failure fallback remain;
`--no-browser` prints only (FIND-TASK-004-14, REQ-011).

**RED.** Windows-target compilation shows the `cmd /C start` path still in use.

**GREEN.** Replace the launcher with `webbrowser::open`. Rerun Scenario 1.

**REFACTOR.** Delete the per-OS launcher code.

## Acceptance Criteria

- AC-004 Rust, Python, and TypeScript saved-login journeys and the CLI journey
  pass on `oauth2`.
- Concurrent refresh under the file lock does not replay a rotated token.
- `--no-browser` works.
- FIND-TASK-004-5, -9, -10, -13 (client), and -14 are closed as stated in
  Scenarios 3–4.
- API-key and workload journeys still pass (AC-005).
- No hand `TokenExchange` POST/decode or `open_browser` launcher remains, and
  no `oauth2` default feature is enabled.

## Expected Write Set and Consumer Closure

`crates/shared/wyrd-client`, `crates/wyrd/wyrd-cli`, workspace and crate
`Cargo.toml`, `Cargo.lock`, the workspace-hack if required, SDK journeys, and
regenerated Python stubs and TypeScript declarations from their sources.

## Verification and Evidence

Focused commands, each under the identity lane's Postgres and IdP setup:

```text
mise exec -- env WYRD_IDENTITY_TARGET=cli WYRD_IDENTITY_FILTER=cli_device_login_journey mise run test:identity:journey
mise exec -- env WYRD_IDENTITY_TARGET=rust WYRD_IDENTITY_FILTER=saved_user_auth_journey mise run test:identity:journey
mise exec -- env WYRD_IDENTITY_TARGET=client WYRD_IDENTITY_FILTER=concurrent_saved_renewal mise run test:identity:journey
mise exec -- env WYRD_IDENTITY_TARGET=python WYRD_IDENTITY_FILTER=test_saved_user_auth_journey mise run test:identity:journey
mise exec -- env WYRD_IDENTITY_TARGET=typescript WYRD_IDENTITY_FILTER='saved user auth journey' mise run test:identity:journey
```

Run every other new or changed named test with its exact `mise exec --`
selector. Then run only the lanes covering this write set:
`mise run test:shared` (`wyrd-client`),
`mise exec -- cargo nextest run --locked -p wyrd-cli --lib`,
`mise run codegen:check` and `mise run ts:napi:check` (regenerated stubs and
declarations), `mise run check:client-tier`, `mise run check:cli-client-tier`,
`mise run check:sdk-client-tier`, `mise run check:workspace-hack` (new
dependencies), `mise run check:unwrap-audit`, `mise run fmt`, and
`mise run lints`; add `mise run py:format`, `mise run py:lints`, and
`mise run py:typecheck` when Python journey or stub files change, and
`mise run ts:typecheck` when TypeScript journey files change. Prove the
Windows path with a Windows-target `cargo check` of `wyrd-cli`. Unfiltered
identity journeys and every-language sweeps run once at change review.

## Material Stop Conditions

Stop and report if a needed behavior has no vetted library and is not covered
by a named RFC section; never write custom protocol logic. Also stop if
`oauth2` cannot run with default features off over the redirect-disabled
client, or if an SDK would need its own token logic.

## Authority Links

[Approved spec](../spec.md) REQ-011, REQ-012, REQ-021;
[research report](../research/auth-standards-recommendation.md) §3.4, §3.6, T4;
[TASK-004 round-2 findings](../review/TASK-004-r2/TASK-004-R2-production-readiness-fixes.md);
[routing](../review/TASK-004-r2/lead-direction-routing.md);
[AGENTS.md](../../../../AGENTS.md);
[RFC 6749](https://www.rfc-editor.org/rfc/rfc6749);
[RFC 7009](https://www.rfc-editor.org/rfc/rfc7009);
[RFC 8628](https://www.rfc-editor.org/rfc/rfc8628);
[RFC 8693](https://www.rfc-editor.org/rfc/rfc8693);
[RFC 7523](https://www.rfc-editor.org/rfc/rfc7523).

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-004 saved-login and CLI journeys pass on `oauth2` | `wyrd-client/src/auth.rs` `TokenExchange::{device_authorization, device_access_token, refresh}` on `oauth2::BasicClient` over the `AuthHttp` adapter; `wyrd-cli/src/auth/login.rs`; `wyrd-testing/src/human_login.rs` | identity journeys `cli`/`cli_device_login_journey`, `rust`/`saved_user_auth_journey`, `python`/`test_saved_user_auth_journey`, `typescript`/`saved user auth journey` all exit 0 | PASS |
| Concurrent refresh under the file lock replays no rotated token | `saved_login.rs` renews via `exchange.refresh` while holding the file lock | identity journey `client`/`concurrent_saved_renewal` exit 0 | PASS |
| `--no-browser` works | `login.rs` `run` prints the URL and skips `webbrowser::open` | `cli_device_login_journey` (drives `--no-browser`) | PASS |
| FIND-TASK-004-5 | `transport/config.rs` parsed `HttpConfig` validation, used by `TokenExchange::new` | `transport::config::tests::remote_cleartext_malformed_and_unsupported_targets_are_refused`, `transport::config::tests::https_and_loopback_http_are_accepted`, `auth::tests::token_exchange_refuses_remote_cleartext` | PASS |
| FIND-TASK-004-9 | `saved_login.rs` `canonical_origin` (`Url::origin`, userinfo refused) | `saved_login::tests::canonical_origin_is_the_url_origin` | PASS |
| FIND-TASK-004-10 | `credentials_file.rs` `check_owned(.., 0o022)` | `saved_login::tests::unsafe_and_corrupt_stores_fail_closed` | PASS |
| FIND-TASK-004-13 (client) | `AuthHttp` reqwest client with `redirect::Policy::none()` for every grant and revocation | `auth::tests::token_exchange_never_follows_a_redirect` (refresh, JWT bearer, revoke: 3 hits on the redirector, 0 on the target) | PASS |
| FIND-TASK-004-14 | `webbrowser::open(url)` replaces the per-OS launcher | `cargo check -p webbrowser --target x86_64-pc-windows-msvc` exit 0; see limits | PASS |
| AC-005 API-key and workload journeys still pass | `TokenExchange::exchange` form POST via the shared adapter | `pg_auth_e2e_against_fixture` `pg_tests::wyrd_client_authenticates_via_wyrd_access_token_header` (in `test:shared`); identity journey `server`/`workload_jwt_bearer_journey_keycloak` exit 0 | PASS |
| No hand device/refresh POST/decode or `open_browser` launcher; no `oauth2` default feature | hand device/refresh code, `poll`, `open_in_browser` and `browser_command` deleted; `Cargo.toml` `oauth2` keeps `default-features = false` | `check:workspace-hack`, `check:client-tier`, `check:cli-client-tier`, `check:sdk-client-tier` exit 0 | PASS |

Lanes, all exit 0 (`CARGO_TARGET_DIR` set to the repo `target/`): `mise run test:shared` (732 passed),
`mise exec -- cargo nextest run --locked -p wyrd-cli --lib` (60 passed), `mise run codegen:check`,
`mise run ts:napi:check`, `mise run check:unwrap-audit`, `mise run fmt`, `mise run lints`,
`git diff --check`. No Python or TypeScript source changed, so the `py:*` and `ts:typecheck` lanes were not
required.

Diagnosis (three `transport/http.rs` tests failed in `test:shared`):
- **Symptom:** API-key re-exchange returned `the auth response does not decode: missing field expires_in`.
- **Evidence:** the `token_response` mock emitted `expires_at`.
- **Cause:** the shared exchange decodes the RFC 6749 §5.1 `TokenResponse`, which the server sends with
  `expires_in`, so the mock was the one off-contract.
- **Fix site:** the test fixture, which now emits `expires_in`. The `auth.rs` unit fixture received the same
  correction.

Lead-decided deviation: RFC 7009 revocation is one form-encoded POST (`token`,
`token_type_hint=refresh_token`, client authenticated as for the other grants) through the redirect-free
adapter. Any 2xx counts as success; anything else becomes the existing best-effort "revocation not
confirmed" warning, and logout always clears local credentials. `oauth2` 5.0's `revoke_token` refuses
non-HTTPS URLs, which would have broken loopback logout; this is noted in rustdoc on
`revoke_refresh_token`.

Limit: a full `cargo check -p wyrd-cli --target x86_64-pc-windows-msvc` stops in the C build scripts of
`blake3`, `zstd-sys` and `aws-lc-sys`, because this host has no Windows C cross toolchain. That failure is
unrelated to the launcher. The Windows proof is therefore the `webbrowser` crate's Windows-target check
plus the deletion of all `cmd /C start` code. No per-OS launch path remains in `wyrd-cli`.

Non-goals stayed excluded, and no file outside the expected write set changed.
