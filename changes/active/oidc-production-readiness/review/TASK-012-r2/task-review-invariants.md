# TASK-012 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Prior candidate used to locate remediation: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`

The candidate remained the named commit throughout this review. I inspected the
complete base-to-candidate range and used the prior-candidate-to-candidate diff
only to locate the remediation. The review follows the lead decisions that RFC
7009 revocation remains one form POST through the redirect-free client, the
Windows launcher proof is the `webbrowser` crate's Windows-target check, and
`PLAT-001` remains rejected.

## Navigation and invariant trace

| Producer or authority | Transition | Sink and sibling consumers | Invariant result |
|---|---|---|---|
| `HttpConfig.base_url` | `HttpConfig::validate` parses with `reqwest::Url` and reduces through `HttpsOrigin::of_url` | `TokenExchange::new`, `HttpTransport::new`, and `canonical_origin`; assembled client, platform, direct CLI, saved-login, and testing callers | One standard URL/origin authority now rejects userinfo and remote cleartext, removes path/query/fragment, normalizes case/default port, and supplies the value retained by each secret-bearing owner. |
| `TokenRequest` producers in `AuthMiddleware` and `TokenExchange::platform_session` | private `TokenExchange::grant` and `post_form` | RFC 8693 API-key/delegation/platform exchange and RFC 7523 workload exchange | The former public arbitrary-grant entry point is gone. Production calls to the custom form owner have only `TokenExchange` and `JwtBearer` producers; device and refresh cannot enter it. |
| `TokenExchange::device_authorization` | `oauth2::BasicClient::exchange_device_code` through `AuthHttp` | CLI login and `HumanSso` test support | Device authorization uses the installed OAuth client and the shared redirect-free adapter. Cancellation leaves only an expiring unused code. |
| `StandardDeviceAuthorizationResponse` | `oauth2::exchange_device_access_token` polling through `AuthHttp` | CLI login and device negative/replay journey paths | Polling, pending/slow-down handling, expiry, and terminal OAuth refusals stay with `oauth2`; the removed raw one-shot poll has no sibling production path. |
| Saved refresh token | `SavedLogins::renew` holds the existing file lock, rereads, then calls `TokenExchange::refresh` | every SDK through `ResolvedCredential::Renewable`; direct CLI refresh separately calls the same OAuth method | Refresh remains on `oauth2`; the saved-login lock/reread/write lifecycle still prevents concurrent reuse of the same rotated token. |
| Logout's removed saved record | `TokenExchange::revoke_refresh_token` sends `token` plus `token_type_hint=refresh_token` once through `AuthHttp` | CLI logout and `HumanSso::revoke` | The locked RFC 7009 exception is redirect-free and idempotent; local-first deletion and best-effort warning behavior are preserved. |
| Verification URL returned by the device endpoint | `webbrowser::open` receives the URL directly | CLI login; `--no-browser` skips it | The repository-owned OS launcher and its command assembly are deleted; the printed fallback and no-browser behavior remain. |
| Saved-login store path and record key | `CredentialsFile` lock/ownership checks and `canonical_origin` | save, select, renew, remove, newest-login fallback | No second store or identity key was added; unsafe writable directories fail closed and equivalent deployment spellings select the same record. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Device authorization and redemption use `oauth2` RFC 8628 behavior | `crates/shared/wyrd-client/src/auth.rs:265-271,316-370`; CLI caller at `crates/wyrd/wyrd-cli/src/auth/login.rs:100-125` | Recorded filtered `cli_device_login_journey`; cumulative journey source exercises approval, replay, denial, expiry, and browser/no-browser behavior | PASS |
| Refresh uses `oauth2` RFC 6749 §6 for both saved renewal and direct CLI refresh | `crates/shared/wyrd-client/src/auth.rs:372-393`; `crates/shared/wyrd-client/src/saved_login.rs:274-333`; `crates/wyrd/wyrd-cli/src/auth/refresh.rs:34-49` | Recorded Rust, Python, and TypeScript saved-login journeys plus CLI journey; exact owner tests and narrow client targets are recorded | PASS |
| Custom form POST remains only for RFC 8693 and RFC 7523 | Public `TokenExchange::exchange` is deleted; production calls to private `grant` are `platform_session` at `auth.rs:299-314` and `AuthMiddleware::post_token_request` at `auth.rs:1057-1074`, whose producers build only `TokenExchange` or `JwtBearer` | Source/caller inspection; exact redirect test passes and covers the form sink | PASS |
| Lead-approved RFC 7009 behavior is preserved without `oauth2::revoke_token` | `crates/shared/wyrd-client/src/auth.rs:395-427,487-505`; logout lifecycle at `crates/wyrd/wyrd-cli/src/auth/login.rs:158-198` | `auth::tests::token_exchange_never_follows_a_redirect` rerun: 1 selected, 1 passed; recorded CLI journey covers successful and unreachable-server logout | PASS |
| Every secret-bearing auth call uses the redirect-disabled adapter | `AuthHttp` owns a reqwest client built with `Policy::none()` at `auth.rs:124-179,241-276`; OAuth grants and custom form/revocation converge on it | `auth::tests::token_exchange_never_follows_a_redirect` rerun: 1 selected, 1 passed for refresh, JWT bearer, and revoke over 307/308, with zero target hits | PASS |
| Concurrent saved-login renewal remains lock/reread/rotate/write and does not replay a rotated token | `SavedLogins::renew` holds `CredentialsFile::lock`, rereads, checks freshness, refreshes, and atomically writes at `saved_login.rs:274-333` | Recorded filtered `concurrent_saved_renewal` identity journey | PASS |
| Remote cleartext, malformed and unsupported targets are refused; HTTPS and real loopback HTTP remain accepted | `crates/shared/wyrd-client/src/transport/config.rs:202-244`; `HttpsOrigin::of_url` is the existing standard parser-backed authority | Both exact config tests rerun: each selected 1 and passed 1 | PASS |
| Userinfo is refused without disclosure and case/default-port/path/query/fragment variants identify one deployment root | `HttpConfig::validate` at `transport/config.rs:202-244`; retained origins at `auth.rs:200-276` and `transport/http.rs:120-172`; saved key at `saved_login.rs:108-129`; absolute request check at `transport/http.rs:745-776` | Config refusal/non-disclosure and normalization cases pass in the two rerun exact tests; the recorded saved-origin exact test passed | PASS |
| Credential directory writable by group or world fails closed; owner-only lifecycle remains usable | `CredentialsFile` directory check uses `0o022` at `crates/shared/wyrd-client/src/credentials_file.rs:187-202`; saved logins retain the same file owner | Recorded exact `saved_login::tests::unsafe_and_corrupt_stores_fail_closed` passed | PASS |
| Browser opening uses `webbrowser = 1.2.4`, preserves printed URL/fallback, and `--no-browser` prints without opening | Dependency pin in workspace `Cargo.toml`; `webbrowser::open` and branch at `crates/wyrd/wyrd-cli/src/auth/login.rs:84-125`; no repository launcher helper remains | Recorded filtered CLI journey passed; accepted `cargo check -p webbrowser --target x86_64-pc-windows-msvc` passed | PASS |
| `oauth2 = 5.0.0` has default features disabled and no language SDK duplicates token behavior | Workspace dependency is `oauth2 = { version = "=5.0.0", default-features = false }`; only `wyrd-client` depends on it and all first-class SDKs continue through the shared client | Recorded workspace-hack and client/CLI/SDK tier checks passed; cumulative diff adds no SDK token code | PASS |
| API-key, workload JWT, and delegated exchange keep one cache/single-flight owner and standard form wire behavior | `AuthMiddleware` retains its cache/gates at `auth.rs:613-1120`; request producers at `auth.rs:998-1074`; platform uses `TokenExchange::platform_session` | Recorded focused Postgres client auth test and workload JWT bearer journey passed | PASS |
| FIND-TASK-004-5, -9, -10, -13 (client), and -14 routed into TASK-012 are closed | Parsed origin is retained by both clients; saved-login origin delegates to it; permission and redirect behavior remain at existing owners; launcher is deleted | Recorded exact tests and focused journeys, plus the three reviewer reruns above | PASS |
| Changed async OAuth boundaries state cancellation, partial progress, idempotency, and retry consequences | `AuthHttp`, device authorization/redemption, refresh, revoke, platform session, `grant`, and `post_form` rustdoc at `auth.rs:124-179,286-484`; normalized transport and saved-origin docs at their owners | Recorded narrow clippy lane passed; source inspection finds the remediation-required contracts on surviving items | PASS |
| Every specifically named Rust test has exact nonzero-selector evidence | `TASK-012-client-oauth2.md:237-249` records all six library selectors and the setup-wrapped Postgres integration selector | Each recorded command reports one selected and one passed; three owner-level selectors were independently rerun here with the same result | PASS |
| No compatibility path, additional grant abstraction, parser, setting, store, recovery protocol, WSL branch, or permanent check entered the implementation | Cumulative source diff uses installed `oauth2`, `webbrowser`, `url`/`HttpsOrigin`, existing credential store, and existing tests; prior broad custom entry point and hand origin parser were deleted | Diff and caller inspection; no source-grep gate or new configurable surface was added | PASS |
| Scope remains client-side; server durable identity and machine paths are unchanged | Write set is the shared client, CLI, testing fixture, manifests/lockfile, task evidence, and prior review artifacts | Recorded AC-005 regression proof; cumulative diff contains no server contract, storage, principal, role, or tenant mutation | PASS |

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-012-1` | The public arbitrary `TokenRequest` exchange is gone. Device and refresh have only their `oauth2` methods; the custom form owner has only RFC 8693/RFC 7523 production producers. The journey's raw device polls were deleted, while replay remains exercised through `device_access_token`. | CLOSED |
| `FIND-TASK-012-2` | `HttpConfig::validate` now returns the normalized `HttpsOrigin`; `TokenExchange`, `HttpTransport`, and `canonical_origin` all consume that authority. Userinfo is rejected before retention, diagnostics, or request construction. | CLOSED |
| `FIND-TASK-012-3` | Surviving changed OAuth owners and fixture helper have substantive rustdoc, including the uncertain refresh-rotation and device-redemption cases and idempotent revocation behavior. Documentation for the deleted broad exchange was removed rather than preserved. | CLOSED |
| `FIND-TASK-012-4` | The original task now records all seven exact selectors and their one-selected/one-passed outcome. | CLOSED |

The four prior failures do not share a remaining source. The custom-grant breadth
and raw-target retention were removed at their producers; the documentation and
evidence obligations were closed in their existing artifacts without adding a
new runtime mechanism.

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None. Optional hardening or additional mechanisms would exceed the approved
task and the standards-only direction.

## Open Questions

None.

## Verification Notes

- Independently reran the exact tests
  `transport::config::tests::remote_cleartext_malformed_and_unsupported_targets_are_refused`,
  `transport::config::tests::https_and_loopback_http_are_accepted`, and
  `auth::tests::token_exchange_never_follows_a_redirect`; each selected and
  passed exactly one test.
- `git diff --check` for the complete base-to-candidate range passed.
- Assessed the recorded focused identity journeys, exact selectors, narrow
  client targets, clippy, client-tier checks, unwrap audit, formatting, and the
  accepted Windows-target `webbrowser` check. In accordance with the review
  direction, this reviewer did not run or require a full identity sweep, full
  journey suite, `test:rust`, `gate`, or another aggregate.
- The accepted Windows proof and rejected WSL proposal are not verification
  gaps: they are locked review inputs and were not reopened.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-012 obligations, closes
`FIND-TASK-012-1` through `FIND-TASK-012-4`, preserves the locked RFC 7009 and
platform decisions, and introduces no independently supportable invariant
finding.
