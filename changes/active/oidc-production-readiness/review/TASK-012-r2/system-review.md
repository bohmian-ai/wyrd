# System-resilience review — TASK-012 round 2

## Immutable subject

- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Overall result: **PASS**

The candidate remained `HEAD` while this review inspected the complete
base-to-candidate range and the focused remediation diff
`29f7ae0ce8580cafc4873705b4c913e93bf7464f..5d9a3ddfad426eb545e866658a74428df72265ef`.
The review traced the changed client paths through their runtime owners and
ordinary failure boundaries. It accepts the settled RFC 7009 form POST, the
`webbrowser` Windows-target proof, and the rejection of a WSL-specific path.
No additional revocation protocol, launcher, retry journal, browser option,
platform harness, or recovery coordinator is warranted.

## Deployed paths and failure assessment

| Deployed path | Runtime ownership and failure propagation | Recovery assessment | Result |
|---|---|---|---|
| CLI device login | `LoginFlow` owns one `TokenExchange`, canonical origin, tenant, and saved-login store (`crates/wyrd/wyrd-cli/src/auth/login.rs:56-81`). `TokenExchange` binds the device and token endpoints on one `oauth2::BasicClient` over a redirect-free `reqwest` pool with a total per-call timeout (`crates/shared/wyrd-client/src/auth.rs:119-180,182-277`). Initial authorization failure ends only the login. During polling, `oauth2` owns the advertised interval, `slow_down` growth, transport-error backoff, and expiry. | A terminal denial or expiry returns no credential. Ctrl-C drops the poll, returns exit 130, and saves nothing; an unredeemed code expires. If redemption completed server-side but its response was lost, the spent code returns `invalid_grant` and the user begins a new standard login (`auth.rs:340-370`; `login.rs:84-126`). No server process or unrelated SDK capability stops. | PASS |
| Browser launch | The CLI prints the verification URL before calling `webbrowser::open` with that URL as one value; `--no-browser` skips the call (`login.rs:100-124`). | An unavailable desktop handler emits a warning while the printed manual URL and OAuth poll remain usable. There is no Wyrd-owned shell command, WSL path, or alternate launcher to fail differently. | PASS |
| Shared saved-login renewal | Rust, Python, and TypeScript resolve the same `SavedLoginSource` through `ClientConfig::resolve_credential` (`crates/shared/wyrd-client/src/config.rs:161-203`; `saved_login.rs:386-435`). A fresh access token requires neither Wyrd renewal nor the IdP. Near expiry, `SavedLogins::renew` takes the stable directory lock, rereads the record, reuses a token another process saved, or makes one refresh request (`saved_login.rs:274-334`). | A definite server refusal requires login again. A transport failure leaves the record unchanged for a later ordinary retry. IdP outage does not affect routine refresh because renewal reaches Wyrd, not the IdP. The refresh call is bounded by the configured HTTP timeout while the cross-process lock is held. | PASS |
| Refresh rotation, cancellation, and process crash | Refresh uses `oauth2` only (`auth.rs:372-393`). `AuthMiddleware` runs the synchronous saved-login source on Tokio's blocking pool and retains the in-flight join handle after a waiter is cancelled (`auth.rs:926-973`); the source holds the file lock through the server exchange and atomic replace (`saved_login.rs:285-333`). | A cancelled request does not start a concurrent second rotation: the next waiter joins the pending mint. A process crash releases the OS lock. A crash before server rotation changes no durable authority; a crash after rotation but before save leaves the predecessor, whose replay triggers the approved server-side family-reuse response and requires login again, exactly as REQ-012 states. Atomic replace and directory sync prevent a partially written record (`credentials_file.rs:154-175,214-239`). | PASS |
| OAuth exchange and revocation | Device and refresh grants use `oauth2`; RFC 8693, RFC 7523, and the settled RFC 7009 request share `AuthHttp` (`auth.rs:182-208,286-484`). The custom `grant` owner is private and is reached only by platform, API-key, delegated, and workload producers; device and refresh can no longer enter it (`auth.rs:286-314,429-445,998-1008,1010-1063`). Every request uses the redirect-free pool. | Timeouts, malformed replies, and non-successes remain local call failures. A 307/308 is surfaced and cannot replay a secret body at another origin. Refresh has no automatic replay after uncertain completion. Revocation is idempotent and best-effort. Device polling performs only the conventional library-owned retries until the device response expires. | PASS |
| Logout | The CLI removes the selected local record under the file lock, then attempts one RFC 7009 form POST (`login.rs:158-198`; `auth.rs:395-427`). | A local-store failure prevents logout from claiming success. After deletion, outage, timeout, cancellation, or revocation refusal leaves local logout complete, emits the explicit warning, and lets remote authority expire normally. Only the selected tenant login is removed. This is REQ-012's intentional local-first boundary, not a need for a durable revocation queue. | PASS |
| Normalized deployment target | `HttpConfig::validate` parses once and returns the shared `HttpsOrigin`; `TokenExchange`, `HttpTransport`, and saved-login selection consume that authority (`transport/config.rs:202-244`; `auth.rs:241-277`; `transport/http.rs:139-173`; `saved_login.rs:108-128`). | Userinfo, malformed URLs, unsupported schemes, and remote cleartext fail before any client or durable login is built. Case, default port, path, query, and fragment variants converge on one root deployment origin. Bad configuration fails construction of the affected client rather than allowing token and API traffic to diverge at runtime. | PASS |
| API-key, workload, delegated, and platform exchange | `AuthMiddleware` and `Platform` retain the same shared `TokenExchange`; no language-specific owner or new durable state was introduced (`auth.rs:613-707,998-1063`; `platform/handle.rs:72-100`). | A dependency outage or refusal fails the affected exchange/request. Existing cache and single-flight behavior contain it to that middleware instance; it does not crash a shared server or disable saved-login, browser login, another tenant, or another credential. | PASS |

## Failure and recovery conclusions

- **Wyrd unavailable during login:** the initial authorization call returns a
  bounded transport failure. During polling, the standard `oauth2` client
  backs off transport errors and stops at the device expiry; Ctrl-C remains
  available through `tokio::select!`. No credential file is written before a
  complete token response is saved.
- **Wyrd unavailable during renewal:** the current saved record is preserved.
  Other local processes may wait on the required refresh lock for at most the
  configured request timeout, then recover after it is released. Removing the
  lock or adding parallel refresh would violate rotation safety.
- **Cancellation after server-side progress:** device redemption may consume
  the code and refresh may rotate the token before the response is observed.
  The candidate documents and preserves the standard outcomes: restart device
  login or sign in again after refresh-family reuse detection. It does not add
  a nonstandard replay or recovery protocol.
- **Redirect or malformed response:** every secret-bearing OAuth request uses
  `Policy::none()`. Redirects, invalid success bodies, and invalid error bodies
  fail the one operation without mutating saved-login state.
- **Browser handler unavailable:** manual use of the already printed URL
  remains available; the CLI does not take down or block any server capability.
- **Revocation unavailable:** local deletion is durable and the warning states
  that remote invalidation was not confirmed. A revocation queue would exceed
  the approved best-effort contract.
- **Restart or rolling client replacement:** only credentials and their tenant
  and origin identity are durable. Connection pools, OAuth client objects, and
  middleware caches are process-local and reconstruct from configuration and
  the protected credential file.

No reachable reviewed path converts an OAuth, browser, filesystem, or network
failure into a shared-server crash or loss of unrelated tenant and machine
capabilities. The remediation removes a duplicate grant path and reuses an
existing URL authority; it does not add an availability mechanism, setting,
check, or protocol beyond standard OAuth/client behavior.

## Affected capabilities

The cumulative change affects CLI device login, human saved-login renewal and
logout used by all first-class SDKs, and the shared unauthenticated exchange
transport used by API-key, workload, delegated, and platform credentials. It
does not change server process lifecycle, readiness, provider federation,
Postgres ownership, authorization, tenant isolation, or the durable saved-login
format.

## Prior-finding closure relevant to resilience

- `FIND-TASK-012-1` is closed: device and refresh can only use the installed
  `oauth2` client; the private form owner retains only grants the library does
  not model, plus the separately settled RFC 7009 call.
- `FIND-TASK-012-2` is closed: all shared HTTP/auth constructors consume the
  normalized, userinfo-free deployment origin rather than retaining divergent
  raw URL spellings.
- `FIND-TASK-012-3` is closed for the resilience boundary: the surviving async
  OAuth operations state cancellation, partial-progress, retry, and
  idempotency consequences at their owners.
- `FIND-TASK-012-4` is closed by the committed exact-selector record. It does
  not require a new permanent check or broader review lane.

## Recovery proof and verification limits

The committed cumulative evidence records successful focused journeys for CLI
device login (including `--no-browser`, denial, expiry, replay, revocation, and
offline logout), saved-login use in Rust/Python/TypeScript, concurrent renewal,
and workload JWT bearer exchange. The remediation evidence records the one
filtered CLI journey after removal of the raw device poll, six exact
`wyrd-client` library selectors, the exact Postgres auth selector, focused
clippy for `wyrd-client` and `wyrd-cli`, client-tier and CLI-tier boundaries,
the unwrap audit, formatting, and `git diff --check`, all successful. The
redirect test exercises refresh, JWT bearer, and revocation against both 307
and 308 and observes no request at the redirect target.

Per the review direction, no full identity sweep, full journey suite,
`test:rust`, `gate`, or other aggregate was run or required here. The settled
Windows proof is `cargo check -p webbrowser --target x86_64-pc-windows-msvc`
plus deletion of Wyrd's per-OS launcher; this host cannot cross-compile the
whole CLI dependency cone without a Windows C toolchain. Residual uncertainty
is limited to instruction-level crash injection between refresh rotation and
file replacement and the behavior of every installed desktop handler. The
approved contract already specifies the former outcome, while `webbrowser`
owns the latter. Neither limit warrants a custom recovery mechanism or blocks
this task.

## Material proposed findings

No material system-resilience findings.

## Overall result

**PASS**

The candidate contains failures at the affected CLI/SDK operation, preserves
the one durable saved-login and cross-process rotation boundary, resumes through
ordinary retry or re-login where the approved contract permits, and does not
amplify client or dependency failure into wider service loss.
