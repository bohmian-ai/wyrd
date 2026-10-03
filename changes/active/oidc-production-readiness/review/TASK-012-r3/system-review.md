# TASK-012 round-3 system-resilience review

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation inputs: `TASK-012-R1-client-oauth2-closure.md` and
  `TASK-012-R2-documentation-closure.md`

The candidate remained the named commit throughout this review. I inspected the
complete base-to-candidate range and used the R1 and R2 deltas only to locate the
changed owners and prior-finding closure. This review covers the deployed client
path and its process, dependency, cancellation, concurrency, persistence, and
recovery behavior. It does not reopen the lead decisions for the one RFC 7009
form POST, the accepted `webbrowser` Windows-target check, or rejected WSL
support.

## Deployed path and affected capabilities

The changed code runs in caller-owned CLI and SDK processes, not in a shared
Wyrd server process:

```text
wyrd CLI / Rust SDK / Python SDK / TypeScript SDK
  -> ClientConfig::resolve_credential
  -> SavedLoginSource when a saved human login is selected
  -> AuthMiddleware cache and single-flight gate
  -> TokenExchange
  -> redirect-free reqwest client
  -> Wyrd /auth endpoints
```

- `ClientConfig::resolve_credential` selects the shared saved-login source after
  explicit and environment credentials and before the legacy file-floor API key
  (`crates/shared/wyrd-client/src/config.rs:161-203`). Python and TypeScript
  therefore consume the same Rust renewal behavior rather than maintaining
  another token implementation.
- `TokenExchange` owns one normalized deployment origin, one redirect-free HTTP
  pool, and one `oauth2::BasicClient` for device and refresh grants
  (`crates/shared/wyrd-client/src/auth.rs:124-208,221-278`). RFC 8693, RFC 7523,
  and the lead-approved RFC 7009 request use the same adapter as one form POST
  (`auth.rs:396-485`).
- `SavedLogins::renew` holds the configuration-directory OS lock across reread,
  refresh rotation, and atomic replacement (`crates/shared/wyrd-client/src/saved_login.rs:274-334`).
  `SavedLoginSource::mint` is the synchronous boundary dispatched onto the
  client's blocking pool (`saved_login.rs:386-435`).
- `AuthMiddleware` keeps one in-process cache and one pending mint. Cancellation
  of a waiter does not start a second refresh: the blocking mint stays in
  `pending_mint` for the next caller (`crates/shared/wyrd-client/src/auth.rs:932-979`).
- CLI login prints the verification URL before attempting the platform browser,
  polls with `oauth2`, and saves only after token redemption succeeds
  (`crates/wyrd/wyrd-cli/src/auth/login.rs:84-145`). CLI logout removes the
  local record before best-effort revocation (`login.rs:158-199`).

The affected user capabilities are interactive device login, saved human-login
selection and renewal in all first-class SDKs, explicit CLI refresh, logout,
API-key and workload token exchange through the shared auth owner, and ordinary
HTTP calls whose bearer comes from that owner. A failure in these paths fails
the calling operation or login; it does not crash a Wyrd server replica or take
unrelated server capabilities offline.

## Failure and recovery assessment

| Failure or recovery path | Source evidence and system behavior | Assessment |
|---|---|---|
| Wyrd dependency outage, connection failure, or request timeout | `AuthHttp` uses the configured finite request timeout and maps request failure into `TransportDown` (`auth.rs:124-180,524-541`). No OAuth retry loop replays a rotating refresh token or secret-bearing form automatically. Device login or the current SDK request fails at the caller boundary; a saved record remains unchanged and a later operation may retry after the dependency recovers (`saved_login.rs:308-329,426-431`). | PASS. The fault is contained to the caller operation, and absence of an automatic refresh retry avoids replay amplification. |
| `307` or `308` from a token or revocation endpoint | The one OAuth HTTP client is built with `Policy::none()` (`auth.rs:242-277`). Both `oauth2` grants and retained form POSTs use it, and `token_exchange_never_follows_a_redirect` sends refresh, JWT-bearer, and revocation requests to `307` and `308` responders while asserting zero requests at the redirect target (`auth.rs:1815-1864`). | PASS. The request fails without replaying a credential body to another origin. |
| Device polling cancellation or lost response | `oauth2` owns interval, `slow_down`, and expiry behavior. Dropping before redemption stops polling and leaves an expiring, non-authoritative device code. If cancellation or transport loss races after redemption, the server may have consumed the code and minted an unreachable credential; the same code then refuses and the user starts a new login (`auth.rs:341-370`). `LoginFlow::run` saves nothing on Ctrl-C (`login.rs:118-125`). | PASS. The documented request boundary fails closed without a custom recovery protocol or persistent client-side transaction. |
| Refresh cancellation, timeout, or process crash between rotation and save | The server may rotate before the client observes the response. Until a complete response is decoded, the old record remains on disk; replay of that predecessor can trigger the approved refresh-family containment and require login again (`auth.rs:373-393`; `saved_login.rs:308-329`). Once the new pair is available, atomic temporary-file write, file `fsync`, rename, and directory `fsync` protect the replacement (`credentials_file.rs:154-175`). | PASS. This is the explicit revision-11 consequence, not a recoverable two-phase state the client should invent. |
| Cancellation of an SDK request while saved renewal is running | Renewal runs in `spawn_blocking`; the join handle remains in `pending_mint` after a waiter is dropped, so the next caller awaits the same mint instead of starting another (`auth.rs:932-979`). The file lock remains held only until the bounded exchange and write complete. | PASS. One client process does not replay the refresh because its first waiter timed out or was cancelled. |
| Client process crash while holding the saved-login lock | The lock is an OS lock attached to an open configuration-directory handle (`credentials_file.rs:58-63,214-239`), so process exit releases it. A successor process rereads the durable file. It sees either the predecessor token or the atomically replaced successor; the former follows the approved replay-containment outcome if the server rotated first. | PASS. Restart needs no lease cleanup, owner token, or repair service. |
| Concurrent SDK processes share one stale login | Every process takes the same directory lock and rereads before deciding whether to refresh (`saved_login.rs:274-329`). The real-server `concurrent_saved_renewal` test starts four child processes, proves one rotation is reused, proves the family remains renewable, and races logout against three renewals without allowing a deleted record to reappear (`crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs:117-306`). | PASS. The lock serializes only the local credential lifecycle and prevents a refresh stampede or family revocation. |
| Browser launcher failure | The CLI prints the code and verification URL first. `webbrowser::open` failure emits a fallback message and polling continues (`login.rs:84-125`). `--no-browser` bypasses launch entirely. No command interpreter or second launcher remains. | PASS. A desktop integration failure does not lose the login transaction or prevent manual completion. |
| Logout while Wyrd is unavailable, revocation times out, or the response is lost | Logout removes the selected record under the same file lock before sending the idempotent RFC 7009 request, then reports unconfirmed revocation as a warning and exits successfully (`login.rs:158-199`; `auth.rs:396-428`). The CLI journey shuts the server down and proves the local record still disappears (`crates/wyrd/wyrd-cli/tests/cli_login_journey.rs:409-436`). | PASS. Local reuse ends deterministically; server-side revocation remains explicitly best effort and may require token expiry, matching REQ-012. No durable retry state or background service is introduced. |
| Client restart after a successful login or renewal | The access and refresh pair is stored in the existing credential file. A new process resolves the same canonical origin and tenant, reuses a still-fresh access token, or renews under the lock when stale (`config.rs:161-203`; `saved_login.rs:108-128,200-248,274-334`). Process-local middleware caches and pending work are intentionally rebuilt. | PASS. Durable state is exactly the credential record; no process affinity exists. |
| Rolling Wyrd replica replacement | Client OAuth calls target the configured deployment origin and carry no replica-local client state. Server-owned refresh/device durability is outside this task; a transient unavailable response is returned to the caller without an automatic secret replay. | PASS. The candidate adds no health-check loop, replica selection, or failure amplification. |

## Documentation and proof assessment

The R2 candidate-to-current change is documentation and generated-schema prose
only. It now matches the runtime boundaries above:

- `AuthHttp::send`, device redemption, refresh rotation, and revocation state
  cancellation or uncertain-completion consequences at their actual owners
  (`auth.rs:138-180,341-428`).
- `HttpConfig` and its generated schemas accurately say that a configured URL
  is reduced to the deployment origin and that path, query, and fragment are
  discarded (`crates/shared/wyrd-client/src/transport/config.rs:146-176,204-246`).
- `TokenExchange`, `AuthMiddleware`, and `HttpTransport` diagnostics state and
  implement the normalized, userinfo-free origin boundary (`auth.rs:210-218,637-649`;
  `crates/shared/wyrd-client/src/transport/http.rs:120-140`).
- Dependency wording now distinguishes `oauth2` ownership of device and refresh
  from the redirect-free adapter's RFC 8693, RFC 7523, and locked RFC 7009 form
  POSTs (`Cargo.toml:101-106`).

The original task records successful filtered CLI, Rust, Python, TypeScript,
and concurrent-process identity journeys, plus exact one-test selectors for
target validation, canonical origin, unsafe storage, redirect refusal, and the
machine-grant regression. The R2 remediation records successful formatting,
focused `wyrd-client`/`wyrd-cli` Clippy, rustdoc, schema drift, and
`git diff --check`. I did not run a journey suite or broad aggregate in this
review; the current remediation changes no runtime behavior, and those lanes
would exceed the caller's required narrow proof scope. Source inspection and
the recorded focused evidence cover every failure path changed by this task.

## Material proposed findings

None. I found no reachable system-resilience regression or inaccurate recovery
contract in the cumulative candidate. The accepted refresh crash consequence,
local-first best-effort logout, browser fallback, Windows proof boundary, and
absence of WSL support are all preserved without an additional availability,
retry, coordination, or persistence mechanism.

## Overall result

**PASS**
