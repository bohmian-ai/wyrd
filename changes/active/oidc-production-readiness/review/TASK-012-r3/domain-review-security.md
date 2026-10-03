# TASK-012 round-3 OAuth/OIDC security domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Round-1 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Round-2 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`

The candidate was `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447` before and after this review. I reviewed the complete base-to-candidate range and used the latest remediation diff only to locate the documentation closure. I did not read another round-3 report.

## Reviewed boundary and authority coverage

| Boundary | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| Deployment target and credential destination | `AGENTS.md` security rules; `architecture/wyrd-security-posture.md`; TASK-012 Scenario 3 and `FIND-TASK-012-2` | `HttpConfig::validate`, `HttpsOrigin::of_url`, `TokenExchange::new`, `HttpTransport::{new,authenticated_url}`, `canonical_origin`; CLI login/logout/refresh and `ClientConfig::resolve_credential` callers | PASS |
| Device authorization and polling | SPEC REQ-011, REQ-021, AC-004, AC-007; RFC 8628 §§3.1–3.5 | `TokenExchange::{device_authorization,device_access_token}`, `LoginFlow::run`, `HumanSso::cli_login` | PASS |
| Refresh and saved-login renewal | SPEC REQ-012, REQ-021, AC-004; RFC 6749 §6; RFC 9700 refresh replay requirement fixed by the spec | `TokenExchange::refresh`, `SavedLogins::renew`, `SavedLoginSource::mint`, middleware renewable caching | PASS |
| RFC 8693 and RFC 7523 retained forms | SPEC REQ-013, REQ-021, INV-005, AC-005; RFC 8693 §2.1; RFC 7523 §2.1 | private `TokenExchange::{grant,post_form}`, `AuthMiddleware::{exchange,exchange_workload,exchange_delegated,post_token_request}`, `platform_session` | PASS |
| Revocation | SPEC REQ-012 and REQ-021; RFC 7009 §2; lead-fixed implementation decision | `TokenExchange::revoke_refresh_token`, `RevocationForm`, CLI local-first `logout` | PASS |
| Redirect and replay safety | TASK-012 prohibition and `FIND-TASK-004-13`; OAuth endpoint confidentiality | `AuthHttp`, `reqwest::redirect::Policy::none`, every `oauth2` request and retained form request, redirect test | PASS |
| Credential selection, storage, and disclosure | SPEC REQ-012, INV-005, AC-004; `architecture/wyrd-security-posture.md` credential rules | `ClientConfig::resolve_credential`, `CredentialChain`, `ResolvedCredential::Debug`, `SavedLogins`, `CredentialsFile`, CLI output and debug owners | PASS |
| Dependencies and platform launch | TASK-012 exact dependency decisions; lead-fixed Windows proof and rejected PLAT-001 | workspace/client/CLI manifests, lockfile, `oauth2` feature graph, `webbrowser::open` caller | PASS |

Primary standards were checked against the RFC Editor publications for [RFC 6749](https://www.rfc-editor.org/rfc/rfc6749), [RFC 7009](https://www.rfc-editor.org/rfc/rfc7009), [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628), [RFC 8693](https://www.rfc-editor.org/rfc/rfc8693), and [RFC 7523](https://www.rfc-editor.org/rfc/rfc7523).

## Security path assessment

- `HttpConfig::validate` parses once and reduces the configured deployment URL through `HttpsOrigin::of_url`. HTTPS and genuine loopback HTTP are accepted; remote cleartext, missing hosts, unsupported schemes, and userinfo are refused without echoing the submitted URL. OAuth, authenticated HTTP, and saved-login identity consume that same normalized origin.
- `TokenExchange::new` derives fixed `/auth` endpoints only from the validated origin. Its `reqwest` client has `Policy::none`, so neither `oauth2` nor the retained form sender follows a `307`/`308` and replays an API key, workload assertion, device code, refresh token, subject token, actor token, or revocation token at another location.
- RFC 8628 device authorization and polling and RFC 6749 refresh are owned by `oauth2` 5.0.0. The private form path has no device or refresh caller; its reachable producers are RFC 8693 token exchange, RFC 7523 JWT bearer, and the separately fixed RFC 7009 revocation form.
- The public client sends one `client_id=wyrd-cli` body parameter. The grant-specific parameters come from typed request structs; no URL query carries a secret. RFC 8693 actor-token type is present exactly when the actor token is present.
- Revocation sends `token`, `token_type_hint=refresh_token`, and the public client identifier in one form POST. It accepts any success status as RFC 7009 requires, follows no redirect, and remains local-first/best-effort at logout. This review does not reopen the approved adapter decision.
- OAuth refusal bodies are decoded into the stable client/server error channels without including request bodies or submitted credentials. Debug output for `TokenExchange`, `AuthMiddleware`, `HttpTransport`, resolved credentials, and saved-login summaries is redacted or limited to the normalized origin and non-secret selectors.
- Explicit caller credentials outrank environment credentials; environment credentials outrank saved login; saved login outranks the file API-key floor. A tenant selector cannot silently retarget a self-identifying bearer or API key, and a requested saved-login tenant cannot fall through to another identity.
- Saved refresh tokens retain the existing owner-only file, directory-ownership, non-symlink, lock, reread, rotation, and atomic-replace controls. The R2 candidate changes documentation and generated descriptions only and does not weaken this boundary.
- `oauth2` is exactly 5.0.0 with default features disabled and uses the repository's redirect-free `reqwest` 0.13 adapter rather than enabling its bundled HTTP integration. `webbrowser` is exactly 1.2.4. No unpinned source or new credential-handling feature entered the dependency graph.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task. I found no reachable security defect for which added client state, validation, or compatibility behavior would be justified.

### Positive Controls

- One normalized, userinfo-free origin owns OAuth endpoints, authenticated HTTP requests, and saved-login identity.
- Every secret-bearing OAuth request uses a redirect-disabled client.
- Device polling and refresh use the pinned `oauth2` implementation; retained forms are private and narrowly owned.
- Secrets use redacted wrappers and do not appear in debug output, request URLs, login output, or refusal text.
- Saved-login renewal remains single-use under the cross-process file lock, with reread-before-refresh and atomic replacement.
- Cross-origin absolute authenticated URLs and unsafe credential stores fail closed.

## Prior finding closure

- `FIND-TASK-012-1`: **CLOSED**. No callable custom form path accepts device or refresh grants.
- `FIND-TASK-012-2`: **CLOSED**. OAuth, HTTP transport, and saved-login consumers share the parsed normalized origin, and userinfo is refused without disclosure.
- `FIND-TASK-012-3`: the R2 diff is documentation-only; the security-relevant diagnostic and origin contracts now accurately describe the implemented controls. No security remainder exists.
- `FIND-TASK-012-4`: no security remainder; the exact-selector evidence remains recorded in the task.
- `PLAT-001`: remains rejected and was not reopened.

## Verification and limits

Focused review verification:

```text
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=transport::config::tests::remote_cleartext_malformed_and_unsupported_targets_are_refused) | test(=transport::config::tests::https_and_loopback_http_are_accepted) | test(=auth::tests::token_exchange_never_follows_a_redirect) | test(=saved_login::tests::canonical_origin_is_the_url_origin) | test(=config::tests::saved_login_ranks_between_env_and_credentials_file)'
```

Result: 5 selected, 5 passed. `git diff --check` for the immutable base-to-candidate range also passed. `cargo tree -p wyrd-client -e features` confirmed `oauth2` 5.0.0 and no `oauth2` HTTP-client feature on the reviewed client path.

Per standing direction, I did not run or require a full journey suite, language sweep, `test:shared`, `test:rust`, `gate`, or another aggregate. The focused evidence directly covers the security write set; there is no security verification limitation material to this verdict.

## Overall result

**PASS**

The cumulative candidate satisfies the TASK-012 OAuth/OIDC client security boundary. No material security finding remains.
