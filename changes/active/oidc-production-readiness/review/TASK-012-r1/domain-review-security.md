# TASK-012 security domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate and reviewed `HEAD`: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`

The candidate remained unchanged while this review was performed.

## Reviewed boundary

This review traced the OAuth client security boundary from external server URL and credential inputs through `HttpConfig`, `TokenExchange`, its redirect-free `AuthHttp` adapter, `oauth2::BasicClient`, the saved-login renewal path, and the CLI login, refresh, logout, and browser-launch consumers. It covered:

- RFC 8628 device authorization and polling, including cancellation and terminal errors;
- RFC 6749 refresh requests and public-client identification;
- the lead-approved RFC 7009 form POST through the redirect-free client;
- RFC 8693 and RFC 7523 form requests that the `oauth2` crate does not model;
- endpoint construction, target validation, and redirect handling;
- token and userinfo exposure through storage, output, errors, and `Debug`;
- the `webbrowser` replacement and removal of command-interpreter launch paths; and
- dependency declarations for `oauth2 = 5.0.0` and `webbrowser = 1.2.4`.

## Authority and source coverage

| Authority or source | Security obligation checked | Result |
|---|---|---|
| `AGENTS.md` §§2, 8, 9, 12 | shared client ownership, server-owned authority, secret handling, narrow verification | One material failure below |
| `architecture/agent-rules.md` | validate the effective value; do not weaken security gates | One material failure below |
| `architecture/wyrd-security-posture.md` | TLS for client/server trust boundaries; secrets absent from logs, traces, and errors | One material failure below |
| Approved spec revision 11: REQ-011, REQ-012, REQ-013, REQ-021, INV-005, AC-004/005/007 | standard device/refresh/revocation wire behavior; no SDK-local token logic; secret-safe client behavior | Pass except target normalization/userinfo refusal |
| TASK-012 Scenario 3 / FIND-TASK-004-5 and -9 | one parsed target decision; path/query/fragment variants share an origin; userinfo is refused and never printed | **FAIL** |
| RFC 6749 §§2.3, 5.1, 5.2, 6 | public-client identification, token success/error decoding, refresh | Pass |
| RFC 7009 §2 | form-encoded token revocation, hint, successful unknown-token semantics | Pass; locked transport decision honored |
| RFC 8628 §§3.1–3.5 | device request, response, polling interval/slow-down/expiry handling | Pass through `oauth2` 5.0 |
| RFC 8693 §2.1 and RFC 7523 §2.1 | form-encoded extension grants | Pass through the shared redirect-free adapter |
| `oauth2` 5.0.0 source (`devicecode.rs`, request preparation and polling) | request-body public client ID, terminal errors, backoff, expiry, cancellation by dropped future | Pass |
| `reqwest` 0.13.4 source (`async_impl/request.rs`) | URL userinfo handling and request/debug behavior | Confirms the finding below |
| `wyrd-spec::operator_connection::HttpsOrigin` | existing standard URL-origin normalization and userinfo rejection | Existing owner available for the correction |

Source paths inspected included the complete cumulative diff and current bodies in:

- `crates/shared/wyrd-client/src/auth.rs`
- `crates/shared/wyrd-client/src/config.rs`
- `crates/shared/wyrd-client/src/saved_login.rs`
- `crates/shared/wyrd-client/src/credentials_file.rs`
- `crates/shared/wyrd-client/src/transport/config.rs`
- `crates/shared/wyrd-client/src/transport/http.rs`
- `crates/wyrd/wyrd-cli/src/auth/login.rs`
- `crates/wyrd/wyrd-cli/src/auth/refresh.rs`
- `crates/wyrd/wyrd-cli/tests/cli_login_journey.rs`
- `crates/wyrd/wyrd-testing/src/human_login.rs`
- workspace and owner manifests plus `Cargo.lock`

## Material proposed findings

### SEC-1 — INCORRECT (Medium): parsed target validation is discarded, so userinfo survives into secret-bearing clients and diagnostics

- **Violated obligation:** TASK-012 Scenario 3 requires every shared caller to use one parsed target decision and requires URL userinfo to be refused and never persisted or printed. The security posture also prohibits secrets in logs, traces, and errors.
- **Locations:** `crates/shared/wyrd-client/src/transport/config.rs:215-240`; `crates/shared/wyrd-client/src/auth.rs:216-252`; `crates/shared/wyrd-client/src/auth.rs:187-194`; `crates/shared/wyrd-client/src/transport/http.rs:128-165`; `crates/shared/wyrd-client/src/config.rs:185-200`.
- **Evidence:** `HttpConfig::validate` parses `base_url` only to inspect scheme and host, accepts every HTTPS URL regardless of userinfo, and returns `()`. Both `TokenExchange::new` and `HttpTransport::new` then retain the original string using `trim_end_matches('/')`. Their `Debug` implementations print that raw value. `TokenExchange` also constructs device, token, platform-token, and revocation endpoints by string concatenation. `canonical_origin` correctly rejects userinfo, but only saved-login selection calls it; explicit API-key/workload clients and `wyrd auth refresh` construct `TokenExchange` without that guard. Thus `https://alice:hunter2@wyrd.example.com` is accepted and `hunter2` appears in `TokenExchange`/`AuthMiddleware`/`HttpTransport` debug output. Path, query, and fragment variants likewise are validated as one origin but then retained as different effective endpoint strings, contrary to the task's claimed shared parsed decision.
- **Plausible exposure path:** a deployment URL supplied through configuration or `WYRD_SERVER_URL` contains standard URL userinfo, often a credential. An error diagnostic or application log formats the client/transport with `Debug`; the raw configured URL, including the password, is emitted. The same root cause can send refresh, revocation, API-key, workload assertion, or delegated-token forms to a path/query derived from the discarded raw spelling rather than the canonical deployment auth endpoint, causing fail-closed login/renewal/logout failures.
- **Observable consequence:** secret-bearing URL material can be disclosed in diagnostics, and equivalent spellings of one deployment do not reliably address the same OAuth endpoints. This leaves FIND-TASK-004-9 and the “one parsed decision” portion of FIND-TASK-004-5 open.
- **Smallest standard correction:** reuse the existing `url::Url` plus `HttpsOrigin::of_url`/`canonical_origin` mechanism as the single construction result for all shared HTTP and OAuth owners. Reject userinfo at that boundary, retain only the normalized origin where the contract is an origin, and derive endpoint URLs with the URL type rather than raw string concatenation. Do not add a new setting, allowlist, URL grammar, or compatibility option.
- **Focused closure proof:** add exact owner-level tests showing (1) a URL with username or password is rejected before either authenticated or OAuth client construction and no error/debug value contains the sentinel; and (2) case/default-port/path/query/fragment spellings use the same normalized origin and send refresh, revocation, and one custom form grant to the root auth endpoints on the existing redirect-free mock. Run only those exact tests and the narrow `wyrd-client` lane covering the write set; no full identity journey or aggregate is required for remediation.

## Positive controls

- `oauth2::BasicClient` owns device authorization, device polling, and refresh, with `AuthType::RequestBody` for the registered public client and no client secret.
- Every grant and the lead-approved revocation request uses the same `reqwest` client with `redirect::Policy::none()`; the redirect regression test covers refresh, JWT bearer, and revocation and observes zero target hits.
- RFC 8693 and RFC 7523 remain one form POST through the shared adapter, rather than gaining a second transport or SDK-specific implementation.
- RFC 7009 revocation sends `token`, `token_type_hint=refresh_token`, and `client_id`; any 2xx is accepted. This review does not reopen the approved loopback-compatible implementation.
- Secret token types remain redacted in `Debug`; login and status output do not print Wyrd access or refresh tokens; refresh tokens rotate under the existing credential-file lock.
- Credential storage rejects unsafe Unix directory/file ownership or modes and performs atomic replacement under the standard file lock.
- `webbrowser::open` replaces all per-OS launcher and command-interpreter construction. The task's approved Windows proof is accepted as stated.
- The new dependencies are exact-version workspace dependencies, `oauth2` keeps default features disabled, and the lockfile resolves registry checksums rather than a git/path override.

## Verification evidence and limits

Per orchestrator direction, this reviewer did not start Cargo or `mise` work while independent reviews were running. The task records successful focused identity journeys, `test:shared`, the `wyrd-cli` library tests, codegen/N-API checks, client-tier checks, workspace-hack, unwrap audit, formatting, lints, and the approved `webbrowser` Windows-target check. Those results support the positive controls but do not exercise accepted userinfo at `HttpConfig`/`TokenExchange`/`HttpTransport`, nor do they prove that path/query/fragment spellings address canonical OAuth endpoints. The finding is established directly from reachable source and requires only the narrow focused proof stated above. Full journey suites and repository aggregates are intentionally neither run nor required at this task-review stage.

## Overall result

**FAIL**

One material security-boundary finding remains: `SEC-1`.
