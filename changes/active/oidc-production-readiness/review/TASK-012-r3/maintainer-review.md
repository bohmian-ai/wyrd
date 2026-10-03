# TASK-012 R3 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation inputs:
  - `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
  - `changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`

The candidate remained at the stated commit throughout this review. The
cumulative base-to-candidate diff and the R2 documentation-only fix diff were
reviewed. No other TASK-012 R3 report was used.

## Changed-surface coverage

| Surface | Owner, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| OAuth transport adapter | `crates/shared/wyrd-client/src/auth.rs`: `AuthHttp::{send, call}`, its `AsyncHttpClient` associated types, and the `oauth2` request paths | The adapter has one responsibility: translate `oauth2` requests onto the workspace `reqwest` client with redirects disabled. Its state is meaningful, its methods sit with that state, and the documentation states buffering, transport errors, cancellation, and partial progress. |
| OAuth client owner | `TokenExchange`, `CliClient`, `RevocationForm`, `PublicClientForm`, `token_response`, `refused`, `refusal`, and `decode_failure`; callers in `AuthMiddleware`, platform handling, saved-login renewal, CLI login/logout/refresh, and `wyrd-testing::HumanSso` | `TokenExchange` remains the cohesive dependency-owning handle for unauthenticated `/auth` operations. Device and refresh are discoverable typed methods backed by `oauth2`; RFC 8693, RFC 7523, and the lead-approved RFC 7009 exception share the narrow form adapter. The former public grant-enum escape hatch is gone. Argument and return types expose the actual protocol values rather than generic maps or raw JSON. |
| Auth middleware integration | `AuthMiddleware::{build, bearer, force_refresh, post_token_request, base_url}` and its `Debug` implementation | The existing cache, single-flight, durable-credential, and renewable-source owner remains intact. The normalized origin is composed through `TokenExchange`, and the changed diagnostic representation documents what is and is not emitted. No parallel token lifecycle was introduced. |
| HTTP target authority | `crates/shared/wyrd-client/src/transport/config.rs`: `HttpConfig::validate`, `TransportConfig::validate`, focused normalization/refusal tests; `HttpsOrigin::{of_url, as_str}` in `wyrd-spec` | Parsing and normalization reuse the existing `HttpsOrigin` domain type. The public config docs and return type now make the origin contract explicit, including userinfo refusal and removal of path, query, and fragment. The helper and changed tests have the required error/panic contracts. |
| Authenticated HTTP transport | `crates/shared/wyrd-client/src/transport/http.rs`: `HttpTransport::{new, base_url, with_auth, authenticated_url}` and `Debug`; same-origin callers and transport tests | `HttpTransport` owns its normalized origin and uses that typed authority for both relative-route construction and absolute-URL admission. This deletes the hand-written origin parser and keeps credential routing with the transport owner. The debug boundary is documented and redacted to the normalized origin. |
| Saved-login identity and renewal | `crates/shared/wyrd-client/src/saved_login.rs`: `canonical_origin`, `SavedLogins::{select, source, renew}`, `SavedLoginSource::mint`, and their tests/callers | Saved-login lookup and network clients consume the same `HttpConfig`/`HttpsOrigin` authority. Renewal still occurs under the existing file lock and reread path, while only the grant implementation changes to `TokenExchange::refresh`. The ownership and failure path remain easy to follow. |
| CLI login, refresh, logout | `crates/wyrd/wyrd-cli/src/auth/{login,refresh}.rs`: `LoginFlow::{new, run, save}`, `login`, `logout`, `dispatch`; CLI argument tests and `cli_login_journey` | The flow remains centered on `LoginFlow`; browser launch is a direct `webbrowser::open` call at the single use site, and device polling belongs to the shared OAuth owner. Removing the bespoke polling and per-platform command helpers reduces indirection without weakening the printed fallback or `--no-browser`. |
| Journey fixture | `crates/wyrd/wyrd-testing/src/human_login.rs`: `HumanSso::cli_login` plus approval, save, and revoke callers | The fixture now exercises the same typed device polling method as the CLI instead of maintaining a second poll loop. Its public helper documentation describes that correspondence. |
| Dependency and generated-contract parity | Workspace and crate manifests, `Cargo.lock`, `workspace-hack`, generated `transport_config_{http,enum}.json` copies, and their schema tests | `oauth2` and `webbrowser` are pinned at the workspace and consumed only by their owning crates. The comments accurately distinguish `oauth2`-owned device/refresh behavior from the retained form requests. Both published and golden schemas carry identical `HttpConfig` description changes. No Python stub or TypeScript declaration surface changed. |
| Focused tests and evidence | `auth.rs` redirect/config tests, transport token fixture, saved-login origin and unsafe-store tests, Postgres auth fixture test, filtered CLI journey, and the exact-selector evidence recorded in TASK-012 | Test names describe caller-visible outcomes, the token mock now uses RFC 6749 `expires_in`, and the remediation evidence records nonempty exact selectors. The R2 write set is documentation/schema text only and its recorded narrow format, clippy, doc, schema-drift, and diff checks match that scope. |

## Material findings

None.

## Maintainer calibration notes

- `TokenExchange::base_url` and a few older surrounding comments retain the
  term “base URL” while the documented value is now the normalized deployment
  origin. The public `HttpConfig`, `TokenExchange`, `AuthMiddleware::base_url`,
  and `HttpTransport::base_url` contracts all state the effective behavior, so
  this is vocabulary continuity rather than an unsafe or contradictory public
  contract. Renaming it would add churn and is not a finding.
- Some import grouping in the touched Rust modules could be reordered, but all
  imports remain in the required module-level dependency block. This has no
  behavioral, security, tenancy, durability, or public-contract consequence
  and is not a finding.
- The public re-export of `StandardDeviceAuthorizationResponse` intentionally
  exposes the vetted library's typed device response used by the paired
  `TokenExchange` methods. The task explicitly requires that standard client;
  adding a Wyrd wrapper would duplicate protocol shape without improving the
  maintenance boundary.
- The locked RFC 7009 form POST, the accepted `webbrowser` Windows-target
  compilation proof, and rejection of PLAT-001 were treated as settled and
  were not reopened.

## Verification assessment

No broad suite or aggregate was run for this maintainer review. The reviewed
task evidence records the focused identity journeys and exact named tests for
the runtime write set, followed by the R1 focused clippy/boundary checks. The
R2 remediation records `mise run fmt`, package-scoped `cargo clippy`,
`cargo doc`, the four schema-drift tests, and `git diff --check`; those are the
narrow checks applicable to its documentation and generated-description write
set. A read-only `git diff --check` of the immutable cumulative range produced
no output during this review.

## Result

**PASS** — every materially changed symbol was traceable through its owner,
callers, relevant tests, and generated contract copies. The resulting layout,
method ownership, types, test intent, and documentation are maintainable, and
there is no material maintainer finding.
