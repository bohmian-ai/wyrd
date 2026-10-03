---
id: TASK-012-R1
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
requirements: [REQ-011, REQ-012, REQ-013, REQ-021, INV-005, AC-004, AC-005, AC-007]
depends_on: []
parent_task: TASK-012
remediates: [FIND-TASK-012-1, FIND-TASK-012-2, FIND-TASK-012-3, FIND-TASK-012-4]
---

# Close TASK-012 client OAuth ownership, target, documentation, and proof gaps

## Authority and immutable review subject

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Review base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Reviewed candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Validated ledger: `changes/active/oidc-production-readiness/review/TASK-012-r1/findings-validation.md`

This task is routed directly to `$wyrd-implement`. It does not reopen the
approved specification or introduce another planning cycle.

## Outcome

Finish the standards-first client migration at its existing owners: only
`oauth2` can perform device and refresh grants; every shared HTTP/auth client
uses one parsed, normalized, userinfo-free deployment origin; the changed OAuth
boundary documents its uncertain-completion behavior; and every named Rust test
has exact nonzero-selector evidence.

Keep the implementation conventional. Reuse the repository's URL/origin owner,
the installed OAuth client, the existing redirect-free adapter, and the current
tests. Add no compatibility path, permanent check, setting, launcher, parser,
store, grant abstraction, or recovery protocol.

## Issue diagnoses and required corrections

### FIND-TASK-012-1 — the custom form owner still admits device and refresh grants

TASK-012 moved RFC 8628 device polling and RFC 6749 refresh to `oauth2`, while
retaining one form path only for RFC 8693 token exchange and RFC 7523 JWT bearer
assertions. The candidate nevertheless leaves public
`TokenExchange::exchange(&TokenRequest)` accepting the full enum. `DeviceCode`
and `RefreshToken` therefore still reach the hand `grant`/`post_form` serializer,
and the CLI journey exercises the device variant for one-shot polling.

This exposes two client implementations of the same grants. External Rust
callers can bypass the vetted client's polling, errors, and future fixes while
remaining on the public shared-client API.

Constrain the existing custom form owner to its two legitimate producers: RFC
8693 token exchange and RFC 7523 JWT bearer. Device and refresh must be
reachable only through the existing `oauth2`-backed methods. Remove journey
assertions that depend on the production bypass when the standard client path
or existing server grant coverage already proves the outcome. Do not replace
them with a public raw-protocol helper, another grant enum, compatibility API,
or source-grep check.

### FIND-TASK-012-2 — the parsed target is discarded before shared clients are built

`HttpConfig::validate` parses a base URL only to inspect its scheme and loopback
host, returns no normalized value, and accepts HTTPS userinfo. `TokenExchange`
and `HttpTransport` then retain and debug-print the raw spelling and construct
endpoints by text concatenation. Saved-login persistence correctly uses
`canonical_origin`, but explicit API-key, bearer, workload, platform, refresh,
and assembled-client paths can bypass that owner.

As a result, URL userinfo can appear in diagnostics, and case/default-port/
path/query/fragment spellings of one deployment need not address the same root
OAuth and API endpoints.

At the shared `HttpConfig` boundary, reuse the existing `url::Url` and
`HttpsOrigin::of_url` authority to produce one normalized, userinfo-free origin.
Make both `TokenExchange` and `HttpTransport` derive their endpoints from that
same result. Preserve HTTPS and real loopback HTTP. Refuse userinfo without
echoing it. Do not add a second parser, redaction wrapper, allowlist, setting,
path-prefix compatibility mode, or per-caller guard.

### FIND-TASK-012-3 — changed OAuth items omit mandatory maintenance contracts

The new `AuthHttp` tuple field and `AsyncHttpClient` associated types are
undocumented, the adapter call documentation only restates delegation, and the
changed async OAuth boundaries do not consistently state cancellation and
partial progress. A refresh response can be lost after the server rotates the
token; device redemption can be lost after the code is consumed and authority
is minted; revocation is idempotent. The changed `token_body` fixture helper
also lacks rustdoc explaining its RFC 6749 `expires_in` shape.

Document the existing adapter field, associated types, call boundary, and
fixture helper. On the surviving async owner methods and shared send/form
boundary, describe applicable cancellation, server-side partial progress,
idempotency, and retry consequences, including refresh rotation and device
redemption explicitly. Delete documentation with code removed for
`FIND-TASK-012-1`; do not document dead breadth or add a documentation gate.

### FIND-TASK-012-4 — named Rust tests lack exact-selector evidence

The implementation evidence names seven Rust tests, but records only family or
target lanes. That does not prove each exact selector is current and nonempty,
as required by `AGENTS.md` and the original task.

Run each exact selector below and append its one-selected/one-passed result to
the existing TASK-012 implementation evidence. Update no script, fixture,
lane, check, or additional evidence file solely for this finding.

## Preserved behavior and non-goals

- Preserve the lead-approved RFC 7009 revocation as one form POST through the
  redirect-free client. Do not switch it to `oauth2::revoke_token` or add an
  HTTPS/loopback branch.
- Preserve `webbrowser = 1.2.4`, `webbrowser::open`, `--no-browser`, the printed
  fallback URL, and the accepted `cargo check -p webbrowser --target
  x86_64-pc-windows-msvc` proof. Add no WSL branch, launcher, escaping layer,
  browser command option, or platform harness.
- Preserve the redirect-free `reqwest` client for every secret-bearing call.
- Preserve `AuthMiddleware` caching/single-flight and saved-login lock, reread,
  atomic replace, newest-login selection, explicit credential precedence, and
  local-first best-effort logout.
- Preserve one shared Rust client implementation for Rust, Python, and
  TypeScript. Add no language-specific token logic.
- Preserve RFC 8693 and RFC 7523 as standard form POSTs through the existing
  adapter; `oauth2` does not model them.
- Add no new public option, store, state machine, retry journal, compatibility
  surface, dependency, permanent repository check, or broad test harness.
- Do not run or require full identity sweeps, full journey suites, `test:rust`,
  `gate`, or another aggregate. Those run once at change review.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-012-1` | No callable custom form path accepts `DeviceCode` or `RefreshToken`; RFC 8628 device and RFC 6749 refresh remain on `oauth2`; RFC 8693, RFC 7523, and the locked RFC 7009 exception retain their current standard wire behavior. |
| `FIND-TASK-012-2` | Every `TokenExchange` and `HttpTransport` construction consumes the same parsed normalized origin; userinfo is refused without disclosure; case/default-port/path/query/fragment variants resolve to the root deployment origin; HTTPS and real loopback HTTP still work. |
| `FIND-TASK-012-3` | Every new or materially modified Rust item in the remediation has substantive rustdoc, and surviving async credential operations state cancellation, partial progress, idempotency, and retry consequences where applicable. |
| `FIND-TASK-012-4` | The original task records the seven exact commands below, each selecting and passing exactly one named test. |

## Focused verification

Run every new or changed named test with its exact `mise exec -- cargo nextest`
selector. Then run only the following existing proof needed by this write set.

Exact named library tests:

```text
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::config::tests::remote_cleartext_malformed_and_unsupported_targets_are_refused)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::config::tests::https_and_loopback_http_are_accepted)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=auth::tests::token_exchange_refuses_remote_cleartext)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=saved_login::tests::canonical_origin_is_the_url_origin)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=saved_login::tests::unsafe_and_corrupt_stores_fail_closed)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=auth::tests::token_exchange_never_follows_a_redirect)'
```

Exact named Postgres integration test:

```text
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-client --test pg_auth_e2e_against_fixture -E 'test(=pg_tests::wyrd_client_authenticates_via_wyrd_access_token_header)'"
```

The one filtered journey affected by removing the manual device path:

```text
mise exec -- env WYRD_IDENTITY_TARGET=cli WYRD_IDENTITY_FILTER=cli_device_login_journey mise run test:identity:journey
```

Narrow static and boundary proof:

```text
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-cli --all-targets --all-features -- -D warnings
mise run check:client-tier
mise run check:cli-client-tier
mise run check:unwrap-audit
git diff --check
```

Do not rerun `test:shared` or any full language/journey/aggregate lane solely to
close this remediation. The later cumulative task review reassesses the complete
base-to-new-candidate range against the original task.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-012-1` | The public `TokenExchange::exchange` is deleted. The private `grant` is reached only by `platform_session` and `AuthMiddleware::post_token_request`, which build the RFC 8693 and RFC 7523 requests. Device and refresh are reachable only through `device_access_token` and `refresh` on `oauth2`. Revocation is unchanged. The CLI journey's single-poll `DeviceCode` assertions are removed: pending and unknown codes are covered by the server's `device_grant_refusal_journey`, and replay is now asserted through `device_access_token`. | `auth::tests::token_exchange_never_follows_a_redirect`; `pg_tests::wyrd_client_authenticates_via_wyrd_access_token_header`; identity journey `cli`/`cli_device_login_journey` exit 0 | PASS |
| `FIND-TASK-012-2` | `HttpConfig::validate` parses once with `reqwest::Url` plus `HttpsOrigin::of_url` and returns the origin. `TokenExchange::new`, `HttpTransport::new` and `canonical_origin` all consume that origin. Endpoints are origin plus path. The absolute-URL check compares `HttpsOrigin`s, and the hand `same_origin`/`split_origin` parser is deleted. Userinfo is refused, and no error repeats it. The existing remote-cleartext guidance message is kept. | `transport::config::tests::remote_cleartext_malformed_and_unsupported_targets_are_refused` (adds userinfo cases and a non-disclosure assertion); `transport::config::tests::https_and_loopback_http_are_accepted` (case, default-port, path, query and fragment variants normalize to the root origin); `auth::tests::token_exchange_refuses_remote_cleartext`; `saved_login::tests::canonical_origin_is_the_url_origin`; `wyrd-client` lib plus the `transport`, `cards_transport` and `storage_dispatch` targets: 307/307 passed | PASS |
| `FIND-TASK-012-3` | Rustdoc now covers the `AuthHttp` field, `send`, `call`, `Error` and `Future`, plus `token_body`. Cancellation, partial-progress, idempotency and retry notes are on `device_authorization`, `device_access_token` (redemption consumes the code), `refresh` (rotation on receipt; replay kills the login), `revoke_refresh_token` (idempotent), `platform_session`, `grant` and `post_form`. Docs are updated on `HttpConfig::validate`, `TransportConfig::validate`, `HttpTransport::{origin, base_url, new, authenticated_url}` and `canonical_origin`. | `cargo clippy -p wyrd-client -p wyrd-cli --all-targets --all-features -D warnings` exit 0 | PASS |
| `FIND-TASK-012-4` | The seven exact commands are recorded in the TASK-012 evidence. | Each reported `1 test run: 1 passed` | PASS |

Static and boundary checks all exited 0: `mise run fmt`, `mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-cli --all-targets --all-features -- -D warnings`, `mise run check:client-tier`, `mise run check:cli-client-tier`, `mise run check:unwrap-audit` and `git diff --check`.

The preserved behaviors are unchanged: the revocation form POST, `webbrowser`, `--no-browser`, the redirect-free client, middleware caching, the saved-login lock, and one shared Rust client. No new option, dependency, parser, check or grant abstraction was added.
