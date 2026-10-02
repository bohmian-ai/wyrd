# TASK-004 round 2 security and OIDC domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Cumulative range reviewed: `06f134dc14164c040c0e5014d21de29c240f4116..362878494ed80ca5c5533a4364f744bf92dd1e06`
- Remediation diff was used only to locate changed owners. Conclusions come from the cumulative candidate and its reachable callers.
- Approved authority: `changes/active/oidc-production-readiness/spec.md` revisions 8 and 9, plus every decision and addendum in `review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`. Those authorities supersede conflicting task prose. Revision 10 REQ-021 is owned by TASK-008 and was excluded from this review.

The candidate remained exactly `362878494ed80ca5c5533a4364f744bf92dd1e06` throughout this review.

## Reviewed boundary

This pass traced the complete laptop-client security boundary: device-code creation and entropy, tenant routing, browser approval and callback binding, expiry, polling throttling, one-time redemption, replay refusal and audit; TLS and redirect handling for every secret-bearing client exchange; browser-opening command boundaries; saved-credential ownership, mode, locking, atomic replacement and secret exposure; tenant and principal binding; refresh rotation, logout revocation and transactional audit; and Rust/Python/TypeScript projection through the one shared client implementation.

### Authority and source coverage

| Boundary | Governing authority | Source and consumers inspected | Result |
|---|---|---|---|
| RFC 8628 device authorization and polling | Spec rev. 8 REQ-011; [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html) §§3.1–3.5, 5.1–5.4 | `wyrd-auth/src/cli_logins.rs`; device SQL and migration; server `/auth/device_authorization`, `/auth/device`, and token routes; `wyrd-client/src/auth.rs`; CLI `auth/login.rs`; CLI journey | **PASS**, except the Windows browser-opening sink in `SEC-R2-003` |
| Tenant, principal, login-state, and callback binding | REQ-011/012; INV-001/003/004/005/007; security posture identity and tenant rules | `CliLogins::{begin,approve,complete,poll,end}`; login-state consumption; tenant-scoped SQL; OIDC browser callback; SDK credential selection | **PASS** |
| Device entropy, expiry, throttle, denial, replay, and audit | RFC 8628 §§3.2, 3.5, 5.2, 5.4; REQ-011; AC-007; canonical-audit rules | OS-random device and user codes; hashed device lookup; DB expiry; poll row lock and interval/`slow_down`; terminal denial/expiry; delete-on-redemption; completion and logout audit | **PASS** |
| Secret transport and endpoint trust | REQ-005/011/012; INV-004; security posture TLS and secret rules; [RFC 8628 §3](https://www.rfc-editor.org/rfc/rfc8628.html#section-3); OAuth security practice; reqwest client behavior | `HttpConfig::validate`; `TokenExchange::{new,post,post_no_content}`; `AuthMiddleware`; saved-login renewal; login/logout; Rust/Python/TypeScript constructors | **FAIL — `SEC-R2-001`, `SEC-R2-002`** |
| Local credential protection and renewal | Spec rev. 8 REQ-012; human-direction file and addenda; security posture file-secret rule | `credentials_file.rs`; `saved_login.rs`; config resolution; refresh source; CLI login/logout/status; cache persistence; unsafe-file and concurrency tests | **PASS** |
| Credential precedence and tenant selector | Human-direction tenant-selector addendum; REQ-012; AC-004 | `ClientConfig::{from_global_with_overrides,resolve_credential,refuse_selector}`; workload route; saved-login selection; all SDK projections and journeys | **PASS** |
| Refresh, revocation, and audit | REQ-012; security posture refresh replay and security-event rules; repository transactional-audit rule | refresh-family server owner and SQL; `SavedLogins::renew`; `CliLogins::end`; canonical `auth.cli_login.logout`; rollback and per-chain tests | **PASS** |
| SDK projections | REQ-012; INV-005; AC-004; repository client ownership | Rust SDK facade, Python PyO3 boundary and package exports, TypeScript N-API boundary/declarations, shared `wyrd-client` config/auth owners, three journeys | **PASS** |
| OAuth form request/error wire representation | Spec revision 10 REQ-021 | Not inspected as an acceptance obligation | **OUT OF SCOPE — TASK-008 owner** |

### Standard-practice / drift audit

The candidate correctly removed the custom handoff and custom saved-renewal state superseded by revision 8: there is no separate login store, local sealing format, `RefreshPending`, generation revalidation, `LoggedOut` tombstone, custom lock timeout, or per-login format version. No new mechanism, check, file, setting, or option lacking a standards or comparable-project basis is required by this report. The three corrections below use existing URL parsing, an existing reqwest policy already used elsewhere in Wyrd, and the native Windows URL-opening boundary.

## Prior round finding closure

| Prior finding | Round-2 source evidence | Status |
|---|---|---|
| `FIND-TASK-004-1` — tenant selection | The binding addendum changed the contract to a tenant-key selector for saved login/workload routing and refusal beside explicit bearer or API-key authority. `ClientConfig::resolve_credential` and `refuse_selector` implement that contract for shared Rust, Python, and TypeScript consumers. | **CLOSED under revised authority** |
| `FIND-TASK-004-4` — local pending-token protection | The human decision requires one `credentials.toml`, 0600/user ownership, no local encryption; rev. 8 then removed pending state. `CredentialsFile` performs fail-closed ownership/mode/symlink checks and atomic locked replacement while preserving unrelated content; `SavedLogins` uses it. | **CLOSED / superseded by binding direction** |
| `FIND-TASK-004-5` — remote cleartext secret transport | `TokenExchange::new` now calls `HttpConfig::validate`, but that validation recognizes only the exact lowercase string prefix `http://`. A standards-valid mixed-case HTTP scheme bypasses it and is accepted by reqwest. The same client also follows redirects by default. | **NOT CLOSED — `SEC-R2-001`; related endpoint-trust gap `SEC-R2-002`** |
| `FIND-TASK-004-7` — logout audit | `CliLogins::end` locks the selected refresh family, revokes that chain, appends redacted canonical `auth.cli_login.logout` evidence through the existing audit owner in the same tenant transaction, and commits together. The Postgres test proves one event and audit-append failure rollback. | **CLOSED** |

## Security Audit

### Critical

None.

### High

- **`SEC-R2-001` — VIOLATION — [`crates/shared/wyrd-client/src/transport/config.rs:201`](../../../../../crates/shared/wyrd-client/src/transport/config.rs#L201), [`crates/shared/wyrd-client/src/transport/config.rs:235`](../../../../../crates/shared/wyrd-client/src/transport/config.rs#L235), [`crates/shared/wyrd-client/src/auth.rs:155`](../../../../../crates/shared/wyrd-client/src/auth.rs#L155). Remote cleartext protection is bypassable by URL spelling, so prior `FIND-TASK-004-5` remains open.** The TLS obligation applies to the parsed destination, but `is_cleartext_remote` uses case-sensitive `strip_prefix("http://")` on the raw configuration string. URI schemes are case-insensitive, and reqwest accepts and normalizes such URLs. A Rust, Python, or TypeScript caller can therefore configure a remote value such as `HTTP://wyrd.example.com`; validation returns success, `TokenExchange` is built, and API keys, workload assertions, device codes, refresh tokens, revocation tokens, or platform credentials can cross the network in plaintext. An on-path attacker can read a renewable credential or machine authority and mint Wyrd access tokens for its bound principal and tenant. The lowercase-only tests at `auth.rs:1480-1499` and `transport/config.rs:350-364` do not exercise this bypass. **Required correction:** at the existing `HttpConfig` owner, parse the base URL once with the already-installed URL parser and decide HTTPS versus the loopback HTTP exception from the parsed scheme and host; reject malformed, non-HTTPS remote, or non-loopback HTTP destinations before constructing either authenticated transport or `TokenExchange`. Do not add a spelling blacklist or a second auth-only parser. **Focused closure proof:** mixed/uppercase HTTP schemes and parser edge forms for remote hosts are refused before any request, while parsed `https` and actual `localhost`, `127.0.0.1`, and `::1` HTTP remain accepted; prove the shared SDK constructor and `TokenExchange` paths use the same result.

- **`SEC-R2-002` — VIOLATION — [`crates/shared/wyrd-client/src/auth.rs:166`](../../../../../crates/shared/wyrd-client/src/auth.rs#L166), [`crates/shared/wyrd-client/src/auth.rs:261`](../../../../../crates/shared/wyrd-client/src/auth.rs#L261), [`crates/shared/wyrd-client/src/auth.rs:283`](../../../../../crates/shared/wyrd-client/src/auth.rs#L283). Secret-bearing OAuth requests follow redirects across endpoint trust boundaries.** `TokenExchange` builds a default reqwest client; [reqwest follows up to ten redirects by default](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#method.redirect). Its shared POST helpers put API keys, workload JWTs, platform credentials, device codes, refresh tokens, and revocation tokens in request bodies. A `307` or `308` returned by a misconfigured or compromised ingress preserves the method and body, allowing that complete secret body to be replayed to a different HTTPS origin. The recipient can then reuse the credential to mint access or refresh authority, or prevent logout by retaining the revocation token. This is a reachable default for every first-class SDK and the CLI. Wyrd already uses `reqwest::redirect::Policy::none()` at auth/network trust boundaries such as `wyrd-auth-oidc/src/screening.rs:171`, so the correction is neither a new mechanism nor project drift. **Required correction:** configure the one `TokenExchange` reqwest client with the existing no-redirect policy; return the redirect response through the normal error path rather than replaying a secret body. Do not create a redirect allowlist or per-endpoint option. **Focused closure proof:** use two local servers and make the configured endpoint return `307`/`308` to the second origin; assert the exchange fails and the second server receives zero requests for representative exchange and revocation bodies.

- **`SEC-R2-003` — VIOLATION — [`crates/wyrd/wyrd-cli/src/auth/login.rs:96`](../../../../../crates/wyrd/wyrd-cli/src/auth/login.rs#L96), [`crates/wyrd/wyrd-cli/src/auth/login.rs:283`](../../../../../crates/wyrd/wyrd-cli/src/auth/login.rs#L283). A server-supplied verification URL is passed through `cmd.exe` on Windows, enabling command injection.** `LoginFlow::run` consumes `verification_uri_complete` from the configured Wyrd server and passes it verbatim to `open_in_browser`. The Windows branch invokes `cmd /C start "" <url>`. `AbsoluteUrl` establishes only an absolute HTTP(S) URL; it does not remove shell metacharacters, and ordinary query strings already contain `&`. Microsoft documents `&` as a `cmd` command separator and requires special handling of shell metacharacters ([`cmd` documentation](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd)). A malicious or compromised Wyrd endpoint can return an otherwise valid HTTPS URL whose query is followed by `& <command>`; `cmd.exe` interprets the suffix rather than treating all bytes solely as a URL. That gives the remote endpoint arbitrary code execution with the CLI user's privileges when browser auto-open is enabled, which is the default REQ-011 journey. The recorded CLI journey always takes the `--no-browser` path and cannot detect this sink. **Required correction:** remove the Windows command interpreter boundary and open the URL through the native Windows shell URL API (for example `ShellExecuteW` with the `open` verb, whose item parameter is the URL) while keeping the printed URL/failure fallback. Do not attempt another shell-escaping scheme or add a configurable command. **Focused closure proof:** a Windows-targeted test or native-boundary test passes a valid HTTPS URL containing `&`, `|`, `^`, quotes, and whitespace and proves it is delivered as one URL item with no second command execution; retain ordinary browser-open behavior and fallback reporting.

### Medium

None.

### Low / Defense In Depth

None. Optional hardening outside the approved task is intentionally omitted.

### Positive Controls

- Device codes contain 32 bytes of OS randomness and are stored only by digest; user codes are independently random, constrained for human entry, tenant-qualified in SQL, and unique within the tenant.
- Device authorization, login state, and completed authority remain server-owned and tenant-bound. Approval consumes the exact login connection/issuer/client/PKCE/nonce state established for the normal browser flow.
- Polling waits for the issued interval, adds the RFC-required five seconds after `slow_down`, locks the poll row, expires terminal state, and deletes redeemed device authority transactionally. Wrong, denied, expired, cross-tenant, and replayed codes yield no credential.
- The approval form checks the configured public Origin, returns no credential material, and sends clickjacking protections. Provider authorization codes and Wyrd access/refresh tokens do not enter redirect URLs, browser page data, CLI output, logs, or audit payloads.
- `SecretBearer` and related secret-bearing values remain redacted. Token exchange tracing does not record serialized request bodies.
- The one approved `credentials.toml` owner rejects symlinks and unsafe ownership/mode, uses a blocking exclusive lock, rereads before refresh, atomically replaces and fsyncs the file and directory, preserves unrelated user content, and never persists renewable access-token cache entries.
- Saved-login selection is by canonical server origin and tenant key, with most-recent login as the approved default. Explicit/self-naming credentials reject a simultaneous tenant selector; workload credentials use the selector as their route rather than silently ignoring it.
- Refresh rotation and per-login logout retain the server's tenant/principal/family bindings. Logout removes the selected local record first, revokes only its own server family best-effort, and the server mutation and canonical audit append share one tenant transaction.
- Rust, Python, and TypeScript expose the same shared `wyrd-client` selection and renewal owner rather than duplicating durable auth behavior.

## Verification evidence and limits

Recorded candidate evidence covers formatting, lints, code generation, boundary checks, the shared-client suite, the CLI device-login journey, principal/OIDC Postgres integration, the Wyrd aggregate, and Rust/Python/TypeScript saved-login journeys. Those results credibly cover device binding, expiry, denial, replay, tenant selection, refresh concurrency, unsafe local files, cross-language projection, logout revocation, and audit rollback.

They do not close the three findings:

- remote-cleartext tests use only lowercase `http://` spellings;
- no secret-bearing `TokenExchange` test returns `307`/`308` to a second origin;
- CLI journeys use `--no-browser`, and no Windows browser-open boundary is exercised.

The review is static and did not send live credentials or run against an external identity provider. Revision 9 replaces live-provider qualification with standards-only Keycloak and Dex journeys, so that is not a verification deficit. REQ-021 form encoding and OAuth response bodies were deliberately not evaluated because TASK-008 owns them.

## Overall result

**FAIL**

The RFC 8628 state machine, saved-credential authority, tenant/principal binding, refresh/revocation behavior, canonical logout audit, and shared SDK projections satisfy the revised approved contract. Acceptance is blocked by three practical trust-boundary defects: a reachable cleartext-TLS validation bypass, automatic redirect replay of secret POST bodies, and Windows command injection from the server-provided verification URL.
