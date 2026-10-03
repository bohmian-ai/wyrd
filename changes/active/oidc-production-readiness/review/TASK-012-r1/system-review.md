# System-resilience review — TASK-012

## Immutable subject

- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Overall result: **PASS**

The candidate remained `HEAD` throughout this review. I inspected the complete
base-to-candidate diff, its runtime owners and callers, the installed `oauth2`
5.0.0 polling implementation, and the recorded task evidence. I did not read
another TASK-012 reviewer's report and did not rerun Cargo or `mise` lanes while
the independent review topology was active.

This review accepts the lead-decided RFC 7009 form POST through the shared
redirect-free adapter and the `webbrowser` Windows-target check as settled. It
does not reopen either decision or require a second revocation path, custom
launcher, retry framework, readiness mechanism, or platform-specific harness.

## Deployed paths and process ownership

| Path | Runtime owner and dependencies | Failure and recovery assessment | Result |
|---|---|---|---|
| CLI device login | `LoginFlow` owns one `TokenExchange`, canonical saved-login origin, tenant, and store (`crates/wyrd/wyrd-cli/src/auth/login.rs:56-81`). `TokenExchange` binds the device and token endpoints on an `oauth2::BasicClient` over one bounded, redirect-free `reqwest` pool (`crates/shared/wyrd-client/src/auth.rs:118-185,197-252`). | Device authorization fails only the login. Standard `oauth2` polling applies the advertised interval, `slow_down` growth, bounded transport-error backoff, and the device response's expiry. A terminal denial/expiry returns no credential. Ctrl-C drops the poll, exits 130, and saves nothing; the unredeemed server code expires (`login.rs:84-125`). No server, SDK, or unrelated CLI capability stops. | PASS |
| Browser launch | After printing the code and URL, the CLI calls `webbrowser::open` with the URL as one item (`login.rs:106-117`). `--no-browser` skips the call. | Launch failure is contained to a warning and the already printed manual URL. Polling still starts. The candidate deletes the Wyrd-owned per-OS launcher rather than retaining a fallback command path. | PASS |
| Rust/Python/TypeScript saved-login use | All first-class SDKs resolve the same `SavedLoginSource` through `ClientConfig::resolve_credential`; no language-specific token path was added (`crates/shared/wyrd-client/src/config.rs:161-203`; `saved_login.rs:388-437`). | A fresh saved access token needs neither Wyrd nor the IdP. Near expiry, the shared source renews against Wyrd only. IdP outage therefore does not interrupt routine SDK use. A definite refresh refusal asks for a new login; a transport failure leaves the record intact for an ordinary later retry. | PASS |
| Cross-process refresh rotation | `SavedLogins::renew` takes the stable directory lock, rereads the record, reuses a token another process already saved, or makes exactly one refresh request and atomically replaces the credential file before releasing the lock (`saved_login.rs:276-335`; `credentials_file.rs:154-175,214-239`). | Concurrent processes cannot concurrently replay one predecessor while both follow this owner. Wyrd outage is bounded by the configured 30-second HTTP timeout while the lock is held; later callers recover after the lock releases. A crash before server rotation changes nothing. A crash after server rotation but before the durable save leaves the predecessor on disk; the approved server-side family-reuse containment refuses the retry and requires login again, exactly as REQ-012 specifies. Atomic replace and directory sync prevent a partially written record. | PASS |
| Reactive SDK refresh cancellation | `AuthMiddleware` runs the synchronous saved-login source on Tokio's blocking pool and retains its join handle across a cancelled waiter (`auth.rs:880-926`). The source performs the async refresh on the existing runtime while holding the file lock (`saved_login.rs:304-331`). | Cancelling one client request does not start a second concurrent rotation: the already-started mint remains pending and a later waiter joins it. If the whole process exits, the OS releases the directory lock and the atomic file boundary leaves either the predecessor or replacement record. There is no unbounded task fan-out. | PASS |
| OAuth grants and revocation | Device and refresh use `oauth2`; RFC 8693, RFC 7523, and the lead-decided RFC 7009 request use one form POST through the same `AuthHttp` (`auth.rs:261-438`). The pool has a total timeout and `Policy::none()` (`auth.rs:216-252`). | Network outage, timeout, malformed response, or non-success ends only the affected call. A 307/308 is returned as a refusal and cannot replay the secret body at another origin. There is no client retry loop for refresh, exchange, or revocation. | PASS |
| Logout | The CLI removes the selected local record under the file lock, then attempts one best-effort RFC 7009 revocation (`login.rs:158-198`). | A local-store failure prevents logout from claiming success. After local deletion, server outage or revocation refusal emits a warning and returns success, matching REQ-012's explicit local-first, best-effort contract. The missing local record prevents this machine from continuing to use the credential; server-side authority expires normally if revocation was not confirmed. Another tenant/login record is untouched. | PASS |
| Process restart and rolling client replacement | Tokens and tenant/server identity are in the protected credential file; only short-lived middleware state and connection pools are process-local (`saved_login.rs:1-17,37-53`; `auth.rs:567-588`). | Restart loses no required durable state. The next process rereads the file and either uses its current access token or renews under the same OS lock. Client process replacement requires no shared daemon, lease, or coordination service. | PASS |

## Failure propagation and recovery boundaries

- **Wyrd unavailable during device login.** Initial authorization returns a
  bounded transport error. During polling, the installed `oauth2` implementation
  backs off transport failures and stops at the device expiry; Ctrl-C remains
  available through `tokio::select!`. No credential file is written until a
  complete token response is available.
- **Wyrd unavailable during saved renewal.** The access request fails, the
  previous record remains intact, and a later request retries. The bounded HTTP
  call can delay other local processes sharing that credential file, but the
  exclusive lock is required to prevent rotated-token replay and the delay is
  finite.
- **IdP unavailable after login.** Saved-login renewal calls Wyrd's refresh
  endpoint, not the IdP. Existing SDK use continues until Wyrd authority itself
  is unavailable or refuses the refresh.
- **Malformed or redirected authorization server.** Shared target validation
  rejects malformed, unsupported, and remote-cleartext targets before a
  request; the saved-login origin boundary additionally rejects userinfo before
  login or logout can persist or print it. Every secret-bearing call uses the
  no-redirect pool. A bad success body or error body fails the one operation and
  does not mutate the saved record.
- **Browser handler unavailable.** The CLI has already printed the standard
  verification URL. Failure does not terminate polling or spawn an alternate
  Wyrd-owned command path; the user opens the URL manually.
- **Refresh response missing a replacement refresh token.** The public-client
  rotation is not persisted as a usable login. The operation fails closed and
  the user must log in again rather than storing a non-renewable partial record.
- **Logout revocation unavailable.** Only the selected local login is removed;
  the warning states that remote validity was not confirmed. This is the exact
  approved best-effort boundary, not a service-health failure.

No reviewed path panics the shared SDK process on dependency failure, crashes a
Wyrd server, disables another tenant or credential, introduces automatic retry
amplification, or adds a second durable authority. The candidate relies on the
standard OAuth client, OS file locking/atomic replacement, and platform browser
launcher; no bespoke resilience mechanism or unsupported setting entered the
diff.

## Affected capabilities

The runtime change affects CLI human device login, saved human-login renewal and
logout for the Rust, Python, and TypeScript SDKs, and the existing unauthenticated
client token-exchange transport used by API-key, workload, delegated, and
platform credential flows. It does not change server lifecycle, readiness,
Postgres ownership, provider federation, authorization, tenant isolation, or
the durable credential format.

## Verification and proof assessment

The committed task evidence records successful focused identity journeys for:

- CLI device login, including `--no-browser`, denial, expiry, replay, revocation,
  and offline-logout warning;
- Rust, Python, and TypeScript saved-login use and renewal;
- concurrent saved renewal across client processes; and
- workload JWT bearer behavior.

It also records `test:shared` (732 tests), the `wyrd-cli` library tests,
`codegen:check`, `ts:napi:check`, the client/CLI/SDK boundary checks,
`check:workspace-hack`, `check:unwrap-audit`, formatting, lints, and
`git diff --check`. The redirect test drives both 307 and 308 through refresh,
JWT bearer, and revocation and observes zero requests at the target
(`auth.rs:1759-1796`). The saved-login unit test proves an unreachable server
leaves the record unchanged (`auth.rs:1643-1707`), while the CLI journey proves
server shutdown still leaves local logout complete with a warning
(`cli_login_journey.rs:432-459`).

Per the review instruction, I did not run or require full journey suites or
aggregates; those run once at change review. The Windows browser path is covered
by the lead-approved `cargo check -p webbrowser --target
x86_64-pc-windows-msvc` and deletion of Wyrd's per-OS launcher. Requiring the
whole `wyrd-cli` dependency cone to cross-link on this host would contradict the
settled proof boundary, not strengthen this task review.

Residual operational limits are conventional and not findings: this review has
no kill-injection proof at every instruction between server rotation and file
rename, cannot make best-effort logout succeed while the server is unavailable,
and does not test every installed desktop browser handler. The approved
contract explicitly defines the rotated-token crash outcome and best-effort
logout, while `webbrowser` owns platform launch behavior. Adding a recovery
journal, revocation queue, custom retry state, browser-command option, or
cross-process coordinator would be unsupported drift.

## Material proposed findings

No material system-resilience findings.

## Overall result

**PASS**

The candidate contains failures at the affected CLI/SDK operation, preserves
the one durable saved-login authority and its concurrency boundary, recovers
through ordinary retry or re-login where the approved contract permits, and
does not amplify a client or dependency failure into wider service loss.
