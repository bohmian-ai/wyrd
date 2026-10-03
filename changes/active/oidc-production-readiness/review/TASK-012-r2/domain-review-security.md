# TASK-012 round 2 security domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate and reviewed `HEAD`: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`

The candidate remained unchanged while this review was performed.

## Reviewed boundary

This review traced every materially changed OAuth/OIDC client trust boundary from external server URL and credential inputs through `HttpConfig::validate`, `HttpsOrigin::of_url`, `TokenExchange`, the redirect-free `AuthHttp`, `oauth2::BasicClient`, `HttpTransport::authenticated_url`, saved-login selection and renewal, and the CLI login, refresh, logout, API-key, workload JWT, delegation, and platform-session callers. It checked URL parsing and origin binding; public-client identification; RFC 8628 device authorization and polling; RFC 6749 refresh; RFC 8693 and RFC 7523 extension grants; the lead-approved RFC 7009 form POST; redirect behavior; cross-origin authenticated URLs; and token/userinfo exposure in `Debug`, errors, output, and persistence.

The locked decisions were treated as closed: RFC 7009 remains one form POST through the redirect-free client because `oauth2` 5.0 refuses non-HTTPS revocation URLs; the accepted Windows proof remains the `webbrowser` target check; and PLAT-001 remains rejected.

## Authority and source coverage

| Authority or source | Security obligation checked | Result |
|---|---|---|
| `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-security-posture.md` | shared client ownership, effective-value validation, TLS/loopback boundary, no secret disclosure, no weakened control | PASS |
| Approved spec revision 11: REQ-011, REQ-012, REQ-013, REQ-021, INV-004/005/007, AC-004/005/007 | standard device/refresh/revocation and extension-grant behavior; shared SDK authority; credential-safe client behavior | PASS |
| Original TASK-012 Scenarios 1–4 and acceptance criteria | vetted OAuth paths, redirect refusal, canonical origin, userinfo refusal, saved-login behavior, no invented option or compatibility path | PASS |
| R1 findings `FIND-TASK-012-1` and `FIND-TASK-012-2` | no custom device/refresh path; one parsed normalized origin for OAuth and authenticated HTTP | PASS, closed with source evidence |
| RFC 6749 §§2.3, 5.1, 5.2, 6; RFC 7009 §2; RFC 8628 §§3.1–3.5; RFC 8693 §2.1; RFC 7523 §2.1 | request form, public-client identification, polling/refresh/error behavior, revocation, extension grants | PASS |
| `oauth2` 5.0.0 source (`devicecode.rs`, `token/mod.rs`, `endpoint.rs`) | request-body client ID, device expiry/polling/slow-down, refresh serialization, response processing | PASS |
| `wyrd_spec::operator_connection::HttpsOrigin` | standard URL parsing, normalization, loopback-only HTTP, userinfo refusal | PASS |

Complete current bodies and cumulative diffs were inspected in `crates/shared/wyrd-client/src/auth.rs`, `saved_login.rs`, `transport/config.rs`, `transport/http.rs`, `config.rs`, `client.rs`, and `platform/handle.rs`; `crates/wyrd/wyrd-cli/src/auth/login.rs` and `refresh.rs`; relevant tests and fixtures; owner manifests and lockfile; and the local `oauth2` 5.0.0 implementation.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No optional hardening is required by the approved task or established OAuth/OIDC practice.

### Positive Controls

- `HttpConfig::validate` parses once and returns `HttpsOrigin`; `TokenExchange` and `HttpTransport` retain that normalized, userinfo-free origin. Case, default port, path, query, and fragment variants converge on the root deployment origin (`transport/config.rs:202-244`, `auth.rs:220-277`, `transport/http.rs:153-173`).
- HTTPS is required except for actual loopback hosts. Userinfo is rejected before any OAuth or authenticated client exists, and refusal messages do not echo the supplied URL or its credentials.
- `HttpTransport::authenticated_url` compares parsed `HttpsOrigin` values before attaching `x-wyrd-access-token`; cross-origin and userinfo-bearing absolute URLs fail closed (`transport/http.rs:745-776`). Relative inputs remain rooted at the configured origin.
- Device authorization and polling and refresh are reachable only through `oauth2::BasicClient`; the prior public custom `exchange(&TokenRequest)` path is gone. The remaining private form owner has only RFC 8693/RFC 7523 producers, plus the locked RFC 7009 request (`auth.rs:286-484`, `auth.rs:998-1069`).
- One `AuthHttp` pool has `reqwest::redirect::Policy::none()` and carries every secret-bearing OAuth request. A 307/308 is returned as a non-success response and cannot replay a token body to another host (`auth.rs:124-179`, `auth.rs:241-276`).
- OAuth public-client identification uses `client_id=wyrd-cli` in the request body with no client secret. Device polling, `authorization_pending`, `slow_down`, expiry, and refresh serialization are owned by `oauth2` 5.0.
- `SecretBearer` and `SecretString` remain redacted under `Debug`. `TokenExchange`, `AuthMiddleware`, and `HttpTransport` debug output contains only the normalized origin and non-secret structure; saved-login status projects no token.
- Login prints only the RFC 8628 user code and verification URL, never access or refresh tokens. Saved-login renewal stays under the existing exclusive file lock and atomic replace. Logout deletes locally before the best-effort idempotent RFC 7009 revocation.
- The R1 remediation adds no dependency, setting, allowlist, parser, compatibility route, retry journal, launcher, or security mechanism beyond the standard URL parser, existing origin type, installed OAuth client, and redirect-free `reqwest` client.

## Prior-finding closure

- `FIND-TASK-012-1`: closed. `TokenExchange::exchange` was removed. Device and refresh grants now have only the `oauth2` paths; RFC 8693/RFC 7523 retain the one necessary private form path and RFC 7009 retains the locked exception.
- `FIND-TASK-012-2`: closed. `HttpConfig::validate` returns the normalized `HttpsOrigin`, and both `TokenExchange` and `HttpTransport` consume it. Userinfo is refused without disclosure; equivalent URL spellings address the same root deployment.

No new security finding was discovered.

## Verification evidence and limits

The remediation records passing exact selectors for the URL-validation, origin-canonicalization, redirect-replay, and shared authenticated-client tests; the focused CLI device journey; the `wyrd-client` target set; focused clippy; client-tier and CLI-tier boundaries; unwrap audit; formatting; and `git diff --check`. Source inspection confirmed that these proofs exercise the changed trust-boundary owners and that the recorded candidate contains the stated code.

In accordance with the task-review direction, this reviewer ran no full journey suite or aggregate and requires none. The accepted Windows and RFC 7009 decisions were not reopened. No verification limit prevents a security conclusion.

## Overall result

**PASS**

The OAuth/OIDC client trust boundary satisfies the original task and R1 security remediation with standard library behavior and no independently proposed material finding.
