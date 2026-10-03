# TASK-012 invariant review

Immutable subject: base `adf349081077b3cfe0d56ab9a665cf01e2d86da4`, candidate
`29f7ae0ce8580cafc4873705b4c913e93bf7464f`, approved
`SPEC-oidc-production-readiness` revision 11, and
`changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`.
The candidate remained at the stated commit throughout this review.

## Review Findings

### Critical

None.

### Important

- **INV-012-001 — DRIFT — the generic hand-written token path still accepts
  device and refresh grants.**
  [`crates/shared/wyrd-client/src/auth.rs:261`](../../../../../crates/shared/wyrd-client/src/auth.rs#L261)
  exposes `TokenExchange::exchange(&TokenRequest)`, and
  [`auth.rs:394`](../../../../../crates/shared/wyrd-client/src/auth.rs#L394)
  sends and decodes that arbitrary request through the hand-written
  `grant`/`post_form` path. `TokenRequest` includes `RefreshToken` and
  `DeviceCode`, not only the RFC 8693 and RFC 7523 variants for which TASK-012
  expressly retains a form POST. This is reachable rather than dormant:
  [`crates/wyrd/wyrd-cli/tests/cli_login_journey.rs:290`](../../../../../crates/wyrd/wyrd-cli/tests/cli_login_journey.rs#L290)
  constructs `TokenRequest::DeviceCode` and calls the generic method at lines
  296, 303, and 344. The same broad method remains the production sink for
  the machine grant constructors through
  [`auth.rs:1011`](../../../../../crates/shared/wyrd-client/src/auth.rs#L1011).
  This violates TASK-012 lines 33–37, 43–44, 75–78, and 139–140: the vetted
  client must own device and refresh mechanics, while custom form logic is
  limited to RFC 8693 and RFC 7523. It also leaves a public shared-client route
  by which a Rust consumer can perform a one-shot manual device poll or refresh
  instead of the standard library behavior, so the invariant is not enforced
  at the owner. Delete the generic arbitrary-`TokenRequest` surface and narrow
  the retained form-post boundary to only the existing RFC 8693 token-exchange
  and RFC 7523 JWT-bearer producers. Device and refresh callers must have only
  the existing `oauth2` methods available. Preserve the lead-directed RFC 7009
  one-form-POST exception unchanged. Replace the journey's manual device polls
  with proof through the standard client path or the server test owner; do not
  add a new protocol helper, option, or source-grep gate. Focused closure is a
  source/API inspection plus the exact affected auth unit tests and the single
  filtered CLI device-login journey, not a full journey suite or aggregate.

### Suggestions

None.

## Complete invariant trace

| Invariant / authority | Producer | State and transformation | Sinks and sibling consumers | Result |
|---|---|---|---|---|
| Secret-bearing calls use one validated target and never follow redirects | `ClientConfig.http` and CLI `Url` values construct `TokenExchange` | `TokenExchange::new` delegates target policy to `HttpConfig::validate`, installs the shared TLS provider, and owns one `AuthHttp` backed by reqwest `Policy::none()` (`auth.rs:197-252`) | Device authorization, device redemption, refresh, machine exchange, platform exchange, and revocation all use this adapter; the redirect unit test covers refresh, JWT bearer, and revocation with zero target hits | PASS |
| Device login uses RFC 8628 through the vetted client and emits no token into browser or CLI output | `LoginFlow` supplies the typed tenant; `TokenExchange::device_authorization` adds it as the sole extension parameter (`login.rs:100-124`, `auth.rs:305-348`) | `oauth2::BasicClient` parses the standard device response and owns polling cadence, `slow_down`, expiry, and cancellation; `token_response` converts only the issued pair into Wyrd wrappers (`auth.rs:461-475`) | `webbrowser::open` receives the verification URL as one item; `SavedLogin::from_token` is the only persistence sink; `HumanSso` and the CLI journey use the same methods | PASS for the shipped CLI path; the parallel generic device path is the drift in `INV-012-001` |
| Public-client refresh rotation remains serialized across processes | A resolved saved login becomes `SavedLoginSource` through `ClientConfig::resolve_credential` and `SavedLogins::source` (`config.rs:161-203`, `saved_login.rs:239-250`) | `AuthMiddleware` runs `AccessTokenSource::mint` on the blocking pool; `SavedLogins::renew` takes the credential-file lock, rereads, reuses a fresh sibling result, otherwise calls `TokenExchange::refresh` through `oauth2`, saves the rotated pair, and only then releases the lock (`saved_login.rs:276-336`, `auth.rs:880-927`) | Rust, Python, and TypeScript all consume the same `wyrd-client` middleware. The recorded `concurrent_saved_renewal` evidence exercises separate processes sharing this store | PASS |
| Logout removes local renewable authority even if network revocation fails | CLI selection canonicalizes the server origin; `SavedLogins::remove` deletes under the file lock before returning the secret (`login.rs:158-183`, `saved_login.rs:252-274`) | The returned refresh token is sent once as RFC 7009 form data through the redirect-free adapter; any 2xx succeeds and failure becomes a warning (`auth.rs:364-391`, `login.rs:184-198`) | The store is already clear on timeout, outage, redirect, or refusal; the server-side family is revoked when reachable | PASS. The hand RFC 7009 call is the explicit lead decision and is not reopened |
| Machine principals remain independent and use only their standard grants | `ResolvedCredential::ApiKey`, `WorkloadJwt`, and `Delegated` select their existing `AuthMiddleware` branches (`auth.rs:740-806`) | API keys/delegation construct RFC 8693 requests; workloads construct RFC 7523 JWT-bearer requests; cache, single-flight, assertion reread, and token dropping remain under the same middleware (`auth.rs:850-1027`) | HTTP/gRPC transports and all language SDKs receive only Wyrd access tokens; no language-specific grant code changed | PASS for behavior and ownership, subject to narrowing the shared custom form sink in `INV-012-001` |
| Platform exchange remains on its distinct authority without another client/store | `platform::handle` constructs the same `TokenExchange`; `platform_session` constructs the platform RFC 8693 request (`auth.rs:278-303`) | The redirect-free form adapter returns the existing Wyrd `TokenResponse`; no refresh token is introduced | Platform callers keep their existing credential and session owner | PASS |
| Saved-login identity is canonical and token-free in projections | Server spelling enters `canonical_origin`, which parses through `HttpsOrigin::of_url`; saved records key by `(origin, tenant)` and summaries omit both tokens (`saved_login.rs:37-130`) | File ownership/mode, lock, atomic replace, and user-authored TOML preservation remain at `CredentialsFile`; selection chooses an exact tenant or the newest same-origin record | CLI login/status/logout and SDK credential resolution share the one store | PASS based on source and the recorded focused tests |
| Only the standard device/refresh owner is callable | `TokenExchange` is a public shared-client type | Dedicated `device_access_token` and `refresh` use `oauth2`, but public `exchange(&TokenRequest)` independently serializes every enum variant | Production machine flows and CLI journey tests share this overly broad sink; a Rust consumer can also call it directly | FAIL — `INV-012-001` |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence reviewed | Result |
|---|---|---|---|
| REQ-011 / RFC 8628 CLI login uses a vetted OAuth client; wrong, denied, expired, or redeemed codes issue no credential; no token is printed or redirected | `TokenExchange::{device_authorization,device_access_token}`, `LoginFlow::run`, and `webbrowser::open` | Recorded filtered `cli_device_login_journey`; denied and expired cases use the library path, while pending/wrong/replay assertions use the drift path | FAIL because the owner still exposes the parallel hand device path (`INV-012-001`) |
| REQ-012 / AC-004 saved credentials renew through the shared client; explicit precedence, tenant/newest selection, refusal, revocation, and cross-process rotation invariants hold | `SavedLogins::renew` calls `TokenExchange::refresh` while holding the existing lock after reread; all SDKs resolve `ResolvedCredential::Renewable` | Recorded filtered Rust, Python, TypeScript, client-concurrency, and CLI journey results | PASS |
| REQ-013 / AC-005 API-key and workload paths remain available and distinct from human login | Existing `AuthMiddleware` API-key, workload JWT, and delegated branches are unchanged apart from their shared HTTP sink | Recorded shared auth test and filtered workload journey | PASS |
| REQ-021 / INV-005 Wyrd SDKs use standard OAuth wire behavior without moving durable identity or role logic into a client | Form bodies and standard response conversion remain in `wyrd-client`; no SDK-specific token code changed | Recorded `codegen:check`, `ts:napi:check`, and client-tier checks | PASS for wire projection and SDK ownership; FAIL for retaining a parallel manual device/refresh-capable client mechanism (`INV-012-001`) |
| `oauth2 = 5.0.0` has default features disabled and uses the workspace reqwest 0.13 redirect-free adapter | Workspace manifest pins `=5.0.0`, `default-features = false`; `AuthHttp` adapts the workspace client | Recorded workspace-hack, shared tests, lints, and lockfile evidence | PASS |
| Device and refresh use `oauth2`; RFC 8693 and RFC 7523 alone retain the custom form POST | Dedicated methods use `oauth2`, but `exchange(&TokenRequest)`/`grant`/`post_form` accepts all variants | Source trace and current CLI test callers | FAIL — `INV-012-001` |
| Lead-directed RFC 7009 exception: one form POST through the redirect-free client, including loopback | `revoke_refresh_token` sends `token`, `token_type_hint`, and public `client_id`, checks only status, and shares `AuthHttp` | Redirect unit coverage and recorded CLI journey | PASS |
| Concurrent renewal never replays a token another process already rotated | Lock, reread, freshness check, library refresh, rotated save are one serialized `SavedLogins::renew` operation | Recorded filtered `concurrent_saved_renewal` | PASS |
| FIND-TASK-004-5: parsed target policy rejects malformed/remote HTTP spellings and accepts HTTPS/actual loopback | Existing `HttpConfig::validate` parser is reused by `TokenExchange::new` | Recorded shared tests and focused target tests | PASS |
| FIND-TASK-004-9: saved identity is a canonical origin; userinfo is refused and absent from output | Existing `canonical_origin` uses `HttpsOrigin::of_url`; summaries carry no tokens | Recorded canonical-origin and CLI journey evidence | PASS |
| FIND-TASK-004-10: group/world-writable credential directories fail closed | Existing `CredentialsFile` mode boundary remains the sole store owner | Recorded shared tests | PASS |
| FIND-TASK-004-13: no 307/308 secret-body replay | `AuthHttp` owns a reqwest client with `Policy::none()` for every auth route | `token_exchange_never_follows_a_redirect` records three redirector hits per status and zero target hits | PASS |
| FIND-TASK-004-14 and `--no-browser`: use `webbrowser`, no bespoke launcher or configurable command | `LoginFlow::run` prints first, conditionally calls `webbrowser::open`, and preserves fallback; old launcher code is deleted | Recorded CLI unit/journey evidence and lead-approved `cargo check -p webbrowser --target x86_64-pc-windows-msvc` | PASS |
| No second credential store, language-specific token logic, compatibility path, or unrelated product behavior | Diff stays within task packet, shared client, CLI, harness/tests, manifests, lockfile, and workspace-hack | Cumulative name-status and source review | PASS |
| No hand `TokenExchange` POST/decode remains except the two required unsupported grants and lead-directed revocation | Generic `exchange`, `grant`, `post_form`, and `TokenRequest` retain device/refresh-capable hand logic | Source trace; the implementation evidence's contrary PASS claim does not match the public signature or journey calls | FAIL — `INV-012-001` |

## Open Questions

None. The approved task already resolves the correction boundary: custom form
logic is limited to RFC 8693, RFC 7523, and the lead-directed RFC 7009
loopback exception.

## Verification Notes

- No Cargo or mise lane was started during the parallel review. The task
  records successful focused identity leaves for CLI, Rust, client
  concurrency, Python, and TypeScript, plus `test:shared`, the `wyrd-cli` lib
  target, codegen/N-API, boundary checks, unwrap audit, format, lints, and
  `git diff --check`.
- The task's Windows limitation is accepted exactly as directed: this host has
  no Windows C cross-compiler, so the relevant proof is the successful
  Windows-target check of `webbrowser` plus deletion of the CLI's per-OS
  launcher.
- Full journey suites and broad aggregates are intentionally neither run nor
  required at task review. They remain change-review evidence.
- Green journeys do not close `INV-012-001`: the defect is the retained public
  and reachable duplicate protocol mechanism, and the CLI journey itself uses
  it for three device polls.

## Overall result

**FAIL**

One bounded drift finding remains: `INV-012-001`.
