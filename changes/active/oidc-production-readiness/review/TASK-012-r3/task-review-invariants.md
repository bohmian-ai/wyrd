# TASK-012 round-3 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Round-1 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Round-2 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`

The candidate remained at the named commit during this review. The complete
base-to-candidate range and the latest round-2-verdict-to-candidate range were
inspected. This review did not reopen the lead-approved RFC 7009 form POST, the
accepted `webbrowser` Windows-target proof, or the rejection of `PLAT-001`.

## Producer-to-sink invariant trace

### OAuth grant ownership and secret transport

- `TokenExchange::new` parses the configured deployment once through
  `HttpConfig::validate`, retains the resulting `HttpsOrigin`, builds the RFC
  8628 and RFC 6749 endpoints from that origin, and constructs one
  redirect-disabled `reqwest` client wrapped by `AuthHttp`
  (`crates/shared/wyrd-client/src/auth.rs:221-278`).
- `device_authorization`, `device_access_token`, and `refresh` use the
  `oauth2` `BasicClient` through that adapter
  (`auth.rs:317-393`). The removed public arbitrary-form `exchange` entry point
  has no remaining caller. The private `grant` path is reached in production
  only by `platform_session` and `AuthMiddleware::post_token_request`, whose
  producers construct the retained RFC 8693 token-exchange and RFC 7523
  jwt-bearer requests (`auth.rs:287-315,430-446,1004-1080`). Searches of the
  cumulative candidate found no client `TokenRequest::DeviceCode` or
  `TokenRequest::RefreshToken` producer.
- The locked RFC 7009 exception is one `token` plus
  `token_type_hint=refresh_token` form POST through the same `AuthHttp`; it
  accepts any 2xx, follows no redirect, and is idempotent at the protocol
  boundary (`auth.rs:396-428,448-485`). No second client, parser, grant enum,
  retry journal, or language-specific token path was added.
- `AuthHttp::send` buffers the response only after the redirect-free request
  completes; `refused`, `refusal`, and `oauth_error` preserve the existing
  separation between transport failure, OAuth refusal, and Wyrd catalog error
  without exposing request secrets (`auth.rs:124-180,524-600`).

### Deployment identity and credential routing

- `HttpConfig::validate` parses the URL and delegates normalization and
  userinfo refusal to the existing `HttpsOrigin::of_url` authority. Its output
  discards path, query, and fragment and permits cleartext only for actual
  loopback hosts (`crates/shared/wyrd-client/src/transport/config.rs:204-246`;
  `crates/wyrd-spec/src/operator_connection.rs:86-127`). Refusals do not repeat
  the configured URL or its userinfo.
- `TokenExchange`, `HttpTransport`, and `canonical_origin` all consume that
  same result (`auth.rs:242-278`;
  `crates/shared/wyrd-client/src/transport/http.rs:143-177`;
  `crates/shared/wyrd-client/src/saved_login.rs:108-128`). Authenticated
  absolute URLs are rechecked against the retained `HttpsOrigin`, and relative
  paths are joined to it (`transport/http.rs:749-780`). Thus API-key, workload,
  delegated, platform, refresh, saved-login, and ordinary HTTP callers cannot
  derive competing deployment identities.
- Debug projections of `TokenExchange`, `AuthMiddleware`, and `HttpTransport`
  expose the normalized origin and omit the configured spelling and clients
  (`auth.rs:210-219,637-650`; `transport/http.rs:131-141`). The round-2
  documentation now states that diagnostic boundary and the root-origin
  contract accurately.

### Saved-login lifecycle, concurrency, and failure ordering

- `ClientConfig::resolve_credential` preserves explicit credential precedence,
  then selects a saved login by the same canonical origin and optional tenant;
  only that selected record becomes a renewable source
  (`crates/shared/wyrd-client/src/config.rs:161-203`). Rust, Python, and
  TypeScript continue to consume this one shared Rust owner.
- `SavedLogins::renew` acquires the credential-file lock, rereads the current
  record, reuses another process's still-fresh token, or invokes the
  `oauth2`-backed refresh once while retaining the lock. It installs the
  returned refresh token and access token before the atomic write
  (`saved_login.rs:274-334`). No parallel writer, cached predecessor, or
  unlocked refresh path entered the candidate.
- CLI login converts the `oauth2` response into `SavedLogin`, writes through
  `SavedLogins::save`, and never prints either token
  (`crates/wyrd/wyrd-cli/src/auth/login.rs:84-146`). Logout still removes the
  selected record locally before attempting best-effort revocation and warns
  on an unconfirmed remote result (`login.rs:158-199`).
- Cancellation and partial-progress contracts now match the actual lifecycle:
  unused device codes expire; an uncertain successful device redemption leaves
  an unreachable minted credential; an uncertain refresh can retire the
  predecessor and cause replay containment; revocation is safely repeatable
  (`auth.rs:317-428`).

### Browser and consumer closure

- `LoginFlow::run` always prints the verification URL, uses
  `webbrowser::open` only when `--no-browser` is absent, and passes the URL as
  the library's single string input (`login.rs:84-125`). No Wyrd-owned
  command-interpreter or per-platform launcher remains.
- The production CLI, shared human-login harness, saved-login renewal, direct
  refresh command, platform client, and middleware callers all use the
  constrained `TokenExchange` methods. The CLI journey exercises successful
  device redemption, replay, denial, expiry, refresh, revocation, local-first
  offline logout, and equivalent-origin spelling
  (`crates/wyrd/wyrd-cli/tests/cli_login_journey.rs:240-430`).

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011: CLI device login uses RFC 8628, opens the system browser, prints no credential, and supports `--no-browser` | `TokenExchange::{device_authorization,device_access_token}` use `oauth2`; `LoginFlow::run` uses `webbrowser::open` and preserves the print-only path | Recorded filtered `cli_device_login_journey` passed; cumulative journey source covers redemption, replay, denial, expiry, and token non-disclosure | PASS |
| REQ-012 / AC-004: one shared saved-login owner, explicit precedence, tenant/newest selection, locked renewal, rotation persistence, and local-first logout | `ClientConfig::resolve_credential`; `SavedLogins::{select,source,renew,remove}`; CLI logout | Recorded Rust, Python, TypeScript, CLI, and concurrent-renewal filtered journeys passed; saved-login exact unit selectors passed | PASS |
| REQ-013 / AC-005: machine credentials remain independent of human SSO and use the shared client | `AuthMiddleware` retains API-key, workload jwt-bearer, bearer, delegation, and renewable arms; retained form owner serves RFC 8693/RFC 7523 only | Recorded workload identity journey and exact `pg_auth_e2e_against_fixture` selector passed | PASS |
| REQ-021: standard OAuth wire behavior for device, refresh, token exchange, jwt-bearer, and revocation | `oauth2::BasicClient` owns device/refresh; retained form serializer owns only RFC 8693, RFC 7523, and locked RFC 7009 | Recorded CLI journey, fixture integration test, and redirect test passed | PASS |
| INV-005: SDKs project server-owned identity and do not create language-specific durable auth logic | All SDK-facing behavior remains in `wyrd-client`; no `sdks/*` auth implementation entered the diff | `check:client-tier`, `check:cli-client-tier`, and `check:sdk-client-tier` recorded passing | PASS |
| AC-007 relevant failures: wrong/expired/denied/redeemed device code yields no credential; unsafe target and redirect replay fail closed | OAuth refusal mapping, normalized-origin validation, redirect-free adapter, and CLI journey negative paths | Recorded device-grant/CLI journey evidence; exact origin and redirect selectors each selected and passed one test | PASS |
| FIND-TASK-004-5: remote cleartext/malformed targets fail before IO; HTTPS and real loopback HTTP work | `HttpConfig::validate` plus `HttpsOrigin::of_url` is the single producer | Both exact transport-config selectors and `token_exchange_refuses_remote_cleartext` recorded one-selected/one-passed | PASS |
| FIND-TASK-004-9: equivalent URL spellings share one origin; userinfo is refused and not disclosed | `HttpConfig::validate` output is reused by token exchange, HTTP transport, and saved-login canonicalization | Exact canonical-origin and transport-config selectors recorded passing; regenerated schema text reflects the contract | PASS |
| FIND-TASK-004-10: unsafe credential storage fails closed | Existing `CredentialsFile` ownership and `SavedLogins` lifecycle remain unchanged by the OAuth migration | Exact `unsafe_and_corrupt_stores_fail_closed` selector recorded one-selected/one-passed | PASS |
| FIND-TASK-004-13: 307/308 never replay secret-bearing bodies | One `AuthHttp` has `Policy::none`; device, refresh, retained grants, and revocation share it | Exact `token_exchange_never_follows_a_redirect` selector recorded one-selected/one-passed | PASS |
| FIND-TASK-004-14: no Wyrd-owned browser launcher or command interpreter remains | `webbrowser::open`; former `open_in_browser` and per-OS command builder deleted | Accepted `cargo check -p webbrowser --target x86_64-pc-windows-msvc` evidence passed | PASS |
| Dependency constraints: exact `oauth2`/`webbrowser` versions and no `oauth2` default feature | Workspace pins `oauth2 = 5.0.0`, `default-features = false`, and `webbrowser = 1.2.4`; narrow owner manifests consume them | `check:workspace-hack` and focused Clippy evidence recorded passing | PASS |
| FIND-TASK-012-1: device and refresh have no custom-form bypass | Public arbitrary `TokenExchange::exchange` is absent; private `grant` has only RFC 8693/RFC 7523 producers | Cumulative caller search plus recorded CLI journey | PASS — CLOSED |
| FIND-TASK-012-2: every affected client consumes one parsed normalized origin | `HttpConfig::validate` returns `HttpsOrigin`; token, transport, saved-login, platform, and middleware paths consume it | Exact origin/refusal tests recorded passing | PASS — CLOSED |
| FIND-TASK-012-3: changed OAuth and origin owners have accurate maintenance, cancellation, error, panic, and diagnostic contracts | Required adapter/method/test docs are present at `auth.rs:124-180,210-241,317-485,637-650,1815-1825`, `transport/config.rs:146-165,204-246,354-410`, and `transport/http.rs:131-168`; schemas carry the corrected public prose | Round-2 evidence records format, focused Clippy, rustdoc, four schema-drift tests, and `git diff --check` passing | PASS — CLOSED |
| FIND-TASK-012-4: named tests have exact nonempty selector evidence | TASK-012 records the six library selectors and one setup-wrapped Postgres selector | Each is recorded as exactly one selected and one passed | PASS — CLOSED |
| Prohibited and non-goal complexity remains absent | No second store, SDK token logic, redirect allowlist, configurable browser command, WSL branch, compatibility mode, parser, retry journal, option, or permanent check entered the implementation | Complete cumulative diff inspection | PASS |
| Round-2 remediation changes documentation only | Latest fix diff changes rustdoc/comments and their generated schema descriptions; no executable expression, type, field, dependency value, feature, or test behavior changed | Latest diff inspection; recorded narrow documentation/schema proof | PASS |

## Proposed findings

None. The producer-to-sink and sibling-caller trace found no reachable
`MISSING`, `INCORRECT`, `DRIFT`, `VIOLATION`, or `REGRESSION` remaining in the
cumulative candidate.

## Prior-finding closure

- `FIND-TASK-012-1`: **CLOSED**. Device and refresh are callable only through
  `oauth2`; the retained private form owner cannot accept either grant.
- `FIND-TASK-012-2`: **CLOSED**. The token client, authenticated HTTP client,
  saved-login key, platform path, and explicit credential paths derive from
  the same parsed `HttpsOrigin` decision.
- `FIND-TASK-012-3`: **CLOSED**. The round-2 candidate supplies the remaining
  item-level rustdoc and corrects the public normalized-origin and RFC 7009
  ownership prose without changing runtime behavior.
- `FIND-TASK-012-4`: **CLOSED**. All seven exact commands and their nonzero
  passing results are recorded in the original task evidence.

## Verification assessment

The cumulative task record contains passing filtered CLI, Rust, Python,
TypeScript, concurrent-renewal, and workload journeys; the `wyrd-client` and
CLI test lanes; all seven exact named selectors; code generation, N-API,
client-tier, CLI-tier, SDK-tier, workspace-hack, unwrap, format, lint, and diff
checks; and the accepted `webbrowser` Windows-target check. The round-2
documentation-only fix records passing format, focused all-target/all-feature
Clippy, rustdoc, four schema-drift tests, and `git diff --check`.

This invariant pass independently ran only `git diff --check` and source/range
inspection. It did not run a full journey suite or aggregate, in accordance
with the human direction. The recorded focused evidence directly covers the
changed behavior and latest documentation/schema write set; there is no
verification limitation that prevents an invariant conclusion.

## Result

**PASS**
